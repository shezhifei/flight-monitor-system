use std::sync::Arc;

use fms_domain::ports::message_queue::MessageQueue;
use fms_domain::ports::unit_of_work::UnitOfWork;

use fms_domain::error::DomainError;

use crate::services::domain_event_outbox_delivery::{
    event_type_metric_label, DomainEventOutboxDelivery,
};
use fms_domain::ports::domain_event_outbox_repository::DomainEventOutboxTransactionalRepository;

/// 调度器需要的中继契约。
///
/// 这个端口的存在是为了**让 `UnitOfWork` 的泛型参数在这里停住**。`fms-api` 的调度器只需要
/// 「跑一轮」和「报告 topic」，不需要知道事务句柄的类型；若让它持有
/// `Arc<DomainEventRelayService<PgUnitOfWork>>`，`fms-api` 就得为了拼出这个类型而把
/// `fms-infrastructure` 提升为生产依赖——那是用 P3 换来一个新的分层反向。
///
/// 顺带修掉一个既有问题：`fms-api` 原先直接点名具体应用服务，现在它只认端口。
#[async_trait::async_trait]
pub trait DomainEventRelay: Send + Sync {
    async fn recover_once(&self) -> Result<i64, DomainError>;
    fn topic(&self) -> &str;
}

const PUBLISHED_TOTAL_METRIC: &str = "domain_event_relay_published_total";
const PUBLISH_FAILED_TOTAL_METRIC: &str = "domain_event_relay_publish_failed_total";

pub struct DomainEventRelayService<U: UnitOfWork> {
    uow: Arc<U>,
    message_queue: Option<Arc<dyn MessageQueue + Send + Sync>>,
    enabled: bool,
    batch_size: i64,
    delivery: DomainEventOutboxDelivery<U::Tx>,
}

impl<U: UnitOfWork> DomainEventRelayService<U> {
    pub fn new(
        uow: Arc<U>,
        message_queue: Option<Arc<dyn MessageQueue + Send + Sync>>,
        enabled: bool,
        batch_size: i64,
        base_backoff_seconds: i64,
        topic: Option<String>,
        outbox_repo: Arc<dyn DomainEventOutboxTransactionalRepository<U::Tx> + Send + Sync>,
    ) -> Self {
        Self {
            uow,
            message_queue,
            enabled,
            batch_size: batch_size.max(1),
            delivery: DomainEventOutboxDelivery::new(base_backoff_seconds, topic, outbox_repo),
        }
    }

    pub fn topic(&self) -> &str {
        self.delivery.topic()
    }

    pub async fn recover_once(&self) -> Result<i64, DomainError> {
        if !self.enabled {
            return Ok(0);
        }

        let mut tx = self.uow.begin().await?;

        // 同事务内领取，让 FOR UPDATE 锁保持到 mark_published / mark_failed。
        let rows = self.delivery.claim_pending(&mut tx, self.batch_size).await?;
        if rows.is_empty() {
            self.uow.commit(tx).await?;
            return Ok(0);
        }

        let message_queue = self
            .message_queue
            .as_ref()
            .ok_or_else(|| DomainError::Internal("message queue gateway unavailable".to_string()))?;

        let mut successful_event_ids = Vec::new();

        for row in &rows {
            self.delivery.observe_relay_lag(row);
            match self.delivery.publish_row(message_queue.as_ref(), row).await {
                Ok(()) => {
                    metrics::counter!(
                        PUBLISHED_TOTAL_METRIC,
                        "event_type" => event_type_metric_label(row)
                    )
                    .increment(1);
                    successful_event_ids.push(row.event_id.clone());
                }
                Err(error) => {
                    metrics::counter!(
                        PUBLISH_FAILED_TOTAL_METRIC,
                        "event_type" => event_type_metric_label(row)
                    )
                    .increment(1);
                    self.delivery.mark_failed(&mut tx, row, &error.to_string()).await?;
                }
            }
        }

        self.delivery.mark_published(&mut tx, &successful_event_ids).await?;

        self.uow.commit(tx).await?;

        Ok(successful_event_ids.len() as i64)
    }

}

#[async_trait::async_trait]
impl<U: UnitOfWork> DomainEventRelay for DomainEventRelayService<U> {
    async fn recover_once(&self) -> Result<i64, DomainError> {
        DomainEventRelayService::recover_once(self).await
    }

    fn topic(&self) -> &str {
        DomainEventRelayService::topic(self)
    }
}
