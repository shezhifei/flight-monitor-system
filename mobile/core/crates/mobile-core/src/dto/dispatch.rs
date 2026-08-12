//! Dispatch DTOs + status-machine constants (plan §3.6).
//!
//! Field authority: legacy `DispatchModels.kt`, cross-checked against the
//! backend `dispatch_schemas.rs` (`MobileSyncRequest` / `MobileSyncResponse`).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Status-machine constants (centralized for UI mapping, plan §3.6)
// ---------------------------------------------------------------------------

/// Dispatch order statuses. Labels in the legacy app:
/// pending=待分配 assigned=待接单 accepted=已接单 checked_in=已签到
/// in_progress=作业中 completed=已完工 cancelled=已取消
/// (`WorkbenchActivity.kt::mapStatusLabel`).
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
/// (plan §3.5 DDL comment) and sent as `MobileSyncAction.action_type`.
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
/// queue, `failed` → keep and increment `retry_count` (plan §3.5).
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
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DispatchOrderItem {
    pub id: String,
    pub flight_id: String,
    pub step_code: String,
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
// Action requests (the 7 dispatch actions on the P1 main flow)
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
