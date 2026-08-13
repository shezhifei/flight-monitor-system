//! Dispatch DTOs + status-machine constants.
//!
//! Field authority: legacy `DispatchModels.kt`, cross-checked against the
//! backend `dispatch_schemas.rs` (`MobileSyncRequest` / `MobileSyncResponse`).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Status-machine constants (centralized for UI mapping)
// ---------------------------------------------------------------------------

/// Dispatch order statuses.
/// pending=待分配 assigned=待接单 accepted=已接单 checked_in=已签到
/// in_progress=作业中 completed=已完工 cancelled=已取消
pub mod order_status {
    pub const PENDING: &str = "pending";
    pub const ASSIGNED: &str = "assigned";
    pub const ACCEPTED: &str = "accepted";
    pub const CHECKED_IN: &str = "checked_in";
    pub const IN_PROGRESS: &str = "in_progress";
    pub const COMPLETED: &str = "completed";
    pub const CANCELLED: &str = "cancelled";
}

/// Offline action types stored in `pending_actions.action_type`
/// and sent as `MobileSyncAction.action_type`.
pub mod action_type {
    pub const ACCEPT: &str = "accept";
    pub const CHECKIN: &str = "checkin";
    pub const CHECKOUT: &str = "checkout";
    pub const START: &str = "start";
    pub const COMPLETE: &str = "complete";
    pub const ETA_REPORT: &str = "eta_report";
    pub const REPORT_ISSUE: &str = "report_issue";
}

/// Server-side per-action sync verdicts
/// (`MobileSyncActionResult.status`): `applied`/`duplicate` → drop from the
/// queue, `failed` → keep and increment `retry_count`.
pub mod sync_status {
    pub const APPLIED: &str = "applied";
    pub const DUPLICATE: &str = "duplicate";
    pub const FAILED: &str = "failed";
}

/// Notification receipt ack statuses (`notifications.ack_status` in the
/// backend schema).
pub mod receipt_status {
    pub const PENDING: &str = "pending";
    pub const ACKNOWLEDGED: &str = "acknowledged";
    pub const REJECTED: &str = "rejected";
}

// ---------------------------------------------------------------------------
// Order list
// ---------------------------------------------------------------------------

/// One dispatch order as returned by the my/assigned list endpoints.
/// `step_code` is absent in the live backend payload — keep it optional.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DispatchOrderItem {
    pub id: String,
    pub flight_id: String,
    pub step_code: Option<String>,
    pub status: String,
    pub terminal: Option<String>,
    pub stand_id: Option<String>,
    pub gate: Option<String>,
    pub estimated_completion_time: Option<String>,
    #[serde(default = "default_origin_type")]
    pub origin_type: String,
    #[serde(default = "default_origin_label")]
    pub origin_label: String,
    #[serde(default)]
    pub notification_receipt_summary: HashMap<String, serde_json::Value>,
}

fn default_origin_type() -> String {
    "manual".to_string()
}

fn default_origin_label() -> String {
    "人工".to_string()
}

// ---------------------------------------------------------------------------
// Action requests (the seven dispatch write actions)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DispatchOrderAcceptRequest {
    pub note: Option<String>,
    pub client_action_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DispatchOrderCheckInRequest {
    pub qr_code: Option<String>,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    pub accuracy_m: Option<f64>,
    pub note: Option<String>,
    pub client_action_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DispatchOrderCheckOutRequest {
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    pub note: Option<String>,
    pub client_action_id: Option<String>,
    pub recorded_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DispatchOrderStartRequest {
    pub actual_start_time: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DispatchOrderCompleteRequest {
    pub actual_end_time: Option<String>,
    pub completion_notes: Option<String>,
    #[serde(default)]
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DispatchOrderEtaReportRequest {
    pub estimated_completion_time: String,
    pub note: Option<String>,
    pub client_action_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DispatchIssueReportRequest {
    pub title: String,
    pub description: Option<String>,
    pub severity: String,
    pub issue_type: String,
    pub note: Option<String>,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    #[serde(default)]
    pub attachments: Vec<String>,
    pub client_action_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Offline sync (`POST /api/v2/dispatch-orders/mobile/sync/actions`)
// ---------------------------------------------------------------------------

/// One queued action. Backend schema `MobileSyncAction`; `action_timestamp`
/// and `payload` are optional there, matching the Kotlin defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DispatchSyncAction {
    pub client_action_id: String,
    pub action_type: String,
    pub dispatch_order_id: String,
    pub action_timestamp: Option<String>,
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DispatchSyncRequest {
    pub actions: Vec<DispatchSyncAction>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DispatchSyncResult {
    pub client_action_id: String,
    pub dispatch_order_id: String,
    pub action_type: String,
    pub status: String,
    pub message: String,
    pub server_timestamp: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DispatchSyncResponse {
    pub total: i64,
    pub applied: i64,
    pub duplicates: i64,
    pub failed: i64,
    #[serde(default)]
    pub results: Vec<DispatchSyncResult>,
}

// ---------------------------------------------------------------------------
// Safety checklist (`/{order_id}/safety-checklist`, legacy DispatchSafetyModels.kt)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DispatchSafetyChecklistItemStatus {
    pub item_code: String,
    pub title: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub allow_na: bool,
    #[serde(default)]
    pub order: i64,
    pub result: Option<String>,
    pub checked_by: Option<String>,
    pub checked_by_username: Option<String>,
    pub checked_at: Option<String>,
    pub note: Option<String>,
    #[serde(default = "default_item_status")]
    pub status: String,
}

fn default_item_status() -> String {
    "pending".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DispatchSafetyChecklistStatus {
    pub dispatch_order_id: String,
    // Absent in the live checklist response — keep optional.
    pub step_code: Option<String>,
    pub template_id: Option<String>,
    pub template_version: Option<String>,
    #[serde(default)]
    pub enforced: bool,
    #[serde(default = "default_ready")]
    pub ready: bool,
    #[serde(default)]
    pub required_total: i64,
    #[serde(default)]
    pub completed_required: i64,
    #[serde(default)]
    pub pending_required_items: Vec<String>,
    #[serde(default)]
    pub failed_required_items: Vec<String>,
    #[serde(default)]
    pub items: Vec<DispatchSafetyChecklistItemStatus>,
}

fn default_ready() -> bool {
    true
}

/// `POST /{order_id}/safety-checklist/items/{item_code}` body.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DispatchSafetyChecklistItemResultRequest {
    pub result: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DispatchSafetyChecklistRecord {
    pub record_id: String,
    pub dispatch_order_id: String,
    pub item_code: String,
    pub result: String,
    pub checked_by: Option<String>,
    pub checked_by_username: Option<String>,
    pub checked_at: String,
    pub note: Option<String>,
    pub template_version: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: the live my/assigned payload omits `step_code`.
    #[test]
    fn order_item_deserializes_without_step_code() {
        let item: DispatchOrderItem = serde_json::from_str(
            r#"{"id":"o1","flight_id":"f1","status":"assigned"}"#,
        )
        .unwrap();
        assert_eq!(item.step_code, None);
        assert_eq!(item.origin_type, "manual");
    }

    /// Regression: the live safety-checklist response omits `step_code`.
    #[test]
    fn checklist_status_deserializes_without_step_code() {
        let status: DispatchSafetyChecklistStatus = serde_json::from_str(
            r#"{"dispatch_order_id":"o1","task_type":"x","template_id":"t1",
                "ready":true,"enforced":false,"items":[]}"#,
        )
        .unwrap();
        assert_eq!(status.step_code, None);
        assert!(status.ready);
    }
}
