//! 实时事件 publisher / metrics recorder 适配器声明。
//!
//! 这些适配器把应用层的 publisher/recorder 端口绑定到具体的 SSE Hub、
//! domain_event_outbox 等基础设施实现。它们彼此独立、不依赖 `build_di_container`
//! 内部的装配局部变量，因此从 `di.rs` 抽出为独立子模块以收敛文件体积。

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use chrono::Utc;
use futures_util::stream::{FuturesUnordered, StreamExt};

use fms_api::services::performance_metrics::PerformanceMetricsService;
use fms_api::sse::hub::SseHub;

use fms_domain::error::DomainError;

use fms_application::services::business_case_service::BusinessCaseEventPublisher;
use fms_application::services::cache_invalidation_service::FlightListResponseCacheInvalidator;
use fms_application::services::dispatch_chat_service::DispatchChatEventPublisher;
use fms_application::services::domain_event_subscriber_service::BusinessCaseEventNotifier;
use fms_application::services::kpi_aggregation_service::KpiAggregationSsePublisher;
use fms_application::services::mobile_device_service::MobileRealtimeMetricsRecorder;
use fms_application::services::notification_service::{NotificationMetricsRecorder, NotificationResponse};
use fms_application::services::todo_scheduler_service::TodoSchedulerSsePublisher;

use fms_infrastructure::repositories::pg_domain_event_outbox_repository::PgDomainEventOutboxRepository;

pub(crate) struct SseDispatchChatEventPublisher {
    hub: Arc<SseHub>,
}

impl SseDispatchChatEventPublisher {
    pub(crate) fn new(hub: Arc<SseHub>) -> Self {
        Self { hub }
    }
}

impl DispatchChatEventPublisher for SseDispatchChatEventPublisher {
    fn publish_user_event<'a>(
        &'a self,
        event_name: &'a str,
        events: Vec<(String, serde_json::Value)>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            // 并行扇出：每个 user_id 独立 await hub.broadcast_event，避免慢订阅者拖累
            // 其余用户。SseHub 内部为 DashMap + tokio broadcast，线程安全。
            let futs: FuturesUnordered<_> = events
                .into_iter()
                .map(|(user_id, payload)| {
                    let hub = self.hub.clone();
                    async move {
                        let topic = format!("user_dispatch_chat_{}", user_id.trim());
                        let _ = hub.broadcast_event(&topic, Some(event_name), payload).await;
                    }
                })
                .collect();
            futs.for_each(|_| async {}).await;
        })
    }
}

pub(crate) struct SseNotificationDeliveryPublisher {
    hub: Arc<SseHub>,
    performance_metrics: Arc<PerformanceMetricsService>,
}

impl SseNotificationDeliveryPublisher {
    pub(crate) fn new(hub: Arc<SseHub>, performance_metrics: Arc<PerformanceMetricsService>) -> Self {
        Self {
            hub,
            performance_metrics,
        }
    }
}

impl fms_application::services::notification_service::NotificationDeliveryPublisher
    for SseNotificationDeliveryPublisher
{
    fn publish_user_notification<'a>(
        &'a self,
        notification: &'a NotificationResponse,
        unread_count: i64,
    ) -> Pin<Box<dyn Future<Output = Result<usize, DomainError>> + Send + 'a>> {
        Box::pin(async move {
            let topic = format!("user_notifications_{}", notification.user_id.trim());
            let payload = serde_json::json!({
                "type": "user_notification",
                "notification": notification,
                "unread_count": unread_count,
                "timestamp": Utc::now().to_rfc3339(),
            });
            let delivered = self
                .hub
                .broadcast_event(&topic, Some("user_notification"), payload)
                .await;
            self.performance_metrics
                .record_notification_delivery("sse", delivered > 0);
            Ok(delivered)
        })
    }

    fn publish_sender_receipt_update<'a>(
        &'a self,
        sender_user_id: &'a str,
        payload: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<usize, DomainError>> + Send + 'a>> {
        Box::pin(async move {
            let topic = format!("user_notifications_{}", sender_user_id.trim());
            let delivered = self
                .hub
                .broadcast_event(&topic, Some("sender_receipt_update"), payload)
                .await;
            self.performance_metrics
                .record_notification_delivery("sse", delivered > 0);
            Ok(delivered)
        })
    }
}

pub(crate) struct PerformanceNotificationMetricsRecorder {
    performance_metrics: Arc<PerformanceMetricsService>,
}

impl PerformanceNotificationMetricsRecorder {
    pub(crate) fn new(performance_metrics: Arc<PerformanceMetricsService>) -> Self {
        Self { performance_metrics }
    }
}

impl NotificationMetricsRecorder for PerformanceNotificationMetricsRecorder {
    fn record_delivery_attempt(&self, channel: &str, success: bool) {
        self.performance_metrics.record_notification_delivery(channel, success);
    }

    fn record_backfill_pending(&self) {
        self.performance_metrics.record_notification_backfill_pending();
    }
}

pub(crate) struct PerformanceMobileRealtimeMetricsRecorder {
    performance_metrics: Arc<PerformanceMetricsService>,
}

impl PerformanceMobileRealtimeMetricsRecorder {
    pub(crate) fn new(performance_metrics: Arc<PerformanceMetricsService>) -> Self {
        Self { performance_metrics }
    }
}

impl MobileRealtimeMetricsRecorder for PerformanceMobileRealtimeMetricsRecorder {
    fn record_sse_reconnects(&self, count: u64) {
        for _ in 0..count {
            self.performance_metrics.record_sse_reconnect();
        }
    }
}

pub(crate) struct SseBusinessCaseEventPublisher {
    hub: Arc<SseHub>,
}

impl SseBusinessCaseEventPublisher {
    pub(crate) fn new(hub: Arc<SseHub>) -> Self {
        Self { hub }
    }
}

pub(crate) struct OutboxBusinessCaseEventPublisher {
    repo: PgDomainEventOutboxRepository,
}

impl OutboxBusinessCaseEventPublisher {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self {
            repo: PgDomainEventOutboxRepository::new(pool),
        }
    }

    async fn insert_event(
        repo: PgDomainEventOutboxRepository,
        aggregate_id: String,
        event_type: String,
        payload: serde_json::Value,
        source_change_id: String,
    ) -> Result<(), DomainError> {
        repo.insert_event_auto("business_case", &aggregate_id, &event_type, payload, &source_change_id)
            .await
            .map_err(|error| {
                DomainError::Internal(format!(
                    "failed to write business_case event to domain_event_outbox: {error}"
                ))
            })?;
        Ok(())
    }
}

impl BusinessCaseEventPublisher for OutboxBusinessCaseEventPublisher {
    fn publish_appended<'a>(
        &'a self,
        business_case: &'a fms_domain::models::business_case::FlightBusinessCase,
        append_entry_id: &'a str,
        operator: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), DomainError>> + Send + 'a>> {
        let repo = self.repo.clone();
        let aggregate_id = business_case.case_id.clone();
        let source_change_id = append_entry_id.to_string();
        let payload = serde_json::json!({
            "case_id": business_case.case_id,
            "append_entry_id": append_entry_id,
            "appended_by": business_case.updated_by,
            "operator": operator,
        });
        Box::pin(async move {
            Self::insert_event(
                repo,
                aggregate_id,
                "business_case.appended".to_string(),
                payload,
                source_change_id,
            )
            .await
        })
    }

    fn publish_updated<'a>(
        &'a self,
        business_case: &'a fms_domain::models::business_case::FlightBusinessCase,
        event_name: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), DomainError>> + Send + 'a>> {
        let repo = self.repo.clone();
        let aggregate_id = business_case.case_id.clone();
        let event_type = event_name.to_string();
        let payload = serde_json::json!({
            "case_id": business_case.case_id,
            "changed_fields": ["context"],
        });
        let source_change_id = ulid::Ulid::new().to_string();
        Box::pin(async move { Self::insert_event(repo, aggregate_id, event_type, payload, source_change_id).await })
    }
}

impl BusinessCaseEventPublisher for SseBusinessCaseEventPublisher {
    fn publish_appended<'a>(
        &'a self,
        _business_case: &'a fms_domain::models::business_case::FlightBusinessCase,
        _append_entry_id: &'a str,
        _operator: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), DomainError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }

    fn publish_updated<'a>(
        &'a self,
        business_case: &'a fms_domain::models::business_case::FlightBusinessCase,
        event_name: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), DomainError>> + Send + 'a>> {
        let hub = self.hub.clone();
        let case_id = business_case.case_id.clone();
        let event_name = event_name.to_string();
        Box::pin(async move {
            let payload = serde_json::json!({
                "event": event_name,
                "case_id": case_id,
                "changed_fields": ["context"],
            });
            let _ = hub.broadcast_event("business_cases", Some(&event_name), payload).await;
            Ok(())
        })
    }
}

impl BusinessCaseEventNotifier for SseBusinessCaseEventPublisher {
    fn publish_business_case_event(
        &self,
        event_name: String,
        payload: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<usize, DomainError>> + Send>> {
        let hub = self.hub.clone();
        Box::pin(async move { Ok(hub.broadcast_event("business_cases", Some(&event_name), payload).await) })
    }
}

pub(crate) struct SseWorkflowDispatchPublisher {
    hub: Arc<SseHub>,
}

impl SseWorkflowDispatchPublisher {
    pub(crate) fn new(hub: Arc<SseHub>) -> Self {
        Self { hub }
    }
}

impl fms_application::services::workflow_dispatch_service::WorkflowDispatchSsePublisher
    for SseWorkflowDispatchPublisher
{
    fn publish_system_alert<'a>(
        &'a self,
        event_name: &'a str,
        payload: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            let _ = self
                .hub
                .broadcast_event("system_alerts", Some(event_name), payload)
                .await;
        })
    }

    fn publish_ai_event<'a>(
        &'a self,
        event_name: &'a str,
        payload: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            let _ = self
                .hub
                .broadcast_event("ai_execution", Some(event_name), payload)
                .await;
        })
    }
}

pub(crate) struct SseTodoSchedulerPublisher {
    hub: Arc<SseHub>,
}

impl SseTodoSchedulerPublisher {
    pub(crate) fn new(hub: Arc<SseHub>) -> Self {
        Self { hub }
    }
}

impl TodoSchedulerSsePublisher for SseTodoSchedulerPublisher {
    fn publish_system_alert<'a>(
        &'a self,
        payload: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<usize, DomainError>> + Send + 'a>> {
        Box::pin(async move { Ok(self.hub.broadcast("system_alerts", payload).await) })
    }
}

pub(crate) struct SseKpiAggregationPublisher {
    hub: Arc<SseHub>,
}

impl SseKpiAggregationPublisher {
    pub(crate) fn new(hub: Arc<SseHub>) -> Self {
        Self { hub }
    }
}

impl KpiAggregationSsePublisher for SseKpiAggregationPublisher {
    fn publish_kpi_updated<'a>(
        &'a self,
        payload: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<usize, DomainError>> + Send + 'a>> {
        Box::pin(async move {
            Ok(self
                .hub
                .broadcast_event("kpi_updated", Some("kpi_updated"), payload)
                .await)
        })
    }
}

pub(crate) struct FlightListResponseCacheInvalidatorAdapter;

#[async_trait::async_trait]
impl FlightListResponseCacheInvalidator for FlightListResponseCacheInvalidatorAdapter {
    async fn invalidate_flight_list_response_cache(&self) {
        fms_api::routes::flights::invalidate_flight_list_response_cache().await;
    }
}

#[cfg(test)]
mod publisher_tests {
    use super::*;

    /// 切换到 FuturesUnordered 并行扇出后，最大的回归风险是「漏掉某个 user」
    /// （例如忘记 drive 所有 future）。本测试订阅多个 user 主题，断言每个用户都
    /// 收到且仅收到属于自己的事件，从而锁死扇出完整性。
    #[tokio::test]
    async fn dispatch_chat_publish_fans_out_to_every_user() {
        let hub = SseHub::new(16);
        let user_ids = ["alice", "bob", "carol", "dave"];

        let mut receivers = Vec::new();
        for uid in user_ids {
            let topic = format!("user_dispatch_chat_{uid}");
            receivers.push((uid.to_string(), hub.subscribe(&topic).await));
        }

        let publisher = SseDispatchChatEventPublisher::new(hub.clone());
        let events: Vec<(String, serde_json::Value)> = user_ids
            .iter()
            .map(|uid| ((*uid).to_string(), serde_json::json!({ "for": uid })))
            .collect();

        publisher.publish_user_event("dispatch_chat_message", events).await;

        for (uid, mut rx) in receivers {
            let msg = rx
                .try_recv()
                .unwrap_or_else(|_| panic!("user {uid} 必须收到自己的事件"));
            assert_eq!(msg.event.as_deref(), Some("dispatch_chat_message"));
            assert!(
                msg.serialized_data.contains(format!("\"for\":\"{uid}\"").as_str()),
                "user {uid} 收到的载荷不匹配: {}",
                msg.serialized_data
            );
            assert!(rx.try_recv().is_err(), "每个用户只应收到一条事件");
        }
    }
}
