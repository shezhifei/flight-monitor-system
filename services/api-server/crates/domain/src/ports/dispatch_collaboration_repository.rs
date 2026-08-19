//! 派工协作 / 聊天查询与写入仓储接口。

use crate::error::DomainError;
use crate::models::dispatch_collaboration::{
    DispatchChatDispatcherCandidate, DispatchChatGroupList, DispatchChatGroupSummary, DispatchChatMember,
    DispatchChatMemberUnread, DispatchChatMemberUpsert, DispatchChatMessage, DispatchChatMessageCursor,
    DispatchChatMessageList, DispatchChatReadCursorUpdate, DispatchChatUserProfile, DispatchCollaborationEvent,
    NewDispatchChatMessage, NotificationReceiptSummary,
};
use crate::models::notification::Notification;
use async_trait::async_trait;

#[async_trait]
pub trait DispatchCollaborationRepository {
    async fn get_group_by_id(&self, group_id: &str) -> Result<Option<DispatchChatGroupSummary>, DomainError>;

    async fn get_group_for_user(
        &self,
        group_id: &str,
        user_id: &str,
    ) -> Result<Option<DispatchChatGroupSummary>, DomainError>;

    async fn get_group_for_user_by_flight(
        &self,
        flight_id: &str,
        user_id: &str,
    ) -> Result<Option<DispatchChatGroupSummary>, DomainError>;

    async fn get_group_by_flight(&self, flight_id: &str) -> Result<Option<DispatchChatGroupSummary>, DomainError>;

    async fn list_user_groups(
        &self,
        user_id: &str,
        status: &str,
        limit: i64,
        offset: i64,
    ) -> Result<DispatchChatGroupList, DomainError>;

    async fn list_group_messages(
        &self,
        group_id: &str,
        limit: i64,
        cursor: DispatchChatMessageCursor,
    ) -> Result<DispatchChatMessageList, DomainError>;

    async fn insert_message(&self, message: &NewDispatchChatMessage) -> Result<DispatchChatMessage, DomainError>;

    /// Look up an already-stored message by its client idempotency key, so a
    /// retried send returns the original instead of creating a duplicate.
    async fn find_message_by_client_id(
        &self,
        group_id: &str,
        client_msg_id: &str,
    ) -> Result<Option<DispatchChatMessage>, DomainError>;

    async fn update_message_event_id(
        &self,
        message_id: &str,
        event_id: &str,
    ) -> Result<Option<DispatchChatMessage>, DomainError>;

    /// Moves the member's read cursor forward to `read_seq` (never backwards).
    ///
    /// Returns the previous cursor alongside the updated member so callers can
    /// distinguish a real advance from a no-op re-read.
    async fn mark_group_read(
        &self,
        group_id: &str,
        user_id: &str,
        read_seq: i64,
    ) -> Result<Option<DispatchChatReadCursorUpdate>, DomainError>;

    async fn get_group_latest_seq(&self, group_id: &str) -> Result<i64, DomainError>;

    async fn count_group_unread(&self, group_id: &str, user_id: &str) -> Result<i64, DomainError>;

    async fn count_total_unread(&self, user_id: &str) -> Result<i64, DomainError>;

    /// Batch form of `count_group_unread` + `count_total_unread` for every
    /// active member of a group.
    ///
    /// Fanning a new message out to M members otherwise costs 2×M round trips;
    /// this resolves both numbers for all members in one query.
    async fn count_unread_for_group_members(
        &self,
        group_id: &str,
    ) -> Result<Vec<DispatchChatMemberUnread>, DomainError>;

    async fn find_active_members(&self, group_id: &str) -> Result<Vec<DispatchChatMember>, DomainError>;

    async fn find_users_by_ids(&self, user_ids: &[String]) -> Result<Vec<DispatchChatUserProfile>, DomainError>;

    async fn find_dispatchers_by_departments(
        &self,
        departments: &[String],
    ) -> Result<Vec<DispatchChatDispatcherCandidate>, DomainError>;

    async fn upsert_group_for_flight(
        &self,
        flight_id: &str,
        group_name: &str,
        archive_at: Option<chrono::DateTime<chrono::Utc>>,
        metadata: &serde_json::Value,
    ) -> Result<DispatchChatGroupSummary, DomainError>;

    async fn upsert_group_memberships(
        &self,
        group_id: &str,
        memberships: &[DispatchChatMemberUpsert],
    ) -> Result<(), DomainError>;

    async fn deactivate_members_except(
        &self,
        group_id: &str,
        active_user_ids: &[String],
    ) -> Result<Vec<DispatchChatMember>, DomainError>;

    async fn clear_group_deprecation(
        &self,
        group_id: &str,
        reason: &str,
    ) -> Result<Option<DispatchChatGroupSummary>, DomainError>;

    async fn mark_group_deprecated(
        &self,
        group_id: &str,
        reason: &str,
    ) -> Result<Option<DispatchChatGroupSummary>, DomainError>;

    async fn find_groups_pending_deprecation(&self, limit: i64) -> Result<Vec<DispatchChatGroupSummary>, DomainError>;

    async fn find_due_archive_groups(&self, limit: i64) -> Result<Vec<DispatchChatGroupSummary>, DomainError>;

    async fn archive_groups_batch(&self, group_ids: &[String]) -> Result<Vec<DispatchChatGroupSummary>, DomainError>;

    async fn create_event(&self, event: &DispatchCollaborationEvent)
        -> Result<DispatchCollaborationEvent, DomainError>;

    async fn list_events_by_flight(
        &self,
        flight_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<DispatchCollaborationEvent>, DomainError>;

    async fn list_events_by_order(
        &self,
        order_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<DispatchCollaborationEvent>, DomainError>;

    async fn find_recent_notifications_by_flight(
        &self,
        flight_id: &str,
        limit: i64,
    ) -> Result<Vec<Notification>, DomainError>;

    async fn find_recent_notifications_by_order(
        &self,
        order_id: &str,
        limit: i64,
    ) -> Result<Vec<Notification>, DomainError>;

    async fn summarize_receipts_for_flight(&self, flight_id: &str) -> Result<NotificationReceiptSummary, DomainError>;

    async fn summarize_receipts_for_order(&self, order_id: &str) -> Result<NotificationReceiptSummary, DomainError>;
}
