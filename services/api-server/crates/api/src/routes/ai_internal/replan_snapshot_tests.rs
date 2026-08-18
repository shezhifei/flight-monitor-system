//! Contract tests for the internal replan snapshot endpoint (Task I1).
//!
//! Frozen contract under test:
//! - no/bad Service Identity                     → 401
//! - token path mismatch                         → 403
//! - window_end <= window_start                  → 400
//! - unknown strategy                            → 422 (ValidationError, same as public face)
//! - run_id not found                            → 404 `AI_RUN_NOT_FOUND`
//! - requester lacks `dispatch:read`             → 403 `TOOL_ACTOR_PERMISSION_DENIED`

use crate::middleware::jwt::JwtSecret;
use crate::middleware::service_identity::ServiceIdentityClaims;
use actix_web::{test, web, App};
use fms_application::services::ai_job_service::AiJobService;
use fms_application::services::dispatch_frontend_replan_service::DispatchFrontendReplanService;
use fms_domain::models::tool_authorization::ToolAuthorizationContext;
use fms_domain::models::tool_governance::RustToolGovernanceResolver;
use fms_domain::ports::ai_auth_context_loader::{AuthContextLoaderError, RunAuthorizationContextLoader};
use serde_json::{json, Value};
use std::sync::Arc;

const SNAPSHOT_PATH: &str = "/internal/ai/v1/dispatch/replan-snapshot";
/// Token for a different internal path, used to prove path-scoping.
const OTHER_PATH: &str = "/internal/ai/v1/ontology/actions/read";

fn create_service_identity_token(secret: &str, path: &str) -> String {
    use jsonwebtoken::{encode, EncodingKey, Header};
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as usize;

    let claims = ServiceIdentityClaims {
        iss: "fms-rust-api".to_string(),
        sub: "rust-api-gateway".to_string(),
        aud: "python-ai-runtime".to_string(),
        iat: now,
        exp: now + 60,
        path: path.to_string(),
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap()
}

/// A pool that is never actually connected to; sufficient for tests where the
/// validation short-circuits before any query runs.
fn lazy_pool() -> sqlx::PgPool {
    sqlx::pool::PoolOptions::<sqlx::Postgres>::new()
        .connect_lazy("postgres://unused:unused@localhost/unused")
        .expect("lazy pool")
}

fn replan_svc_for_pool(pool: &sqlx::PgPool) -> Arc<DispatchFrontendReplanService> {
    use fms_infrastructure::repositories::pg_dispatch_order_member_repository::PgDispatchOrderMemberRepository;
    use fms_infrastructure::repositories::pg_dispatch_order_repository::PgDispatchOrderRepository;

    Arc::new(DispatchFrontendReplanService::new(
        Arc::new(PgDispatchOrderRepository::new(pool.clone())),
        Arc::new(PgDispatchOrderMemberRepository::new(pool.clone())),
    ))
}

fn ai_job_service_for_pool(pool: &sqlx::PgPool) -> Arc<AiJobService> {
    use fms_infrastructure::repositories::pg_ai_job_repository::PgAiJobRepository;
    use fms_infrastructure::repositories::pg_ai_run_event_repository::PgAiRunEventRepository;
    use fms_infrastructure::repositories::pg_ai_run_repository::PgAiRunRepository;
    Arc::new(AiJobService::new(
        Arc::new(PgAiJobRepository::new(pool.clone())),
        Arc::new(PgAiRunRepository::new(pool.clone())),
        Arc::new(PgAiRunEventRepository::new(pool.clone())),
    ))
}

fn pg_auth_loader(pool: &sqlx::PgPool) -> Arc<dyn RunAuthorizationContextLoader + Send + Sync> {
    use fms_infrastructure::repositories::pg_ai_auth_context_loader::PgRunAuthorizationContextLoader;
    use fms_infrastructure::repositories::pg_ai_entity_config_repository::PgAiEntityConfigRepository;
    Arc::new(PgRunAuthorizationContextLoader::new(
        pool.clone(),
        Arc::new(PgAiEntityConfigRepository::new(pool.clone())),
    ))
}

/// Stub loader for tests that never reach the authorization step.
struct StubAuthLoader {
    permissions: Vec<String>,
}

#[async_trait::async_trait]
impl RunAuthorizationContextLoader for StubAuthLoader {
    async fn load_context(
        &self,
        _run_id: &str,
        _job_id: &str,
        tool_call_pk: &str,
        tool_name: &str,
        tool_args: &Value,
    ) -> Result<ToolAuthorizationContext, AuthContextLoaderError> {
        Ok(ToolAuthorizationContext {
            requester_user_id: "stub-requester".to_string(),
            requester_user_roles: vec![],
            requester_permissions: self.permissions.clone(),
            requester_object_policies: vec![],
            entity_tool_allowlist: vec![],
            tool_governance: RustToolGovernanceResolver::resolve(tool_name),
            tool_call_pk: tool_call_pk.to_string(),
            tool_args: tool_args.clone(),
            feature_flags: std::collections::HashMap::new(),
        })
    }
}

fn valid_window_body(run_id: &str) -> Value {
    json!({
        "run_id": run_id,
        "window_start": "2026-08-18T00:00:00Z",
        "window_end": "2026-08-18T06:00:00Z",
    })
}

fn build_validation_app() -> actix_web::App<
    impl actix_web::dev::ServiceFactory<
        actix_web::dev::ServiceRequest,
        Config = (),
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
        InitError = (),
    >,
> {
    let pool = lazy_pool();
    let loader: Arc<dyn RunAuthorizationContextLoader + Send + Sync> = Arc::new(StubAuthLoader {
        permissions: vec!["*".to_string()],
    });
    App::new()
        .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
        .app_data(web::Data::new(ai_job_service_for_pool(&pool)))
        .app_data(web::Data::new(loader))
        .app_data(web::Data::new(replan_svc_for_pool(&pool)))
        .configure(super::configure)
}

// ─── Identity gate (no database required) ────────────────────────────────────

#[actix_web::test]
async fn replan_snapshot_without_service_identity_returns_401() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .configure(super::configure),
    )
    .await;

    let req = test::TestRequest::post()
        .uri(SNAPSHOT_PATH)
        .set_json(valid_window_body("run_1"))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn replan_snapshot_with_path_mismatched_token_returns_403() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .configure(super::configure),
    )
    .await;

    let token = create_service_identity_token("test-secret", OTHER_PATH);
    let req = test::TestRequest::post()
        .uri(SNAPSHOT_PATH)
        .insert_header(("X-Service-Identity", token))
        .set_json(valid_window_body("run_1"))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 403);
}

// ─── Input validation (short-circuits before storage) ────────────────────────

#[actix_web::test]
async fn replan_snapshot_rejects_inverted_window() {
    let app = test::init_service(build_validation_app()).await;
    let token = create_service_identity_token("test-secret", SNAPSHOT_PATH);

    let mut body = valid_window_body("run_1");
    body["window_start"] = json!("2026-08-18T06:00:00Z");
    body["window_end"] = json!("2026-08-18T00:00:00Z");
    let req = test::TestRequest::post()
        .uri(SNAPSHOT_PATH)
        .insert_header(("X-Service-Identity", token))
        .set_json(body)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn replan_snapshot_rejects_unknown_strategy() {
    let app = test::init_service(build_validation_app()).await;
    let token = create_service_identity_token("test-secret", SNAPSHOT_PATH);

    let mut body = valid_window_body("run_1");
    body["strategy"] = json!("chaos");
    let req = test::TestRequest::post()
        .uri(SNAPSHOT_PATH)
        .insert_header(("X-Service-Identity", token))
        .set_json(body)
        .to_request();
    let resp = test::call_service(&app, req).await;

    // ValidationError maps to 422, matching the public replan-snapshot face.
    assert_eq!(resp.status(), 422);
}

#[actix_web::test]
async fn replan_snapshot_requires_window_bounds() {
    let app = test::init_service(build_validation_app()).await;
    let token = create_service_identity_token("test-secret", SNAPSHOT_PATH);

    let req = test::TestRequest::post()
        .uri(SNAPSHOT_PATH)
        .insert_header(("X-Service-Identity", token))
        .set_json(json!({"run_id": "run_1"}))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 400);
}

// ─── Run resolution and permission enforcement (real database) ───────────────

fn has_pool() -> bool {
    std::env::var("TEST_DATABASE_URL").is_ok()
}

async fn create_pool() -> sqlx::PgPool {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    sqlx::PgPool::connect(&url).await.expect("test db")
}

async fn create_job_and_run(job_service: &Arc<AiJobService>, requester: &str) -> (String, String) {
    let job = job_service
        .create_job("test", Some(requester), None, None, None)
        .await
        .expect("create job");
    let run = job_service
        .create_run(&job.job_id, "test", None, None)
        .await
        .expect("create run");
    (job.job_id, run.run_id)
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn replan_snapshot_unknown_run_returns_404() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let pool = create_pool().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(ai_job_service_for_pool(&pool)))
            .app_data(web::Data::new(pg_auth_loader(&pool)))
            .app_data(web::Data::new(replan_svc_for_pool(&pool)))
            .configure(super::configure),
    )
    .await;

    let token = create_service_identity_token("test-secret", SNAPSHOT_PATH);
    let req = test::TestRequest::post()
        .uri(SNAPSHOT_PATH)
        .insert_header(("X-Service-Identity", token))
        .set_json(valid_window_body("run_missing"))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 404);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], false);
    assert_eq!(body["error_code"], "AI_RUN_NOT_FOUND");
    assert_eq!(body["run_id"], "run_missing");
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn replan_snapshot_requester_without_permission_returns_403() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let pool = create_pool().await;
    let job_service = ai_job_service_for_pool(&pool);
    // Requester with no roles/permissions: recomputed grant set is empty.
    let requester = format!("solver_deny_{}", std::process::id());
    let (_job_id, run_id) = create_job_and_run(&job_service, &requester).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_service))
            .app_data(web::Data::new(pg_auth_loader(&pool)))
            .app_data(web::Data::new(replan_svc_for_pool(&pool)))
            .configure(super::configure),
    )
    .await;

    let token = create_service_identity_token("test-secret", SNAPSHOT_PATH);
    let req = test::TestRequest::post()
        .uri(SNAPSHOT_PATH)
        .insert_header(("X-Service-Identity", token))
        .set_json(valid_window_body(&run_id))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 403);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], false);
    assert_eq!(body["error_code"], "TOOL_ACTOR_PERMISSION_DENIED");
    assert_eq!(body["required_permission"], "dispatch:read");
}
