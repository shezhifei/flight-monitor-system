//! Chat exports.

use mobile_core::dto::chat as core;

use super::runtime;

/// Mirror of `ChatGroupSummary`.
pub struct ChatGroup {
    pub group_id: String,
    pub channel_type: String,
    pub flight_id: String,
    pub group_name: String,
    pub status: String,
    pub read_only: bool,
    pub member_count: i64,
    pub unread_count: i64,
    pub last_message_seq: Option<i64>,
    pub last_message_preview: Option<String>,
    pub last_message_at: Option<String>,
}

impl From<core::ChatGroupSummary> for ChatGroup {
    fn from(g: core::ChatGroupSummary) -> Self {
        Self {
            group_id: g.group_id,
            channel_type: g.channel_type,
            flight_id: g.flight_id,
            group_name: g.group_name,
            status: g.status,
            read_only: g.read_only,
            member_count: g.member_count,
            unread_count: g.unread_count,
            last_message_seq: g.last_message_seq,
            last_message_preview: g.last_message_preview,
            last_message_at: g.last_message_at,
        }
    }
}

/// Mirror of `ChatGroupListResponse`.
pub struct ChatGroupList {
    pub items: Vec<ChatGroup>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
    pub unread_total: i64,
}

impl From<core::ChatGroupListResponse> for ChatGroupList {
    fn from(r: core::ChatGroupListResponse) -> Self {
        Self {
            items: r.items.into_iter().map(Into::into).collect(),
            total: r.total,
            limit: r.limit,
            offset: r.offset,
            unread_total: r.unread_total,
        }
    }
}

/// Mirror of `ChatMessage`.
pub struct ChatMessage {
    pub message_id: String,
    pub seq_no: i64,
    pub group_id: String,
    pub sender_user_id: Option<String>,
    pub sender_username: Option<String>,
    pub message_type: String,
    pub content: String,
    pub is_at_all: bool,
    pub sent_at: String,
}

impl From<core::ChatMessage> for ChatMessage {
    fn from(m: core::ChatMessage) -> Self {
        Self {
            message_id: m.message_id,
            seq_no: m.seq_no,
            group_id: m.group_id,
            sender_user_id: m.sender_user_id,
            sender_username: m.sender_username,
            message_type: m.message_type,
            content: m.content,
            is_at_all: m.is_at_all,
            sent_at: m.sent_at,
        }
    }
}

/// Mirror of `ChatMessageListResponse`.
pub struct ChatMessageList {
    pub items: Vec<ChatMessage>,
    pub total: i64,
    pub limit: i64,
    pub before_seq: Option<i64>,
    pub has_more: bool,
    pub next_before_seq: Option<i64>,
}

impl From<core::ChatMessageListResponse> for ChatMessageList {
    fn from(r: core::ChatMessageListResponse) -> Self {
        Self {
            items: r.items.into_iter().map(Into::into).collect(),
            total: r.total,
            limit: r.limit,
            before_seq: r.before_seq,
            has_more: r.has_more,
            next_before_seq: r.next_before_seq,
        }
    }
}

/// Mirror of `ChatReadResult`.
pub struct ChatReadResult {
    pub group_id: Option<String>,
    pub unread_count: i64,
    pub unread_total: i64,
    pub read_seq: Option<i64>,
    pub read_at: Option<String>,
}

impl From<core::ChatReadResult> for ChatReadResult {
    fn from(r: core::ChatReadResult) -> Self {
        Self {
            group_id: r.group_id,
            unread_count: r.unread_count,
            unread_total: r.unread_total,
            read_seq: r.read_seq,
            read_at: r.read_at,
        }
    }
}

/// `GET /api/v2/dispatch/collaboration/groups`.
pub async fn chat_groups(
    status: String,
    limit: i64,
    offset: i64,
) -> anyhow::Result<ChatGroupList> {
    let rt = runtime()?;
    Ok(mobile_core::api::chat::chat_groups(&rt.client, &status, limit, offset)
        .await?
        .into())
}

/// `GET .../groups/{group_id}/messages`.
pub async fn chat_messages(
    group_id: String,
    limit: i64,
    before_seq: Option<i64>,
) -> anyhow::Result<ChatMessageList> {
    let rt = runtime()?;
    Ok(
        mobile_core::api::chat::chat_messages(&rt.client, &group_id, limit, before_seq)
            .await?
            .into(),
    )
}

/// `POST .../groups/{group_id}/messages`.
pub async fn send_chat_message(
    group_id: String,
    content: String,
    at_all: bool,
) -> anyhow::Result<ChatMessage> {
    let rt = runtime()?;
    Ok(
        mobile_core::api::chat::send_chat_message(&rt.client, &group_id, &content, at_all)
            .await?
            .into(),
    )
}

/// `POST .../groups/{group_id}/read`.
pub async fn mark_chat_read(
    group_id: String,
    read_seq: Option<i64>,
) -> anyhow::Result<ChatReadResult> {
    let rt = runtime()?;
    Ok(
        mobile_core::api::chat::mark_chat_read(&rt.client, &group_id, read_seq)
            .await?
            .into(),
    )
}
