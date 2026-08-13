//! Mobile-domain DTOs.
//!
//! Field authority: legacy `MobileModels.kt`, cross-checked against
//! `services/api-server/crates/api/src/routes/mobile.rs`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// `GET /api/v2/mobile/workbench` response data.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MobileWorkbenchResponse {
    pub user_id: String,
    pub generated_at: String,
    #[serde(default)]
    pub my_orders: Vec<MobileWorkbenchOrderItem>,
    pub order_counts: MobileWorkbenchCounts,
    #[serde(default)]
    pub notification_unread_count: i64,
    #[serde(default)]
    pub chat_unread_total: i64,
    #[serde(default)]
    pub pending_shift_handover_count: i64,
    #[serde(default)]
    pub pending_sync_action_count: i64,
    #[serde(default)]
    pub channel_recommendation: HashMap<String, bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MobileWorkbenchOrderItem {
    pub order_id: String,
    pub flight_id: String,
    // Absent in the live workbench my_orders payload — keep optional.
    pub step_code: Option<String>,
    pub status: String,
    pub terminal: Option<String>,
    pub stand_id: Option<String>,
    pub gate: Option<String>,
    pub planned_start_time: Option<String>,
    pub planned_end_time: Option<String>,
    pub actual_start_time: Option<String>,
    pub assignment_deadline: Option<String>,
    #[serde(default)]
    pub supervisor_notified: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MobileWorkbenchCounts {
    #[serde(default)]
    pub pending: i64,
    #[serde(default)]
    pub assigned: i64,
    #[serde(default)]
    pub in_progress: i64,
    #[serde(default)]
    pub completed: i64,
    #[serde(default)]
    pub cancelled: i64,
    #[serde(default)]
    pub total: i64,
}

/// `POST /api/v2/mobile/uploads` response data.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MobileUploadAsset {
    pub upload_id: String,
    pub original_filename: String,
    pub content_type: Option<String>,
    #[serde(default)]
    pub file_size: i64,
    pub checksum_sha256: Option<String>,
    pub created_at: String,
    pub attachment_url: String,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// `POST /api/v2/mobile/devices/register` body.
///
/// `platform` is always `"android"` (backend enum).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct MobileDeviceRegisterRequest {
    pub device_id: String,
    pub platform: String,
    pub push_channel: String,
    pub push_token: Option<String>,
    pub app_version: Option<String>,
    pub os_version: Option<String>,
    pub device_model: Option<String>,
    pub manufacturer: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl MobileDeviceRegisterRequest {
    /// Minimal registration payload.
    pub fn new(device_id: impl Into<String>) -> Self {
        Self {
            device_id: device_id.into(),
            platform: "android".to_string(),
            push_channel: "none".to_string(),
            push_token: None,
            app_version: None,
            os_version: None,
            device_model: None,
            manufacturer: None,
            metadata: HashMap::new(),
        }
    }
}

/// `POST /api/v2/mobile/devices/{device_id}/heartbeat` body.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct MobileDeviceHeartbeatRequest {
    pub network_status: Option<String>,
    pub battery_level: Option<i64>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Device register/heartbeat response data. Extra backend fields are
/// ignored on deserialization.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MobileDeviceResponse {
    pub device_id: String,
    pub user_id: String,
    pub is_active: bool,
    pub last_heartbeat_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: the live workbench my_orders payload omits `step_code`.
    #[test]
    fn workbench_order_item_deserializes_without_step_code() {
        let item: MobileWorkbenchOrderItem = serde_json::from_str(
            r#"{"order_id":"o1","flight_id":"f1","status":"assigned"}"#,
        )
        .unwrap();
        assert_eq!(item.step_code, None);
        assert!(!item.supervisor_notified);
    }
}
