use std::future::Future;
use std::pin::Pin;

use fms_domain::error::DomainError;

pub trait NotificationDeliveryPublisher: Send + Sync {
    fn publish_user_notification<'a>(
        &'a self,
        notification: &'a super::NotificationResponse,
        unread_count: i64,
    ) -> Pin<Box<dyn Future<Output = Result<usize, DomainError>> + Send + 'a>>;

    fn publish_sender_receipt_update<'a>(
        &'a self,
        sender_user_id: &'a str,
        payload: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<usize, DomainError>> + Send + 'a>>;
}

pub trait NotificationMetricsRecorder: Send + Sync {
    fn record_delivery_attempt(&self, channel: &str, success: bool);
    fn record_backfill_pending(&self);
}

pub trait NotificationReceiptGroupSync: Send + Sync {
    fn sync_receipt_group<'a>(
        &'a self,
        receipt_group_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), DomainError>> + Send + 'a>>;
}
