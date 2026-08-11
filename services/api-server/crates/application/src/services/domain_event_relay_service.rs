use std::sync::Arc;

use fms_domain::ports::message_queue::MessageQueue;
use sqlx::{PgPool, Postgres, Transaction};

use fms_domain::error::DomainError;

use crate::services::domain_event_outbox_delivery::{
    event_type_metric_label, DomainEventOutboxDelivery, DomainEventOutboxRow,
};
use crate::sqlx_transactional_repositories::SqlxDomainEventOutboxTransactionalRepository;

const PUBLISHED_TOTAL_METRIC: &str = "domain_event_relay_published_total";
const PUBLISH_FAILED_TOTAL_METRIC: &str = "domain_event_relay_publish_failed_total";

pub struct DomainEventRelayService {
    pool: PgPool,
    message_queue: Option<Arc<dyn MessageQueue + Send + Sync>>,
    enabled: bool,
    batch_size: i64,
    delivery: DomainEventOutboxDelivery,
}

impl DomainEventRelayService {
    pub fn new(
        pool: PgPool,
        message_queue: Option<Arc<dyn MessageQueue + Send + Sync>>,
        enabled: bool,
        batch_size: i64,
        base_backoff_seconds: i64,
        topic: Option<String>,
        outbox_repo: Arc<dyn SqlxDomainEventOutboxTransactionalRepository>,
    ) -> Self {
        Self {
            pool: pool.clone(),
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

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| DomainError::Internal(format!("failed to start relay transaction: {error}")))?;

        let rows = self.lock_pending_rows(&mut tx).await?;
        if rows.is_empty() {
            tx.commit()
                .await
                .map_err(|error| DomainError::Internal(format!("failed to commit empty relay transaction: {error}")))?;
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

        tx.commit().await.map_err(|error| {
            DomainError::Internal(format!("failed to commit domain event relay transaction: {error}"))
        })?;

        Ok(successful_event_ids.len() as i64)
    }

    pub async fn relay_once(&self) -> Result<i64, DomainError> {
        self.recover_once().await
    }

    async fn lock_pending_rows(
        &self,
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<Vec<DomainEventOutboxRow>, DomainError> {
        // Same-tx claim so FOR UPDATE locks are held until mark_published/mark_failed.
        self.delivery.claim_pending(tx, self.batch_size).await
    }
}
