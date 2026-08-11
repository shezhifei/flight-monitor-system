//! Domain event subscription state (ACK / DLQ / consumer offsets).
//!
//! Abstracts access to `domain_event_processed`, `domain_event_dead_letters`,
//! and `domain_event_consumer_offsets` so the subscriber service issues no raw SQL.

use async_trait::async_trait;
use serde_json::Value;

use crate::error::DomainError;

/// Identity + routing fields for a domain event being marked processed/failed.
#[derive(Debug, Clone)]
pub struct DomainEventProcessingRecord {
    pub event_id: String,
    pub source_change_id: Option<String>,
    pub event_type: String,
    pub aggregate_type: String,
    pub aggregate_id: String,
}

/// Dead-letter payload for a domain event that exhausted retries.
#[derive(Debug, Clone)]
pub struct DomainEventDeadLetterRecord {
    pub event_id: String,
    pub source_change_id: Option<String>,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub event_type: String,
    pub payload: Value,
    pub stream_message_id: String,
    pub retry_count: i32,
    pub error_message: String,
}

/// Persistence port for subscriber ACK / DLQ / offset state.
#[async_trait]
pub trait DomainEventSubscriptionStateRepository: Send + Sync {
    /// True when the event was already processed successfully.
    async fn is_processed(&self, event_id: &str) -> Result<bool, DomainError>;

    /// Upsert a successful processing record.
    async fn mark_processed(&self, record: &DomainEventProcessingRecord) -> Result<(), DomainError>;

    /// Upsert a failed attempt and return the updated retry_count.
    async fn mark_failed(&self, record: &DomainEventProcessingRecord, error_message: &str) -> Result<i32, DomainError>;

    /// Upsert a dead-letter row.
    async fn insert_dead_letter(&self, record: &DomainEventDeadLetterRecord) -> Result<(), DomainError>;

    /// Upsert the consumer's last acknowledged message id.
    async fn upsert_consumer_offset(
        &self,
        consumer_group: &str,
        consumer_name: &str,
        topic: &str,
        message_id: &str,
    ) -> Result<(), DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_is_object_safe() {
        fn assert_object_safe(_: &dyn DomainEventSubscriptionStateRepository) {}

        struct Stub;
        #[async_trait]
        impl DomainEventSubscriptionStateRepository for Stub {
            async fn is_processed(&self, _: &str) -> Result<bool, DomainError> {
                Ok(false)
            }
            async fn mark_processed(&self, _: &DomainEventProcessingRecord) -> Result<(), DomainError> {
                Ok(())
            }
            async fn mark_failed(&self, _: &DomainEventProcessingRecord, _: &str) -> Result<i32, DomainError> {
                Ok(1)
            }
            async fn insert_dead_letter(&self, _: &DomainEventDeadLetterRecord) -> Result<(), DomainError> {
                Ok(())
            }
            async fn upsert_consumer_offset(&self, _: &str, _: &str, _: &str, _: &str) -> Result<(), DomainError> {
                Ok(())
            }
        }

        assert_object_safe(&Stub);
    }
}
