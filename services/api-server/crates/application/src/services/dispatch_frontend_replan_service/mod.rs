mod helpers;
pub mod service;
#[cfg(test)]
mod test_support;

use std::pin::Pin;

use serde_json::Value;

use crate::services::dispatch_chat_service::DispatchChatEventPublisher;
use crate::services::notification_service::{
    NotificationDeliveryPublisher, NotificationMetricsRecorder, NotificationReceiptGroupSync, NotificationResponse,
};
use fms_domain::error::DomainError;
use fms_domain::ports::NullRepository;

impl NotificationDeliveryPublisher for NullRepository {
    fn publish_user_notification<'a>(
        &'a self,
        _notification: &'a NotificationResponse,
        _unread_count: i64,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<usize, DomainError>> + Send + 'a>> {
        Box::pin(async { Ok(0) })
    }
    fn publish_sender_receipt_update<'a>(
        &'a self,
        _sender_user_id: &'a str,
        _payload: Value,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<usize, DomainError>> + Send + 'a>> {
        Box::pin(async { Ok(0) })
    }
}

impl NotificationMetricsRecorder for NullRepository {
    fn record_delivery_attempt(&self, _channel: &str, _success: bool) {}
    fn record_backfill_pending(&self) {}
}

impl NotificationReceiptGroupSync for NullRepository {
    fn sync_receipt_group<'a>(
        &'a self,
        _receipt_group_id: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), DomainError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
}

impl DispatchChatEventPublisher for NullRepository {
    fn publish_user_event<'a>(
        &'a self,
        _event_name: &'a str,
        _events: Vec<(String, Value)>,
    ) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        Box::pin(async {})
    }
}

pub use service::DispatchFrontendReplanService;
