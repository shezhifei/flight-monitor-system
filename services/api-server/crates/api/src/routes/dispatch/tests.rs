use super::{merge_cancel_request, DispatchOrderCancelQuery};
use actix_web::web;
use fms_application::schemas::dispatch_schemas::DispatchOrderCancelRequest;

#[test]
fn cancel_request_uses_query_reason_when_body_missing() {
    let dto = merge_cancel_request(
        Some(web::Query(DispatchOrderCancelQuery {
            reason: Some("from-query".to_string()),
            client_action_id: None,
        })),
        None,
    );

    assert_eq!(dto.reason.as_deref(), Some("from-query"));
}

#[test]
fn cancel_request_prefers_non_empty_body_fields() {
    let dto = merge_cancel_request(
        Some(web::Query(DispatchOrderCancelQuery {
            reason: Some("from-query".to_string()),
            client_action_id: Some("query-action".to_string()),
        })),
        Some(web::Json(DispatchOrderCancelRequest {
            reason: Some("from-body".to_string()),
            client_action_id: Some("body-action".to_string()),
        })),
    );

    assert_eq!(dto.reason.as_deref(), Some("from-body"));
    assert_eq!(dto.client_action_id.as_deref(), Some("body-action"));
}

#[test]
fn cancel_request_falls_back_to_query_for_blank_body_fields() {
    let dto = merge_cancel_request(
        Some(web::Query(DispatchOrderCancelQuery {
            reason: Some("from-query".to_string()),
            client_action_id: Some("query-action".to_string()),
        })),
        Some(web::Json(DispatchOrderCancelRequest {
            reason: Some("   ".to_string()),
            client_action_id: Some(String::new()),
        })),
    );

    assert_eq!(dto.reason.as_deref(), Some("from-query"));
    assert_eq!(dto.client_action_id.as_deref(), Some("query-action"));
}

#[test]
fn batch_deprecated_endpoint_has_warning() {
    let source = include_str!("replan.rs");
    let test_marker = "#[cfg(test)]";
    let main_code = &source[..source.find(test_marker).unwrap_or(source.len())];
    assert!(
        main_code.contains("tracing::warn") || main_code.contains("DEPRECATED"),
        "POST /batch endpoint should log deprecation warning"
    );
}
