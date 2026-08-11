use super::*;
use crate::middleware::jwt::JwtSecret;
use crate::services::ai_runtime_client::AiRuntimeClient;
use crate::test_support::{
    load_contract_field_manifest, load_shared_runtime_contract_fixture, repository_root, seed_ai_runtime_test_flights,
    start_fake_sidecar, start_fake_sidecar_delayed_sse, start_fake_sidecar_sse,
    start_fake_sidecar_sse_for_stream_with_tools, start_fake_sidecar_sse_with_status,
};
use actix_web::{http::StatusCode, test, App};
use fms_application::services::ai_action_proposal_service::AiActionProposalService;
use fms_application::services::ai_context_service::AiContextService;
use fms_application::services::ai_job_service::AiJobService;
use fms_application::services::ai_output_validator::AiOutputValidator;
use fms_application::services::ai_proposal_ingest_service::AiProposalIngestService;
use fms_domain::models::ai_structured_output::AiStructuredOutput;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

static TOOL_STREAMING_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn tool_streaming_env_lock() -> MutexGuard<'static, ()> {
    TOOL_STREAMING_ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("tool streaming env lock poisoned")
}

fn make_valid_jwt() -> String {
    use chrono::Utc;
    use jsonwebtoken::{encode, EncodingKey, Header};
    let now = Utc::now().timestamp();
    let claims = json!({
        "sub": "test_user",
        "username": "tester",
        "permissions": ["ai:chat", "flight:write"],
        "is_admin": false,
        "iat": now,
        "exp": now + 3600, // 1 hour expiry
        "type": "access",
    });
    encode(&Header::default(), &claims, &EncodingKey::from_secret(b"test-secret")).expect("jwt encoding")
}

fn make_no_permission_jwt() -> String {
    use chrono::Utc;
    use jsonwebtoken::{encode, EncodingKey, Header};
    let now = Utc::now().timestamp();
    let claims = json!({
        "sub": "test_user",
        "username": "tester",
        "permissions": ["flight:read"],
        "is_admin": false,
        "iat": now,
        "exp": now + 3600, // 1 hour expiry
        "type": "access",
    });
    encode(&Header::default(), &claims, &EncodingKey::from_secret(b"test-secret")).expect("jwt encoding")
}

fn decode_service_identity_claims_for_test(token: &str) -> serde_json::Value {
    use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};

    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&["python-ai-runtime"]);
    validation.set_issuer(&["fms-rust-api"]);

    decode::<serde_json::Value>(token, &DecodingKey::from_secret(b"test-secret"), &validation)
        .expect("decode service identity token")
        .claims
}

fn assert_streaming_service_identity_claims(request: &crate::test_support::FakeSidecarRequest) {
    let si_token = request.service_identity_token.as_ref().expect("service identity token");
    let claims = decode_service_identity_claims_for_test(si_token);
    assert_eq!(claims["path"], "/internal/ai/v1/runs/stream");
    assert_eq!(claims["aud"], "python-ai-runtime");
    assert_eq!(claims["iss"], "fms-rust-api");
    assert_eq!(claims["sub"], "rust-api-gateway");
}

fn assert_stream_with_tools_service_identity_claims(request: &crate::test_support::FakeSidecarRequest) {
    let si_token = request.service_identity_token.as_ref().expect("service identity token");
    let claims = decode_service_identity_claims_for_test(si_token);
    assert_eq!(claims["path"], "/internal/ai/v1/runs/stream-with-tools");
    assert_eq!(claims["aud"], "python-ai-runtime");
    assert_eq!(claims["iss"], "fms-rust-api");
    assert_eq!(claims["sub"], "rust-api-gateway");
}

struct EnvGuard {
    key: &'static str,
    previous: Option<String>,
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

fn has_pool() -> bool {
    std::env::var("TEST_DATABASE_URL").is_ok()
}

async fn create_pool() -> sqlx::PgPool {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    sqlx::PgPool::connect(&url).await.expect("test db")
}

async fn build_services(
    pool: &sqlx::PgPool,
    sidecar_url: &str,
) -> (
    Arc<AiJobService>,
    Arc<AiContextService>,
    Arc<AiRuntimeClient>,
    Arc<AiProposalIngestService>,
    Arc<AiActionProposalService>,
) {
    if std::env::var("TEST_DATABASE_URL").is_ok() {
        seed_ai_runtime_test_flights(pool).await;
    }

    let flight_repo =
        Arc::new(fms_infrastructure::repositories::pg_flight_repository::PgFlightRepository::new(pool.clone()));
    let flight_svc = Arc::new(fms_application::services::flight_service::FlightService::new(
        flight_repo,
    ));
    let auth_svc = Arc::new(fms_application::services::authorization_service::AuthorizationService);
    let context_svc = Arc::new(AiContextService::new(flight_svc, auth_svc));
    let job_svc = Arc::new(AiJobService::new(
        Arc::new(fms_infrastructure::repositories::pg_ai_job_repository::PgAiJobRepository::new(pool.clone())),
        Arc::new(fms_infrastructure::repositories::pg_ai_run_repository::PgAiRunRepository::new(pool.clone())),
        Arc::new(
            fms_infrastructure::repositories::pg_ai_run_event_repository::PgAiRunEventRepository::new(pool.clone()),
        ),
    ));
    let runtime_client = Arc::new(AiRuntimeClient::with_base_url(sidecar_url));

    let validator = Arc::new(AiOutputValidator::new(
        fms_domain::ontology::flight_ops_v1::build_flight_ops_v1_schema(),
    ));
    let proposal_svc = Arc::new(AiActionProposalService::new());
    let ingest_svc = Arc::new(AiProposalIngestService::new(
        validator,
        proposal_svc.clone(),
        job_svc.clone(),
    ));

    (job_svc, context_svc, runtime_client, ingest_svc, proposal_svc)
}

/// Returns a valid AiStructuredOutput with a single proposal that passes
/// the AiOutputValidator (Flight.add_note, risk low, no-ops ceiling medium).
fn valid_proposals_body(run_id: &str) -> Value {
    json!({
        "contract_version": "ai-structured-output.v1",
        "run_id": run_id,
        "status": "succeeded",
        "answer": "I've added a note to the flight.",
        "reasoning_steps": [{"step": "analyze", "summary": "Analyzed the request"}],
        "evidence": [],
        "proposals": [{
            "object_type": "Flight",
            "object_id": "FL123",
            "action_name": "add_note",
            "arguments": {"note_content": "test note"},
            "risk_level": "low",
            "confidence": 0.95,
            "reasoning": "User requested",
            "requires_approval": false
        }],
        "limitations": [],
        "metrics": {"model": "test", "duration_ms": 100}
    })
}

/// Returns an AiStructuredOutput with proposals that will fail validation
/// (Flight.cancel is not in allowed_actions).
fn invalid_proposals_body(run_id: &str) -> Value {
    json!({
        "contract_version": "ai-structured-output.v1",
        "run_id": run_id,
        "status": "succeeded",
        "answer": "I attempted to cancel the flight.",
        "reasoning_steps": [{"step": "analyze", "summary": "Analyzed the request"}],
        "evidence": [],
        "proposals": [{
            "object_type": "Flight",
            "object_id": "FL123",
            "action_name": "cancel",
            "arguments": {"reason": "weather"},
            "risk_level": "high",
            "confidence": 0.8,
            "reasoning": "Weather conditions",
            "requires_approval": true
        }],
        "limitations": [],
        "metrics": {"model": "test", "duration_ms": 100}
    })
}

#[actix_web::test]
async fn test_route_requires_authentication() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .configure(super::configure),
    )
    .await;
    let req = test::TestRequest::post().uri("/api/v2/ai/nl-query").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn test_streaming_route_requires_authentication() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .configure(super::configure),
    )
    .await;
    let req = test::TestRequest::post().uri("/api/v2/ai/nl-query/stream").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn test_followup_requires_authentication() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .configure(super::configure),
    )
    .await;
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query/followup")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn test_suggestions_requires_authentication() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .configure(super::configure),
    )
    .await;
    let req = test::TestRequest::get()
        .uri("/api/v2/ai/nl-query/suggestions")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn test_conversations_requires_authentication() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .configure(super::configure),
    )
    .await;
    let req = test::TestRequest::get()
        .uri("/api/v2/ai/nl-query/conversations")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn test_followup_stream_returns_degraded_fallback() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .configure(super::configure),
    )
    .await;
    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query/followup")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({
            "question": "add note to flight FL123",
            "context": {"selected_flight_id": "FL123"}
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_ne!(resp.status(), StatusCode::NOT_FOUND);
    assert_ne!(resp.status(), StatusCode::OK);
}

#[actix_web::test]
async fn test_suggestions_returns_degraded_fallback() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .configure(super::configure),
    )
    .await;
    let token = make_valid_jwt();
    let req = test::TestRequest::get()
        .uri("/api/v2/ai/nl-query/suggestions")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_ne!(resp.status(), StatusCode::NOT_FOUND);
    assert_ne!(resp.status(), StatusCode::OK);
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_non_streaming_python_503_fails_run() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let fake = start_fake_sidecar(503, json!({"degraded": true})).await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({"question": "test flight status"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], false);

    let job_id = body["data"]["job_id"].as_str().unwrap_or("").to_string();
    let run_id = body["data"]["run_id"].as_str().unwrap_or("").to_string();
    assert!(!job_id.is_empty(), "job_id must be present");
    assert!(!run_id.is_empty(), "run_id must be present");
    assert_eq!(body["data"]["status"], "failed_terminal");

    let job = job_svc.get_job(&job_id).await.expect("job must exist");
    assert_eq!(job.status, "failed_terminal");
    assert_eq!(job.job_type, "nl_query");

    let run = job_svc.get_run(&run_id).await.expect("run must exist");
    assert_eq!(run.job_id, job_id);
    assert_eq!(run.status, "failed_terminal");

    let env = run.input_envelope.expect("input_envelope must exist");
    let env_job_id = env.get("job_id").and_then(|v| v.as_str()).unwrap_or("");
    let env_run_id = env.get("run_id").and_then(|v| v.as_str()).unwrap_or("");
    assert_eq!(env_job_id, job_id, "envelope job_id must match db job_id");
    assert_eq!(env_run_id, run_id, "envelope run_id must match db run_id");

    let requests = fake.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/internal/ai/v1/runs");
    assert!(request.has_service_identity);
    assert_eq!(request.body["job_id"], job_id);
    assert_eq!(request.body["run_id"], run_id);
    assert!(request.body.get("requester").is_some());
    assert!(request.body.get("context").is_some());
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_non_streaming_forbidden_without_ai_chat() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let fake = start_fake_sidecar(200, json!({})).await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_no_permission_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({
            "question": "add note to flight FL123",
            "context": {"selected_flight_id": "FL123"}
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_non_streaming_python_200_success_false_fails_run() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let fake = start_fake_sidecar(
        200,
        json!({
            "success": false,
            "status": "failed",
            "error": "business logic error",
        }),
    )
    .await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({
            "question": "add note to flight FL123",
            "context": {"selected_flight_id": "FL123"}
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], false);
    assert_eq!(body["data"]["status"], "failed_terminal");

    let job_id = body["data"]["job_id"].as_str().unwrap().to_string();
    let run_id = body["data"]["run_id"].as_str().unwrap().to_string();

    let job = job_svc.get_job(&job_id).await.expect("job must exist");
    assert_eq!(job.status, "failed_terminal");

    let run = job_svc.get_run(&run_id).await.expect("run must exist");
    assert_eq!(run.status, "failed_terminal");
    assert!(run.input_envelope.is_some(), "input_envelope must exist");
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_non_streaming_python_200_status_failed_fails_run() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let fake = start_fake_sidecar(
        200,
        json!({
            "success": true,
            "status": "failed",
            "error": "LLM error: token limit exceeded",
        }),
    )
    .await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({
            "question": "add note to flight FL123",
            "context": {"selected_flight_id": "FL123"}
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], false);
    assert_eq!(body["data"]["status"], "failed_terminal");

    let job_id = body["data"]["job_id"].as_str().unwrap().to_string();
    let job = job_svc.get_job(&job_id).await.expect("job must exist");
    assert_eq!(job.status, "failed_terminal");

    let requests = fake.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/internal/ai/v1/runs");
    assert!(request.has_service_identity);
    assert_eq!(request.body["job_id"], job_id);
    assert_eq!(request.body["run_id"], body["data"]["run_id"]);
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_non_streaming_python_200_valid_proposals_returns_created_ids() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    // We don't know the run_id in advance, so we use a hack: the fake sidecar returns
    // a fixed run_id; the handler creates a real run but ignores the sidecar's run_id.
    // The test asserts the response shape and DB state, which is sufficient.
    let fake_body = valid_proposals_body("run_fake_ignored");
    let fake = start_fake_sidecar(200, fake_body).await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({
            "question": "cancel flight FL123",
            "context": {"selected_flight_id": "FL123"}
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
    assert!(
        body["data"].get("created_proposal_ids").is_some(),
        "created_proposal_ids must be present in response"
    );

    let job_id = body["data"]["job_id"].as_str().unwrap().to_string();
    let run_id = body["data"]["run_id"].as_str().unwrap().to_string();

    let job = job_svc.get_job(&job_id).await.expect("job must exist");
    assert_eq!(job.status, "succeeded");

    let run = job_svc.get_run(&run_id).await.expect("run must exist");
    assert_eq!(run.status, "succeeded");
    assert!(run.input_envelope.is_some(), "input_envelope must exist");

    let requests = fake.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/internal/ai/v1/runs");
    assert!(request.has_service_identity);
    assert_eq!(request.body["job_id"], job_id);
    assert_eq!(request.body["run_id"], run_id);
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_non_streaming_python_200_invalid_proposals_rejected() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let fake_body = invalid_proposals_body("run_fake_ignored");
    let fake = start_fake_sidecar(200, fake_body).await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({
            "question": "add note to flight FL123",
            "context": {"selected_flight_id": "FL123"}
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], false);
    assert_eq!(body["data"]["status"], "failed_terminal");
    assert!(
        body.get("rejected_proposals").is_some(),
        "rejected_proposals must be present"
    );

    let job_id = body["data"]["job_id"].as_str().unwrap().to_string();
    let job = job_svc.get_job(&job_id).await.expect("job must exist");
    assert_eq!(job.status, "failed_terminal");

    let requests = fake.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/internal/ai/v1/runs");
    assert!(request.has_service_identity);
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_non_streaming_python_200_no_proposals_succeeds() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let fake = start_fake_sidecar(
        200,
        json!({
            "success": true,
            "status": "succeeded",
            "answer": "Flight BA123 is on time.",
            "degraded": false,
        }),
    )
    .await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({
            "question": "add note to flight FL123",
            "context": {"selected_flight_id": "FL123"}
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;

    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "shared fixture response body: {body}");
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["status"], "succeeded");
    assert_eq!(body["data"]["answer"], "Flight BA123 is on time.");

    let job_id = body["data"]["job_id"].as_str().unwrap().to_string();
    let run_id = body["data"]["run_id"].as_str().unwrap().to_string();

    let job = job_svc.get_job(&job_id).await.expect("job must exist");
    assert_eq!(job.status, "succeeded");

    let run = job_svc.get_run(&run_id).await.expect("run must exist");
    assert_eq!(run.status, "succeeded");
    assert!(run.input_envelope.is_some(), "input_envelope must exist");
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_non_streaming_python_200_degraded_success() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let fake = start_fake_sidecar(
        200,
        json!({
            "success": true,
            "status": "succeeded",
            "degraded": true,
            "answer": "（启发式运行时）已按「通用」理解您的请求：test。",
            "limitations": ["LLM not configured (set OPENAI_API_KEY for full model-backed answers)"],
            "metrics": {"model": "heuristic-runtime-v1", "duration_ms": 5},
        }),
    )
    .await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({
            "question": "add note to flight FL123",
            "context": {"selected_flight_id": "FL123"}
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;

    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "shared fixture response body: {body}");
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["status"], "succeeded");
    assert_eq!(body["data"]["degraded"], true);
    assert!(body["data"]["answer"].as_str().unwrap_or("").contains("启发式"));

    let job_id = body["data"]["job_id"].as_str().unwrap().to_string();
    let run_id = body["data"]["run_id"].as_str().unwrap().to_string();

    let job = job_svc.get_job(&job_id).await.expect("job must exist");
    assert_eq!(job.status, "succeeded");

    let run = job_svc.get_run(&run_id).await.expect("run must exist");
    assert_eq!(run.status, "succeeded");
    assert!(run.input_envelope.is_some(), "input_envelope must exist");
}

#[actix_web::test]
async fn test_shared_fixture_deserializes_into_rust_models() {
    let fixture = load_shared_runtime_contract_fixture();
    let envelope: fms_domain::models::ai_context_envelope::ContextEnvelope =
        serde_json::from_value(fixture["context_envelope"].clone()).expect("fixture context_envelope must deserialize");
    assert_eq!(envelope.contract_version, "ai-runtime.v1");
    assert_eq!(envelope.run_id, "run_fixture_001");

    // Semantic consistency: user_message should match proposal action
    let user_msg = envelope.task.user_message.to_lowercase();
    let has_note_intent = user_msg.contains("备注") || user_msg.contains("note") || user_msg.contains("add");
    assert!(has_note_intent, "user_message should match proposal action (add_note)");

    let output: fms_domain::models::ai_structured_output::AiStructuredOutput =
        serde_json::from_value(fixture["ai_structured_output"].clone())
            .expect("fixture ai_structured_output must deserialize");
    assert_eq!(output.status, "succeeded");
    assert_eq!(output.run_id, "run_fixture_001");
    assert_eq!(output.proposals.len(), 1);
    assert_eq!(output.proposals[0].action_name, "add_note");
    assert_eq!(output.proposals[0].risk_level, "low");
    assert!(output.metrics.is_some());
}

/// Recursively assert that two JSON values have an identical object-key structure.
/// Values are ignored; only the set of keys at every nesting level is compared.
/// Arrays are compared element-wise (round-trip preserves length/order).
fn assert_same_key_structure(path: &str, expected: &Value, actual: &Value) {
    match (expected, actual) {
        (Value::Object(exp), Value::Object(act)) => {
            let exp_keys: std::collections::BTreeSet<&String> = exp.keys().collect();
            let act_keys: std::collections::BTreeSet<&String> = act.keys().collect();
            assert_eq!(
                exp_keys, act_keys,
                "contract key drift at '{path}':\n  in fixture only (Rust struct missing field?): {:?}\n  in Rust round-trip only (struct has extra field?): {:?}",
                exp_keys.difference(&act_keys).collect::<Vec<_>>(),
                act_keys.difference(&exp_keys).collect::<Vec<_>>(),
            );
            for (key, exp_child) in exp {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                assert_same_key_structure(&child_path, exp_child, act.get(key).unwrap());
            }
        }
        (Value::Array(exp), Value::Array(act)) => {
            for (idx, (exp_child, act_child)) in exp.iter().zip(act.iter()).enumerate() {
                assert_same_key_structure(&format!("{path}[{idx}]"), exp_child, act_child);
            }
        }
        // Scalars (and Object-vs-non-Object mismatches would already have failed above): no keys to compare.
        _ => {}
    }
}

/// The Rust structs must model exactly the wire contract: deserializing the exhaustive
/// shared fixture and re-serializing must reproduce the same key structure (no dropped
/// fields, no unexpected extra fields). This is the Rust half of the cross-language drift
/// gate; the Python half asserts model fields against the same manifest in
/// tests/sidecar/test_shared_fixture.py.
#[actix_web::test]
async fn test_shared_fixture_round_trips_without_field_drift() {
    let fixture = load_shared_runtime_contract_fixture();
    let manifest = load_contract_field_manifest();

    // Fixture must stay in sync with the manifest: top-level field sets must match, so an
    // enriched manifest cannot silently outrun a stale fixture (which would blunt the gate).
    let manifest_envelope_fields: std::collections::BTreeSet<String> = manifest["context_envelope_contract"]["types"]
        ["ContextEnvelope"]
        .as_array()
        .expect("manifest ContextEnvelope fields")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let fixture_envelope_fields: std::collections::BTreeSet<String> = fixture["context_envelope"]
        .as_object()
        .expect("fixture context_envelope")
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        manifest_envelope_fields, fixture_envelope_fields,
        "manifest and fixture ContextEnvelope field sets diverge"
    );

    let manifest_output_fields: std::collections::BTreeSet<String> = manifest["structured_output_contract"]["types"]
        ["AiStructuredOutput"]
        .as_array()
        .expect("manifest AiStructuredOutput fields")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let fixture_output_fields: std::collections::BTreeSet<String> = fixture["ai_structured_output"]
        .as_object()
        .expect("fixture ai_structured_output")
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        manifest_output_fields, fixture_output_fields,
        "manifest and fixture AiStructuredOutput field sets diverge"
    );

    // Round-trip the envelope through the Rust struct and compare key structure.
    let envelope: fms_domain::models::ai_context_envelope::ContextEnvelope =
        serde_json::from_value(fixture["context_envelope"].clone()).expect("envelope must deserialize");
    let envelope_roundtrip = serde_json::to_value(&envelope).expect("envelope must serialize");
    assert_same_key_structure("context_envelope", &fixture["context_envelope"], &envelope_roundtrip);

    // Round-trip the structured output and compare key structure.
    let output: fms_domain::models::ai_structured_output::AiStructuredOutput =
        serde_json::from_value(fixture["ai_structured_output"].clone()).expect("output must deserialize");
    let output_roundtrip = serde_json::to_value(&output).expect("output must serialize");
    assert_same_key_structure(
        "ai_structured_output",
        &fixture["ai_structured_output"],
        &output_roundtrip,
    );

    // token_usage must survive the round-trip (it was previously dropped by the Rust struct).
    let token_usage = output.token_usage.expect("token_usage must deserialize");
    assert_eq!(token_usage.prompt_tokens, 64);
    assert_eq!(token_usage.completion_tokens, 16);
    assert_eq!(token_usage.total_tokens, 80);
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_non_streaming_python_200_shared_fixture_proposal_ingest() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let fixture = load_shared_runtime_contract_fixture();
    let fake = start_fake_sidecar(200, fixture["ai_structured_output"].clone()).await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({
            "question": "add note to flight FL123",
            "context": {"selected_flight_id": "FL123"}
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;

    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "shared fixture response body: {body}");
    assert_eq!(body["success"], true);
    assert!(
        body["data"].get("created_proposal_ids").is_some(),
        "created_proposal_ids must be present"
    );

    let job_id = body["data"]["job_id"].as_str().unwrap().to_string();
    let run_id = body["data"]["run_id"].as_str().unwrap().to_string();

    let job = job_svc.get_job(&job_id).await.expect("job must exist");
    assert_eq!(job.status, "succeeded");

    let run = job_svc.get_run(&run_id).await.expect("run must exist");
    assert_eq!(run.status, "succeeded");
    assert!(run.input_envelope.is_some(), "input_envelope must exist");
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_streaming_python_200_no_proposals_succeeds() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let fake_sse_events = vec![
        "event: progress\ndata: {\"step\":\"init\"}\n\n".to_string(),
        "event: token\ndata: {\"delta\":\"test\"}\n\n".to_string(),
        format!(
            "event: run.complete\ndata: {}\n\n",
            json!({
                "contract_version": "ai-structured-output.v1",
                "run_id": "ignored_id",
                "status": "succeeded",
                "answer": "Test streaming answer",
                "reasoning_steps": [],
                "evidence": [],
                "proposals": [],
                "limitations": [],
                "metrics": {"model": "test", "duration_ms": 10}
            })
        ),
    ];

    let fake = start_fake_sidecar_sse(fake_sse_events).await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query/stream")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({"question": "test"}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = actix_web::body::to_bytes(resp.into_body()).await.unwrap();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert!(body_str.contains("event: token\ndata: {\"delta\":\"test\"}"));

    // Wait a bit for background tasks (DB writes) to complete since actix test doesn't poll everything.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let requests = fake.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/internal/ai/v1/runs/stream");
    assert!(request.has_service_identity);
    assert_streaming_service_identity_claims(request);

    let env_job_id = request.body["job_id"].as_str().unwrap().to_string();
    let env_run_id = request.body["run_id"].as_str().unwrap().to_string();

    let job = job_svc.get_job(&env_job_id).await.expect("job");
    assert_eq!(job.status, "succeeded");

    let run = job_svc.get_run(&env_run_id).await.expect("run");
    assert_eq!(run.status, "succeeded");
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_streaming_python_run_fail_fails_run() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let fake_sse_events = vec![format!(
        "event: run.fail\ndata: {}\n\n",
        json!({
            "contract_version": "ai-structured-output.v1",
            "run_id": "ignored_id",
            "status": "failed",
            "answer": "Validation failed",
            "reasoning_steps": [],
            "evidence": [],
            "proposals": [],
            "limitations": [],
            "metrics": {"model": "test", "duration_ms": 5},
            "error": "Validation error"
        })
    )];

    let fake = start_fake_sidecar_sse(fake_sse_events).await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query/stream")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({"question": "fail stream"}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    let _ = actix_web::body::to_bytes(resp.into_body()).await; // Drain stream
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let requests = fake.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/internal/ai/v1/runs/stream");
    assert!(request.has_service_identity);
    assert_streaming_service_identity_claims(request);

    let env_job_id = request.body["job_id"].as_str().unwrap().to_string();
    let env_run_id = request.body["run_id"].as_str().unwrap().to_string();

    let job = job_svc.get_job(&env_job_id).await.expect("job");
    assert_eq!(job.status, "failed_terminal");

    let run = job_svc.get_run(&env_run_id).await.expect("run");
    assert_eq!(run.status, "failed_terminal");
    assert!(!matches!(run.error_message, Some(ref reason) if reason == "No terminal event received from SSE stream"));
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_streaming_python_missing_terminal_fails_run() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let fake_sse_events = vec!["event: progress\ndata: {\"step\":\"init\"}\n\n".to_string()];

    let fake = start_fake_sidecar_sse(fake_sse_events).await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query/stream")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({"question": "no terminal"}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    let body_bytes = actix_web::body::to_bytes(resp.into_body()).await.unwrap();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();

    assert!(body_str.contains("event: run.fail"));

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let requests = fake.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/internal/ai/v1/runs/stream");
    assert!(request.has_service_identity);
    assert_streaming_service_identity_claims(request);

    let env_job_id = request.body["job_id"].as_str().unwrap().to_string();
    let env_run_id = request.body["run_id"].as_str().unwrap().to_string();

    let job = job_svc.get_job(&env_job_id).await.expect("job");
    assert_eq!(job.status, "failed_terminal");

    let run = job_svc.get_run(&env_run_id).await.expect("run");
    assert_eq!(run.status, "failed_terminal");
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_streaming_python_200_degraded_success_succeeds() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let fake_sse_events = vec![
        "event: progress\ndata: {\"step\":\"init\"}\n\n".to_string(),
        format!(
            "event: run.complete\ndata: {}\n\n",
            json!({
                "contract_version": "ai-structured-output.v1",
                "run_id": "ignored_id",
                "status": "succeeded",
                "answer": "（启发式运行时）已按「通用」理解您的请求",
                "reasoning_steps": [],
                "evidence": [],
                "proposals": [],
                "limitations": ["LLM not configured"],
                "metrics": {"model": "heuristic-runtime-v1", "duration_ms": 5}
            })
        ),
    ];

    let fake = start_fake_sidecar_sse(fake_sse_events).await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query/stream")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({"question": "test"}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = actix_web::body::to_bytes(resp.into_body()).await.unwrap();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert!(body_str.contains("启发式") || body_str.contains("limitations"));

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let requests = fake.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/internal/ai/v1/runs/stream");
    assert!(request.has_service_identity);
    assert_streaming_service_identity_claims(request);

    let env_job_id = request.body["job_id"].as_str().unwrap().to_string();
    let env_run_id = request.body["run_id"].as_str().unwrap().to_string();

    let job = job_svc.get_job(&env_job_id).await.expect("job");
    assert_eq!(job.status, "succeeded");

    let run = job_svc.get_run(&env_run_id).await.expect("run");
    assert_eq!(run.status, "succeeded");
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_streaming_python_200_status_failed_fails_run() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let fake_sse_events = vec![format!(
        "event: run.fail\ndata: {}\n\n",
        json!({
            "contract_version": "ai-structured-output.v1",
            "run_id": "ignored_id",
            "status": "failed",
            "answer": "Validation failed",
            "reasoning_steps": [],
            "evidence": [],
            "proposals": [],
            "limitations": [],
            "metrics": {"model": "test", "duration_ms": 5}
        })
    )];

    let fake = start_fake_sidecar_sse(fake_sse_events).await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query/stream")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({"question": "fail"}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    let _ = actix_web::body::to_bytes(resp.into_body()).await; // Drain stream
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let requests = fake.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/internal/ai/v1/runs/stream");
    assert!(request.has_service_identity);
    assert_streaming_service_identity_claims(request);

    let env_job_id = request.body["job_id"].as_str().unwrap().to_string();
    let env_run_id = request.body["run_id"].as_str().unwrap().to_string();

    let job = job_svc.get_job(&env_job_id).await.expect("job");
    assert_eq!(job.status, "failed_terminal");

    let run = job_svc.get_run(&env_run_id).await.expect("run");
    assert_eq!(run.status, "failed_terminal");
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_streaming_python_valid_proposals_creates_proposals() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let fake_sse_events = vec![format!(
        "event: run.complete\ndata: {}\n\n",
        valid_proposals_body("ignored_id")
    )];

    let fake = start_fake_sidecar_sse(fake_sse_events).await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query/stream")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({
            "question": "add note to flight FL123",
            "context": {"selected_flight_id": "FL123"}
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    let _ = actix_web::body::to_bytes(resp.into_body()).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let requests = fake.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/internal/ai/v1/runs/stream");
    assert!(request.has_service_identity);
    assert_streaming_service_identity_claims(request);

    let env_job_id = request.body["job_id"].as_str().unwrap().to_string();
    let env_run_id = request.body["run_id"].as_str().unwrap().to_string();

    let job = job_svc.get_job(&env_job_id).await.expect("job");
    assert_eq!(job.status, "succeeded");

    let run = job_svc.get_run(&env_run_id).await.expect("run");
    assert_eq!(run.status, "succeeded");

    let envelope = run.input_envelope.as_ref().expect("input_envelope");
    assert_eq!(
        envelope.get("job_id").and_then(|v| v.as_str()),
        Some(env_job_id.as_str())
    );
    assert_eq!(
        envelope.get("run_id").and_then(|v| v.as_str()),
        Some(env_run_id.as_str())
    );

    use fms_domain::models::ai_proposal::ActionProposalQuery;

    let proposals = proposal_svc
        .list_proposals(&ActionProposalQuery {
            job_id: Some(env_job_id.clone()),
            run_id: Some(env_run_id.clone()),
            action_name: Some("add_note".to_string()),
            ..Default::default()
        })
        .await
        .expect("list proposals");
    assert!(
        proposals.len() >= 1,
        "at least one proposal must be created for run {}",
        env_run_id
    );
    let proposal = &proposals[0];
    assert_eq!(proposal.action_name, "add_note");
    assert_eq!(proposal.job_id, env_job_id);
    assert_eq!(proposal.run_id, env_run_id);
    assert_eq!(proposal.object_type, "Flight");
    assert_eq!(proposal.object_id, "FL123");
    assert!(!proposal.proposal_id.is_empty(), "proposal_id must be assigned");
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_streaming_client_early_drop_server_finalizes_run() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let chunks = vec![
        (
            "event: token\ndata: {\"delta\":\"hello\"}\n\n".to_string(),
            std::time::Duration::from_millis(10),
        ),
        (
            format!(
                "event: run.complete\ndata: {}\n\n",
                json!({
                    "contract_version": "ai-structured-output.v1",
                    "run_id": "ignored_id",
                    "status": "succeeded",
                    "answer": "Test delayed answer",
                    "reasoning_steps": [],
                    "evidence": [],
                    "proposals": [],
                    "limitations": [],
                    "metrics": {"model": "test", "duration_ms": 10}
                })
            ),
            std::time::Duration::from_millis(300),
        ),
    ];

    let fake = start_fake_sidecar_delayed_sse(chunks).await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query/stream")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({"question": "test early drop"}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Drop the response immediately to simulate client disconnect.
    // The background task must continue consuming the Python stream.
    drop(resp);

    // Wait for the background task to finish (second chunk has 300ms delay + processing).
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;

    let requests = fake.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    let env_job_id = request.body["job_id"].as_str().unwrap().to_string();
    let env_run_id = request.body["run_id"].as_str().unwrap().to_string();

    let job = job_svc.get_job(&env_job_id).await.expect("job");
    assert_eq!(
        job.status, "succeeded",
        "job must be finalized to succeeded even after client drop"
    );

    let run = job_svc.get_run(&env_run_id).await.expect("run");
    assert_eq!(
        run.status, "succeeded",
        "run must be finalized to succeeded even after client drop"
    );
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_streaming_proposal_ingest_failure_fails_run() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let fake_sse_events = vec![format!(
        "event: run.complete\ndata: {}\n\n",
        invalid_proposals_body("ignored_id")
    )];

    let fake = start_fake_sidecar_sse(fake_sse_events).await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query/stream")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({"question": "invalid proposal"}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    let _ = actix_web::body::to_bytes(resp.into_body()).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let requests = fake.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    let env_job_id = request.body["job_id"].as_str().unwrap().to_string();
    let env_run_id = request.body["run_id"].as_str().unwrap().to_string();

    let job = job_svc.get_job(&env_job_id).await.expect("job");
    assert_eq!(
        job.status, "failed_terminal",
        "job must be failed_terminal when proposal ingest fails"
    );

    let run = job_svc.get_run(&env_run_id).await.expect("run");
    assert_eq!(
        run.status, "failed_terminal",
        "run must be failed_terminal when proposal ingest fails"
    );
    assert!(
        run.error_message
            .as_ref()
            .unwrap()
            .contains("proposal_validation_failed"),
        "error_message should mention proposal_validation_failed"
    );
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_complete_run_idempotent() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let pool = create_pool().await;
    let (job_svc, _context_svc, _runtime_client, _ingest_svc, _proposal_svc) = build_services(&pool, "").await;

    let job = job_svc
        .create_job("nl_query", Some("test_user"), None, None, None)
        .await
        .expect("create job");
    let run = job_svc
        .create_run(&job.job_id, "test_engine", None, Some(json!({"envelope": {}})))
        .await
        .expect("create run");

    let _ = job_svc
        .transition_run(&run.run_id, fms_domain::models::ai_job::AiRunStatus::Claimed)
        .await;
    let _ = job_svc
        .transition_run(&run.run_id, fms_domain::models::ai_job::AiRunStatus::Running)
        .await;

    // First complete
    let run1 = job_svc
        .complete_run(&run.run_id, Some(json!({"answer": "ok"})), None, None)
        .await
        .expect("first complete_run");
    assert_eq!(run1.status, "succeeded");

    // Second complete should be idempotent (no panic, same status)
    let run2 = job_svc
        .complete_run(&run.run_id, Some(json!({"answer": "ok2"})), None, None)
        .await
        .expect("second complete_run");
    assert_eq!(run2.status, "succeeded");
    assert_eq!(run2.run_id, run1.run_id);
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_streaming_python_transport_failure_fails_run() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    // For now, test with upstream returning 500 since mid-stream drop isn't easily faked here
    let fake = start_fake_sidecar_sse_with_status(500, vec![]).await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query/stream")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({"question": "transport fail"}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    let _ = actix_web::body::to_bytes(resp.into_body()).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let requests = fake.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/internal/ai/v1/runs/stream");
    assert!(request.has_service_identity);
    assert_streaming_service_identity_claims(request);

    let env_job_id = request.body["job_id"].as_str().unwrap().to_string();
    let env_run_id = request.body["run_id"].as_str().unwrap().to_string();

    let job = job_svc.get_job(&env_job_id).await.expect("job");
    assert_eq!(job.status, "failed_terminal");

    let run = job_svc.get_run(&env_run_id).await.expect("run");
    assert_eq!(run.status, "failed_terminal");
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_streaming_python_mid_stream_error_fails_run() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let fake_sse_events = vec!["event: progress\ndata: {\"step\":\"init\"}\n\n".to_string()];
    let fake =
        crate::test_support::start_fake_sidecar_error_stream(fake_sse_events, "mid-stream abort".to_string()).await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query/stream")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({"question": "transport fail"}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    let _ = actix_web::body::to_bytes(resp.into_body()).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let requests = fake.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/internal/ai/v1/runs/stream");
    assert!(request.has_service_identity);

    let env_job_id = request.body["job_id"].as_str().unwrap().to_string();
    let env_run_id = request.body["run_id"].as_str().unwrap().to_string();

    let job = job_svc.get_job(&env_job_id).await.expect("job");
    assert_eq!(job.status, "failed_terminal");

    let run = job_svc.get_run(&env_run_id).await.expect("run");
    assert_eq!(run.status, "failed_terminal");
    assert!(
        run.error_message.as_ref().unwrap().contains("mid-stream abort"),
        "error_message should contain mid-stream abort"
    );
    assert_eq!(run.error_code.as_deref(), Some("transport_error"));

    // Assert telemetry events were written
    let events = job_svc.list_events_for_run(&env_run_id, 50).await.expect("list events");
    let event_names: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert!(
        event_names.contains(&"provider_stream_started"),
        "must have provider_stream_started"
    );
    assert!(
        event_names.contains(&"provider_stream_aborted"),
        "must have provider_stream_aborted"
    );
    assert!(
        event_names.contains(&"finalization_failed_transport_error"),
        "must have finalization_failed_transport_error"
    );
    assert!(
        event_names.contains(&"runtime_completed"),
        "must have runtime_completed"
    );
}

// =====================================================================
// P2.3 Telemetry event taxonomy DB ignored tests
// =====================================================================

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_streaming_success_event_sequence() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let fake_sse_events = vec![
        "event: token\ndata: {\"delta\":\"hello\"}\n\n".to_string(),
        format!(
            "event: run.complete\ndata: {}\n\n",
            json!({
                "contract_version": "ai-structured-output.v1",
                "run_id": "ignored_id",
                "status": "succeeded",
                "answer": "Test answer",
                "reasoning_steps": [],
                "evidence": [],
                "proposals": [],
                "limitations": [],
                "metrics": {"model": "test", "duration_ms": 10}
            })
        ),
    ];

    let fake = start_fake_sidecar_sse(fake_sse_events).await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query/stream")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({"question": "test"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let _ = actix_web::body::to_bytes(resp.into_body()).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let requests = fake.requests();
    let env_run_id = requests[0].body["run_id"].as_str().unwrap().to_string();

    let events = job_svc.list_events_for_run(&env_run_id, 50).await.expect("list events");
    let event_names: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert!(event_names.contains(&"runtime_started"), "must have runtime_started");
    assert!(
        event_names.contains(&"provider_stream_started"),
        "must have provider_stream_started"
    );
    assert!(
        event_names.contains(&"first_token_emitted"),
        "must have first_token_emitted"
    );
    assert!(
        event_names.contains(&"provider_stream_completed"),
        "must have provider_stream_completed"
    );
    assert!(
        event_names.contains(&"runtime_completed"),
        "must have runtime_completed"
    );
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_streaming_degraded_success_event_sequence() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let fake_sse_events = vec![format!(
        "event: run.complete\ndata: {}\n\n",
        json!({
            "contract_version": "ai-structured-output.v1",
            "run_id": "ignored_id",
            "status": "succeeded",
            "answer": "heuristic answer",
            "reasoning_steps": [],
            "evidence": [],
            "proposals": [],
            "limitations": ["LLM not configured"],
            "metrics": {"model": "heuristic-runtime-v1", "duration_ms": 5}
        })
    )];

    let fake = start_fake_sidecar_sse(fake_sse_events).await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query/stream")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({"question": "test"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let _ = actix_web::body::to_bytes(resp.into_body()).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let requests = fake.requests();
    let env_run_id = requests[0].body["run_id"].as_str().unwrap().to_string();

    let events = job_svc.list_events_for_run(&env_run_id, 50).await.expect("list events");
    let event_names: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert!(event_names.contains(&"runtime_started"), "must have runtime_started");
    assert!(
        event_names.contains(&"provider_stream_started"),
        "must have provider_stream_started"
    );
    assert!(
        event_names.contains(&"runtime_completed"),
        "must have runtime_completed"
    );
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_streaming_missing_terminal_event_sequence() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let fake_sse_events = vec!["event: progress\ndata: {\"step\":\"init\"}\n\n".to_string()];

    let fake = start_fake_sidecar_sse(fake_sse_events).await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query/stream")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({"question": "no terminal"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let _ = actix_web::body::to_bytes(resp.into_body()).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let requests = fake.requests();
    let env_run_id = requests[0].body["run_id"].as_str().unwrap().to_string();

    let events = job_svc.list_events_for_run(&env_run_id, 50).await.expect("list events");
    let event_names: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert!(
        event_names.contains(&"finalization_failed_missing_terminal"),
        "must have finalization_failed_missing_terminal"
    );
    assert!(
        event_names.contains(&"runtime_completed"),
        "must have runtime_completed"
    );
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_streaming_proposal_ingest_success_event_sequence() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let fake_sse_events = vec![format!(
        "event: run.complete\ndata: {}\n\n",
        valid_proposals_body("ignored_id")
    )];

    let fake = start_fake_sidecar_sse(fake_sse_events).await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query/stream")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({
            "question": "add note to flight FL123",
            "context": {"selected_flight_id": "FL123"}
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let _ = actix_web::body::to_bytes(resp.into_body()).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let requests = fake.requests();
    let env_run_id = requests[0].body["run_id"].as_str().unwrap().to_string();

    let events = job_svc.list_events_for_run(&env_run_id, 50).await.expect("list events");
    let event_names: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert!(
        event_names.contains(&"proposal_ingest_started"),
        "must have proposal_ingest_started"
    );
    assert!(
        event_names.contains(&"proposal_ingest_succeeded"),
        "must have proposal_ingest_succeeded"
    );
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_streaming_proposal_ingest_failure_event_sequence() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let fake_sse_events = vec![format!(
        "event: run.complete\ndata: {}\n\n",
        invalid_proposals_body("ignored_id")
    )];

    let fake = start_fake_sidecar_sse(fake_sse_events).await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query/stream")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({"question": "invalid proposal"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let _ = actix_web::body::to_bytes(resp.into_body()).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let requests = fake.requests();
    let env_run_id = requests[0].body["run_id"].as_str().unwrap().to_string();

    let events = job_svc.list_events_for_run(&env_run_id, 50).await.expect("list events");
    let event_names: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert!(
        event_names.contains(&"proposal_ingest_started"),
        "must have proposal_ingest_started"
    );
    assert!(
        event_names.contains(&"proposal_ingest_failed"),
        "must have proposal_ingest_failed"
    );
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_non_streaming_valid_proposals_event_sequence() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let fake_body = valid_proposals_body("run_fake_ignored");
    let fake = start_fake_sidecar(200, fake_body).await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({
            "question": "add note to flight FL123",
            "context": {"selected_flight_id": "FL123"}
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);

    let run_id = body["data"]["run_id"].as_str().unwrap().to_string();

    let events = job_svc.list_events_for_run(&run_id, 50).await.expect("list events");
    let event_names: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert!(event_names.contains(&"runtime_started"), "must have runtime_started");
    assert!(
        event_names.contains(&"proposal_ingest_started"),
        "must have proposal_ingest_started"
    );
    assert!(
        event_names.contains(&"proposal_ingest_succeeded"),
        "must have proposal_ingest_succeeded"
    );
    assert!(
        event_names.contains(&"runtime_completed"),
        "must have runtime_completed"
    );
}

// =====================================================================
// P2.4-alpha tool streaming feature gate tests (non-DB)
// =====================================================================

#[actix_web::test]
async fn test_stream_with_tools_default_disabled_returns_non_success() {
    let _env_lock = tool_streaming_env_lock();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .configure(super::configure),
    )
    .await;

    // Ensure env var is NOT set
    let _guard = EnvGuard::remove("AI_RUNTIME_ENABLE_TOOL_STREAMING");

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query/stream-with-tools")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({"question": "test"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    // Route exists (not 404) but returns non-success (500 due to missing app_data or 503 from gate)
    assert_ne!(resp.status(), StatusCode::NOT_FOUND, "route must exist");
    assert_ne!(resp.status(), StatusCode::OK, "must not succeed when gate disabled");
}

#[actix_web::test]
async fn test_stream_with_tools_requires_authentication_when_disabled() {
    let _env_lock = tool_streaming_env_lock();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .configure(super::configure),
    )
    .await;

    let _guard = EnvGuard::remove("AI_RUNTIME_ENABLE_TOOL_STREAMING");

    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query/stream-with-tools")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// =====================================================================
// P2.4-alpha tool streaming enabled path DB-backed acceptance tests
// =====================================================================

#[actix_web::test]
async fn test_stream_with_tools_default_disabled_returns_503() {
    let _env_lock = tool_streaming_env_lock();
    let _guard = EnvGuard::remove("AI_RUNTIME_ENABLE_TOOL_STREAMING");

    // Build app with lazy dummy services. The gate returns 503 before any
    // DB access, so this non-ignored test must not require TEST_DATABASE_URL.
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgresql://postgres:postgres@127.0.0.1:1/fms_disabled_gate_test")
        .expect("lazy test db pool");
    let fake = start_fake_sidecar_sse_for_stream_with_tools(vec![]).await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query/stream-with-tools")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({"question": "test"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "must return 503 when feature gate disabled"
    );

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], false);
    assert_eq!(body["degraded"], true);
    assert_eq!(body["feature_gate"], "AI_RUNTIME_ENABLE_TOOL_STREAMING");

    // Must NOT have called the fake sidecar
    let requests = fake.requests();
    assert_eq!(
        requests.len(),
        0,
        "must not call fake sidecar when feature gate disabled"
    );
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_stream_with_tools_enabled_read_only_succeeds() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let _env_lock = tool_streaming_env_lock();
    let _guard = EnvGuard::set("AI_RUNTIME_ENABLE_TOOL_STREAMING", "1");

    let fake_sse_events = vec![
            "event: tool.call\ndata: {\"run_id\":\"ignored\",\"tool_call_id\":\"tc1\",\"tool_name\":\"flight_status_lookup\",\"arguments\":{\"flight_id\":\"CA1234\"}}\n\n".to_string(),
            "event: tool.result\ndata: {\"run_id\":\"ignored\",\"tool_call_id\":\"tc1\",\"tool_name\":\"flight_status_lookup\",\"result\":{\"flight_id\":\"CA1234\",\"status\":\"on_time\"}}\n\n".to_string(),
            "event: token\ndata: {\"delta\":\"hello\"}\n\n".to_string(),
            format!(
                "event: run.complete\ndata: {}\n\n",
                json!({
                    "contract_version": "ai-structured-output.v1",
                    "run_id": "ignored_id",
                    "status": "succeeded",
                    "answer": "Flight CA1234 is on time at gate A12.",
                    "reasoning_steps": [],
                    "evidence": [],
                    "proposals": [],
                    "limitations": [],
                    "metrics": {"model": "test", "duration_ms": 10}
                })
            ),
        ];

    let fake = start_fake_sidecar_sse_for_stream_with_tools(fake_sse_events).await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query/stream-with-tools")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({"question": "check flight CA1234 status"}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "must return 200 when enabled and successful"
    );

    let body_bytes = actix_web::body::to_bytes(resp.into_body()).await.unwrap();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert!(body_str.contains("event: token"), "must forward token events");
    assert!(body_str.contains("event: tool.call"), "must forward tool.call events");
    assert!(
        body_str.contains("event: tool.result"),
        "must forward tool.result events"
    );

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let requests = fake.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/internal/ai/v1/runs/stream-with-tools");
    assert!(request.has_service_identity);
    assert_stream_with_tools_service_identity_claims(request);

    let env_job_id = request.body["job_id"].as_str().unwrap().to_string();
    let env_run_id = request.body["run_id"].as_str().unwrap().to_string();

    let job = job_svc.get_job(&env_job_id).await.expect("job");
    assert_eq!(job.status, "succeeded");

    let run = job_svc.get_run(&env_run_id).await.expect("run");
    assert_eq!(run.status, "succeeded");

    let events = job_svc.list_events_for_run(&env_run_id, 50).await.expect("list events");
    let event_names: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert!(event_names.contains(&"runtime_started"), "must have runtime_started");
    assert!(
        event_names.contains(&"provider_stream_started"),
        "must have provider_stream_started"
    );
    assert!(
        event_names.contains(&"first_token_emitted"),
        "must have first_token_emitted"
    );
    assert!(
        event_names.contains(&"provider_stream_completed"),
        "must have provider_stream_completed"
    );
    assert!(
        event_names.contains(&"runtime_completed"),
        "must have runtime_completed"
    );
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_stream_with_tools_enabled_write_action_creates_proposal() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let _env_lock = tool_streaming_env_lock();
    let _guard = EnvGuard::set("AI_RUNTIME_ENABLE_TOOL_STREAMING", "1");

    let fake_sse_events = vec![
            "event: tool.call\ndata: {\"run_id\":\"ignored\",\"tool_call_id\":\"tc1\",\"tool_name\":\"add_flight_note\",\"arguments\":{\"flight_id\":\"CA1234\",\"note_content\":\"test note\"}}\n\n".to_string(),
            "event: tool.result\ndata: {\"run_id\":\"ignored\",\"tool_call_id\":\"tc1\",\"tool_name\":\"add_flight_note\",\"result\":{\"status\":\"proposal_created\"}}\n\n".to_string(),
            format!(
                "event: run.complete\ndata: {}\n\n",
                json!({
                    "contract_version": "ai-structured-output.v1",
                    "run_id": "ignored_id",
                    "status": "succeeded",
                    "answer": "I've created a proposal to add a note.",
                    "reasoning_steps": [],
                    "evidence": [],
                    "proposals": [{
                        "object_type": "Flight",
                        "object_id": "CA1234",
                        "action_name": "add_note",
                        "arguments": {"note_content": "test note"},
                        "risk_level": "low",
                        "confidence": 0.9,
                        "reasoning": "User requested",
                        "requires_approval": true
                    }],
                    "limitations": [],
                    "metrics": {"model": "test", "duration_ms": 10}
                })
            ),
        ];

    let fake = start_fake_sidecar_sse_for_stream_with_tools(fake_sse_events).await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query/stream-with-tools")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({"question": "add note to flight CA1234"}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    let _ = actix_web::body::to_bytes(resp.into_body()).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let requests = fake.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    let env_job_id = request.body["job_id"].as_str().unwrap().to_string();
    let env_run_id = request.body["run_id"].as_str().unwrap().to_string();

    let job = job_svc.get_job(&env_job_id).await.expect("job");
    assert_eq!(job.status, "succeeded");

    let run = job_svc.get_run(&env_run_id).await.expect("run");
    assert_eq!(run.status, "succeeded");

    use fms_domain::models::ai_proposal::ActionProposalQuery;

    let proposals = proposal_svc
        .list_proposals(&ActionProposalQuery {
            job_id: Some(env_job_id.clone()),
            run_id: Some(env_run_id.clone()),
            action_name: Some("add_note".to_string()),
            ..Default::default()
        })
        .await
        .expect("list proposals");
    assert!(
        proposals.len() >= 1,
        "at least one proposal must be created for run {}",
        env_run_id
    );
    let proposal = &proposals[0];
    assert_eq!(proposal.action_name, "add_note");
    assert_eq!(proposal.object_type, "Flight");
    assert_eq!(proposal.object_id, "CA1234");

    let events = job_svc.list_events_for_run(&env_run_id, 50).await.expect("list events");
    let event_names: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert!(
        event_names.contains(&"proposal_ingest_started"),
        "must have proposal_ingest_started"
    );
    assert!(
        event_names.contains(&"proposal_ingest_succeeded"),
        "must have proposal_ingest_succeeded"
    );
    assert!(
        event_names.contains(&"runtime_completed"),
        "must have runtime_completed"
    );
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_stream_with_tools_enabled_invalid_proposal_fails_run() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let _env_lock = tool_streaming_env_lock();
    let _guard = EnvGuard::set("AI_RUNTIME_ENABLE_TOOL_STREAMING", "1");

    let fake_sse_events = vec![format!(
        "event: run.complete\ndata: {}\n\n",
        json!({
            "contract_version": "ai-structured-output.v1",
            "run_id": "ignored_id",
            "status": "succeeded",
            "answer": "I attempted to cancel.",
            "reasoning_steps": [],
            "evidence": [],
            "proposals": [{
                "object_type": "Flight",
                "object_id": "CA1234",
                "action_name": "cancel",
                "arguments": {"reason": "weather"},
                "risk_level": "high",
                "confidence": 0.8,
                "reasoning": "Weather conditions",
                "requires_approval": true
            }],
            "limitations": [],
            "metrics": {"model": "test", "duration_ms": 10}
        })
    )];

    let fake = start_fake_sidecar_sse_for_stream_with_tools(fake_sse_events).await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query/stream-with-tools")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({"question": "cancel flight CA1234"}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    let body_bytes = actix_web::body::to_bytes(resp.into_body()).await.unwrap();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert!(
        body_str.contains("event: run.fail"),
        "response must contain run.fail for invalid proposal"
    );

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let requests = fake.requests();
    let env_job_id = requests[0].body["job_id"].as_str().unwrap().to_string();
    let env_run_id = requests[0].body["run_id"].as_str().unwrap().to_string();

    let job = job_svc.get_job(&env_job_id).await.expect("job");
    assert_eq!(job.status, "failed_terminal");

    let run = job_svc.get_run(&env_run_id).await.expect("run");
    assert_eq!(run.status, "failed_terminal");
    assert_eq!(run.error_code.as_deref(), Some("proposal_validation_failed"));

    let events = job_svc.list_events_for_run(&env_run_id, 50).await.expect("list events");
    let event_names: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert!(
        event_names.contains(&"proposal_ingest_started"),
        "must have proposal_ingest_started"
    );
    assert!(
        event_names.contains(&"proposal_ingest_failed"),
        "must have proposal_ingest_failed"
    );
    assert!(
        event_names.contains(&"runtime_completed"),
        "must have runtime_completed"
    );
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_stream_with_tools_enabled_transport_abort_fails_run() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let _env_lock = tool_streaming_env_lock();
    let _guard = EnvGuard::set("AI_RUNTIME_ENABLE_TOOL_STREAMING", "1");

    let fake_sse_events = vec![
        "event: token\ndata: {\"delta\":\"hello\"}\n\n".to_string(),
        "event: transport.abort\ndata: {\"message\":\"provider timeout\"}\n\n".to_string(),
    ];

    let fake = start_fake_sidecar_sse_for_stream_with_tools(fake_sse_events).await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query/stream-with-tools")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({"question": "transport abort test"}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    let body_bytes = actix_web::body::to_bytes(resp.into_body()).await.unwrap();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert!(
        body_str.contains("event: run.fail"),
        "response must contain run.fail for transport abort"
    );

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let requests = fake.requests();
    let env_job_id = requests[0].body["job_id"].as_str().unwrap().to_string();
    let env_run_id = requests[0].body["run_id"].as_str().unwrap().to_string();

    let job = job_svc.get_job(&env_job_id).await.expect("job");
    assert_eq!(job.status, "failed_terminal");

    let run = job_svc.get_run(&env_run_id).await.expect("run");
    assert_eq!(run.status, "failed_terminal");
    assert_eq!(run.error_code.as_deref(), Some("transport_error"));

    let events = job_svc.list_events_for_run(&env_run_id, 50).await.expect("list events");
    let event_names: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert!(
        event_names.contains(&"provider_stream_aborted"),
        "must have provider_stream_aborted"
    );
    assert!(
        event_names.contains(&"finalization_failed_transport_error"),
        "must have finalization_failed_transport_error"
    );
    assert!(
        event_names.contains(&"runtime_completed"),
        "must have runtime_completed"
    );
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_stream_with_tools_enabled_missing_terminal_fails_run() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let _env_lock = tool_streaming_env_lock();
    let _guard = EnvGuard::set("AI_RUNTIME_ENABLE_TOOL_STREAMING", "1");

    let fake_sse_events = vec![
            "event: tool.call\ndata: {\"run_id\":\"ignored\",\"tool_call_id\":\"tc1\",\"tool_name\":\"flight_status_lookup\",\"arguments\":{\"flight_id\":\"CA1234\"}}\n\n".to_string(),
            "event: tool.result\ndata: {\"run_id\":\"ignored\",\"tool_call_id\":\"tc1\",\"tool_name\":\"flight_status_lookup\",\"result\":{\"status\":\"ok\"}}\n\n".to_string(),
            "event: token\ndata: {\"delta\":\"partial\"}\n\n".to_string(),
        ];

    let fake = start_fake_sidecar_sse_for_stream_with_tools(fake_sse_events).await;
    let pool = create_pool().await;
    let (job_svc, context_svc, runtime_client, ingest_svc, _proposal_svc) = build_services(&pool, fake.url()).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(context_svc.clone()))
            .app_data(web::Data::new(runtime_client))
            .app_data(web::Data::new(ingest_svc))
            .configure(super::configure),
    )
    .await;

    let token = make_valid_jwt();
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/nl-query/stream-with-tools")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({"question": "missing terminal test"}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    let body_bytes = actix_web::body::to_bytes(resp.into_body()).await.unwrap();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert!(
        body_str.contains("event: run.fail"),
        "response must contain run.fail for missing terminal"
    );

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let requests = fake.requests();
    let env_job_id = requests[0].body["job_id"].as_str().unwrap().to_string();
    let env_run_id = requests[0].body["run_id"].as_str().unwrap().to_string();

    let job = job_svc.get_job(&env_job_id).await.expect("job");
    assert_eq!(job.status, "failed_terminal");

    let run = job_svc.get_run(&env_run_id).await.expect("run");
    assert_eq!(run.status, "failed_terminal");

    let events = job_svc.list_events_for_run(&env_run_id, 50).await.expect("list events");
    let event_names: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert!(
        event_names.contains(&"finalization_failed_missing_terminal"),
        "must have finalization_failed_missing_terminal"
    );
    assert!(
        event_names.contains(&"runtime_completed"),
        "must have runtime_completed"
    );
}

/// Opt-in live smoke: spawns real Python sidecar, exercises `/internal/ai/v1/runs`.
///
/// This test is `#[ignore]` like DB integration tests, but must **not** fail when
/// `cargo test -p fms-api nl_query -- --ignored` runs without live smoke enabled.
/// When `RUN_LIVE_AI_SIDECAR_SMOKE` is unset, it prints a skip message and returns OK.
///
/// DB-only ignored suite (requires `TEST_DATABASE_URL`, not live smoke):
///   cd backend
///   cargo test -p fms-api nl_query -- --ignored --nocapture
///
/// Explicit live smoke (requires `RUN_LIVE_AI_SIDECAR_SMOKE=1` and repo `.venv`):
///   set RUN_LIVE_AI_SIDECAR_SMOKE=1
///   cd backend && cargo test -p fms-api live_sidecar_smoke -- --ignored --nocapture
#[actix_web::test]
#[ignore = "opt-in live smoke; skipped at runtime unless RUN_LIVE_AI_SIDECAR_SMOKE=1"]
async fn test_live_sidecar_smoke_ai_runtime_contract() {
    if std::env::var("RUN_LIVE_AI_SIDECAR_SMOKE").unwrap_or_default() != "1" {
        eprintln!(
            "SKIP live_sidecar_smoke: RUN_LIVE_AI_SIDECAR_SMOKE is not set to 1. \
                 Default `cargo test -p fms-api nl_query -- --ignored` only runs DB tests. \
                 To run this opt-in smoke: set RUN_LIVE_AI_SIDECAR_SMOKE=1 and \
                 cargo test -p fms-api live_sidecar_smoke -- --ignored --nocapture"
        );
        return;
    }

    use std::net::TcpListener;
    use std::process::{Command, Stdio};

    // Find a free port
    let port = TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("local_addr")
        .port();

    let repo_root = repository_root();
    let python_exe = repo_root.join(".venv").join("Scripts").join("python.exe");
    let entrypoint = repo_root.join("scripts").join("host").join("ai_sidecar_entrypoint.py");

    let mut child = Command::new(&python_exe)
        .arg(&entrypoint)
        .env("API_HOST", "127.0.0.1")
        .env("API_PORT", port.to_string())
        .env("JWT_SECRET", "test-secret-for-live-smoke")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(&repo_root)
        .spawn()
        .expect("failed to spawn Python sidecar");

    let base_url = format!("http://127.0.0.1:{}", port);
    let client = reqwest::Client::new();

    // Wait for sidecar to be ready
    let mut ready = false;
    for _ in 0..50 {
        if let Ok(resp) = client
            .get(format!("{}/internal/ai/v1/health", base_url))
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
        {
            if resp.status().is_success() {
                ready = true;
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }
    assert!(ready, "Live sidecar did not start within 15s");

    // Build envelope
    let envelope = json!({
        "contract_version": "ai-runtime.v1",
        "job_id": "job_rust_live_smoke",
        "run_id": "run_rust_live_smoke",
        "correlation_id": "corr_rust_live",
        "requester": {"user_id": "user_1", "roles": ["ai:chat"]},
        "ontology": {
            "version": "flight-ops.v1",
            "allowed_object_types": ["Flight"],
            "allowed_actions": ["Flight.add_note"],
            "risk_ceiling": "medium",
        },
        "context": {
            "objects": [{
                "object_type": "Flight",
                "object_id": "FL123",
                "data": {"flight_number": "CA1234", "status": "scheduled"},
            }],
            "limits": {},
        },
        "task": {
            "task_type": "nl_query",
            "user_message": "给航班 CA1234 添加备注: Rust live smoke",
        },
    });

    // Call without service identity -> 401
    let resp = client
        .post(format!("{}/internal/ai/v1/runs", base_url))
        .json(&envelope)
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 401, "no identity should return 401");

    // Call with valid service identity -> 200 succeeded
    // We need to build a JWT the same way python_sidecar_proxy does
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize;
    let si_claims = json!({
        "iss": "fms-rust-api",
        "sub": "rust-api-gateway",
        "aud": "python-ai-runtime",
        "iat": now,
        "exp": now + 60,
        "path": "/internal/ai/v1/runs",
    });
    let si_token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &si_claims,
        &jsonwebtoken::EncodingKey::from_secret(b"test-secret-for-live-smoke"),
    )
    .expect("encode service identity");

    let resp = client
        .post(format!("{}/internal/ai/v1/runs", base_url))
        .header("X-Service-Identity", &si_token)
        .json(&envelope)
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 200, "valid identity should return 200");

    let body: Value = resp.json().await.expect("parse json");

    // AiStructuredOutput deserializable
    let structured: Result<AiStructuredOutput, _> = serde_json::from_value(body.clone());
    assert!(
        structured.is_ok(),
        "response must deserialize into AiStructuredOutput: {:?}",
        structured
    );

    let output = structured.unwrap();
    assert_eq!(output.run_id, "run_rust_live_smoke", "run_id must match envelope");
    assert_eq!(output.status, "succeeded", "degraded answer is still succeeded");

    // degraded=true + status=succeeded is NOT a business failure
    assert!(output.limitations.len() > 0, "no LLM key should produce limitations");
    assert_eq!(output.status, "succeeded", "degraded must not be treated as failed");

    // Cleanup
    let _ = child.kill();
    let _ = child.wait();
}
