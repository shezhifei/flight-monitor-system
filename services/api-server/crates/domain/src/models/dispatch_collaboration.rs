//! 派工协作 / 聊天领域模型。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchCollaborationEvent {
    pub event_id: String,
    pub flight_id: String,
    pub dispatch_order_id: Option<String>,
    pub group_id: Option<String>,
    pub event_type: String,
    pub actor_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_username: Option<String>,
    pub correlation_id: Option<String>,
    #[serde(default)]
    pub payload: serde_json::Value,
    pub occurred_at: DateTime<Utc>,
    pub source_table: Option<String>,
    pub source_record_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchChatMember {
    pub id: String,
    pub group_id: String,
    pub user_id: String,
    pub username: Option<String>,
    #[serde(default)]
    pub is_assignee: bool,
    #[serde(default)]
    pub is_dispatcher: bool,
    #[serde(default = "default_true")]
    pub is_active: bool,
    pub joined_at: Option<DateTime<Utc>>,
    pub left_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_read_seq: i64,
    pub last_read_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct DispatchChatMemberUpsert {
    pub user_id: String,
    pub is_assignee: bool,
    pub is_dispatcher: bool,
    pub last_read_seq: i64,
    pub last_read_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchChatUserProfile {
    pub user_id: String,
    pub username: Option<String>,
    pub department: Option<String>,
    pub job_title: Option<String>,
    #[serde(default = "default_true")]
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchChatDispatcherCandidate {
    pub user_id: String,
    pub username: Option<String>,
    pub department: Option<String>,
    pub job_title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchChatGroupSummary {
    pub group_id: String,
    pub channel_type: String,
    pub flight_id: String,
    pub group_name: String,
    #[serde(default = "default_active")]
    pub status: String,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default)]
    pub deprecated: bool,
    pub deprecated_at: Option<DateTime<Utc>>,
    pub deprecation_reason: Option<String>,
    pub archive_at: Option<DateTime<Utc>>,
    pub archived_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub metadata: serde_json::Value,
    #[serde(default)]
    pub member_count: i64,
    #[serde(default)]
    pub unread_count: i64,
    pub last_message_seq: Option<i64>,
    pub last_message_preview: Option<String>,
    pub last_message_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing)]
    pub member_is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchChatGroupList {
    #[serde(default)]
    pub items: Vec<DispatchChatGroupSummary>,
    #[serde(default)]
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
    #[serde(default)]
    pub unread_total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchChatMessage {
    pub message_id: String,
    pub seq_no: i64,
    pub group_id: String,
    pub sender_user_id: Option<String>,
    pub sender_username: Option<String>,
    #[serde(default = "default_text")]
    pub message_type: String,
    pub content: String,
    #[serde(default)]
    pub is_at_all: bool,
    #[serde(default)]
    pub metadata: serde_json::Value,
    pub sent_at: DateTime<Utc>,
    #[serde(skip_serializing)]
    pub dispatch_order_id: Option<String>,
    #[serde(skip_serializing)]
    pub event_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchChatMessageList {
    #[serde(default)]
    pub items: Vec<DispatchChatMessage>,
    #[serde(default)]
    pub total: i64,
    pub limit: i64,
    pub before_seq: Option<i64>,
    #[serde(default)]
    pub has_more: bool,
    pub next_before_seq: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct NewDispatchChatMessage {
    pub message_id: String,
    pub group_id: String,
    pub sender_user_id: Option<String>,
    pub dispatch_order_id: Option<String>,
    pub event_id: Option<String>,
    pub message_type: String,
    pub content: String,
    pub is_at_all: bool,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationReceiptSummary {
    #[serde(default)]
    pub total_count: i64,
    #[serde(default)]
    pub pending_count: i64,
    #[serde(default)]
    pub acknowledged_count: i64,
    #[serde(default)]
    pub rejected_count: i64,
    pub latest_updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub receipt_group_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchFlightCollaborationView {
    pub flight_id: String,
    #[serde(default)]
    pub orders: Vec<serde_json::Value>,
    pub group: Option<DispatchChatGroupSummary>,
    #[serde(default)]
    pub recent_messages: Vec<DispatchChatMessage>,
    #[serde(default)]
    pub recent_notifications: Vec<serde_json::Value>,
    pub notification_receipt_summary: NotificationReceiptSummary,
    #[serde(default)]
    pub events: Vec<DispatchCollaborationEvent>,
    #[serde(default)]
    pub total_orders: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchOrderCollaborationView {
    pub order: serde_json::Value,
    pub group: Option<DispatchChatGroupSummary>,
    #[serde(default)]
    pub recent_messages: Vec<DispatchCollaborationEvent>,
    #[serde(default)]
    pub recent_notifications: Vec<serde_json::Value>,
    pub notification_receipt_summary: NotificationReceiptSummary,
    #[serde(default)]
    pub events: Vec<DispatchCollaborationEvent>,
}

fn default_true() -> bool {
    true
}

fn default_active() -> String {
    "active".to_string()
}

fn default_text() -> String {
    "text".to_string()
}
