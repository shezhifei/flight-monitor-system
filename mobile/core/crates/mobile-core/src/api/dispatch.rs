//! Dispatch API wrappers + offline-aware action dispatch.
//!
//! Endpoints (backend `routes/dispatch/mod.rs`,
//! `routes/dispatch_resources/mod.rs`, mirrored from legacy
//! `DispatchApi.kt`):
//! - `GET /api/v2/dispatch-orders/my/assigned` → enveloped
//!   `List<DispatchOrderItem>`;
//! - the 7 order actions under `/api/v2/dispatch-orders/{id}/...` →
//!   enveloped map payload (returned as raw JSON);
//! - `GET/POST /api/v2/dispatch-orders/{id}/safety-checklist[/items/{code}]`.
//!
//! [`dispatch_action`] is the offline-aware entry: send directly, and only
//! when the failure is network-class ([`crate::offline::should_enqueue`])
//! queue the action into the sqlite store for later replay. HTTP 4xx/5xx
//! rejections are final and never queued.

use serde_json::Value;

use crate::client::ApiClient;
use crate::dto::dispatch::{
    action_type, DispatchOrderItem, DispatchSafetyChecklistItemResultRequest,
    DispatchSafetyChecklistRecord, DispatchSafetyChecklistStatus,
};
use crate::error::CoreError;
use crate::offline::{should_enqueue, OfflineQueue};

/// Outcome of [`dispatch_action`]: sent now, or queued for offline replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchActionOutcome {
    Sent,
    Queued,
}

/// Map a queued action type to its REST path segment (legacy
/// `DispatchApi.kt` routes).
fn action_path_segment(action: &str) -> Option<&'static str> {
    match action {
        action_type::ACCEPT => Some("accept"),
        action_type::CHECKIN => Some("checkin"),
        action_type::CHECKOUT => Some("checkout"),
        action_type::START => Some("start"),
        action_type::COMPLETE => Some("complete"),
        action_type::ETA_REPORT => Some("eta-report"),
        action_type::REPORT_ISSUE => Some("report-issue"),
        _ => None,
    }
}

/// `GET /api/v2/dispatch-orders/my/assigned`.
pub async fn my_assigned_orders(
    client: &ApiClient,
    status: Option<&str>,
) -> Result<Vec<DispatchOrderItem>, CoreError> {
    client
        .call_with_envelope::<Vec<DispatchOrderItem>, ()>("GET", &my_assigned_path(status), None)
        .await
}

/// Live route is `/api/v2/dispatch-orders/my/assigned`.
fn my_assigned_path(status: Option<&str>) -> String {
    match status {
        Some(s) => format!("/api/v2/dispatch-orders/my/assigned?status={s}"),
        None => "/api/v2/dispatch-orders/my/assigned".to_string(),
    }
}

/// Send one dispatch action, falling back to the offline queue on
/// network-class errors only.
///
/// `payload_json` is the action-specific request body (e.g. the serialized
/// `DispatchOrderCheckInRequest`). A fresh `client_action_id` (UUID) is
/// injected into the JSON object for idempotent replay, matching the legacy
/// contract; the same id keys the sqlite row when queued.
pub async fn dispatch_action(
    client: &ApiClient,
    queue: &OfflineQueue,
    order_id: &str,
    action: &str,
    payload_json: &str,
) -> Result<DispatchActionOutcome, CoreError> {
    let segment = action_path_segment(action).ok_or_else(|| {
        CoreError::InvalidConfig(format!("unknown dispatch action: {action}"))
    })?;
    let client_action_id = uuid::Uuid::new_v4().to_string();
    let body = inject_client_action_id(payload_json, &client_action_id)?;
    let path = format!("/api/v2/dispatch-orders/{order_id}/{segment}");

    match client
        .call_with_envelope::<Value, Value>("POST", &path, Some(&body))
        .await
    {
        Ok(_) => Ok(DispatchActionOutcome::Sent),
        Err(err) if should_enqueue(&err) => {
            let stored = serde_json::to_string(&body)?;
            queue.enqueue(&client_action_id, order_id, action, &stored)?;
            tracing::warn!(
                order_id,
                action,
                "dispatch action queued for offline replay (network error)"
            );
            Ok(DispatchActionOutcome::Queued)
        }
        Err(err) => Err(err),
    }
}

/// Parse the payload and add `client_action_id` (without overwriting an
/// existing one). Non-object payloads are rejected — every dispatch action
/// body is a JSON object by contract.
fn inject_client_action_id(payload_json: &str, client_action_id: &str) -> Result<Value, CoreError> {
    let mut value: Value = serde_json::from_str(payload_json)?;
    let object = value.as_object_mut().ok_or_else(|| {
        CoreError::Serialization("dispatch action payload must be a JSON object".to_string())
    })?;
    object
        .entry("client_action_id")
        .or_insert_with(|| Value::String(client_action_id.to_string()));
    Ok(value)
}

/// `GET /api/v2/dispatch-orders/{order_id}/safety-checklist`.
pub async fn safety_checklist(
    client: &ApiClient,
    order_id: &str,
) -> Result<DispatchSafetyChecklistStatus, CoreError> {
    client
        .call_with_envelope::<DispatchSafetyChecklistStatus, ()>(
            "GET",
            &format!("/api/v2/dispatch-orders/{order_id}/safety-checklist"),
            None,
        )
        .await
}

/// `POST /api/v2/dispatch-orders/{order_id}/safety-checklist/items/{item_code}`.
pub async fn submit_checklist_item(
    client: &ApiClient,
    order_id: &str,
    item_code: &str,
    result: &str,
    note: Option<&str>,
) -> Result<DispatchSafetyChecklistRecord, CoreError> {
    client
        .call_with_envelope(
            "POST",
            &format!("/api/v2/dispatch-orders/{order_id}/safety-checklist/items/{item_code}"),
            Some(&DispatchSafetyChecklistItemResultRequest {
                result: result.to_string(),
                note: note.map(str::to_string),
            }),
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_segments_match_legacy_routes() {
        assert_eq!(action_path_segment("accept"), Some("accept"));
        assert_eq!(action_path_segment("checkin"), Some("checkin"));
        assert_eq!(action_path_segment("checkout"), Some("checkout"));
        assert_eq!(action_path_segment("start"), Some("start"));
        assert_eq!(action_path_segment("complete"), Some("complete"));
        assert_eq!(action_path_segment("eta_report"), Some("eta-report"));
        assert_eq!(action_path_segment("report_issue"), Some("report-issue"));
        assert_eq!(action_path_segment("bogus"), None);
    }

    #[test]
    fn my_assigned_uses_rust_backend_route() {
        assert_eq!(my_assigned_path(None), "/api/v2/dispatch-orders/my/assigned");
        assert_eq!(
            my_assigned_path(Some("assigned")),
            "/api/v2/dispatch-orders/my/assigned?status=assigned"
        );
    }

    #[test]
    fn client_action_id_is_injected_without_overwrite() {
        let value = inject_client_action_id(r#"{"note":"x"}"#, "id-1").unwrap();
        assert_eq!(value["client_action_id"], "id-1");
        assert_eq!(value["note"], "x");

        let kept = inject_client_action_id(r#"{"client_action_id":"id-0"}"#, "id-1").unwrap();
        assert_eq!(kept["client_action_id"], "id-0");

        assert!(inject_client_action_id(r#"[1,2]"#, "id-1").is_err());
        assert!(inject_client_action_id("not json", "id-1").is_err());
    }
}
