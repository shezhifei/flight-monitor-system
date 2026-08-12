//! Notification DTOs (plan §0.5 Notifications group).
//!
//! List / unread-count / receipt endpoints return **raw** objects.
//! mark-read / ack / read-all return the standard envelope.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NotificationItem {
    pub notification_id: String,
    pub user_id: String,
    pub title: String,
    pub body: String,
    pub category: String,
    pub severity: String,
    #[serde(default)]
    pub is_read: bool,
    #[serde(default = "default_unread")]
    pub read_status: String,
    #[serde(default = "default_sent")]
    pub delivery_status: String,
    pub delivered_at: Option<String>,
    #[serde(default = "default_manual")]
    pub origin_type: String,
    #[serde(default = "default_origin_label")]
    pub origin_label: String,
    #[serde(default)]
    pub receipt_required: bool,
    pub receipt_group_id: Option<String>,
    #[serde(default = "default_pending")]
    pub ack_status: String,
    pub ack_at: Option<String>,
    pub ack_note: Option<String>,
    pub related_entity_type: Option<String>,
    pub related_entity_id: Option<String>,
    pub created_at: String,
    pub read_at: Option<String>,
}

fn default_unread() -> String {
    "unread".to_string()
}
fn default_sent() -> String {
    "sent".to_string()
}
fn default_manual() -> String {
    "manual".to_string()
}
fn default_origin_label() -> String {
    "人工".to_string()
}
fn default_pending() -> String {
    "pending".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NotificationListResponse {
    #[serde(default)]
    pub items: Vec<NotificationItem>,
    #[serde(default)]
    pub total: i64,
    #[serde(default)]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NotificationUnreadCountResponse {
    #[serde(default)]
    pub unread_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct NotificationAcknowledgeRequest {
    pub action: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NotificationReceipt {
    pub notification_id: String,
    pub user_id: String,
    pub title: Option<String>,
    #[serde(default = "default_manual")]
    pub origin_type: String,
    #[serde(default = "default_origin_label")]
    pub origin_label: String,
    pub receipt_group_id: Option<String>,
    pub delivery_status: String,
    pub delivered_at: Option<String>,
    pub read_status: String,
    pub read_at: Option<String>,
    pub ack_status: String,
    pub ack_at: Option<String>,
    pub ack_note: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NotificationReceiptSummary {
    #[serde(default)]
    pub total_count: i64,
    #[serde(default)]
    pub pending_count: i64,
    #[serde(default)]
    pub acknowledged_count: i64,
    #[serde(default)]
    pub rejected_count: i64,
    pub latest_updated_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NotificationReceiptGroup {
    pub receipt_group_id: String,
    pub title: Option<String>,
    pub flight_id: Option<String>,
    pub dispatch_order_id: Option<String>,
    pub group_id: Option<String>,
    #[serde(default = "default_manual")]
    pub origin_type: String,
    #[serde(default = "default_origin_label")]
    pub origin_label: String,
    #[serde(default = "default_true")]
    pub receipt_required: bool,
    pub summary: NotificationReceiptSummary,
    #[serde(default)]
    pub items: Vec<NotificationReceipt>,
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_list_and_unread_parse() {
        let list: NotificationListResponse =
            serde_json::from_str(r#"{"items":[],"limit":5,"offset":0,"total":0}"#).unwrap();
        assert_eq!(list.total, 0);
        let unread: NotificationUnreadCountResponse =
            serde_json::from_str(r#"{"unread_count":0}"#).unwrap();
        assert_eq!(unread.unread_count, 0);
    }
}
