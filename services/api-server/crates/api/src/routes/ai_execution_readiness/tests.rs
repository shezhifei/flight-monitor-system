use super::*;
use crate::middleware::jwt::JwtSecret;
use crate::test_support::{
    cleanup_proposal_by_id, create_test_pool, has_test_db, insert_smoke_proposal_default_created_at,
    insert_smoke_proposal_with_created_at, make_test_jwt, proposal_exists_by_id, EnvGuard,
};
use actix_web::{http::StatusCode, test, App};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use fms_application::services::ai_execution_metrics_service::AiExecutionMetricsService;
use fms_domain::error::DomainError;
use fms_domain::events::DomainEventOutboxRow;
use fms_domain::ports::database_metadata_port::DatabaseMetadataPort;
use fms_domain::ports::domain_event_outbox_repository::DomainEventOutboxRepository;
use fms_infrastructure::repositories::pg_ai_proposal_repository::PgAiProposalRepository;
use fms_infrastructure::repositories::pg_database_metadata_adapter::PgDatabaseMetadataAdapter;
use fms_infrastructure::repositories::pg_domain_event_outbox_repository::PgDomainEventOutboxRepository;
use fms_infrastructure::repositories::pg_todo_repository::PgTodoRepository;
use serde_json::json;

struct StubOutbox;

#[async_trait]
impl DomainEventOutboxRepository for StubOutbox {
    async fn claim_pending_for_relay(&self, _limit: i64) -> Result<Vec<DomainEventOutboxRow>, DomainError> {
        Ok(vec![])
    }
    async fn count_unpublished(&self) -> Result<i64, DomainError> {
        Ok(0)
    }
    async fn oldest_unpublished(&self) -> Result<Option<DateTime<Utc>>, DomainError> {
        Ok(None)
    }
    async fn delete_by_aggregate_and_type(
        &self,
        _aggregate_id: &str,
        _event_type: &str,
        _older_than: DateTime<Utc>,
    ) -> Result<u64, DomainError> {
        Ok(0)
    }
    async fn count_by_aggregates_and_type(
        &self,
        _aggregate_ids: &[String],
        _event_type: &str,
        _older_than: DateTime<Utc>,
    ) -> Result<i64, DomainError> {
        Ok(0)
    }
    async fn delete_by_aggregates_and_type(
        &self,
        _aggregate_ids: &[String],
        _event_type: &str,
        _older_than: DateTime<Utc>,
    ) -> Result<u64, DomainError> {
        Ok(0)
    }
}

fn stub_outbox() -> Arc<dyn DomainEventOutboxRepository + Send + Sync> {
    Arc::new(StubOutbox)
}

fn pg_outbox(pool: &sqlx::PgPool) -> Arc<dyn DomainEventOutboxRepository + Send + Sync> {
    Arc::new(PgDomainEventOutboxRepository::new(pool.clone()))
}

fn metrics_for(pool: sqlx::PgPool) -> Arc<AiExecutionMetricsService> {
    Arc::new(AiExecutionMetricsService::new(
        Arc::new(PgAiProposalRepository::new(pool)),
        stub_outbox(),
    ))
}

fn metrics_for_pool(pool: &sqlx::PgPool) -> Arc<AiExecutionMetricsService> {
    Arc::new(AiExecutionMetricsService::new(
        Arc::new(PgAiProposalRepository::new(pool.clone())),
        pg_outbox(pool),
    ))
}

fn readiness_for(pool: &sqlx::PgPool) -> Arc<AiExecutionReadinessService> {
    Arc::new(AiExecutionReadinessService::new(
        Some(db_metadata_for(pool)),
        Some(pg_outbox(pool)),
    ))
}

fn db_metadata_for(pool: &sqlx::PgPool) -> Arc<dyn DatabaseMetadataPort + Send + Sync> {
    Arc::new(PgDatabaseMetadataAdapter::new(pool.clone()))
}

fn rollout_for(
    readiness: Arc<AiExecutionReadinessService>,
    metrics: Arc<AiExecutionMetricsService>,
    pool: sqlx::PgPool,
) -> Arc<AiRolloutStatusService> {
    let outbox = pg_outbox(&pool);
    let run_events = Arc::new(
        fms_infrastructure::repositories::pg_ai_run_event_repository::PgAiRunEventRepository::new(pool.clone()),
    );
    let proposal_repo = Arc::new(PgAiProposalRepository::new(pool.clone()));
    let todo_repo = Arc::new(PgTodoRepository::new(pool.clone()));
    let db_metadata = db_metadata_for(&pool);
    Arc::new(AiRolloutStatusService::new(
        readiness,
        metrics,
        proposal_repo,
        todo_repo,
        db_metadata,
        outbox,
        run_events,
    ))
}

fn build_test_app_full(
    readiness: Arc<AiExecutionReadinessService>,
    metrics: Arc<AiExecutionMetricsService>,
    rollout: Arc<AiRolloutStatusService>,
) -> App<
    impl actix_web::dev::ServiceFactory<
        actix_web::dev::ServiceRequest,
        Config = (),
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
        InitError = (),
    >,
> {
    App::new()
        .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
        .app_data(web::Data::new(readiness))
        .app_data(web::Data::new(metrics))
        .app_data(web::Data::new(rollout))
        .configure(configure)
}

fn build_test_app(
    readiness: Arc<AiExecutionReadinessService>,
    metrics: Arc<AiExecutionMetricsService>,
) -> App<
    impl actix_web::dev::ServiceFactory<
        actix_web::dev::ServiceRequest,
        Config = (),
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
        InitError = (),
    >,
> {
    let pool = sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap();
    let run_events = Arc::new(
        fms_infrastructure::repositories::pg_ai_run_event_repository::PgAiRunEventRepository::new(pool.clone()),
    );
    let proposal_repo = Arc::new(PgAiProposalRepository::new(pool.clone()));
    let todo_repo = Arc::new(PgTodoRepository::new(pool.clone()));
    let db_metadata = db_metadata_for(&pool);
    let rollout = Arc::new(AiRolloutStatusService::new(
        readiness.clone(),
        metrics.clone(),
        proposal_repo,
        todo_repo,
        db_metadata,
        stub_outbox(),
        run_events,
    ));
    build_test_app_full(readiness, metrics, rollout)
}

#[actix_web::test]
async fn readiness_route_requires_authenticated_operator() {
    let readiness = Arc::new(AiExecutionReadinessService::new_for_test());
    let pool = sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap();
    let metrics = metrics_for(pool);
    let app = test::init_service(build_test_app(readiness, metrics)).await;

    let req = test::TestRequest::get()
        .uri("/api/v2/ai/execution-readiness")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn readiness_route_returns_report_for_authorized_operator() {
    let readiness = Arc::new(AiExecutionReadinessService::new_for_test());
    let pool = sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap();
    let metrics = metrics_for(pool);
    let app = test::init_service(build_test_app(readiness, metrics)).await;

    let token = make_test_jwt(&["system.config_read"]);
    let req = test::TestRequest::get()
        .uri("/api/v2/ai/execution-readiness")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[actix_web::test]
async fn readiness_route_rejects_unauthorized_permission() {
    let readiness = Arc::new(AiExecutionReadinessService::new_for_test());
    let pool = sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap();
    let metrics = metrics_for(pool);
    let app = test::init_service(build_test_app(readiness, metrics)).await;

    let token = make_test_jwt(&["ai.chat"]);
    let req = test::TestRequest::get()
        .uri("/api/v2/ai/execution-readiness")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn readiness_route_accepts_ai_execution_readiness_permission() {
    let readiness = Arc::new(AiExecutionReadinessService::new_for_test());
    let pool = sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap();
    let metrics = metrics_for(pool);
    let app = test::init_service(build_test_app(readiness, metrics)).await;

    let token = make_test_jwt(&["ai.execution.readiness"]);
    let req = test::TestRequest::get()
        .uri("/api/v2/ai/execution-readiness")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[actix_web::test]
async fn metrics_route_requires_authentication() {
    let readiness = Arc::new(AiExecutionReadinessService::new_for_test());
    let pool = sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap();
    let metrics = metrics_for(pool);
    let app = test::init_service(build_test_app(readiness, metrics)).await;

    let req = test::TestRequest::get()
        .uri("/api/v2/ai/execution-readiness/metrics")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn metrics_route_rejects_unauthorized_permission() {
    let readiness = Arc::new(AiExecutionReadinessService::new_for_test());
    let pool = sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap();
    let metrics = metrics_for(pool);
    let app = test::init_service(build_test_app(readiness, metrics)).await;

    let token = make_test_jwt(&["ai.chat"]);
    let req = test::TestRequest::get()
        .uri("/api/v2/ai/execution-readiness/metrics")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn metrics_route_accepts_system_config_read() {
    let readiness = Arc::new(AiExecutionReadinessService::new_for_test());
    let pool = sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap();
    let metrics = metrics_for(pool);
    let app = test::init_service(build_test_app(readiness, metrics)).await;

    let token = make_test_jwt(&["system.config_read"]);
    let req = test::TestRequest::get()
        .uri("/api/v2/ai/execution-readiness/metrics")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    // Will be 500 because the pool is lazy and DB is unreachable,
    // but it passes auth (not 401 or 403)
    assert!(
        resp.status() == StatusCode::OK || resp.status() == StatusCode::INTERNAL_SERVER_ERROR,
        "expected 200 or 500 (DB unreachable), got {}",
        resp.status()
    );
}

// ── Readiness API Smoke Tests ─────────────────────────────────────────

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; API readiness smoke — run via scripts/dev/run_aip_api_staging_smoke.ps1"]
async fn api_readiness_smoke_returns_ready_with_staging_override() {
    if !has_test_db() {
        return;
    }
    let pool = create_test_pool().await;
    let readiness = readiness_for(&pool);
    let metrics = metrics_for(pool);
    let app = test::init_service(build_test_app(readiness, metrics)).await;

    let _guard1 = EnvGuard::set("FMS_AI_PROPOSAL_EXECUTION_ENABLED", "true");
    let _guard2 = EnvGuard::set("FMS_AI_EXECUTION_READINESS_OVERRIDE", "staging");

    let token = make_test_jwt(&["system.config_read"]);
    let req = test::TestRequest::get()
        .uri("/api/v2/ai/execution-readiness")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(
        body.get("overall_status").and_then(|v| v.as_str()),
        Some("Ready"),
        "readiness should be Ready with staging override, got: {body:?}"
    );
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; API readiness smoke — run via scripts/dev/run_aip_api_staging_smoke.ps1"]
async fn api_readiness_smoke_returns_not_ready_without_override() {
    if !has_test_db() {
        return;
    }
    let pool = create_test_pool().await;
    let readiness = readiness_for(&pool);
    let metrics = metrics_for(pool);
    let app = test::init_service(build_test_app(readiness, metrics)).await;

    let _guard1 = EnvGuard::set("FMS_AI_PROPOSAL_EXECUTION_ENABLED", "true");
    let _guard2 = EnvGuard::remove("FMS_AI_EXECUTION_READINESS_OVERRIDE");

    let token = make_test_jwt(&["system.config_read"]);
    let req = test::TestRequest::get()
        .uri("/api/v2/ai/execution-readiness")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(
        body.get("overall_status").and_then(|v| v.as_str()),
        Some("NotReady"),
        "readiness should be NotReady without staging override, got: {body:?}"
    );
}

#[actix_web::test]
async fn rollout_status_route_requires_authenticated_operator() {
    let readiness = Arc::new(AiExecutionReadinessService::new_for_test());
    let pool = sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap();
    let metrics = metrics_for(pool);
    let app = test::init_service(build_test_app(readiness, metrics)).await;

    let req = test::TestRequest::get()
        .uri("/api/v2/ai/execution-readiness/rollout-status")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn rollout_status_route_rejects_unauthorized_permission() {
    let readiness = Arc::new(AiExecutionReadinessService::new_for_test());
    let pool = sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap();
    let metrics = metrics_for(pool);
    let app = test::init_service(build_test_app(readiness, metrics)).await;

    let token = make_test_jwt(&["ai.chat"]);
    let req = test::TestRequest::get()
        .uri("/api/v2/ai/execution-readiness/rollout-status")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn rollout_status_route_accepts_system_config_read() {
    let readiness = Arc::new(AiExecutionReadinessService::new_for_test());
    let pool = sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap();
    let metrics = metrics_for(pool);
    let app = test::init_service(build_test_app(readiness, metrics)).await;

    let token = make_test_jwt(&["system.config_read"]);
    let req = test::TestRequest::get()
        .uri("/api/v2/ai/execution-readiness/rollout-status")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    // Should pass auth, return 500 (lazy pool unreachable) or 200
    assert!(
        resp.status() == StatusCode::OK || resp.status() == StatusCode::INTERNAL_SERVER_ERROR,
        "expected 200 or 500, got {}",
        resp.status()
    );
}

#[actix_web::test]
async fn cleanup_smoke_route_requires_authenticated_operator() {
    let readiness = Arc::new(AiExecutionReadinessService::new_for_test());
    let pool = sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap();
    let metrics = metrics_for(pool);
    let app = test::init_service(build_test_app(readiness, metrics)).await;

    let req = test::TestRequest::post()
        .uri("/api/v2/ai/execution-readiness/cleanup-smoke")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn cleanup_smoke_route_rejects_unauthorized_permission() {
    let readiness = Arc::new(AiExecutionReadinessService::new_for_test());
    let pool = sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap();
    let metrics = metrics_for(pool);
    let app = test::init_service(build_test_app(readiness, metrics)).await;

    let token = make_test_jwt(&["system.config_read"]);
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/execution-readiness/cleanup-smoke")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn cleanup_smoke_route_accepts_system_ops_admin() {
    let readiness = Arc::new(AiExecutionReadinessService::new_for_test());
    let pool = sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap();
    let metrics = metrics_for(pool);
    let app = test::init_service(build_test_app(readiness, metrics)).await;

    let token = make_test_jwt(&["system.ops_admin"]);
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/execution-readiness/cleanup-smoke?older_than_hours=24&dry_run=true")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    // Will be 400 (FMS_AI_SMOKE_CLEANUP_ENABLED=true env variable is required) or 500 (lazy pool unreachable),
    // but it passes auth (not 401 or 403)
    assert!(
        resp.status() == StatusCode::BAD_REQUEST || resp.status() == StatusCode::INTERNAL_SERVER_ERROR,
        "expected 400 or 500, got {}",
        resp.status()
    );
}

// ── Rollout Status & Cleanup DB Smoke Tests ─────────────────────────

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn api_rollout_status_smoke_returns_aggregate() {
    if !has_test_db() {
        return;
    }
    let pool = create_test_pool().await;
    let readiness = readiness_for(&pool);
    let metrics = metrics_for_pool(&pool);
    let rollout = rollout_for(readiness.clone(), metrics.clone(), pool.clone());
    let app = test::init_service(build_test_app_full(readiness, metrics, rollout)).await;

    let _guard1 = EnvGuard::set("FMS_AI_PROPOSAL_EXECUTION_ENABLED", "Todo.create");
    let _guard2 = EnvGuard::set("FMS_AI_EXECUTION_READINESS_OVERRIDE", "staging");

    let token = make_test_jwt(&["system.config_read"]);
    let req = test::TestRequest::get()
        .uri("/api/v2/ai/execution-readiness/rollout-status")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body.get("execution_mode").and_then(|v| v.as_str()), Some("allowlist"));
    assert!(body
        .get("allowed_actions")
        .and_then(|v| v.as_array())
        .unwrap()
        .contains(&json!("Todo.create")));
    assert!(body.get("metrics").is_some());
    assert!(body.get("readiness").is_some());
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn cleanup_smoke_dry_run_does_not_delete_data() {
    if !has_test_db() {
        return;
    }
    let pool = create_test_pool().await;
    let readiness = readiness_for(&pool);
    let metrics = metrics_for_pool(&pool);
    let rollout = rollout_for(readiness.clone(), metrics.clone(), pool.clone());
    let app = test::init_service(build_test_app_full(readiness, metrics, rollout)).await;

    let _guard1 = EnvGuard::set("FMS_AI_SMOKE_CLEANUP_ENABLED", "true");

    // Insert a recent smoke proposal (30 min ago). Because older_than_hours=1
    // means cutoff = NOW() - 1h, this proposal is too new to be matched by
    // any cleanup query using older_than_hours >= 1. This isolates the test
    // from cleanup_smoke_execute_removes_only_old_smoke_data (which also uses
    // older_than_hours=1 but with dry_run=false) running in parallel.
    let thirty_min_ago = Utc::now() - chrono::Duration::minutes(30);
    let proposal_id = format!("dryrun_prop_{}", Utc::now().timestamp_micros());
    insert_smoke_proposal_with_created_at(
        &pool,
        &proposal_id,
        "dry_run_job",
        "dry_run_run",
        "obj_dryrun",
        json!({"smoke":"true"}),
        thirty_min_ago,
    )
    .await
    .unwrap();

    let token = make_test_jwt(&["system.ops_admin"]);
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/execution-readiness/cleanup-smoke?older_than_hours=1&dry_run=true")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body.get("dry_run").and_then(|v| v.as_bool()), Some(true));

    // Verify proposal still exists — dry-run must never delete data,
    // and the parallel execute test's older_than_hours=1 cutoff can't
    // reach this 30-minute-old proposal either.
    let exists = proposal_exists_by_id(&pool, &proposal_id).await.unwrap();
    assert!(exists, "dry-run must not delete matching proposals");

    // Cleanup after test
    cleanup_proposal_by_id(&pool, &proposal_id).await.unwrap();
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn cleanup_smoke_execute_removes_only_old_smoke_data() {
    if !has_test_db() {
        return;
    }
    let pool = create_test_pool().await;
    let readiness = readiness_for(&pool);
    let metrics = metrics_for_pool(&pool);
    let rollout = rollout_for(readiness.clone(), metrics.clone(), pool.clone());
    let app = test::init_service(build_test_app_full(readiness, metrics, rollout)).await;

    let _guard1 = EnvGuard::set("FMS_AI_SMOKE_CLEANUP_ENABLED", "true");

    let old_smoke_id = format!("old_smoke_{}", Utc::now().timestamp_micros());
    let new_smoke_id = format!("new_smoke_{}", Utc::now().timestamp_micros());

    // Insert an old smoke proposal (created 2 hours ago)
    let two_hours_ago = Utc::now() - chrono::Duration::hours(2);
    insert_smoke_proposal_with_created_at(
        &pool,
        &old_smoke_id,
        "smoke_job_old",
        "run_old",
        "obj_old",
        json!({"smoke":"true"}),
        two_hours_ago,
    )
    .await
    .unwrap();

    // Insert a new smoke proposal (created now)
    insert_smoke_proposal_default_created_at(
        &pool,
        &new_smoke_id,
        "smoke_job_new",
        "run_new",
        "obj_new",
        json!({"smoke":"true"}),
    )
    .await
    .unwrap();

    let token = make_test_jwt(&["system.ops_admin"]);
    // older_than_hours=1, confirm=true, dry_run=false
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/execution-readiness/cleanup-smoke?older_than_hours=1&dry_run=false&confirm=true")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify old one is deleted, new one remains
    let old_exists = proposal_exists_by_id(&pool, &old_smoke_id).await.unwrap();
    let new_exists = proposal_exists_by_id(&pool, &new_smoke_id).await.unwrap();

    assert!(!old_exists, "Old smoke proposal should be deleted");
    assert!(new_exists, "New smoke proposal should NOT be deleted");

    // Clean up new one
    cleanup_proposal_by_id(&pool, &new_smoke_id).await.unwrap();
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn cleanup_smoke_leaves_non_smoke_ai_action_todo_with_new_source_id() {
    if !has_test_db() {
        return;
    }
    let pool = create_test_pool().await;
    let readiness = readiness_for(&pool);
    let metrics = metrics_for_pool(&pool);
    let rollout = rollout_for(readiness.clone(), metrics.clone(), pool.clone());
    let app = test::init_service(build_test_app_full(readiness, metrics, rollout)).await;

    let _guard1 = EnvGuard::set("FMS_AI_SMOKE_CLEANUP_ENABLED", "true");

    let non_smoke_id = format!("non_smoke_{}", Utc::now().timestamp_micros());
    let two_hours_ago = Utc::now() - chrono::Duration::hours(2);

    // Insert non-smoke proposal
    insert_smoke_proposal_with_created_at(
        &pool,
        &non_smoke_id,
        "non_smoke_job",
        "run_non",
        "obj_non",
        json!({"smoke":"false"}),
        two_hours_ago,
    )
    .await
    .unwrap();

    let token = make_test_jwt(&["system.ops_admin"]);
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/execution-readiness/cleanup-smoke?older_than_hours=1&dry_run=false&confirm=true")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify non-smoke still exists
    let exists = proposal_exists_by_id(&pool, &non_smoke_id).await.unwrap();
    assert!(exists, "Non-smoke proposal should remain untouched");

    // Clean up
    cleanup_proposal_by_id(&pool, &non_smoke_id).await.unwrap();
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn cleanup_smoke_requires_enable_flag() {
    if !has_test_db() {
        return;
    }
    let pool = create_test_pool().await;
    let readiness = readiness_for(&pool);
    let metrics = metrics_for_pool(&pool);
    let rollout = rollout_for(readiness.clone(), metrics.clone(), pool.clone());
    let app = test::init_service(build_test_app_full(readiness, metrics, rollout)).await;

    let _guard1 = EnvGuard::set("FMS_AI_SMOKE_CLEANUP_ENABLED", "false");

    let token = make_test_jwt(&["system.ops_admin"]);
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/execution-readiness/cleanup-smoke?older_than_hours=1&dry_run=true")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
