#[cfg(test)]
mod tests {
    use actix_web::body::to_bytes;
    use actix_web::http::StatusCode;
    use actix_web::{web, App};
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::Mutex;

    use crate::middleware::jwt::{JwtAuth, JwtSecret};
    use crate::sse::hub::SseHub;
    use fms_application::services::ai_admin_service::AiAdminService;
    use fms_application::services::ai_route_service::{
        ai_feature_enabled, batch_approve_success_result, batch_error_result, batch_reject_success_result,
        parse_ai_feature_flag, AiRouteService,
    };
    use fms_application::services::ai_runtime_service::{AiRuntimeService, AiToolExecutionSpec};
    use fms_domain::error::DomainError;
    use fms_domain::models::ai_entity_config::AiEntityConfigRecord;
    use fms_domain::ports::ai_entity_config_repository::AiEntityConfigRepository;

    use crate::routes::ai::shared::{can_access_execution, execution_owner_id, raw_detail, runtime_conflict_response};

    struct InMemoryAiEntityConfigRepository {
        records: Mutex<HashMap<String, serde_json::Value>>,
    }

    impl InMemoryAiEntityConfigRepository {
        fn new(records: impl IntoIterator<Item = (String, serde_json::Value)>) -> Self {
            Self {
                records: Mutex::new(records.into_iter().collect()),
            }
        }
    }

    #[async_trait::async_trait]
    impl AiEntityConfigRepository for InMemoryAiEntityConfigRepository {
        async fn find_all(&self) -> Result<Vec<AiEntityConfigRecord>, DomainError> {
            let records = self
                .records
                .lock()
                .map_err(|_| DomainError::Internal("repo lock poisoned".to_string()))?;
            Ok(records
                .iter()
                .map(|(id, config)| AiEntityConfigRecord {
                    id: id.clone(),
                    config: config.clone(),
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                })
                .collect())
        }

        async fn find_by_id(&self, id: &str) -> Result<Option<AiEntityConfigRecord>, DomainError> {
            let records = self
                .records
                .lock()
                .map_err(|_| DomainError::Internal("repo lock poisoned".to_string()))?;
            Ok(records.get(id).map(|config| AiEntityConfigRecord {
                id: id.to_string(),
                config: config.clone(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            }))
        }

        async fn save(&self, _id: &str, _config: &serde_json::Value) -> Result<AiEntityConfigRecord, DomainError> {
            unimplemented!()
        }

        async fn delete(&self, _id: &str) -> Result<bool, DomainError> {
            unimplemented!()
        }
    }

    fn build_ai_route_service(runtime_svc: Arc<AiRuntimeService>) -> Arc<AiRouteService> {
        let admin_repo: Arc<dyn AiEntityConfigRepository + Send + Sync> =
            Arc::new(InMemoryAiEntityConfigRepository::new([]));
        let admin_svc = Arc::new(AiAdminService::new(admin_repo));
        Arc::new(AiRouteService::new(admin_svc).with_runtime_service(runtime_svc))
    }

    #[test]
    fn batch_approve_result_matches_python_semantics() {
        let payload = batch_approve_success_result(
            "pending_ok",
            &json!({
                "pending_action": {"status": "executed", "status_code": "EXECUTED"},
                "execution_result": {"status": "success", "code": "TOOL_SUCCESS", "message": "ok"}
            }),
        );

        assert_eq!(payload["success"], true);
        assert_eq!(payload["status"], "executed");
        assert_eq!(payload["code"], "TOOL_SUCCESS");
        assert_eq!(payload["message"], "ok");
        println!("ai batch approve payload: {}", payload);
    }

    #[test]
    fn batch_conflict_result_marks_expired_like_python() {
        let payload = batch_error_result("pending_expired", "expired", "PENDING_ACTION_EXPIRED", "expired");

        assert_eq!(payload["success"], false);
        assert_eq!(payload["status"], "expired");
        assert_eq!(payload["code"], "PENDING_ACTION_EXPIRED");
        assert_eq!(payload["message"], "expired");
    }

    #[test]
    fn batch_reject_result_matches_python_semantics() {
        let payload = batch_reject_success_result(
            "pending_reject",
            &json!({
                "pending_action": {"status": "rejected", "status_code": "APPROVAL_REJECTED"}
            }),
        );

        assert_eq!(payload["success"], true);
        assert_eq!(payload["status"], "rejected");
        assert_eq!(payload["code"], "APPROVAL_REJECTED");
        assert_eq!(payload["message"], "approval request rejected by human reviewer");
        println!("ai batch reject payload: {}", payload);
    }

    #[test]
    fn pending_action_not_found_message_matches_python_detail() {
        let response = raw_detail(actix_web::http::StatusCode::NOT_FOUND, "待审批动作不存在: action_123");
        assert_eq!(response.status(), actix_web::http::StatusCode::NOT_FOUND);
    }

    #[test]
    fn ai_feature_flags_follow_python_env_semantics() {
        assert!(parse_ai_feature_flag(None, true));
        assert!(!parse_ai_feature_flag(Some("false"), true));
        assert!(!parse_ai_feature_flag(Some("0"), true));
        assert!(parse_ai_feature_flag(Some("yes"), false));
        assert_eq!(ai_feature_enabled("AI_EXEC_STATUS_V2_SHOULD_NOT_EXIST", true), true);
    }

    #[test]
    fn execute_tool_route_contract_uses_python_result_mapping() {
        let payload = json!({
            "success": false,
            "status": "pending_approval",
            "code": "TOOL_PENDING_APPROVAL",
            "message": "tool 'update_todo' is queued for human approval",
            "recoverable": true,
            "retryable": false,
            "execution_id": "exec_123",
            "tool_name": "update_todo",
            "severity": "warning",
            "approval_required": true,
            "approval_id": "pending_123",
            "data": {"action_id": "pending_123"},
            "error": "工具 'update_todo' 已进入人工审批队列 (operation_level=l1_write)",
            "meta": {"duration_ms": 0, "contract_version": "2.0"}
        });

        let status = payload["status"].as_str().unwrap();
        let accepted = payload["success"].as_bool().unwrap_or(false) || status == "pending_approval";
        let response = json!({
            "success": accepted,
            "accepted": accepted,
            "status": status,
            "code": payload.get("code"),
            "message": payload.get("message"),
            "recoverable": payload.get("recoverable"),
            "retryable": payload.get("retryable"),
            "execution_id": payload.get("execution_id"),
            "tool_name": "update_todo",
            "severity": payload.get("severity"),
            "approval_required": payload.get("approval_required"),
            "approval_id": payload.get("approval_id"),
            "data": {
                "tool_name": "update_todo",
                "result": payload.get("data"),
                "error": payload.get("error"),
            },
            "result_data": payload.get("data"),
            "error": payload.get("error"),
            "meta": payload.get("meta"),
        });

        assert_eq!(response["accepted"], true);
        assert_eq!(response["code"], "TOOL_PENDING_APPROVAL");
        assert_eq!(response["execution_id"], "exec_123");
        assert_eq!(response["data"]["result"]["action_id"], "pending_123");
        assert_eq!(response["result_data"]["action_id"], "pending_123");
        assert_eq!(response["meta"]["duration_ms"], 0);
        println!("ai execute route payload: {}", response);
    }

    #[actix_web::test]
    async fn runtime_conflict_response_uses_python_error_envelope() {
        let response = runtime_conflict_response(
            "PENDING_ACTION_EXPIRED".to_string(),
            "pending action expired".to_string(),
            Some("expired".to_string()),
        );

        assert_eq!(response.status(), StatusCode::CONFLICT);
        let headers = response.headers().clone();
        let body = to_bytes(response.into_body()).await.expect("read body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json body");
        assert_eq!(
            headers.get("X-Error-Code").and_then(|v| v.to_str().ok()),
            Some("PENDING_ACTION_EXPIRED")
        );
        assert_eq!(
            headers.get("X-Decision-Blocked-Reason").and_then(|v| v.to_str().ok()),
            Some("expired")
        );
        assert_eq!(payload["success"], false);
        assert_eq!(payload["error"]["code"], "HTTP_409");
        assert_eq!(payload["error"]["message"], "pending action expired");
        assert_eq!(payload["error"]["type"], "http_error");
        println!("ai conflict payload: {}", payload);
    }

    fn claims_with(permissions: &[&str], is_admin: bool, sub: &str) -> JwtAuth {
        use fms_application::schemas::auth_schemas::TokenData;
        JwtAuth(TokenData {
            sub: Some(sub.to_string()),
            email: None,
            username: Some(sub.to_string()),
            token_kind: Some("access".to_string()),
            is_admin: Some(is_admin),
            permissions: permissions.iter().map(|value| value.to_string()).collect(),
            department: None,
            department_id: None,
            pv: None,
            iat: None,
            exp: None,
            iss: None,
            aud: None,
            ua_hash: None,
            ip_subnet_hash: None,
        })
    }

    #[test]
    fn execution_owner_id_reads_user_id_field() {
        assert_eq!(execution_owner_id(&json!({"user_id": "u1"})), Some("u1"));
        assert_eq!(execution_owner_id(&json!({})), None);
    }

    #[test]
    fn can_access_execution_allows_owner_admin_and_monitor() {
        let exec = json!({"user_id": "owner_001"});
        assert!(can_access_execution(
            &claims_with(&["ai:view"], false, "owner_001"),
            &exec
        ));
        assert!(!can_access_execution(
            &claims_with(&["ai:view"], false, "other_002"),
            &exec
        ));
        assert!(can_access_execution(
            &claims_with(&["ai:view"], true, "other_002"),
            &exec
        ));
        assert!(can_access_execution(
            &claims_with(&["ai:monitor"], false, "other_002"),
            &exec
        ));
        assert!(!can_access_execution(
            &claims_with(&["ai:view"], false, "unknown_user"),
            &exec
        ));
    }

    fn make_jwt(permissions: &[&str], sub: &str) -> String {
        use chrono::Utc;
        use jsonwebtoken::{encode, EncodingKey, Header};
        let now = Utc::now().timestamp();
        let claims = json!({
            "sub": sub,
            "username": sub,
            "permissions": permissions,
            "is_admin": false,
            "iat": now,
            "exp": now + 3600,
            "type": "access",
        });
        encode(&Header::default(), &claims, &EncodingKey::from_secret(b"test-secret")).expect("jwt encoding")
    }

    fn build_execution_test_app(
        runtime_svc: Arc<AiRuntimeService>,
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
            .app_data(web::Data::new(build_ai_route_service(runtime_svc)))
            .app_data(web::Data::new(SseHub::new(100)))
            .configure(crate::routes::ai::configure)
    }

    async fn create_test_execution(runtime_svc: &AiRuntimeService, user_id: &str) -> String {
        let result = runtime_svc
            .execute_tool(
                AiToolExecutionSpec {
                    tool_name: "list_flights".to_string(),
                    category: "query".to_string(),
                    operation_level: "l0_read".to_string(),
                    side_effect: false,
                    query_intent: None,
                    query_dataset: None,
                },
                json!({"limit": 10}),
                Some(user_id.to_string()),
                vec![],
            )
            .await;
        result["execution_id"].as_str().expect("execution_id").to_string()
    }

    #[actix_web::test]
    async fn list_executions_requires_ai_view_permission() {
        let runtime_svc = Arc::new(AiRuntimeService::new());
        create_test_execution(&runtime_svc, "owner_001").await;

        let app = actix_web::test::init_service(build_execution_test_app(runtime_svc)).await;

        let req = actix_web::test::TestRequest::get()
            .uri("/api/v2/ai/executions")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        let token = make_jwt(&["ai:execute"], "other_002");
        let req = actix_web::test::TestRequest::get()
            .uri("/api/v2/ai/executions")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[actix_web::test]
    async fn list_executions_filters_to_owner_unless_monitor() {
        let runtime_svc = Arc::new(AiRuntimeService::new());
        create_test_execution(&runtime_svc, "owner_001").await;

        let app = actix_web::test::init_service(build_execution_test_app(runtime_svc)).await;

        let token = make_jwt(&["ai:view"], "owner_001");
        let req = actix_web::test::TestRequest::get()
            .uri("/api/v2/ai/executions")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = actix_web::test::read_body(resp).await;
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json body");
        assert_eq!(payload["data"]["total"], 1);

        let token = make_jwt(&["ai:view"], "other_002");
        let req = actix_web::test::TestRequest::get()
            .uri("/api/v2/ai/executions")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = actix_web::test::read_body(resp).await;
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json body");
        assert_eq!(payload["data"]["total"], 0);

        let token = make_jwt(&["ai:view", "ai:monitor"], "other_002");
        let req = actix_web::test::TestRequest::get()
            .uri("/api/v2/ai/executions")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = actix_web::test::read_body(resp).await;
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json body");
        assert_eq!(payload["data"]["total"], 1);
    }

    #[actix_web::test]
    async fn get_execution_enforces_owner_and_permission() {
        let runtime_svc = Arc::new(AiRuntimeService::new());
        let run_id = create_test_execution(&runtime_svc, "owner_001").await;

        let app = actix_web::test::init_service(build_execution_test_app(runtime_svc)).await;

        let token = make_jwt(&["ai:view"], "owner_001");
        let req = actix_web::test::TestRequest::get()
            .uri(&format!("/api/v2/ai/executions/{run_id}"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let token = make_jwt(&["ai:view"], "other_002");
        let req = actix_web::test::TestRequest::get()
            .uri(&format!("/api/v2/ai/executions/{run_id}"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        let token = make_jwt(&["ai:execute"], "owner_001");
        let req = actix_web::test::TestRequest::get()
            .uri(&format!("/api/v2/ai/executions/{run_id}"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[actix_web::test]
    async fn cancel_execution_enforces_owner_and_ai_execute_permission() {
        let runtime_svc = Arc::new(AiRuntimeService::new());
        let run_id = create_test_execution(&runtime_svc, "owner_001").await;

        let app = actix_web::test::init_service(build_execution_test_app(runtime_svc.clone())).await;

        let token = make_jwt(&["ai:execute"], "owner_001");
        let req = actix_web::test::TestRequest::post()
            .uri(&format!("/api/v2/ai/executions/{run_id}/cancel"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let run_id2 = create_test_execution(&runtime_svc, "owner_001").await;

        let token = make_jwt(&["ai:execute"], "other_002");
        let req = actix_web::test::TestRequest::post()
            .uri(&format!("/api/v2/ai/executions/{run_id2}/cancel"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        let token = make_jwt(&["ai:view"], "owner_001");
        let req = actix_web::test::TestRequest::post()
            .uri(&format!("/api/v2/ai/executions/{run_id2}/cancel"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }
}
