use super::*;
use crate::middleware::jwt::JwtSecret;
use crate::test_support::{
    ai_run_event_types, cleanup_outbox_by_aggregate_id, cleanup_todo_by_id, ensure_test_user,
    insert_idempotent_conflict_proposal, outbox_count_by_aggregate_id, outbox_count_by_event_type,
    proposal_status_by_id, todo_count_by_source, todo_exists_by_source, todo_exists_by_source_id,
    todo_title_by_source_id,
};
use actix_web::{http::StatusCode, test, App};
use fms_application::services::ai_action_proposal_service::{
    AiActionProposalService, GenerateProposalRequest, ValidateProposalRequest,
};
use fms_application::services::business_case_service::{BusinessCaseMentionAudience, CollaborationMentionAudience};
use fms_domain::models::ai_proposal::{ActionProposalStatus, ApprovalPolicy, RiskLevel};
use fms_domain::ports::ai_object_policy_repository::{
    AiObjectAccessDecision, AiObjectAccessRequest, AiObjectPolicyRepository, AiObjectPolicyRepositoryError,
};
use fms_domain::ports::flight_repository::FlightRepository;
use fms_infrastructure::repositories::pg_domain_event_outbox_repository::PgDomainEventOutboxRepository;
use fms_infrastructure::repositories::pg_flight_repository::PgFlightRepository;
use serde_json::json;
use std::sync::{Arc, Mutex};

struct SequenceObjectPolicyRepository {
    pub(crate) decisions: Mutex<Vec<AiObjectAccessDecision>>,
}

impl SequenceObjectPolicyRepository {
    fn new(decisions: Vec<AiObjectAccessDecision>) -> Self {
        Self {
            decisions: Mutex::new(decisions),
        }
    }
}

#[async_trait::async_trait]
impl AiObjectPolicyRepository for SequenceObjectPolicyRepository {
    async fn evaluate_access(
        &self,
        _request: &AiObjectAccessRequest,
    ) -> Result<AiObjectAccessDecision, AiObjectPolicyRepositoryError> {
        let mut decisions = self.decisions.lock().unwrap();
        if decisions.is_empty() {
            Ok(AiObjectAccessDecision::NoPolicy)
        } else {
            Ok(decisions.remove(0))
        }
    }
}

struct EnvGuard {
    pub(crate) key: &'static str,
    pub(crate) previous: Option<String>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self { key, previous }
    }

    fn remove(key: &'static str) -> Self {
        let previous = std::env::var(key).ok();
        std::env::remove_var(key);
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        if let Some(value) = &self.previous {
            std::env::set_var(self.key, value);
        } else {
            std::env::remove_var(self.key);
        }
    }
}

// 多个 DB smoke 测试会并行读写 FMS_AI_PROPOSAL_EXECUTION_ENABLED 等进程级环境变量，
// 互相覆盖会导致 flaky，用静态锁串行化环境变量相关段落。
static SMOKE_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn make_jwt(permissions: &[&str], department_id: Option<&str>) -> String {
    use chrono::Utc;
    use jsonwebtoken::{encode, EncodingKey, Header};
    let now = Utc::now().timestamp();
    let claims = json!({
        "sub": "test_user",
        "username": "tester",
        "permissions": permissions,
        "department_id": department_id,
        "is_admin": false,
        "iat": now,
        "exp": now + 3600,
        "type": "access",
    });
    encode(&Header::default(), &claims, &EncodingKey::from_secret(b"test-secret")).expect("jwt encoding")
}

async fn create_test_proposal(service: &AiActionProposalService, action_name: &str) -> String {
    let req = GenerateProposalRequest {
        job_id: "test_job".to_string(),
        run_id: "test_run".to_string(),
        ontology_version: Some("flight-ops.v1".to_string()),
        object_type: "Flight".to_string(),
        object_id: "FL123".to_string(),
        action_name: action_name.to_string(),
        arguments: json!({"new_stand": "S02"}),
        reasoning: Some("test reason".to_string()),
        confidence: Some(0.9),
        requester_user_id: Some("generator".to_string()),
        requester_user_roles: vec!["flight:write".to_string()],
        requester_department_id: None,
        correlation_id: None,
        idempotency_key: None,
        expected_object_version: None,
        risk_level: None,
        approval_policy: None,
        required_permissions: None,
    };
    let proposal = service
        .generate_proposal(req)
        .await
        .expect("failed to generate proposal");

    proposal.proposal_id
}

async fn validate_test_proposal(service: &AiActionProposalService, id: &str) {
    let req = ValidateProposalRequest {
        proposal_id: id.to_string(),
        before_snapshot: Some(json!({"stand": "S01"})),
        after_preview: Some(json!({"stand": "S02"})),
        constraint_results: None,
    };
    service
        .validate_proposal(req)
        .await
        .expect("failed to validate proposal");
}

#[actix_web::test]
async fn test_approve_proposal_forbidden_without_jwt() {
    let service = Arc::new(AiActionProposalService::new());
    let proposal_id = create_test_proposal(&service, "change_stand").await;
    validate_test_proposal(&service, &proposal_id).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(service))
            .configure(configure),
    )
    .await;

    let req = test::TestRequest::post()
        .uri(&format!("/api/v2/ai/proposals/{proposal_id}/approve"))
        .set_json(json!({}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn list_proposals_requires_authentication() {
    let service = Arc::new(AiActionProposalService::new());

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(service))
            .configure(configure),
    )
    .await;

    let req = test::TestRequest::get().uri("/api/v2/ai/proposals").to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn list_proposals_requires_ai_view_permission() {
    let service = Arc::new(AiActionProposalService::new());

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(service))
            .configure(configure),
    )
    .await;

    let token = make_jwt(&["ai:execute"], None);
    let req = test::TestRequest::get()
        .uri("/api/v2/ai/proposals")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn expire_stale_proposals_requires_ai_execute_permission() {
    let service = Arc::new(AiActionProposalService::new());

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(service))
            .configure(configure),
    )
    .await;

    let token = make_jwt(&["ai:view"], None);
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/proposals/expire-stale")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn test_approve_proposal_forbidden_without_required_action_permission() {
    let service = Arc::new(AiActionProposalService::new());
    let proposal_id = create_test_proposal(&service, "change_stand").await;
    validate_test_proposal(&service, &proposal_id).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(service))
            .configure(configure),
    )
    .await;

    let token = make_jwt(&["flight:read"], None);
    let req = test::TestRequest::post()
        .uri(&format!("/api/v2/ai/proposals/{proposal_id}/approve"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn test_approve_proposal_forbidden_when_object_policy_denies() {
    let policy_repo = Arc::new(SequenceObjectPolicyRepository::new(vec![
        AiObjectAccessDecision::Allow, // allow for generation
        AiObjectAccessDecision::Deny,  // deny for approval
    ]));
    let service = Arc::new(AiActionProposalService::new().with_object_policy_repository(policy_repo));
    let proposal_id = create_test_proposal(&service, "change_stand").await;
    validate_test_proposal(&service, &proposal_id).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(service))
            .configure(configure),
    )
    .await;

    let token = make_jwt(&["flight:write"], None);
    let req = test::TestRequest::post()
        .uri(&format!("/api/v2/ai/proposals/{proposal_id}/approve"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn test_execute_proposal_forbidden_without_required_action_permission() {
    let service = Arc::new(AiActionProposalService::new());
    let proposal_id = create_test_proposal(&service, "change_stand").await;
    validate_test_proposal(&service, &proposal_id).await;

    let approver_token = make_jwt(&["flight:write"], None);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(service.clone()))
            .configure(configure),
    )
    .await;

    let approve_req = test::TestRequest::post()
        .uri(&format!("/api/v2/ai/proposals/{proposal_id}/approve"))
        .insert_header(("Authorization", format!("Bearer {approver_token}")))
        .set_json(json!({}))
        .to_request();
    let approve_resp = test::call_service(&app, approve_req).await;
    assert_eq!(approve_resp.status(), StatusCode::OK);

    let executor_token = make_jwt(&["flight:read"], None);
    let exec_req = test::TestRequest::post()
        .uri(&format!("/api/v2/ai/proposals/{proposal_id}/execute"))
        .insert_header(("Authorization", format!("Bearer {executor_token}")))
        .to_request();

    let resp = test::call_service(&app, exec_req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn test_execute_proposal_forbidden_when_object_policy_denies() {
    let policy_repo = Arc::new(SequenceObjectPolicyRepository::new(vec![
        AiObjectAccessDecision::Allow, // allow for generation
        AiObjectAccessDecision::Allow, // allow for approval
        AiObjectAccessDecision::Deny,  // deny for execution
    ]));
    let service = Arc::new(AiActionProposalService::new().with_object_policy_repository(policy_repo));
    let proposal_id = create_test_proposal(&service, "change_stand").await;
    validate_test_proposal(&service, &proposal_id).await;

    let approver_token = make_jwt(&["flight:write"], None);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(service.clone()))
            .configure(configure),
    )
    .await;

    let approve_req = test::TestRequest::post()
        .uri(&format!("/api/v2/ai/proposals/{proposal_id}/approve"))
        .insert_header(("Authorization", format!("Bearer {approver_token}")))
        .set_json(json!({}))
        .to_request();
    let approve_resp = test::call_service(&app, approve_req).await;
    assert_eq!(approve_resp.status(), StatusCode::OK);

    let executor_token = make_jwt(&["flight:write"], None);
    let exec_req = test::TestRequest::post()
        .uri(&format!("/api/v2/ai/proposals/{proposal_id}/execute"))
        .insert_header(("Authorization", format!("Bearer {executor_token}")))
        .to_request();

    let resp = test::call_service(&app, exec_req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn test_execute_proposal_respects_execution_feature_flag() {
    let _guard = EnvGuard::set("FMS_AI_PROPOSAL_EXECUTION_ENABLED", "0");

    let service = Arc::new(AiActionProposalService::new());
    let proposal_id = create_test_proposal(&service, "change_stand").await;
    validate_test_proposal(&service, &proposal_id).await;

    let token = make_jwt(&["flight:write"], None);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(service.clone()))
            .configure(configure),
    )
    .await;

    let approve_req = test::TestRequest::post()
        .uri(&format!("/api/v2/ai/proposals/{proposal_id}/approve"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({}))
        .to_request();
    let approve_resp = test::call_service(&app, approve_req).await;
    assert_eq!(approve_resp.status(), StatusCode::OK);

    let exec_req = test::TestRequest::post()
        .uri(&format!("/api/v2/ai/proposals/{proposal_id}/execute"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, exec_req).await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

fn has_pool() -> bool {
    std::env::var("TEST_DATABASE_URL").is_ok()
}

async fn create_pool() -> sqlx::PgPool {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    sqlx::PgPool::connect(&url).await.expect("test db")
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_execute_proposal_expected_version_mismatch() {
    if !has_pool() {
        return;
    }
    let pool = create_pool().await;
    let service = Arc::new(
        AiActionProposalService::new()
            .with_pool(pool.clone())
            .with_flight_repository(Arc::new(PgFlightRepository::new(pool.clone()))),
    );

    let req = GenerateProposalRequest {
        job_id: "test_job".to_string(),
        run_id: "test_run".to_string(),
        ontology_version: Some("flight-ops.v1".to_string()),
        object_type: "Flight".to_string(),
        object_id: "FL_NONEXISTENT_OR_LOW".to_string(),
        action_name: "change_stand".to_string(),
        arguments: json!({"new_stand": "S02"}),
        reasoning: Some("test reason".to_string()),
        confidence: Some(0.9),
        requester_user_id: Some("generator".to_string()),
        requester_user_roles: vec!["flight:write".to_string()],
        requester_department_id: None,
        correlation_id: None,
        idempotency_key: None,
        expected_object_version: Some(9999),
        risk_level: None,
        approval_policy: None,
        required_permissions: None,
    };
    let proposal = service
        .generate_proposal(req)
        .await
        .expect("failed to generate proposal");

    validate_test_proposal(&service, &proposal.proposal_id).await;

    let approver_token = make_jwt(&["flight:write"], None);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(service.clone()))
            .configure(configure),
    )
    .await;

    let approve_req = test::TestRequest::post()
        .uri(&format!("/api/v2/ai/proposals/{}/approve", proposal.proposal_id))
        .insert_header(("Authorization", format!("Bearer {approver_token}")))
        .set_json(json!({}))
        .to_request();
    let approve_resp = test::call_service(&app, approve_req).await;
    assert_eq!(approve_resp.status(), StatusCode::OK);

    let exec_req = test::TestRequest::post()
        .uri(&format!("/api/v2/ai/proposals/{}/execute", proposal.proposal_id))
        .insert_header(("Authorization", format!("Bearer {approver_token}")))
        .to_request();

    let resp = test::call_service(&app, exec_req).await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_execute_proposal_idempotency_conflict() {
    if !has_pool() {
        return;
    }
    let pool = create_pool().await;
    let service = Arc::new(AiActionProposalService::new().with_pool(pool.clone()));

    let idemp_key = format!("idemp_{}", ulid::Ulid::new());
    let prop_id = format!("prop_{}", ulid::Ulid::new());
    insert_idempotent_conflict_proposal(&pool, &prop_id, json!({"idempotency_key": idemp_key}))
        .await
        .unwrap();

    let req = GenerateProposalRequest {
        job_id: "test_job".to_string(),
        run_id: "test_run".to_string(),
        ontology_version: Some("flight-ops.v1".to_string()),
        object_type: "Flight".to_string(),
        object_id: "FL123".to_string(),
        action_name: "change_stand".to_string(),
        arguments: json!({"new_stand": "S02"}),
        reasoning: Some("test reason".to_string()),
        confidence: Some(0.9),
        requester_user_id: Some("generator".to_string()),
        requester_user_roles: vec!["flight:write".to_string()],
        requester_department_id: None,
        correlation_id: None,
        idempotency_key: Some(idemp_key),
        expected_object_version: None,
        risk_level: None,
        approval_policy: None,
        required_permissions: None,
    };
    let proposal = service
        .generate_proposal(req)
        .await
        .expect("failed to generate proposal");

    validate_test_proposal(&service, &proposal.proposal_id).await;

    let approver_token = make_jwt(&["flight:write"], None);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(service.clone()))
            .configure(configure),
    )
    .await;

    let approve_req = test::TestRequest::post()
        .uri(&format!("/api/v2/ai/proposals/{}/approve", proposal.proposal_id))
        .insert_header(("Authorization", format!("Bearer {approver_token}")))
        .set_json(json!({}))
        .to_request();
    let approve_resp = test::call_service(&app, approve_req).await;
    assert_eq!(approve_resp.status(), StatusCode::OK);

    let exec_req = test::TestRequest::post()
        .uri(&format!("/api/v2/ai/proposals/{}/execute", proposal.proposal_id))
        .insert_header(("Authorization", format!("Bearer {approver_token}")))
        .to_request();

    let resp = test::call_service(&app, exec_req).await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

async fn build_test_executor(
    pool: sqlx::PgPool,
) -> fms_application::services::domain_action_executor::DomainActionExecutor {
    use crate::types::{
        NoopBroadcaster, NoopBusinessCaseEventPublisher, NoopNotificationDeliveryPublisher,
        NoopNotificationMetricsRecorder, NoopNotificationReceiptGroupSync,
    };
    use fms_application::services::{
        business_case_service::{BusinessCaseEventPublisher, BusinessCaseService, BusinessCaseWriter},
        dispatch_service::DispatchService,
        flight_service::FlightService,
        label_service::LabelService,
        notification_service::{
            CollaborationEventRecorder, NotificationCollaborationEvents, NotificationDeliveryPublisher,
            NotificationMetricsRecorder, NotificationReceiptGroupSync, NotificationService,
        },
        todo_service::TodoWriter,
    };
    use fms_infrastructure::repositories::{
        pg_anomaly_repository::PgAnomalyRepository, pg_business_case_repository::PgBusinessCaseRepository,
        pg_dispatch_collaboration_repository::PgDispatchCollaborationRepository,
        pg_dispatch_order_repository::PgDispatchOrderRepository, pg_flight_repository::PgFlightRepository,
        pg_label_repository::PgLabelRepository, pg_notification_repository::PgNotificationRepository,
        pg_todo_repository::PgTodoRepository,
    };

    let flight_repo = Arc::new(PgFlightRepository::new(pool.clone()));
    let outbox_repo = Arc::new(PgDomainEventOutboxRepository::new(pool.clone()));
    let flight_service = Arc::new(
        FlightService::new(flight_repo.clone())
            .with_transactional_repository(flight_repo)
            .with_outbox_repository(outbox_repo.clone()),
    );

    let dispatch_order_repo = Arc::new(PgDispatchOrderRepository::new(pool.clone()));
    // 本测试只接 order_repo 与其事务变体；其余端口是桩（与接线前的 None 行为一致）。
    let mut dispatch_deps = fms_application::test_support::stub_dispatch_dependencies();
    dispatch_deps.order.order_repo = dispatch_order_repo.clone();
    dispatch_deps.order.order_tx_repo = dispatch_order_repo;
    let dispatch_service = Arc::new(DispatchService::new(dispatch_deps));

    let notification_repo = Arc::new(PgNotificationRepository::new(pool.clone()));
    let collaboration_repo = Arc::new(PgDispatchCollaborationRepository::new(pool.clone()));
    let notification_repo_port: Arc<
        dyn fms_domain::ports::notification_repository::NotificationRepository + Send + Sync,
    > = notification_repo.clone();
    let notification_pref_repo_port: Arc<
        dyn fms_domain::ports::notification_repository::NotificationPreferenceRepository + Send + Sync,
    > = notification_repo.clone();
    let notification_collaboration_repo_port: Arc<
        dyn fms_domain::ports::dispatch_collaboration_repository::DispatchCollaborationRepository + Send + Sync,
    > = collaboration_repo.clone();
    let notification_tx_repo_port: Arc<
        dyn fms_application::sqlx_transactional_repositories::SqlxNotificationTransactionalRepository,
    > = notification_repo.clone();
    let notification_service = Arc::new(NotificationService::new(
        notification_repo_port,
        notification_pref_repo_port,
        Arc::new(CollaborationEventRecorder::new(notification_collaboration_repo_port))
            as Arc<dyn NotificationCollaborationEvents>,
        Arc::new(NoopNotificationDeliveryPublisher) as Arc<dyn NotificationDeliveryPublisher>,
        Arc::new(NoopNotificationMetricsRecorder) as Arc<dyn NotificationMetricsRecorder>,
        Arc::new(NoopNotificationReceiptGroupSync) as Arc<dyn NotificationReceiptGroupSync>,
    ));

    let anomaly_repo = Arc::new(PgAnomalyRepository::new(pool.clone()));

    let label_repo = Arc::new(PgLabelRepository::new(pool.clone()));
    let label_service = Arc::new(LabelService::new(label_repo, Arc::new(NoopBroadcaster)));

    let todo_repo = Arc::new(PgTodoRepository::new(pool.clone()));
    let todo_writer: Arc<TodoWriter<sqlx::Transaction<'static, sqlx::Postgres>>> =
        Arc::new(TodoWriter::new(todo_repo.clone(), todo_repo));

    let business_case_pg_repo = Arc::new(PgBusinessCaseRepository::new(pool.clone()));
    let business_case_repo: Arc<dyn fms_domain::ports::business_case_repository::BusinessCaseRepository + Send + Sync> =
        business_case_pg_repo.clone();
    let business_case_writer: Arc<BusinessCaseWriter<sqlx::Transaction<'static, sqlx::Postgres>>> = Arc::new(
        BusinessCaseWriter::new(business_case_pg_repo.clone(), business_case_pg_repo),
    );
    let business_case_collaboration_repo: Arc<
        dyn fms_domain::ports::dispatch_collaboration_repository::DispatchCollaborationRepository + Send + Sync,
    > = collaboration_repo;
    let business_case_service = Arc::new(BusinessCaseService::new(
        business_case_repo,
        Arc::new(NoopBusinessCaseEventPublisher) as Arc<dyn BusinessCaseEventPublisher>,
        Arc::new(CollaborationMentionAudience::new(business_case_collaboration_repo))
            as Arc<dyn BusinessCaseMentionAudience>,
    ));

    fms_application::services::domain_action_executor::DomainActionExecutor::new(
        flight_service,
        dispatch_service,
        notification_service,
        label_service,
        todo_writer,
        business_case_service,
        business_case_writer,
        outbox_repo,
        anomaly_repo,
        notification_tx_repo_port,
        pool,
    )
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_execute_proposal_success() {
    if !has_pool() {
        return;
    }
    let pool = create_pool().await;

    // Clean up previous test runs if any
    let _ = cleanup_todo_by_id(&pool, "TD_API_SUCCESS").await;
    let _ = cleanup_outbox_by_aggregate_id(&pool, "TD_API_SUCCESS").await;
    // 不删除共享 tester 用户：并行 smoke 测试都依赖它，ensure_test_user 幂等即可。

    // Insert tester user to satisfy foreign keys
    ensure_test_user(&pool).await.unwrap();

    let executor = Arc::new(build_test_executor(pool.clone()).await);
    let service = Arc::new(
        AiActionProposalService::new()
            .with_pool(pool.clone())
            .with_domain_action_executor(executor),
    );

    // Generate proposal
    let req = GenerateProposalRequest {
        job_id: "test_job".to_string(),
        run_id: "test_run".to_string(),
        ontology_version: Some("flight-ops.v1".to_string()),
        object_type: "Todo".to_string(),
        object_id: "TD_API_SUCCESS".to_string(),
        action_name: "create".to_string(),
        arguments: json!({"title": "Test Todo from API"}),
        reasoning: Some("test reason".to_string()),
        confidence: Some(0.9),
        requester_user_id: Some("test_user".to_string()),
        requester_user_roles: vec!["todo:write".to_string()],
        requester_department_id: None,
        correlation_id: None,
        idempotency_key: None,
        expected_object_version: None,
        risk_level: None,
        approval_policy: None,
        required_permissions: None,
    };
    let proposal = service
        .generate_proposal(req)
        .await
        .expect("failed to generate proposal");

    let val_req = ValidateProposalRequest {
        proposal_id: proposal.proposal_id.clone(),
        before_snapshot: Some(json!({})),
        after_preview: Some(json!({"title": "Test Todo from API"})),
        constraint_results: None,
    };
    let validated = service.validate_proposal(val_req).await.expect("validate proposal");

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(service.clone()))
            .configure(configure),
    )
    .await;

    let approver_token = make_jwt(&["*"], None);

    // Approve proposal（Todo.create 低风险经 schema 驱动策略在 validate 后自动批准，
    // 此时重复 approve 会 409，仅在 Pending 时显式审批）
    if validated.status == ActionProposalStatus::Pending {
        let approve_req = test::TestRequest::post()
            .uri(&format!("/api/v2/ai/proposals/{}/approve", proposal.proposal_id))
            .insert_header(("Authorization", format!("Bearer {approver_token}")))
            .set_json(json!({}))
            .to_request();
        let approve_resp = test::call_service(&app, approve_req).await;
        assert_eq!(approve_resp.status(), StatusCode::OK);
    }

    // Enable proposal execution via environment flag and execute
    let _env_guard = SMOKE_ENV_LOCK.lock().unwrap();
    let _guard = EnvGuard::set("FMS_AI_PROPOSAL_EXECUTION_ENABLED", "true");
    let executor_token = make_jwt(&["*"], None);
    let exec_req = test::TestRequest::post()
        .uri(&format!("/api/v2/ai/proposals/{}/execute", proposal.proposal_id))
        .insert_header(("Authorization", format!("Bearer {executor_token}")))
        .to_request();

    let resp = test::call_service(&app, exec_req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify the Todo item is created in the database
    let todo_exists = todo_exists_by_source_id(&pool, "TD_API_SUCCESS").await.unwrap_or(false);
    assert!(todo_exists, "Todo item TD_API_SUCCESS must be created in database");

    let todo_title = todo_title_by_source_id(&pool, "TD_API_SUCCESS").await.unwrap();
    assert_eq!(todo_title, "Test Todo from API");

    // Verify the event is recorded in the domain outbox
    let outbox_count = outbox_count_by_aggregate_id(&pool, "TD_API_SUCCESS").await.unwrap();
    assert!(outbox_count > 0, "Outbox event must be created");
}

// ── API DB Smoke helpers ──────────────────────────────────────────────

fn smoke_todo_create_proposal(
    proposal_id: &str,
    object_id: &str,
    correlation_id: &str,
) -> fms_domain::models::ai_proposal::AiActionProposal {
    use fms_domain::models::ai_proposal::AiActionProposal;

    AiActionProposal {
        proposal_id: proposal_id.to_string(),
        job_id: format!("api_smoke_job_{proposal_id}"),
        run_id: format!("api_smoke_run_{proposal_id}"),
        ontology_version: "flight-ops.v1".to_string(),
        object_type: "Todo".to_string(),
        object_id: object_id.to_string(),
        action_name: "create".to_string(),
        arguments: json!({ "title": format!("API smoke todo {correlation_id}") }),
        risk_level: RiskLevel::Low,
        required_permissions: vec!["todo:write".to_string()],
        approval_policy: ApprovalPolicy::AutoExecute,
        before_snapshot: None,
        after_preview: None,
        constraint_results: vec![],
        confidence: 0.95,
        reasoning: "API smoke Todo.create".to_string(),
        status: ActionProposalStatus::Approved,
        pending_action_id: None,
        approved_by: Some("api_smoke_approver".to_string()),
        approved_at: Some(chrono::Utc::now()),
        rejected_by: None,
        rejected_reason: None,
        rejected_at: None,
        executed_by: None,
        executed_at: None,
        execution_result: None,
        execution_error: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        expires_at: Some(chrono::Utc::now() + chrono::Duration::hours(1)),
        correlation_id: Some(correlation_id.to_string()),
        metadata: json!({ "smoke": true, "api_smoke": true }),
    }
}

async fn build_smoke_proposal_service(
    pool: sqlx::PgPool,
) -> (
    Arc<AiActionProposalService>,
    Arc<fms_application::services::ai_execution_readiness_service::AiExecutionReadinessService>,
) {
    use fms_application::services::ai_execution_readiness_service::AiExecutionReadinessService;
    use fms_application::services::ai_proposal_audit_recorder::PgAiProposalAuditEventRecorder;
    use fms_infrastructure::repositories::{
        pg_ai_object_policy_repository::PgAiObjectPolicyRepository, pg_ai_ontology_repository::PgAiOntologyRepository,
        pg_ai_proposal_repository::PgAiProposalRepository, pg_database_metadata_adapter::PgDatabaseMetadataAdapter,
    };

    let executor = Arc::new(build_test_executor(pool.clone()).await);
    let readiness = Arc::new(AiExecutionReadinessService::new(
        Some(Arc::new(PgDatabaseMetadataAdapter::new(pool.clone()))),
        None,
    ));
    let event_repo = Arc::new(
        fms_infrastructure::repositories::pg_ai_run_event_repository::PgAiRunEventRepository::new(pool.clone()),
    );
    let audit = Arc::new(PgAiProposalAuditEventRecorder::new(event_repo));

    let service = Arc::new(
        AiActionProposalService::new()
            .with_repository(Arc::new(PgAiProposalRepository::new(pool.clone())))
            .with_domain_action_executor(executor)
            .with_object_policy_repository(Arc::new(PgAiObjectPolicyRepository::new(pool.clone())))
            .with_ontology_repository(Arc::new(PgAiOntologyRepository::new(pool.clone())))
            .with_pool(pool)
            .with_readiness_service(readiness.clone())
            .with_audit_recorder(audit),
    );

    (service, readiness)
}

async fn build_smoke_proposal_service_with_readiness(
    pool: sqlx::PgPool,
    readiness: Arc<fms_application::services::ai_execution_readiness_service::AiExecutionReadinessService>,
) -> Arc<AiActionProposalService> {
    use fms_application::services::ai_proposal_audit_recorder::PgAiProposalAuditEventRecorder;
    use fms_infrastructure::repositories::{
        pg_ai_object_policy_repository::PgAiObjectPolicyRepository, pg_ai_ontology_repository::PgAiOntologyRepository,
        pg_ai_proposal_repository::PgAiProposalRepository,
    };

    let executor = Arc::new(build_test_executor(pool.clone()).await);
    let event_repo = Arc::new(
        fms_infrastructure::repositories::pg_ai_run_event_repository::PgAiRunEventRepository::new(pool.clone()),
    );
    let audit = Arc::new(PgAiProposalAuditEventRecorder::new(event_repo));

    Arc::new(
        AiActionProposalService::new()
            .with_repository(Arc::new(PgAiProposalRepository::new(pool.clone())))
            .with_domain_action_executor(executor)
            .with_object_policy_repository(Arc::new(PgAiObjectPolicyRepository::new(pool.clone())))
            .with_ontology_repository(Arc::new(PgAiOntologyRepository::new(pool.clone())))
            .with_pool(pool)
            .with_readiness_service(readiness)
            .with_audit_recorder(audit),
    )
}

async fn save_smoke_proposal(pool: &sqlx::PgPool, proposal: &fms_domain::models::ai_proposal::AiActionProposal) {
    use fms_domain::ports::ai_proposal_repository::AiProposalRepository;
    use fms_infrastructure::repositories::pg_ai_proposal_repository::PgAiProposalRepository;

    let repo = PgAiProposalRepository::new(pool.clone());
    repo.save(proposal).await.expect("save smoke proposal");
}

// ── API Proposal Smoke Tests ──────────────────────────────────────────

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; API proposal smoke — run via scripts/dev/run_aip_api_staging_smoke.ps1"]
async fn api_proposal_smoke_happy_path_todo_create_execute() {
    if !has_pool() {
        return;
    }
    let pool = create_pool().await;
    ensure_test_user(&pool).await.unwrap();

    let (service, _readiness) = build_smoke_proposal_service(pool.clone()).await;

    let correlation_id = format!("urn:ulid:{}", ulid::Ulid::new());
    let proposal_id = format!("api_smoke_{}", ulid::Ulid::new());
    let object_id = ulid::Ulid::new().to_string(); // ≤26 chars for outbox aggregate_id
    let proposal = smoke_todo_create_proposal(&proposal_id, &object_id, &correlation_id);
    save_smoke_proposal(&pool, &proposal).await;

    // Set execution env vars
    let _env_guard = SMOKE_ENV_LOCK.lock().unwrap();
    let _guard1 = EnvGuard::set("FMS_AI_PROPOSAL_EXECUTION_ENABLED", "true");
    let _guard2 = EnvGuard::set("FMS_AI_EXECUTION_READINESS_OVERRIDE", "staging");

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(service.clone()))
            .configure(configure),
    )
    .await;

    let token = make_jwt(&["todo:write", "system.config_read"], None);
    let exec_req = test::TestRequest::post()
        .uri(&format!("/api/v2/ai/proposals/{proposal_id}/execute"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, exec_req).await;
    let resp_status = resp.status();
    let resp_body = test::read_body(resp).await;
    let resp_text = String::from_utf8_lossy(&resp_body);
    assert_eq!(
        resp_status,
        StatusCode::OK,
        "API execute should return 200 for approved Todo.create, got body: {resp_text}"
    );

    // 1. Proposal status is Executed
    let reloaded = proposal_status_by_id(&pool, &proposal_id).await.unwrap();
    assert_eq!(
        reloaded,
        Some(6i16), // Executed = 6
        "proposal should be in Executed status"
    );

    // 2. Todo business row exists (source_id = object_id set by executor)
    let todo_exists = todo_exists_by_source(&pool, "ai_action", &object_id)
        .await
        .unwrap_or(false);
    assert!(todo_exists, "todo row should exist after API smoke execution");

    // 3. Domain event outbox has a Todo.create event (aggregate_id = object_id)
    let outbox_count = outbox_count_by_event_type(&pool, "Todo.create", &object_id)
        .await
        .unwrap();
    assert!(
        outbox_count >= 1,
        "outbox should contain Todo.create event for proposal"
    );

    // 4. Audit events recorded
    let event_types = ai_run_event_types(&pool, &proposal.job_id, &proposal.run_id)
        .await
        .unwrap();
    assert!(
        event_types.contains(&"proposal.execution_requested".to_string()),
        "should have execution_requested audit event"
    );
    assert!(
        event_types.contains(&"proposal.execution_started".to_string()),
        "should have execution_started audit event"
    );
    assert!(
        event_types.contains(&"proposal.execution_succeeded".to_string()),
        "should have execution_succeeded audit event"
    );
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; API proposal smoke — run via scripts/dev/run_aip_api_staging_smoke.ps1"]
async fn api_proposal_smoke_execution_disabled_returns_conflict() {
    if !has_pool() {
        return;
    }
    let pool = create_pool().await;
    ensure_test_user(&pool).await.unwrap();

    let (service, _readiness) = build_smoke_proposal_service(pool.clone()).await;

    let correlation_id = format!("urn:ulid:{}", ulid::Ulid::new());
    let proposal_id = format!("api_smoke_{}", ulid::Ulid::new());
    let object_id = ulid::Ulid::new().to_string();
    let proposal = smoke_todo_create_proposal(&proposal_id, &object_id, &correlation_id);
    save_smoke_proposal(&pool, &proposal).await;

    // Count before
    let todo_count_before = todo_count_by_source(&pool, "ai_action", &object_id).await.unwrap();
    let outbox_count_before = outbox_count_by_event_type(&pool, "Todo.create", &object_id)
        .await
        .unwrap();

    // Execution DISABLED
    let _env_guard = SMOKE_ENV_LOCK.lock().unwrap();
    let _guard1 = EnvGuard::set("FMS_AI_PROPOSAL_EXECUTION_ENABLED", "false");

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(service.clone()))
            .configure(configure),
    )
    .await;

    let token = make_jwt(&["todo:write"], None);
    let exec_req = test::TestRequest::post()
        .uri(&format!("/api/v2/ai/proposals/{proposal_id}/execute"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, exec_req).await;
    assert_eq!(
        resp.status(),
        StatusCode::CONFLICT,
        "API execute should return 409 when execution is disabled"
    );

    // Proposal remains Approved
    let reloaded = proposal_status_by_id(&pool, &proposal_id).await.unwrap();
    assert_eq!(
        reloaded,
        Some(3i16), // Approved = 3
        "proposal should remain Approved when execution is disabled"
    );

    // No side effects
    let todo_count_after = todo_count_by_source(&pool, "ai_action", &object_id).await.unwrap();
    let outbox_count_after = outbox_count_by_event_type(&pool, "Todo.create", &object_id)
        .await
        .unwrap();
    assert_eq!(
        todo_count_after, todo_count_before,
        "execution-disabled must not create todo rows"
    );
    assert_eq!(
        outbox_count_after, outbox_count_before,
        "execution-disabled must not create outbox events"
    );
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; API proposal smoke — run via scripts/dev/run_aip_api_staging_smoke.ps1"]
async fn api_proposal_smoke_permission_denied_returns_403() {
    if !has_pool() {
        return;
    }
    let pool = create_pool().await;
    ensure_test_user(&pool).await.unwrap();

    let (service, _readiness) = build_smoke_proposal_service(pool.clone()).await;

    let correlation_id = format!("urn:ulid:{}", ulid::Ulid::new());
    let proposal_id = format!("api_smoke_{}", ulid::Ulid::new());
    let object_id = ulid::Ulid::new().to_string();
    let proposal = smoke_todo_create_proposal(&proposal_id, &object_id, &correlation_id);
    save_smoke_proposal(&pool, &proposal).await;

    let _env_guard = SMOKE_ENV_LOCK.lock().unwrap();
    let _guard1 = EnvGuard::set("FMS_AI_PROPOSAL_EXECUTION_ENABLED", "true");
    let _guard2 = EnvGuard::set("FMS_AI_EXECUTION_READINESS_OVERRIDE", "staging");

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(service.clone()))
            .configure(configure),
    )
    .await;

    // Token missing todo:write → should be rejected
    let token = make_jwt(&["flight:read"], None);
    let exec_req = test::TestRequest::post()
        .uri(&format!("/api/v2/ai/proposals/{proposal_id}/execute"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, exec_req).await;
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "API execute should return 403 when user lacks todo:write"
    );

    // Proposal remains Approved, no side effects
    let reloaded = proposal_status_by_id(&pool, &proposal_id).await.unwrap();
    assert_eq!(
        reloaded,
        Some(3i16), // Approved = 3
        "proposal should remain Approved when permission denied"
    );

    let todo_exists = todo_exists_by_source(&pool, "ai_action", &object_id)
        .await
        .unwrap_or(false);
    assert!(!todo_exists, "no todo should be created when permission denied");
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; API proposal smoke — run via scripts/dev/run_aip_api_staging_smoke.ps1"]
async fn api_proposal_smoke_readiness_not_ready_blocks_execute() {
    if !has_pool() {
        return;
    }
    let pool = create_pool().await;
    ensure_test_user(&pool).await.unwrap();

    // Readiness with no pool → static checks only; with execution enabled
    // but no staging override, readiness will fail.
    let readiness = Arc::new(
        fms_application::services::ai_execution_readiness_service::AiExecutionReadinessService::new(None, None),
    );
    let service = build_smoke_proposal_service_with_readiness(pool.clone(), readiness).await;

    let correlation_id = format!("urn:ulid:{}", ulid::Ulid::new());
    let proposal_id = format!("api_smoke_{}", ulid::Ulid::new());
    let object_id = ulid::Ulid::new().to_string();
    let proposal = smoke_todo_create_proposal(&proposal_id, &object_id, &correlation_id);
    save_smoke_proposal(&pool, &proposal).await;

    // Execution enabled but NO staging override → readiness fails
    let _env_guard = SMOKE_ENV_LOCK.lock().unwrap();
    let _guard1 = EnvGuard::set("FMS_AI_PROPOSAL_EXECUTION_ENABLED", "true");
    let _guard2 = EnvGuard::remove("FMS_AI_EXECUTION_READINESS_OVERRIDE");

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(service.clone()))
            .configure(configure),
    )
    .await;

    let token = make_jwt(&["todo:write"], None);
    let exec_req = test::TestRequest::post()
        .uri(&format!("/api/v2/ai/proposals/{proposal_id}/execute"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, exec_req).await;
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "API execute should return 403 when readiness is not ready"
    );

    // Audit event: execution_blocked_readiness
    let event_types = ai_run_event_types(&pool, &proposal.job_id, &proposal.run_id)
        .await
        .unwrap();
    assert!(
        event_types.contains(&"proposal.execution_blocked_readiness".to_string()),
        "should record readiness block audit event, got: {:?}",
        event_types
    );

    // Proposal remains Approved
    let reloaded = proposal_status_by_id(&pool, &proposal_id).await.unwrap();
    assert_eq!(
        reloaded,
        Some(3i16), // Approved = 3
        "proposal should remain Approved when readiness is not ready"
    );

    // No side effects
    let todo_exists = todo_exists_by_source(&pool, "ai_action", &object_id)
        .await
        .unwrap_or(false);
    assert!(!todo_exists, "no todo should be created when readiness blocks");
}
