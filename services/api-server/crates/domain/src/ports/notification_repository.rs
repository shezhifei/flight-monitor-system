//! 通知仓储 trait

use crate::error::DomainError;
use crate::models::notification::{Notification, NotificationPreference};
use async_trait::async_trait;

/// 通知仓储接口
#[async_trait]
pub trait NotificationRepository {
    async fn save(&self, notification: &Notification) -> Result<(), DomainError>;
    async fn find_by_id(&self, notification_id: &str) -> Result<Option<Notification>, DomainError>;
    async fn find_by_id_for_user(
        &self,
        notification_id: &str,
        user_id: &str,
    ) -> Result<Option<Notification>, DomainError>;
    async fn find_by_user(
        &self,
        user_id: &str,
        unread_only: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Notification>, DomainError>;
    async fn mark_read(&self, notification_id: &str, user_id: &str) -> Result<bool, DomainError>;
    async fn mark_delivered(&self, notification_id: &str, user_id: &str) -> Result<bool, DomainError>;
    async fn mark_all_read(&self, user_id: &str) -> Result<i64, DomainError>;
    async fn count_unread(&self, user_id: &str) -> Result<i64, DomainError>;
    async fn acknowledge(
        &self,
        notification_id: &str,
        user_id: &str,
        action: &str,
        note: Option<&str>,
    ) -> Result<Option<Notification>, DomainError>;
    async fn find_by_receipt_group(&self, receipt_group_id: &str) -> Result<Vec<Notification>, DomainError>;
    async fn summarize_receipt_group(&self, receipt_group_id: &str) -> Result<Option<serde_json::Value>, DomainError>;
    async fn list_sent_receipt_groups(
        &self,
        sender_user_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<serde_json::Value>, DomainError>;
}

#[async_trait]
pub trait NotificationTransactionalRepository<Tx>: Send + Sync {
    async fn save_in_tx(&self, tx: &mut Tx, notification: &Notification) -> Result<(), DomainError>;
}

/// 通知偏好仓储接口
#[async_trait]
pub trait NotificationPreferenceRepository {
    async fn find_by_user(&self, user_id: &str) -> Result<Option<NotificationPreference>, DomainError>;
    async fn save(&self, pref: &NotificationPreference) -> Result<(), DomainError>;
}
