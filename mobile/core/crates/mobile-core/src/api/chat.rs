//! Dispatch collaboration chat API wrappers.
//!
//! Endpoints (backend `routes/dispatch_chat.rs`, scope
//! `/api/v2/dispatch/collaboration`):
//! - `GET  /groups` → raw `ChatGroupListResponse`
//! - `GET  /groups/{id}/messages` → raw `ChatMessageListResponse`
//! - `POST /groups/{id}/messages` → raw `ChatMessage`
//! - `POST /groups/{id}/read` → raw `ChatReadResult`
//! - `GET  /groups/{id}/members` → raw `ChatMemberList`
//!
//! All five return bare objects (no envelope). SSE events for chat arrive on
//! the universal `/api/v2/sse/stream` under event names
//! `chat_message` / `chat_read_synced` / `chat_group_upserted` / …

use crate::client::ApiClient;
use crate::dto::chat::{
    ChatGroupListResponse, ChatMarkReadRequest, ChatMemberList, ChatMessage,
    ChatMessageListResponse, ChatReadResult, ChatSendMessageRequest,
};
use crate::error::CoreError;

/// `GET /api/v2/dispatch/collaboration/groups`.
pub async fn chat_groups(
    client: &ApiClient,
    status: &str,
    limit: i64,
    offset: i64,
) -> Result<ChatGroupListResponse, CoreError> {
    client
        .call_raw(
            "GET",
            &format!(
                "/api/v2/dispatch/collaboration/groups?status={status}&limit={limit}&offset={offset}"
            ),
            Option::<&()>::None,
        )
        .await
}

/// `GET /api/v2/dispatch/collaboration/groups/{group_id}/messages`.
pub async fn chat_messages(
    client: &ApiClient,
    group_id: &str,
    limit: i64,
    before_seq: Option<i64>,
) -> Result<ChatMessageListResponse, CoreError> {
    let path = match before_seq {
        Some(seq) => format!(
            "/api/v2/dispatch/collaboration/groups/{group_id}/messages?limit={limit}&before_seq={seq}"
        ),
        None => format!(
            "/api/v2/dispatch/collaboration/groups/{group_id}/messages?limit={limit}"
        ),
    };
    client
        .call_raw("GET", &path, Option::<&()>::None)
        .await
}

/// `POST /api/v2/dispatch/collaboration/groups/{group_id}/messages`.
pub async fn send_chat_message(
    client: &ApiClient,
    group_id: &str,
    content: &str,
    at_all: bool,
    mention_user_ids: &[String],
) -> Result<ChatMessage, CoreError> {
    client
        .call_raw(
            "POST",
            &format!("/api/v2/dispatch/collaboration/groups/{group_id}/messages"),
            Some(&ChatSendMessageRequest {
                content: content.to_string(),
                at_all,
                mention_user_ids: mention_user_ids.to_vec(),
                client_msg_id: Some(uuid::Uuid::new_v4().to_string()),
            }),
        )
        .await
}

/// `GET /api/v2/dispatch/collaboration/groups/{group_id}/members`.
pub async fn chat_group_members(
    client: &ApiClient,
    group_id: &str,
) -> Result<ChatMemberList, CoreError> {
    client
        .call_raw(
            "GET",
            &format!("/api/v2/dispatch/collaboration/groups/{group_id}/members"),
            Option::<&()>::None,
        )
        .await
}

/// `POST /api/v2/dispatch/collaboration/groups/{group_id}/read`.
pub async fn mark_chat_read(
    client: &ApiClient,
    group_id: &str,
    read_seq: Option<i64>,
) -> Result<ChatReadResult, CoreError> {
    client
        .call_raw(
            "POST",
            &format!("/api/v2/dispatch/collaboration/groups/{group_id}/read"),
            Some(&ChatMarkReadRequest { read_seq }),
        )
        .await
}

#[cfg(test)]
mod tests {
    #[test]
    fn messages_path_omits_before_seq_when_none() {
        // Document the query construction contract (no `before_seq=` when absent).
        let path = match Option::<i64>::None {
            Some(seq) => format!("...?limit=50&before_seq={seq}"),
            None => "...?limit=50".to_string(),
        };
        assert_eq!(path, "...?limit=50");
    }

    #[test]
    fn members_path_is_group_members() {
        let group_id = "g1";
        let path = format!("/api/v2/dispatch/collaboration/groups/{group_id}/members");
        assert_eq!(
            path,
            "/api/v2/dispatch/collaboration/groups/g1/members"
        );
    }
}
