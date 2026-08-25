use std::future::Future;
use std::pin::Pin;

use fms_domain::error::DomainError;
use fms_domain::models::dispatch_collaboration::DispatchCollaborationEvent;

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

/// 通知服务在协作流里只落一条事件。`DispatchCollaborationRepository` 有 33 个方法，
/// 为了调其中 1 个就要求每个构造点提供 33 个——这正是这个依赖当初只能是可选的原因。
/// 收窄成 1 个方法之后，它才可能是必填的。
pub trait NotificationCollaborationEvents: Send + Sync {
    fn create_event<'a>(
        &'a self,
        event: &'a DispatchCollaborationEvent,
    ) -> Pin<Box<dyn Future<Output = Result<(), DomainError>> + Send + 'a>>;
}
