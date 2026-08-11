//! Domain event outbox repository port.
//!
//! Abstracts read / claim / retention access to `domain_event_outbox` so
//! application services do not issue raw SQL against the table.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::error::DomainError;
use crate::events::DomainEventOutboxRow;

/// Persistence port for the domain-event transactional outbox.
#[async_trait]
pub trait DomainEventOutboxRepository: Send + Sync {
    /// Claim a batch of unpublished, due rows for relay recovery.
    ///
    /// Implementations must use `FOR UPDATE SKIP LOCKED` (or equivalent) so
    /// concurrent relay workers do not process the same row.
    async fn claim_pending_for_relay(&self, limit: i64) -> Result<Vec<DomainEventOutboxRow>, DomainError>;

    /// Count outbox rows that have not been published yet.
    async fn count_unpublished(&self) -> Result<i64, DomainError>;

    /// Oldest `occurred_at` among unpublished rows, if any.
    async fn oldest_unpublished(&self) -> Result<Option<DateTime<Utc>>, DomainError>;

    /// Delete outbox rows for one aggregate + event type older than `older_than`.
    ///
    /// Returns the number of rows deleted.
    async fn delete_by_aggregate_and_type(
        &self,
        aggregate_id: &str,
        event_type: &str,
        older_than: DateTime<Utc>,
    ) -> Result<u64, DomainError>;

    /// Count outbox rows for any of `aggregate_ids` + event type older than `older_than`.
    ///
    /// Used by smoke-cleanup dry-run (`aggregate_id = ANY(...)`).
    async fn count_by_aggregates_and_type(
        &self,
        aggregate_ids: &[String],
        event_type: &str,
        older_than: DateTime<Utc>,
    ) -> Result<i64, DomainError>;

    /// Delete outbox rows for any of `aggregate_ids` + event type older than `older_than`.
    ///
    /// Returns the number of rows deleted. Used by smoke-cleanup execute path.
    async fn delete_by_aggregates_and_type(
        &self,
        aggregate_ids: &[String],
        event_type: &str,
        older_than: DateTime<Utc>,
    ) -> Result<u64, DomainError>;
}

/// Transactional persistence port for the domain-event outbox.
///
/// Methods that require an open transaction (insert, mark published, mark failed,
/// claim with lock) live on this separate trait so application services can
/// depend on `Arc<dyn SqlxDomainEventOutboxTransactionalRepository>` instead of
/// the concrete `PgDomainEventOutboxRepository`.
#[async_trait]
pub trait DomainEventOutboxTransactionalRepository<Tx>: Send + Sync {
    /// Insert a domain event into the outbox within an existing transaction.
    ///
    /// `event_id` and `occurred_at` are generated internally; `source_change_id`
    /// is provided by the caller to keep CDC decoding column-aligned.
    async fn insert_event_in_tx(
        &self,
        tx: &mut Tx,
        aggregate_type: &str,
        aggregate_id: &str,
        event_type: &str,
        payload: Value,
        source_change_id: &str,
    ) -> Result<String, DomainError>;

    /// Mark a batch of events as published within an existing transaction.
    async fn mark_published_in_tx(&self, tx: &mut Tx, event_ids: &[String]) -> Result<(), DomainError>;

    /// Mark a single event as failed with exponential backoff within an existing transaction.
    async fn mark_failed_in_tx(
        &self,
        tx: &mut Tx,
        row: &DomainEventOutboxRow,
        error: &str,
        backoff_seconds: i64,
    ) -> Result<(), DomainError>;

    /// Claim pending rows inside an existing transaction (holds `FOR UPDATE` locks).
    async fn claim_pending_in_tx(&self, tx: &mut Tx, limit: i64) -> Result<Vec<DomainEventOutboxRow>, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compile-time / name surface check so the port stays aligned with
    /// Task 7 consumers (`claim_pending_for_relay`, `count_unpublished`,
    /// `oldest_unpublished`, `delete_by_aggregate_and_type`).
    #[test]
    fn trait_method_names_match_consumer_surface() {
        fn assert_object_safe(_: &dyn DomainEventOutboxRepository) {}

        struct Stub;
        #[async_trait]
        impl DomainEventOutboxRepository for Stub {
            async fn claim_pending_for_relay(&self, _limit: i64) -> Result<Vec<DomainEventOutboxRow>, DomainError> {
                Ok(vec![])
            }
            async fn count_unpublished(&self) -> Result<i64, DomainError> {
                Ok(0)
            }
            async fn oldest_unpublished(&self) -> Result<Option<DateTime<Utc>>, DomainError> {
                Ok(None)
            }
            async fn delete_by_aggregate_and_type(
                &self,
                _aggregate_id: &str,
                _event_type: &str,
                _older_than: DateTime<Utc>,
            ) -> Result<u64, DomainError> {
                Ok(0)
            }
            async fn count_by_aggregates_and_type(
                &self,
                _aggregate_ids: &[String],
                _event_type: &str,
                _older_than: DateTime<Utc>,
            ) -> Result<i64, DomainError> {
                Ok(0)
            }
            async fn delete_by_aggregates_and_type(
                &self,
                _aggregate_ids: &[String],
                _event_type: &str,
                _older_than: DateTime<Utc>,
            ) -> Result<u64, DomainError> {
                Ok(0)
            }
        }

        assert_object_safe(&Stub);
    }

    #[test]
    fn transactional_trait_is_object_safe() {
        fn assert_object_safe<Tx>(_repo: &dyn DomainEventOutboxTransactionalRepository<Tx>) {}

        struct StubTx;
        #[async_trait]
        impl DomainEventOutboxTransactionalRepository<StubTx> for () {
            async fn insert_event_in_tx(
                &self,
                _tx: &mut StubTx,
                _aggregate_type: &str,
                _aggregate_id: &str,
                _event_type: &str,
                _payload: Value,
                _source_change_id: &str,
            ) -> Result<String, DomainError> {
                Ok("event_id".to_string())
            }
            async fn mark_published_in_tx(&self, _tx: &mut StubTx, _event_ids: &[String]) -> Result<(), DomainError> {
                Ok(())
            }
            async fn mark_failed_in_tx(
                &self,
                _tx: &mut StubTx,
                _row: &DomainEventOutboxRow,
                _error: &str,
                _backoff_seconds: i64,
            ) -> Result<(), DomainError> {
                Ok(())
            }
            async fn claim_pending_in_tx(
                &self,
                _tx: &mut StubTx,
                _limit: i64,
            ) -> Result<Vec<DomainEventOutboxRow>, DomainError> {
                Ok(vec![])
            }
        }

        assert_object_safe(&());
    }
}
