use super::*;
use crate::middleware::jwt::JwtSecret;
use actix_web::{test, App};
use fms_domain::models::micro_model::MicroModelRegistry;
use serde_json::json;

/// RAII guard for temporarily setting an environment variable in tests.
struct EnvGuard {
    pub(crate) key: String,
    pub(crate) previous: Option<String>,
}

impl EnvGuard {
    fn set(key: &str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self {
            key: key.to_string(),
            previous,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(val) => std::env::set_var(&self.key, val),
            None => std::env::remove_var(&self.key),
        }
    }
}

fn test_registry() -> Arc<MicroModelRegistry> {
    Arc::new(MicroModelRegistry::with_default_models())
}

fn make_jwt(permissions: &[&str]) -> String {
    use chrono::Utc;
    use jsonwebtoken::{encode, EncodingKey, Header};
    let now = Utc::now().timestamp();
    let claims = json!({
        "sub": "test_user",
        "username": "tester",
        "permissions": permissions,
        "is_admin": false,
        "iat": now,
        "exp": now + 3600,
        "type": "access",
    });
    encode(&Header::default(), &claims, &EncodingKey::from_secret(b"test-secret")).expect("jwt encoding")
}

fn test_app(
    registry: Arc<MicroModelRegistry>,
) -> actix_web::App<
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
        .app_data(web::Data::new(registry))
        .configure(configure)
}

#[actix_web::test]
async fn micro_model_routes_require_authentication() {
    let registry = test_registry();
    let app = test::init_service(test_app(registry)).await;

    let req = test::TestRequest::get().uri("/api/v2/ai/micro-models").to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn list_models_returns_all_five_default_models() {
    let registry = test_registry();
    let app = test::init_service(test_app(registry)).await;
    let token = make_jwt(&["ai:view"]);

    let req = test::TestRequest::get()
        .uri("/api/v2/ai/micro-models")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);

    let data = body["data"].as_array().expect("data should be array");
    assert_eq!(data.len(), 5, "should have 5 default models");

    let model_ids: Vec<&str> = data.iter().filter_map(|m| m["model_id"].as_str()).collect();
    assert!(model_ids.contains(&"flight_risk_v1"));
    assert!(model_ids.contains(&"dispatch_replan_v1"));
    assert!(model_ids.contains(&"stand_conflict_v1"));
    assert!(model_ids.contains(&"anomaly_triage_v1"));
    assert!(model_ids.contains(&"ops_briefing_v1"));

    // Each model should have enabled and feature_flag fields
    for model in data {
        assert!(model.get("enabled").is_some(), "model should have enabled field");
        assert!(
            model.get("feature_flag").is_some(),
            "model should have feature_flag field"
        );
        assert!(
            model.get("evaluation_dataset_id").is_some(),
            "model should have evaluation_dataset_id field"
        );
    }
}

#[actix_web::test]
async fn get_model_exposes_feature_flag_and_schemas() {
    let registry = test_registry();
    let app = test::init_service(test_app(registry)).await;
    let token = make_jwt(&["ai:view"]);

    let req = test::TestRequest::get()
        .uri("/api/v2/ai/micro-models/flight_risk_v1")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    let data = &body["data"];

    assert_eq!(data["model_id"], "flight_risk_v1");
    assert_eq!(data["feature_flag"], "FMS_AI_MICROMODEL_FLIGHT_RISK_ENABLED");
    assert_eq!(data["evaluation_dataset_id"], "eval_flight_risk_v1_baseline");
    assert_eq!(data["advisory_output"], true);

    // Should have proper JSON schemas
    let input_schema = &data["input_schema"];
    assert_eq!(input_schema["type"], "object");
    assert!(input_schema["required"].as_array().is_some());
    assert!(input_schema["properties"]["flight_id"]["type"].as_str().is_some());

    let output_schema = &data["output_schema"];
    assert_eq!(output_schema["type"], "object");
    assert!(output_schema["properties"]["risk_score"]["type"].as_str().is_some());
}

#[actix_web::test]
async fn execute_model_forbidden_when_feature_flag_disabled() {
    // Ensure the flag is explicitly disabled
    let _guard = EnvGuard::set("FMS_AI_MICROMODEL_FLIGHT_RISK_ENABLED", "false");

    let registry = test_registry();
    let app = test::init_service(test_app(registry)).await;
    let token = make_jwt(&["ai:execute"]);

    let req = test::TestRequest::post()
        .uri("/api/v2/ai/micro-models/flight_risk_v1/execute")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({
            "input": {"flight_id": "FL001"},
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403, "should be 403 when feature flag is disabled");

    let body: Value = test::read_body_json(resp).await;
    let error_msg = body["error"]["message"].as_str().unwrap_or("");
    assert!(
        error_msg.contains("FMS_AI_MICROMODEL_FLIGHT_RISK_ENABLED"),
        "error should mention the feature flag name, got: {}",
        error_msg,
    );
}

#[actix_web::test]
async fn execute_dispatch_replan_returns_typed_advisory_output_when_enabled() {
    let _guard = EnvGuard::set("FMS_AI_MICROMODEL_DISPATCH_REPLAN_ENABLED", "true");

    let registry = test_registry();
    let app = test::init_service(test_app(registry)).await;
    let token = make_jwt(&["ai:execute"]);

    let req = test::TestRequest::post()
        .uri("/api/v2/ai/micro-models/dispatch_replan_v1/execute")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({
            "input": {
                "shift_id": "shift_A01",
                "target_time_window": {
                    "start": "2026-06-04T08:00:00Z",
                    "end": "2026-06-04T16:00:00Z"
                },
                "dispatch_order_ids": ["order_1", "order_2"],
                "optimization_objective": "MinimizeDelay",
            },
            "generate_proposals": true,
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    let data = &body["data"];

    assert_eq!(data["status"], "success");
    assert_eq!(data["model_id"], "dispatch_replan_v1");
    assert!(data["model_version"].as_str().is_some());
    assert!(data["execution_time_ms"].as_u64().is_some());

    // Output should be typed (not hardcoded)
    assert_eq!(data["output"]["model_id"], "dispatch_replan_v1");
    assert_eq!(data["output"]["shift_id"], "shift_A01");
    assert!(data["output"]["replan_recommended"].as_bool().is_some());

    // Advisory proposal candidates, NOT canonical proposals
    let candidates = data["proposal_candidates"]
        .as_array()
        .expect("should have proposal_candidates");
    assert!(
        !candidates.is_empty(),
        "should generate advisory candidates when generate_proposals=true"
    );

    let canonical = data["canonical_proposals_created"]
        .as_array()
        .expect("should have canonical_proposals_created");
    assert!(
        canonical.is_empty(),
        "canonical proposals should be empty (not going through ingest)"
    );
}

#[actix_web::test]
async fn execute_stand_conflict_returns_candidate_not_canonical_proposal() {
    let _guard = EnvGuard::set("FMS_AI_MICROMODEL_STAND_CONFLICT_ENABLED", "true");

    let registry = test_registry();
    let app = test::init_service(test_app(registry)).await;
    let token = make_jwt(&["ai:execute"]);

    let req = test::TestRequest::post()
        .uri("/api/v2/ai/micro-models/stand_conflict_v1/execute")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({
            "input": {
                "flight_id": "FL123",
                "current_stand_id": "S01",
                "conflict_flight_id": "FL456",
                "conflict_window_minutes": 20,
            },
            "generate_proposals": true,
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    let data = &body["data"];

    assert_eq!(data["status"], "success");
    assert!(data["output"]["conflict_detected"].as_bool().unwrap_or(false));

    // Has advisory candidates
    let candidates = data["proposal_candidates"]
        .as_array()
        .expect("should have proposal_candidates");
    assert!(!candidates.is_empty());

    // Candidates should NOT have proposal_id (they're not canonical)
    for candidate in candidates {
        assert!(
            candidate.get("proposal_id").is_none() || candidate["proposal_id"].is_null(),
            "advisory candidates should not have proposal_id"
        );
    }

    // No canonical proposals created
    let canonical = data["canonical_proposals_created"].as_array().unwrap();
    assert!(canonical.is_empty());
}

#[actix_web::test]
async fn execute_anomaly_triage_validates_input() {
    let _guard = EnvGuard::set("FMS_AI_MICROMODEL_ANOMALY_TRIAGE_ENABLED", "true");

    let registry = test_registry();
    let app = test::init_service(test_app(registry)).await;
    let token = make_jwt(&["ai:execute"]);

    // Send invalid input (missing required fields)
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/micro-models/anomaly_triage_v1/execute")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({
            "input": {"wrong_field": "value"},
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400, "should return 400 for invalid input");
}

#[actix_web::test]
async fn execute_ops_briefing_includes_input_snapshot_when_requested() {
    let _guard = EnvGuard::set("FMS_AI_MICROMODEL_OPS_BRIEFING_ENABLED", "true");

    let registry = test_registry();
    let app = test::init_service(test_app(registry)).await;
    let token = make_jwt(&["ai:execute"]);

    let input_payload = json!({
        "shift_id": "shift_B01",
        "include_flight_ids": ["FL100", "FL200"],
        "focus_areas": ["turnaround"],
    });

    // With include_input_snapshot = true
    let req = test::TestRequest::post()
        .uri("/api/v2/ai/micro-models/ops_briefing_v1/execute")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({
            "input": input_payload.clone(),
            "include_input_snapshot": true,
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    let data = &body["data"];

    assert_eq!(data["status"], "success");
    assert!(
        data["input_snapshot"].is_object(),
        "should include input_snapshot when requested"
    );
    assert_eq!(data["input_snapshot"]["shift_id"], "shift_B01");

    // Without include_input_snapshot — should not have snapshot
    let req2 = test::TestRequest::post()
        .uri("/api/v2/ai/micro-models/ops_briefing_v1/execute")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({
            "input": input_payload,
            "include_input_snapshot": false,
        }))
        .to_request();

    let resp2 = test::call_service(&app, req2).await;
    assert_eq!(resp2.status(), 200);

    let body2: Value = test::read_body_json(resp2).await;
    let data2 = &body2["data"];
    assert!(
        data2.get("input_snapshot").is_none() || data2["input_snapshot"].is_null(),
        "should not include input_snapshot when not requested"
    );
}
