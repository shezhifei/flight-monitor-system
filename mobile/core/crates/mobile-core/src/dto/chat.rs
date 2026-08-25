//! Dispatch collaboration chat DTOs.
//!
//! Backend routes live under `/api/v2/dispatch/collaboration/*`.
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
    #[serde(default)]
    pub mention_user_ids: Vec<String>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    pub sent_at: String,
}

impl ChatMessage {
    /// Top-level `mention_user_ids`, falling back to `metadata.mention_user_ids`
    /// for older payloads that only stored mentions in metadata.
    pub fn mention_user_ids_resolved(&self) -> Vec<String> {
        if !self.mention_user_ids.is_empty() {
            return self.mention_user_ids.clone();
        }
        self.metadata
            .as_ref()
            .and_then(|m| m.get("mention_user_ids"))
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
    #[serde(default)]
    pub mention_user_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_msg_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatMember {
    pub user_id: String,
    pub username: String,
    #[serde(default)]
    pub is_assignee: bool,
    #[serde(default)]
    pub is_dispatcher: bool,
    #[serde(default)]
    pub is_active: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatMemberList {
    #[serde(default)]
    pub items: Vec<ChatMember>,
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
    #[serde(default, alias = "last_read_seq")]
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

    #[test]
    fn read_result_accepts_last_read_seq_alias() {
        let raw = r#"{"group_id":"g1","last_read_seq":5,"unread_count":0,"unread_total":0}"#;
        let got: ChatReadResult = serde_json::from_str(raw).unwrap();
        assert_eq!(got.read_seq, Some(5));
        assert_eq!(got.unread_count, 0);
    }

    #[test]
    fn message_parses_top_level_mention_user_ids() {
        let raw = r#"{
            "message_id":"m1","seq_no":1,"group_id":"g1",
            "sender_user_id":"u1","sender_username":"alice",
            "message_type":"text","content":"hi @bob",
            "is_at_all":false,
            "mention_user_ids":["u2"],
            "sent_at":"2026-01-01T00:00:00Z"
        }"#;
        let msg: ChatMessage = serde_json::from_str(raw).unwrap();
        assert_eq!(msg.mention_user_ids, vec!["u2".to_string()]);
        assert_eq!(msg.mention_user_ids_resolved(), vec!["u2".to_string()]);
    }

    #[test]
    fn message_falls_back_to_metadata_mention_user_ids() {
        let raw = r#"{
            "message_id":"m1","seq_no":1,"group_id":"g1",
            "sender_user_id":"u1","sender_username":"alice",
            "content":"hi @bob",
            "sent_at":"2026-01-01T00:00:00Z",
            "metadata":{"mention_user_ids":["u2","u3"]}
        }"#;
        let msg: ChatMessage = serde_json::from_str(raw).unwrap();
        assert!(msg.mention_user_ids.is_empty());
        assert_eq!(
            msg.mention_user_ids_resolved(),
            vec!["u2".to_string(), "u3".to_string()]
        );
    }

    #[test]
    fn send_request_serializes_mention_user_ids() {
        let req = ChatSendMessageRequest {
            content: "hi".into(),
            at_all: false,
            mention_user_ids: vec!["u2".into()],
            client_msg_id: Some("c1".into()),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["mention_user_ids"], serde_json::json!(["u2"]));
        assert_eq!(json["client_msg_id"], "c1");
        assert_eq!(json["at_all"], false);
        assert_eq!(json["content"], "hi");
    }
}
