use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct NotificationCreate {
    pub user_id: String,
    pub title: String,
    pub body: String,
    pub category: Option<String>,
    pub severity: Option<String>,
    pub flight_id: Option<String>,
    pub related_entity_type: Option<String>,
    pub related_entity_id: Option<String>,
    pub dispatch_order_id: Option<String>,
    pub group_id: Option<String>,
    pub sender_user_id: Option<String>,
    pub sender_username_snapshot: Option<String>,
    pub origin_type: Option<String>,
    #[serde(default)]
    pub receipt_required: bool,
    pub receipt_group_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DispatchBatchNotificationCreate {
    pub user_ids: Vec<String>,
    pub title: String,
    pub body: String,
    pub category: String,
    pub severity: String,
    pub flight_id: Option<String>,
    pub related_entity_type: Option<String>,
    pub related_entity_id: Option<String>,
    pub dispatch_order_id: Option<String>,
    pub group_id: Option<String>,
    pub sender_user_id: Option<String>,
    pub sender_username_snapshot: Option<String>,
    pub origin_type: String,
    pub receipt_required: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NotificationPreferenceUpdate {
    pub in_app_enabled: Option<bool>,
    pub external_enabled: Option<bool>,
    pub external_channel: Option<String>,
    pub mute_start: Option<String>,
    pub mute_end: Option<String>,
    pub critical_override: Option<bool>,
    pub category_overrides: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationResponse {
    pub notification_id: String,
    pub user_id: String,
    pub title: String,
    pub body: String,
    pub category: String,
    pub severity: String,
    pub is_read: bool,
    pub read_status: String,
    pub delivery_status: String,
    pub delivered_at: Option<chrono::DateTime<chrono::Utc>>,
    pub origin_type: String,
    pub origin_label: String,
    pub receipt_required: bool,
    pub receipt_group_id: Option<String>,
    pub ack_status: String,
    pub ack_at: Option<chrono::DateTime<chrono::Utc>>,
    pub ack_note: Option<String>,
    pub related_entity_type: Option<String>,
    pub related_entity_id: Option<String>,
    pub flight_id: Option<String>,
    pub group_id: Option<String>,
    pub sender_user_id: Option<String>,
    pub sender_username: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub read_at: Option<chrono::DateTime<chrono::Utc>>,
}
