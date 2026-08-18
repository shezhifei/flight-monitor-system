//! Contract tests for the internal ontology action endpoints (Task F1).
//!
//! Frozen contract under test:
//! - no/bad Service Identity                     → 401
//! - token path mismatch                         → 403
//! - unknown read/advisory action                → 400 `unknown {read,advisory} action`
//! - run_id not found                            → 404 `AI_RUN_NOT_FOUND`
//! - requester lacks the action's permission     → 403 `TOOL_ACTOR_PERMISSION_DENIED`
//! - permitted request dispatches to the shared ontology service surface → 200

use crate::middleware::jwt::JwtSecret;
use crate::middleware::service_identity::ServiceIdentityClaims;
use actix_web::{test, web, App};
use fms_application::services::ai_job_service::AiJobService;
use fms_application::services::ontology_actions::OntologyActionServices;
use fms_domain::models::tool_authorization::ToolAuthorizationContext;
use fms_domain::models::tool_governance::RustToolGovernanceResolver;
use fms_domain::ports::ai_auth_context_loader::{AuthContextLoaderError, RunAuthorizationContextLoader};
use serde_json::{json, Value};
use std::sync::Arc;

const READ_PATH: &str = "/internal/ai/v1/ontology/actions/read";
const ADVISORY_PATH: &str = "/internal/ai/v1/ontology/actions/advisory";

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
/// whitelist check short-circuits before any query runs.
fn lazy_pool() -> sqlx::PgPool {
    sqlx::pool::PoolOptions::<sqlx::Postgres>::new()
        .connect_lazy("postgres://unused:unused@localhost/unused")
        .expect("lazy pool")
}

fn ontology_actions_for_pool(pool: &sqlx::PgPool) -> Arc<OntologyActionServices> {
    use fms_infrastructure::repositories::pg_anomaly_repository::PgAnomalyRepository;
    use fms_infrastructure::repositories::pg_business_case_repository::PgBusinessCaseRepository;
    use fms_infrastructure::repositories::pg_dispatch_order_repository::PgDispatchOrderRepository;
    use fms_infrastructure::repositories::pg_flight_repository::PgFlightRepository;
    use fms_infrastructure::repositories::pg_ontology_repository::PgStandOccupationRepository;
    use fms_infrastructure::repositories::pg_stand_repository::PgStandRepository;
    use fms_infrastructure::repositories::pg_team_repository::PgTeamRepository;

    Arc::new(OntologyActionServices::new(
        Arc::new(PgFlightRepository::new(pool.clone())),
        Arc::new(PgDispatchOrderRepository::new(pool.clone())),
        Arc::new(PgAnomalyRepository::new(pool.clone())),
        Arc::new(PgTeamRepository::new(pool.clone())),
        Arc::new(PgStandRepository::new(pool.clone())),
        Arc::new(PgStandOccupationRepository::new(pool.clone())),
        Arc::new(PgBusinessCaseRepository::new(pool.clone())),
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

async fn read_body_json(resp: actix_web::dev::ServiceResponse) -> Value {
    test::read_body_json(resp).await
}

// ─── Identity gate (no database required) ────────────────────────────────────

#[actix_web::test]
async fn read_action_without_service_identity_returns_401() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .configure(super::configure),
    )
    .await;

    let req = test::TestRequest::post()
        .uri(READ_PATH)
        .set_json(json!({"run_id": "run_1", "action_name": "flight.search"}))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn read_action_with_path_mismatched_token_returns_403() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .configure(super::configure),
    )
    .await;

    let token = create_service_identity_token("test-secret", ADVISORY_PATH);
    let req = test::TestRequest::post()
        .uri(READ_PATH)
        .insert_header(("X-Service-Identity", token))
        .set_json(json!({"run_id": "run_1", "action_name": "flight.search"}))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 403);
}

// ─── Action whitelist (no database required: short-circuits before storage) ──

fn build_whitelist_app() -> actix_web::App<
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
        .app_data(web::Data::new(ontology_actions_for_pool(&pool)))
        .configure(super::configure)
}

#[actix_web::test]
async fn read_action_unknown_action_returns_400() {
    let app = test::init_service(build_whitelist_app()).await;
    let token = create_service_identity_token("test-secret", READ_PATH);

    let req = test::TestRequest::post()
        .uri(READ_PATH)
        .insert_header(("X-Service-Identity", token))
        .set_json(json!({"run_id": "run_1", "action_name": "flight.does_not_exist"}))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 400);
    let body = read_body_json(resp).await;
    assert_eq!(body["success"], false);
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("unknown read action"),
        "expected unknown read action message, got: {body}"
    );
}

#[actix_web::test]
async fn advisory_action_unknown_action_returns_400() {
    let app = test::init_service(build_whitelist_app()).await;
    let token = create_service_identity_token("test-secret", ADVISORY_PATH);

    let req = test::TestRequest::post()
        .uri(ADVISORY_PATH)
        .insert_header(("X-Service-Identity", token))
        .set_json(json!({"run_id": "run_1", "action_name": "flight.does_not_exist"}))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 400);
    let body = read_body_json(resp).await;
    assert_eq!(body["success"], false);
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("unknown advisory action"),
        "expected unknown advisory action message, got: {body}"
    );
}

// ─── Run resolution, permission enforcement and dispatch (real database) ─────

fn has_pool() -> bool {
    std::env::var("TEST_DATABASE_URL").is_ok()
}

async fn create_pool() -> sqlx::PgPool {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    sqlx::PgPool::connect(&url).await.expect("test db")
}

/// Create a job owned by `requester` plus one running run. The run's
/// input_envelope is left empty so the loader falls back to resolver-only
/// governance (no entity config required).
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
async fn read_action_unknown_run_returns_404() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let pool = create_pool().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(ai_job_service_for_pool(&pool)))
            .app_data(web::Data::new(pg_auth_loader(&pool)))
            .app_data(web::Data::new(ontology_actions_for_pool(&pool)))
            .configure(super::configure),
    )
    .await;

    let token = create_service_identity_token("test-secret", READ_PATH);
    let req = test::TestRequest::post()
        .uri(READ_PATH)
        .insert_header(("X-Service-Identity", token))
        .set_json(json!({"run_id": "run_missing", "action_name": "flight.search"}))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 404);
    let body = read_body_json(resp).await;
    assert_eq!(body["success"], false);
    assert_eq!(body["error_code"], "AI_RUN_NOT_FOUND");
    assert_eq!(body["run_id"], "run_missing");
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn read_action_requester_without_permission_returns_403() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let pool = create_pool().await;
    let job_service = ai_job_service_for_pool(&pool);
    // Requester with no roles/permissions at all: permissions recompute from
    // Rust-persisted tables yields an empty grant set.
    let requester = format!("ont_deny_{}", std::process::id());
    let (_job_id, run_id) = create_job_and_run(&job_service, &requester).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_service))
            .app_data(web::Data::new(pg_auth_loader(&pool)))
            .app_data(web::Data::new(ontology_actions_for_pool(&pool)))
            .configure(super::configure),
    )
    .await;

    let token = create_service_identity_token("test-secret", READ_PATH);
    let req = test::TestRequest::post()
        .uri(READ_PATH)
        .insert_header(("X-Service-Identity", token))
        .set_json(json!({"run_id": run_id, "action_name": "flight.search"}))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 403);
    let body = read_body_json(resp).await;
    assert_eq!(body["success"], false);
    assert_eq!(body["error_code"], "TOOL_ACTOR_PERMISSION_DENIED");
    assert_eq!(body["action_name"], "flight.search");
    assert_eq!(body["required_permission"], "flight:read");
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn read_action_with_granted_permission_dispatches_to_shared_surface() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let pool = create_pool().await;
    let job_service = ai_job_service_for_pool(&pool);

    // Seed a requester that carries `flight:read` through role grants.
    // The permission is looked up by name because migrations may already
    // seed it; roles are named with the PID to stay idempotent across runs.
    let requester = format!("ont_grant_{}", std::process::id());
    let role_id = format!("role_ont_{}", std::process::id());
    sqlx::query("INSERT INTO users (id, username) VALUES ($1, $2) ON CONFLICT (id) DO NOTHING")
        .bind(&requester)
        .bind(&requester)
        .execute(&pool)
        .await
        .expect("seed user");
    sqlx::query("INSERT INTO roles (id, name) VALUES ($1, $2) ON CONFLICT (id) DO NOTHING")
        .bind(&role_id)
        .bind(format!("ontology-test-role-{}", std::process::id()))
        .execute(&pool)
        .await
        .expect("seed role");
    let permission_id: String = sqlx::query_scalar(
        "INSERT INTO permissions (id, name) \
         SELECT $1, 'flight:read' WHERE NOT EXISTS (SELECT 1 FROM permissions WHERE name = 'flight:read') \
         ON CONFLICT (name) DO NOTHING \
         RETURNING id",
    )
    .bind(format!("perm_ont_{}", std::process::id()))
    .fetch_optional(&pool)
    .await
    .expect("seed permission")
    .unwrap_or_else(|| {
        // Already present: fetch the existing id.
        String::new()
    });
    let permission_id = if permission_id.is_empty() {
        sqlx::query_scalar("SELECT id FROM permissions WHERE name = 'flight:read'")
            .fetch_one(&pool)
            .await
            .expect("existing flight:read permission")
    } else {
        permission_id
    };
    sqlx::query("INSERT INTO role_permissions (role_id, permission_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(&role_id)
        .bind(&permission_id)
        .execute(&pool)
        .await
        .expect("seed role_permission");
    sqlx::query("INSERT INTO user_roles (user_id, role_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(&requester)
        .bind(&role_id)
        .execute(&pool)
        .await
        .expect("seed user_role");

    let (_job_id, run_id) = create_job_and_run(&job_service, &requester).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_service))
            .app_data(web::Data::new(pg_auth_loader(&pool)))
            .app_data(web::Data::new(ontology_actions_for_pool(&pool)))
            .configure(super::configure),
    )
    .await;

    let token = create_service_identity_token("test-secret", READ_PATH);
    let req = test::TestRequest::post()
        .uri(READ_PATH)
        .insert_header(("X-Service-Identity", token))
        .set_json(json!({"run_id": run_id, "action_name": "flight.search", "arguments": {}}))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(
        resp.status(),
        200,
        "unexpected status: {}",
        String::from_utf8_lossy(&test::read_body(resp).await)
    );
    let body = read_body_json(resp).await;
    // The shared FlightSearchService surface returns the flight list envelope.
    assert!(body.get("flights").is_some(), "expected flight.search envelope, got: {body}");
    assert!(body.get("evidence").is_some(), "expected evidence block, got: {body}");
}
