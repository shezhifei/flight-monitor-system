//! Dispatch collaboration chat DTOs (plan §0.5 Chat group).
//!
//! Backend routes live under `/api/v2/dispatch/collaboration/*`
//! (NOT the legacy Python `/api/v2/dispatch-chat/*` which 404s).
//! List / message / send / mark-read responses are **raw** (no envelope).

use serde::{Deserialize, Serialize};

/// One chat group summary as returned by `GET .../groups`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatGroupSummary {
    pub group_id: String,
    pub channel_type: String,
    pub flight_id: String,
    pub group_name: String,
    #[serde(default = "default_active")]
    pub status: String,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default)]
    pub member_count: i64,
    #[serde(default)]
    pub unread_count: i64,
    pub last_message_seq: Option<i64>,
    pub last_message_preview: Option<String>,
    pub last_message_at: Option<String>,
}

fn default_active() -> String {
    "active".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatGroupListResponse {
    #[serde(default)]
    pub items: Vec<ChatGroupSummary>,
    #[serde(default)]
    pub total: i64,
    #[serde(default)]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    #[serde(default)]
    pub unread_total: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatMessage {
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
    pub sent_at: String,
}

fn default_text() -> String {
    "text".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatMessageListResponse {
    #[serde(default)]
    pub items: Vec<ChatMessage>,
    #[serde(default)]
    pub total: i64,
    #[serde(default)]
    pub limit: i64,
    pub before_seq: Option<i64>,
    #[serde(default)]
    pub has_more: bool,
    pub next_before_seq: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatSendMessageRequest {
    pub content: String,
    #[serde(default)]
    pub at_all: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatMarkReadRequest {
    pub read_seq: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatReadResult {
    pub group_id: Option<String>,
    #[serde(default)]
    pub unread_count: i64,
    #[serde(default)]
    pub unread_total: i64,
    pub read_seq: Option<i64>,
    pub read_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_list_parses_live_shape() {
        let raw = r#"{
            "items":[{
                "group_id":"g1","channel_type":"system_flight_dispatch",
                "flight_id":"f1","group_name":"CK230 保障协同群",
                "status":"active","read_only":false,"member_count":1,
                "unread_count":0,"last_message_seq":null,
                "last_message_preview":null,"last_message_at":null,
                "metadata":{"source":"dispatch_chat_v2"}
            }],
            "total":1,"limit":5,"offset":0,"unread_total":0
        }"#;
        let list: ChatGroupListResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(list.items.len(), 1);
        assert_eq!(list.items[0].group_name, "CK230 保障协同群");
        assert!(!list.items[0].read_only);
    }

    #[test]
    fn message_list_parses_empty() {
        let raw = r#"{"items":[],"total":0,"limit":5,"before_seq":null,"has_more":false,"next_before_seq":null}"#;
        let list: ChatMessageListResponse = serde_json::from_str(raw).unwrap();
        assert!(list.items.is_empty());
        assert!(!list.has_more);
    }
}
