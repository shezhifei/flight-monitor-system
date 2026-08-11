use super::configure;
use actix_web::{body::to_bytes, http::StatusCode, test, web, App};
use chrono::Utc;
use jsonwebtoken::{encode, EncodingKey, Header};

use crate::middleware::jwt::{JwtSecret, WorkflowInternalToken};
use fms_application::schemas::auth_schemas::TokenData;

fn bearer_token(permissions: &[&str]) -> String {
    encode(
        &Header::default(),
        &TokenData {
            sub: Some("user-1".to_string()),
            email: None,
            username: Some("tester".to_string()),
            token_kind: Some("access".to_string()),
            is_admin: Some(false),
            permissions: permissions.iter().map(|value| (*value).to_string()).collect(),
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
    .expect("test jwt should encode")
}

#[actix_web::test]
async fn trigger_returns_503_when_workflow_service_missing() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(WorkflowInternalToken(Some(
                "internal-token".to_string(),
            ))))
            .configure(configure),
    )
    .await;

    let request = test::TestRequest::post()
        .uri("/api/v2/workflows/integrations/dispatch/trigger")
        .insert_header(("X-Workflow-Token", "internal-token"))
        .set_json(serde_json::json!({}))
        .to_request();

    let response = test::call_service(&app, request).await;

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = to_bytes(response.into_body()).await.expect("read body");
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("json body");
    assert_eq!(payload["detail"], "workflow dispatch service unavailable");
    println!("workflow dispatch trigger 503 payload: {}", payload);
}

#[actix_web::test]
async fn pending_returns_503_when_query_service_missing() {
    let token = bearer_token(&["dispatch:view"]);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .configure(configure),
    )
    .await;

    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v2/workflows/integrations/dispatch/pending?page=1&page_size=1")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = to_bytes(response.into_body()).await.expect("read body");
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("json body");
    assert_eq!(payload["detail"], "dispatch query service unavailable");
    println!("workflow dispatch pending 503 payload: {}", payload);
}

#[actix_web::test]
async fn assign_returns_503_when_workflow_service_missing() {
    let token = bearer_token(&["dispatch:manage"]);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .configure(configure),
    )
    .await;

    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v2/workflows/integrations/dispatch/order-1/assign")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(serde_json::json!({"assigned_user_ids": ["user-2"]}))
            .to_request(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = to_bytes(response.into_body()).await.expect("read body");
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("json body");
    assert_eq!(payload["detail"], "workflow dispatch service unavailable");
    println!("workflow dispatch assign 503 payload: {}", payload);
}

#[actix_web::test]
async fn trigger_json_decode_error_uses_python_style_422_envelope() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .app_data(web::Data::new(WorkflowInternalToken(Some(
                "internal-token".to_string(),
            ))))
            .configure(configure),
    )
    .await;

    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v2/workflows/integrations/dispatch/trigger")
            .insert_header(("X-Workflow-Token", "internal-token"))
            .insert_header(("content-type", "application/json"))
            .set_payload("{invalid")
            .to_request(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = to_bytes(response.into_body()).await.expect("read body");
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("json body");
    assert_eq!(payload["success"], false);
    assert_eq!(payload["error"]["code"], "HTTP_422");
    assert_eq!(payload["error"]["type"], "validation_error");
    assert_eq!(payload["error"]["message"], "输入验证失败");
    assert_eq!(payload["error"]["details"][0]["loc"][0], "body");
}
