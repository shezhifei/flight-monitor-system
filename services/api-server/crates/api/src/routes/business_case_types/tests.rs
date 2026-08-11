use super::configure;
use crate::middleware::jwt::JwtSecret;
use actix_web::{body::to_bytes, http::StatusCode, test, web, App};
use chrono::Utc;
use fms_application::schemas::auth_schemas::TokenData;
use fms_application::services::authorization_service::PermissionCatalog;
use fms_application::services::business_case_type_service::BusinessCaseTypeService;
use fms_domain::error::DomainError;
use fms_domain::models::business_case::{BusinessCaseType, VisibilityScope};
use fms_domain::ports::business_case_repository::BusinessCaseTypeRepository;
use jsonwebtoken::{encode, EncodingKey, Header};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct FakeBusinessCaseTypeRepo {
    pub(crate) items: Arc<Mutex<Vec<BusinessCaseType>>>,
}

#[async_trait::async_trait]
impl BusinessCaseTypeRepository for FakeBusinessCaseTypeRepo {
    async fn find_all(&self, active_only: bool) -> Result<Vec<BusinessCaseType>, DomainError> {
        Ok(self
            .items
            .lock()
            .expect("lock items")
            .iter()
            .filter(|item| !active_only || item.is_active)
            .cloned()
            .collect())
    }

    async fn find_all_scoped(
        &self,
        active_only: bool,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common: bool,
    ) -> Result<Vec<BusinessCaseType>, DomainError> {
        Ok(self
            .find_all(active_only)
            .await?
            .into_iter()
            .filter(|item| is_case_type_visible(item, viewer_department_id, viewer_department_name, include_common))
            .collect())
    }

    async fn find_by_code(&self, code: &str) -> Result<Option<BusinessCaseType>, DomainError> {
        Ok(self
            .items
            .lock()
            .expect("lock items")
            .iter()
            .find(|item| item.code == code)
            .cloned())
    }

    async fn find_by_code_scoped(
        &self,
        code: &str,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common: bool,
    ) -> Result<Option<BusinessCaseType>, DomainError> {
        Ok(self
            .find_by_code(code)
            .await?
            .filter(|item| is_case_type_visible(item, viewer_department_id, viewer_department_name, include_common)))
    }

    async fn save(&self, entity: &BusinessCaseType) -> Result<BusinessCaseType, DomainError> {
        let mut guard = self.items.lock().expect("lock items");
        guard.retain(|item| item.code != entity.code);
        guard.push(entity.clone());
        Ok(entity.clone())
    }

    async fn update_bpmn_xml(
        &self,
        code: &str,
        bpmn_xml: &str,
        description: Option<&str>,
    ) -> Result<bool, DomainError> {
        let mut guard = self.items.lock().expect("lock items");
        if let Some(item) = guard.iter_mut().find(|item| item.code == code) {
            item.bpmn_xml = Some(bpmn_xml.to_string());
            if let Some(description) = description {
                item.description = Some(description.to_string());
            }
            item.updated_at = Some(Utc::now());
            return Ok(true);
        }
        Ok(false)
    }

    async fn update_status(&self, code: &str, is_active: bool) -> Result<bool, DomainError> {
        let mut guard = self.items.lock().expect("lock items");
        if let Some(item) = guard.iter_mut().find(|item| item.code == code) {
            item.is_active = is_active;
            item.updated_at = Some(Utc::now());
            return Ok(true);
        }
        Ok(false)
    }

    async fn update_ai_extraction_config(
        &self,
        code: &str,
        config: &serde_json::Value,
    ) -> Result<Option<BusinessCaseType>, DomainError> {
        let mut guard = self.items.lock().expect("lock items");
        if let Some(item) = guard.iter_mut().find(|item| item.code == code) {
            item.ai_extraction_config = config.clone();
            item.updated_at = Some(Utc::now());
            return Ok(Some(item.clone()));
        }
        Ok(None)
    }

    async fn update_case_properties(
        &self,
        code: &str,
        properties: &serde_json::Value,
    ) -> Result<Option<BusinessCaseType>, DomainError> {
        let mut guard = self.items.lock().expect("lock items");
        if let Some(item) = guard.iter_mut().find(|item| item.code == code) {
            item.case_properties = properties.clone();
            item.updated_at = Some(Utc::now());
            return Ok(Some(item.clone()));
        }
        Ok(None)
    }
}

fn bearer_token() -> String {
    encode(
        &Header::default(),
        &TokenData {
            sub: Some("user-1".to_string()),
            email: None,
            username: Some("tester".to_string()),
            token_kind: Some("access".to_string()),
            is_admin: Some(false),
            permissions: vec![
                PermissionCatalog::WORKFLOW_DEFINITION_READ.to_string(),
                PermissionCatalog::WORKFLOW_DEFINITION_EDIT.to_string(),
                PermissionCatalog::WORKFLOW_DEFINITION_PUBLISH.to_string(),
                PermissionCatalog::WORKFLOW_DEFINITION_DEPRECATE.to_string(),
            ],
            department: Some("ops".to_string()),
            department_id: Some("ops-1".to_string()),
            pv: Some(1),
            iat: Some(Utc::now().timestamp()),
            exp: Some((Utc::now() + chrono::Duration::hours(1)).timestamp()),
            iss: None,
            aud: None,
            ua_hash: None,
            ip_subnet_hash: None,
        },
        &EncodingKey::from_secret(b"test-secret"),
    )
    .expect("jwt")
}

fn build_service() -> Arc<BusinessCaseTypeService> {
    Arc::new(BusinessCaseTypeService::new(Arc::new(FakeBusinessCaseTypeRepo {
        items: Arc::new(Mutex::new(vec![BusinessCaseType {
            id: "case_type_1".to_string(),
            code: "gate_check".to_string(),
            name: "Gate Check".to_string(),
            bpmn_xml: None,
            description: Some("Gate workflow".to_string()),
            is_active: true,
            visibility_scope: VisibilityScope::Department,
            department_id: Some("ops-1".to_string()),
            department_name_snapshot: Some("ops".to_string()),
            created_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
            ai_extraction_config: serde_json::json!({}),
            case_properties: serde_json::json!({}),
        }])),
    })))
}

fn is_case_type_visible(
    item: &BusinessCaseType,
    viewer_department_id: Option<&str>,
    viewer_department_name: Option<&str>,
    include_common: bool,
) -> bool {
    match item.visibility_scope {
        VisibilityScope::Common => include_common,
        VisibilityScope::Department => {
            item.department_id.as_deref() == viewer_department_id
                || item.department_name_snapshot.as_deref() == viewer_department_name
        }
    }
}

#[actix_web::test]
async fn case_type_routes_return_python_style_payloads() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(build_service()))
            .configure(configure),
    )
    .await;

    let list_response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v2/business-case-types")
            .insert_header(("Authorization", format!("Bearer {}", bearer_token())))
            .to_request(),
    )
    .await;
    assert_eq!(list_response.status(), StatusCode::OK);
    let list_body = to_bytes(list_response.into_body()).await.expect("list body");
    let list_payload: serde_json::Value = serde_json::from_slice(&list_body).expect("list json");
    assert_eq!(list_payload["success"], true);
    assert_eq!(list_payload["data"][0]["code"], "gate_check");

    let create_response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v2/business-case-types")
            .insert_header(("Authorization", format!("Bearer {}", bearer_token())))
            .set_json(serde_json::json!({
                "code": "baggage_delay",
                "name": "Baggage Delay",
                "description": "Delay workflow"
            }))
            .to_request(),
    )
    .await;
    assert_eq!(create_response.status(), StatusCode::CREATED);
    let create_body = to_bytes(create_response.into_body()).await.expect("create body");
    let create_payload: serde_json::Value = serde_json::from_slice(&create_body).expect("create json");
    assert_eq!(create_payload["success"], true);
    assert_eq!(create_payload["data"]["code"], "baggage_delay");
    println!("business case type list payload: {}", list_payload);
    println!("business case type create payload: {}", create_payload);
}

#[actix_web::test]
async fn save_bpmn_route_matches_python_status_semantics() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(build_service()))
            .configure(configure),
    )
    .await;

    let missing_xml = test::call_service(
        &app,
        test::TestRequest::put()
            .uri("/api/v2/business-case-types/gate_check/bpmn")
            .insert_header(("Authorization", format!("Bearer {}", bearer_token())))
            .set_json(serde_json::json!({}))
            .to_request(),
    )
    .await;
    assert_eq!(missing_xml.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let missing_xml_body = to_bytes(missing_xml.into_body()).await.expect("missing xml body");
    let missing_xml_payload: serde_json::Value = serde_json::from_slice(&missing_xml_body).expect("missing xml json");
    assert_eq!(missing_xml_payload["detail"], "缺少 bpmn_xml 参数");

    let not_found = test::call_service(
        &app,
        test::TestRequest::put()
            .uri("/api/v2/business-case-types/unknown/bpmn")
            .insert_header(("Authorization", format!("Bearer {}", bearer_token())))
            .set_json(serde_json::json!({"bpmn_xml": "<xml />"}))
            .to_request(),
    )
    .await;
    assert_eq!(not_found.status(), StatusCode::NOT_FOUND);
    let not_found_body = to_bytes(not_found.into_body()).await.expect("not found body");
    let not_found_payload: serde_json::Value = serde_json::from_slice(&not_found_body).expect("not found json");
    assert_eq!(not_found_payload["detail"], "业务事项类型 unknown 不存在");

    let success = test::call_service(
        &app,
        test::TestRequest::put()
            .uri("/api/v2/business-case-types/gate_check/bpmn")
            .insert_header(("Authorization", format!("Bearer {}", bearer_token())))
            .set_json(serde_json::json!({"bpmn_xml": "<xml />"}))
            .to_request(),
    )
    .await;
    assert_eq!(success.status(), StatusCode::OK);
    let body = to_bytes(success.into_body()).await.expect("success body");
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("success json");
    assert_eq!(payload["success"], true);
    assert_eq!(payload["message"], "BPMN 已保存至 gate_check");
    println!("business case type bpmn payload: {}", payload);

    let whitespace_xml = test::call_service(
        &app,
        test::TestRequest::put()
            .uri("/api/v2/business-case-types/gate_check/bpmn")
            .insert_header(("Authorization", format!("Bearer {}", bearer_token())))
            .set_json(serde_json::json!({"bpmn_xml": "   "}))
            .to_request(),
    )
    .await;
    assert_eq!(whitespace_xml.status(), StatusCode::OK);
    let whitespace_body = to_bytes(whitespace_xml.into_body()).await.expect("whitespace body");
    let whitespace_payload: serde_json::Value = serde_json::from_slice(&whitespace_body).expect("whitespace json");
    assert_eq!(whitespace_payload["success"], true);
    println!("business case type whitespace bpmn payload: {}", whitespace_payload);
}

#[actix_web::test]
async fn update_ai_extraction_config_route_works() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(build_service()))
            .configure(configure),
    )
    .await;

    let unauthorized_res = test::call_service(
        &app,
        test::TestRequest::put()
            .uri("/api/v2/business-case-types/gate_check/ai-extraction-config")
            .set_json(serde_json::json!({
                "ai_extraction_config": {
                    "enabled": true
                }
            }))
            .to_request(),
    )
    .await;
    assert_eq!(unauthorized_res.status(), StatusCode::UNAUTHORIZED);

    let success_res = test::call_service(
        &app,
        test::TestRequest::put()
            .uri("/api/v2/business-case-types/gate_check/ai-extraction-config")
            .insert_header(("Authorization", format!("Bearer {}", bearer_token())))
            .set_json(serde_json::json!({
                "ai_extraction_config": {
                    "enabled": true,
                    "fields": {
                        "seat_no": {
                            "type": "string",
                            "required": true
                        }
                    }
                }
            }))
            .to_request(),
    )
    .await;
    assert_eq!(success_res.status(), StatusCode::OK);
    let body = to_bytes(success_res.into_body()).await.expect("body");
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(payload["success"], true);
    assert_eq!(payload["data"]["ai_extraction_config"]["enabled"], true);

    let bad_request_res = test::call_service(
        &app,
        test::TestRequest::put()
            .uri("/api/v2/business-case-types/gate_check/ai-extraction-config")
            .insert_header(("Authorization", format!("Bearer {}", bearer_token())))
            .set_json(serde_json::json!({
                "ai_extraction_config": "not_an_object"
            }))
            .to_request(),
    )
    .await;
    assert_eq!(bad_request_res.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let bad_type_res = test::call_service(
        &app,
        test::TestRequest::put()
            .uri("/api/v2/business-case-types/gate_check/ai-extraction-config")
            .insert_header(("Authorization", format!("Bearer {}", bearer_token())))
            .set_json(serde_json::json!({
                "ai_extraction_config": {
                    "enabled": true,
                    "fields": {
                        "seat_no": {
                            "type": "array"
                        }
                    }
                }
            }))
            .to_request(),
    )
    .await;
    assert_eq!(bad_type_res.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let enum_no_values_res = test::call_service(
        &app,
        test::TestRequest::put()
            .uri("/api/v2/business-case-types/gate_check/ai-extraction-config")
            .insert_header(("Authorization", format!("Bearer {}", bearer_token())))
            .set_json(serde_json::json!({
                "ai_extraction_config": {
                    "enabled": true,
                    "fields": {
                        "reason": {
                            "type": "enum"
                        }
                    }
                }
            }))
            .to_request(),
    )
    .await;
    assert_eq!(enum_no_values_res.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let enum_empty_values_res = test::call_service(
        &app,
        test::TestRequest::put()
            .uri("/api/v2/business-case-types/gate_check/ai-extraction-config")
            .insert_header(("Authorization", format!("Bearer {}", bearer_token())))
            .set_json(serde_json::json!({
                "ai_extraction_config": {
                    "enabled": true,
                    "fields": {
                        "reason": {
                            "type": "enum",
                            "enum_values": []
                        }
                    }
                }
            }))
            .to_request(),
    )
    .await;
    assert_eq!(enum_empty_values_res.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let bad_leg_res = test::call_service(
        &app,
        test::TestRequest::put()
            .uri("/api/v2/business-case-types/gate_check/ai-extraction-config")
            .insert_header(("Authorization", format!("Bearer {}", bearer_token())))
            .set_json(serde_json::json!({
                "ai_extraction_config": {
                    "enabled": true,
                    "leg_binding": {
                        "allowed": ["both"]
                    }
                }
            }))
            .to_request(),
    )
    .await;
    assert_eq!(bad_leg_res.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let bad_score_res = test::call_service(
        &app,
        test::TestRequest::put()
            .uri("/api/v2/business-case-types/gate_check/ai-extraction-config")
            .insert_header(("Authorization", format!("Bearer {}", bearer_token())))
            .set_json(serde_json::json!({
                "ai_extraction_config": {
                    "enabled": true,
                    "flight_matching": {
                        "min_auto_match_score": 1.2
                    }
                }
            }))
            .to_request(),
    )
    .await;
    assert_eq!(bad_score_res.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let bad_window_res = test::call_service(
        &app,
        test::TestRequest::put()
            .uri("/api/v2/business-case-types/gate_check/ai-extraction-config")
            .insert_header(("Authorization", format!("Bearer {}", bearer_token())))
            .set_json(serde_json::json!({
                "ai_extraction_config": {
                    "enabled": true,
                    "flight_matching": {
                        "window_hours_before": -1
                    }
                }
            }))
            .to_request(),
    )
    .await;
    assert_eq!(bad_window_res.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[actix_web::test]
async fn update_case_properties_route_works() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(build_service()))
            .configure(configure),
    )
    .await;

    let unauthorized_res = test::call_service(
        &app,
        test::TestRequest::put()
            .uri("/api/v2/business-case-types/gate_check/case-properties")
            .set_json(serde_json::json!({
                "case_properties": {
                    "workflow_policy": {
                        "batch_notification_enabled": true
                    }
                }
            }))
            .to_request(),
    )
    .await;
    assert_eq!(unauthorized_res.status(), StatusCode::UNAUTHORIZED);

    let invalid_duplicate_fields = test::call_service(
        &app,
        test::TestRequest::put()
            .uri("/api/v2/business-case-types/gate_check/case-properties")
            .insert_header(("Authorization", format!("Bearer {}", bearer_token())))
            .set_json(serde_json::json!({
                "case_properties": {
                    "duplicate_policy": {
                        "enabled": true,
                        "fields": ["seat_no", 42]
                    }
                }
            }))
            .to_request(),
    )
    .await;
    assert_eq!(invalid_duplicate_fields.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let success_res = test::call_service(
        &app,
        test::TestRequest::put()
            .uri("/api/v2/business-case-types/gate_check/case-properties")
            .insert_header(("Authorization", format!("Bearer {}", bearer_token())))
            .set_json(serde_json::json!({
                "case_properties": {
                    "binding_policy": {
                        "flight_required": true,
                        "allowed_leg_types": ["outbound"],
                        "default_leg_type": "outbound",
                        "leg_type_required": true,
                        "flight_match_policy": {
                            "allow_numeric_suffix": true,
                            "exclude_cancelled": true,
                            "exclude_departed": true,
                            "exclude_actual_departure": true,
                            "time_window_hours_before": 3,
                            "time_window_hours_after": 8,
                            "min_auto_match_score": 0.85
                        }
                    },
                    "extra_info_schema": {
                        "fields": {
                            "seat_no": {
                                "type": "string",
                                "label": "座位号",
                                "required": true
                            }
                        },
                        "summary_template": "座位号 {{seat_no}}"
                    },
                    "workflow_policy": {
                        "batch_notification_enabled": true,
                        "batch_receipt_mode": "shared_group"
                    },
                    "duplicate_policy": {
                        "enabled": true,
                        "fields": ["seat_no"],
                        "include_extra_info": false,
                        "include_bound_leg": true,
                        "active_statuses": ["INITIAL", "PENDING"]
                    }
                }
            }))
            .to_request(),
    )
    .await;
    assert_eq!(success_res.status(), StatusCode::OK);
    let body = to_bytes(success_res.into_body()).await.expect("body");
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(payload["success"], true);
    assert_eq!(
        payload["data"]["case_properties"]["duplicate_policy"]["fields"][0],
        "seat_no"
    );
    assert_eq!(
        payload["data"]["case_properties"]["workflow_policy"]["batch_receipt_mode"],
        "shared_group"
    );
}
