//! 通知领域模型
//!
//! 对应 Python `src/domain/models/notification.py`。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 站内通知实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub notification_id: String,
    pub user_id: String,
    pub title: String,
    pub body: String,
    pub category: String,
    pub severity: String,
    #[serde(default)]
    pub is_read: bool,
    pub flight_id: Option<String>,
    pub related_entity_type: Option<String>,
    pub related_entity_id: Option<String>,
    pub dispatch_order_id: Option<String>,
    pub group_id: Option<String>,
    pub event_id: Option<String>,
    pub sender_user_id: Option<String>,
    pub sender_username_snapshot: Option<String>,
    pub recipient_username_snapshot: Option<String>,
    pub recipient_display_name_snapshot: Option<String>,
    pub recipient_department_snapshot: Option<String>,
    pub recipient_job_title_snapshot: Option<String>,
    #[serde(default = "default_origin")]
    pub origin_type: String,
    #[serde(default)]
    pub receipt_required: bool,
    pub receipt_group_id: Option<String>,
    #[serde(default = "default_delivery_status")]
    pub delivery_status: String,
    pub delivered_at: Option<DateTime<Utc>>,
    #[serde(default = "default_ack_status")]
    pub ack_status: String,
    pub ack_at: Option<DateTime<Utc>>,
    pub ack_note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
}

fn default_origin() -> String {
    "manual".to_string()
}
fn default_delivery_status() -> String {
    "sent".to_string()
}
fn default_ack_status() -> String {
    "pending".to_string()
}

/// 用户通知偏好
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationPreference {
    pub user_id: String,
    #[serde(default = "default_true")]
    pub in_app_enabled: bool,
    #[serde(default)]
    pub external_enabled: bool,
    #[serde(default = "default_none_channel")]
    pub external_channel: String,
    pub mute_start: Option<String>,
    pub mute_end: Option<String>,
    #[serde(default = "default_true")]
    pub critical_override: bool,
    #[serde(default)]
    pub category_overrides: HashMap<String, bool>,
    pub updated_at: DateTime<Utc>,
}

fn default_true() -> bool {
    true
}
fn default_none_channel() -> String {
    "none".to_string()
}
