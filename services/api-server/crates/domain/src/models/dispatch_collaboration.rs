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

/// Result of moving a member's read cursor.
///
/// Carries the cursor value from *before* the write so callers can tell a real
/// advance from an idempotent re-read; only a real advance is worth an audit
/// ledger row and an SSE fan-out.
#[derive(Debug, Clone)]
pub struct DispatchChatReadCursorUpdate {
    pub member: DispatchChatMember,
    pub previous_last_read_seq: i64,
}

impl DispatchChatReadCursorUpdate {
    /// True when the write actually moved the cursor forward.
    pub fn advanced(&self) -> bool {
        self.member.last_read_seq > self.previous_last_read_seq
    }
}

/// Per-member unread badge numbers for one group, resolved in a single query.
#[derive(Debug, Clone)]
pub struct DispatchChatMemberUnread {
    pub user_id: String,
    /// Unread messages in this group.
    pub unread_count: i64,
    /// Unread messages across every active group this member belongs to.
    pub unread_total: i64,
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
    /// Mentions stored in `metadata.mention_user_ids`, echoed so clients do not
    /// have to dig into metadata. Empty when the message did not @ anyone.
    #[serde(default)]
    pub mention_user_ids: Vec<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
    pub sent_at: DateTime<Utc>,
    /// Client-supplied idempotency key, unique per group. Echoed back so a
    /// client can match the stored message to its optimistic placeholder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_msg_id: Option<String>,
    #[serde(skip_serializing)]
    pub dispatch_order_id: Option<String>,
    #[serde(skip_serializing)]
    pub event_id: Option<String>,
}

impl DispatchChatMessage {
    /// Pulls `mention_user_ids` out of message metadata, ignoring blanks.
    pub fn mention_user_ids_from_metadata(metadata: &serde_json::Value) -> Vec<String> {
        metadata
            .get("mention_user_ids")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .collect()
    }
}

/// Which slice of a group's history to read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DispatchChatMessageCursor {
    /// The newest page.
    #[default]
    Latest,
    /// Older history, strictly before this `seq_no` — scroll-back.
    Before(i64),
    /// Messages after this `seq_no` — the reconnect gap-fill direction, for a
    /// client that knows the last seq it rendered.
    ///
    /// `seq_no` comes from a table-global `BIGSERIAL` assigned at INSERT but
    /// only visible at COMMIT, so in principle a lower seq_no can appear after
    /// a higher one has already been streamed. A client that must not miss a
    /// message should reconcile the returned page by `message_id` rather than
    /// trusting the cursor alone.
    After(i64),
}

impl DispatchChatMessageCursor {
    pub fn before_seq(&self) -> Option<i64> {
        match self {
            Self::Before(seq) => Some(*seq),
            _ => None,
        }
    }

    pub fn after_seq(&self) -> Option<i64> {
        match self {
            Self::After(seq) => Some(*seq),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchChatMessageList {
    #[serde(default)]
    pub items: Vec<DispatchChatMessage>,
    #[serde(default)]
    pub total: i64,
    pub limit: i64,
    pub before_seq: Option<i64>,
    pub after_seq: Option<i64>,
    #[serde(default)]
    pub has_more: bool,
    pub next_before_seq: Option<i64>,
    pub next_after_seq: Option<i64>,
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
    /// Idempotency key from the sending client. `None` for server-originated
    /// messages, which are not retried by a client and need no key.
    pub client_msg_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
