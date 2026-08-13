use super::shared::{broadcast_business_case_event, has_grant};
use crate::middleware::jwt::JwtAuth;
use crate::sse::hub::SseHub;
use fms_application::schemas::auth_schemas::TokenData;
use fms_application::services::authorization_service::PermissionCatalog;
use serde_json::json;

fn build_claims(username: &str, sub: &str, permissions: &[&str]) -> JwtAuth {
    JwtAuth(TokenData {
        sub: Some(sub.to_string()),
        email: None,
        username: Some(username.to_string()),
        token_kind: Some("access".to_string()),
        is_admin: Some(false),
        permissions: permissions.iter().map(|value| value.to_string()).collect(),
        department: None,
        department_id: None,
        pv: Some(1),
        iat: None,
        exp: None,
        iss: None,
        aud: None,
        ua_hash: None,
        ip_subnet_hash: None,
    })
}

#[actix_web::test]
async fn business_case_events_are_broadcast_on_allowed_topic() {
    let hub = SseHub::new(8);
    let mut receiver = hub.subscribe("business_cases").await;

    broadcast_business_case_event(
        Some(&hub),
        "business_case.updated",
        json!({
            "event": "business_case.updated",
            "case_id": "bc_001",
            "changed_fields": ["status", "description"],
        }),
    )
    .await;

    let message = receiver.recv().await.expect("receive broadcast");
    assert_eq!(message.topic, "business_cases");
    assert_eq!(message.event.as_deref(), Some("business_case.updated"));
    let data: serde_json::Value = serde_json::from_str(&message.serialized_data).unwrap();
    assert_eq!(data["case_id"], "bc_001");
    assert_eq!(data["changed_fields"][0], "status");
    println!("business case sse payload: {}", message.serialized_data);
}

#[actix_web::test]
async fn business_case_created_event_payload_matches_python_fields() {
    let hub = SseHub::new(8);
    let mut receiver = hub.subscribe("business_cases").await;

    broadcast_business_case_event(
        Some(&hub),
        "business_case.created",
        json!({
            "event": "business_case.created",
            "case_id": "bc_001",
            "case_type": "gate_change",
            "flight_id": "flight_001"
        }),
    )
    .await;

    let message = receiver.recv().await.expect("receive create broadcast");
    let data: serde_json::Value = serde_json::from_str(&message.serialized_data).unwrap();
    assert_eq!(data["case_type"], "gate_change");
    assert_eq!(data["flight_id"], "flight_001");
    println!("business case created payload: {}", message.serialized_data);
}

#[actix_web::test]
async fn business_case_deleted_event_payload_matches_python_fields() {
    let hub = SseHub::new(8);
    let mut receiver = hub.subscribe("business_cases").await;

    broadcast_business_case_event(
        Some(&hub),
        "business_case.deleted",
        json!({
            "event": "business_case.deleted",
            "case_id": "bc_001"
        }),
    )
    .await;

    let message = receiver.recv().await.expect("receive delete broadcast");
    let data: serde_json::Value = serde_json::from_str(&message.serialized_data).unwrap();
    assert_eq!(data["case_id"], "bc_001");
    assert!(data.get("operator").is_none());
    println!("business case deleted payload: {}", message.serialized_data);
}

#[test]
fn flight_manage_does_not_map_to_business_case_grants() {
    let claims = build_claims("manager", "user-1", &["flight:manage"]);

    assert!(!has_grant(&claims, PermissionCatalog::BUSINESS_CASE_UPDATE));
    assert!(!has_grant(&claims, PermissionCatalog::BUSINESS_CASE_STATUS_TRANSITION));
}
