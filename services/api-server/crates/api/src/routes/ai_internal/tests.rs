use super::*;
use crate::middleware::jwt::JwtSecret;
use crate::middleware::service_identity::ServiceIdentityClaims;
use actix_web::{test, web, App};
use fms_application::services::ai_action_proposal_service::AiActionProposalService;
use fms_application::services::ai_context_service::AiContextService;
use fms_application::services::ai_job_service::AiJobService;
use fms_application::services::ai_output_validator::AiOutputValidator;
use fms_application::services::ai_proposal_ingest_service::AiProposalIngestService;
use fms_application::services::authorization_service::AuthorizationService;
use fms_application::services::flight_service::FlightService;
use serde_json::json;

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

fn has_pool() -> bool {
    std::env::var("TEST_DATABASE_URL").is_ok()
}

async fn create_pool() -> sqlx::PgPool {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    sqlx::PgPool::connect(&url).await.expect("test db")
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

async fn create_test_job_and_run(job_service: &Arc<AiJobService>, initial_status: &str) -> (String, String) {
    let job = job_service
        .create_job("test", Some("test_user"), None, None, None)
        .await
        .unwrap();

    let mut envelope = fms_domain::models::ai_context_envelope::ContextEnvelope {
        contract_version: "ai-runtime.v1".to_string(),
        job_id: job.job_id.clone(),
        run_id: "".to_string(),
        correlation_id: "test-correlation".to_string(),
        requester: fms_domain::models::ai_context_envelope::EnvelopeRequester {
            user_id: "test_user".to_string(),
            roles: vec![],
            department_id: None,
            permission_version: None,
        },
        ontology: fms_domain::models::ai_context_envelope::EnvelopeOntology {
            version: "flight-ops.v1".to_string(),
            allowed_object_types: vec![],
            allowed_actions: vec![],
            risk_ceiling: "medium".to_string(),
        },
        context: fms_domain::models::ai_context_envelope::EnvelopeContext {
            objects: vec![],
            relations: vec![],
            evidence: vec![],
            limits: fms_domain::models::ai_context_envelope::EnvelopeLimits {
                max_objects: 100,
                max_tokens: 12000,
                redaction: "standard".to_string(),
            },
        },
        task: fms_domain::models::ai_context_envelope::EnvelopeTask {
            task_type: "test".to_string(),
            user_message: "test question".to_string(),
        },
    };

    let run = job_service.create_run(&job.job_id, "test", None, None).await.unwrap();
    envelope.run_id = run.run_id.clone();
    let envelope_value = serde_json::to_value(&envelope).unwrap();
    job_service
        .update_run_input_envelope(&run.run_id, envelope_value)
        .await
        .unwrap();

    // Transition through valid states: pending -> claimed -> running -> target status
    job_service
        .transition_run(&run.run_id, fms_domain::models::ai_job::AiRunStatus::Claimed)
        .await
        .unwrap();
    job_service
        .transition_run(&run.run_id, fms_domain::models::ai_job::AiRunStatus::Running)
        .await
        .unwrap();

    // Transition to initial status
    match initial_status {
        "succeeded" => {
            job_service.complete_run(&run.run_id, None, None, None).await.unwrap();
        }
        "failed_terminal" => {
            job_service
                .fail_run(&run.run_id, None, Some("test error"), None)
                .await
                .unwrap();
        }
        "cancelled" => {
            job_service
                .transition_run(&run.run_id, fms_domain::models::ai_job::AiRunStatus::Cancelled)
                .await
                .unwrap();
        }
        _ => {}
    }

    (job.job_id, run.run_id)
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_complete_run_on_succeeded_returns_409() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let pool = create_pool().await;
    let job_service = ai_job_service_for_pool(&pool);
    let flight_repo = Arc::new(fms_infrastructure::repositories::pg_flight_repository::PgFlightRepository::new(pool));
    let flight_service = Arc::new(FlightService::new(flight_repo));
    let auth_service = Arc::new(AuthorizationService);
    let context_service = Arc::new(AiContextService::new(flight_service, auth_service));
    let validator = Arc::new(AiOutputValidator::new(
        fms_domain::ontology::flight_ops_v1::build_flight_ops_v1_schema(),
    ));
    let proposal_service = Arc::new(AiActionProposalService::new());
    let ingest_service = Arc::new(AiProposalIngestService::new(
        validator,
        proposal_service,
        job_service.clone(),
    ));

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_service.clone()))
            .app_data(web::Data::new(context_service.clone()))
            .app_data(web::Data::new(ingest_service))
            .configure(super::configure),
    )
    .await;

    let (_job_id, run_id) = create_test_job_and_run(&job_service, "succeeded").await;
    let path = format!("/internal/ai/v1/runs/{}/complete", run_id);
    let token = create_service_identity_token("test-secret", &path);

    let req = test::TestRequest::post()
        .uri(&path)
        .insert_header(("X-Service-Identity", token))
        .set_json(json!({}))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 409);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], false);
    assert_eq!(body["data"]["status"], "succeeded");
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_complete_run_on_failed_terminal_returns_409() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let pool = create_pool().await;
    let job_service = ai_job_service_for_pool(&pool);
    let flight_repo = Arc::new(fms_infrastructure::repositories::pg_flight_repository::PgFlightRepository::new(pool));
    let flight_service = Arc::new(FlightService::new(flight_repo));
    let auth_service = Arc::new(AuthorizationService);
    let context_service = Arc::new(AiContextService::new(flight_service, auth_service));
    let validator = Arc::new(AiOutputValidator::new(
        fms_domain::ontology::flight_ops_v1::build_flight_ops_v1_schema(),
    ));
    let proposal_service = Arc::new(AiActionProposalService::new());
    let ingest_service = Arc::new(AiProposalIngestService::new(
        validator,
        proposal_service,
        job_service.clone(),
    ));

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_service.clone()))
            .app_data(web::Data::new(context_service.clone()))
            .app_data(web::Data::new(ingest_service))
            .configure(super::configure),
    )
    .await;

    let (_job_id, run_id) = create_test_job_and_run(&job_service, "failed_terminal").await;
    let path = format!("/internal/ai/v1/runs/{}/complete", run_id);
    let token = create_service_identity_token("test-secret", &path);

    let req = test::TestRequest::post()
        .uri(&path)
        .insert_header(("X-Service-Identity", token))
        .set_json(json!({}))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 409);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], false);
    assert_eq!(body["data"]["status"], "failed_terminal");
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_complete_run_on_cancelled_returns_409() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let pool = create_pool().await;
    let job_service = ai_job_service_for_pool(&pool);
    let flight_repo = Arc::new(fms_infrastructure::repositories::pg_flight_repository::PgFlightRepository::new(pool));
    let flight_service = Arc::new(FlightService::new(flight_repo));
    let auth_service = Arc::new(AuthorizationService);
    let context_service = Arc::new(AiContextService::new(flight_service, auth_service));
    let validator = Arc::new(AiOutputValidator::new(
        fms_domain::ontology::flight_ops_v1::build_flight_ops_v1_schema(),
    ));
    let proposal_service = Arc::new(AiActionProposalService::new());
    let ingest_service = Arc::new(AiProposalIngestService::new(
        validator,
        proposal_service,
        job_service.clone(),
    ));

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_service.clone()))
            .app_data(web::Data::new(context_service.clone()))
            .app_data(web::Data::new(ingest_service))
            .configure(super::configure),
    )
    .await;

    let (_job_id, run_id) = create_test_job_and_run(&job_service, "cancelled").await;
    let path = format!("/internal/ai/v1/runs/{}/complete", run_id);
    let token = create_service_identity_token("test-secret", &path);

    let req = test::TestRequest::post()
        .uri(&path)
        .insert_header(("X-Service-Identity", token))
        .set_json(json!({}))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 409);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], false);
    assert_eq!(body["data"]["status"], "cancelled");
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_fail_run_on_succeeded_returns_409() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let pool = create_pool().await;
    let job_service = ai_job_service_for_pool(&pool);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_service.clone()))
            .configure(super::configure),
    )
    .await;

    let (_job_id, run_id) = create_test_job_and_run(&job_service, "succeeded").await;
    let path = format!("/internal/ai/v1/runs/{}/fail", run_id);
    let token = create_service_identity_token("test-secret", &path);

    let req = test::TestRequest::post()
        .uri(&path)
        .insert_header(("X-Service-Identity", token))
        .set_json(json!({}))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 409);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], false);
    assert_eq!(body["data"]["status"], "succeeded");
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
async fn test_fail_run_on_failed_terminal_returns_409() {
    if !has_pool() {
        panic!("TEST_DATABASE_URL not set");
    }

    let pool = create_pool().await;
    let job_service = ai_job_service_for_pool(&pool);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(job_service.clone()))
            .configure(super::configure),
    )
    .await;

    let (_job_id, run_id) = create_test_job_and_run(&job_service, "failed_terminal").await;
    let path = format!("/internal/ai/v1/runs/{}/fail", run_id);
    let token = create_service_identity_token("test-secret", &path);

    let req = test::TestRequest::post()
        .uri(&path)
        .insert_header(("X-Service-Identity", token))
        .set_json(json!({}))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 409);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], false);
    assert_eq!(body["data"]["status"], "failed_terminal");
}

mod pure_function_tests {
    use super::is_run_terminal;

    #[test]
    fn is_run_terminal_returns_true_for_succeeded() {
        assert!(is_run_terminal("succeeded"));
    }

    #[test]
    fn is_run_terminal_returns_true_for_failed_terminal() {
        assert!(is_run_terminal("failed_terminal"));
    }

    #[test]
    fn is_run_terminal_returns_true_for_cancelled() {
        assert!(is_run_terminal("cancelled"));
    }

    #[test]
    fn is_run_terminal_returns_false_for_running() {
        assert!(!is_run_terminal("running"));
    }

    #[test]
    fn is_run_terminal_returns_false_for_pending() {
        assert!(!is_run_terminal("pending"));
    }

    #[test]
    fn is_run_terminal_returns_false_for_claimed() {
        assert!(!is_run_terminal("claimed"));
    }

    #[test]
    fn is_run_terminal_returns_false_for_timed_out() {
        assert!(!is_run_terminal("timed_out"));
    }
}
