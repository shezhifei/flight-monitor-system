//! Flight dispatch timeline event persistence port
//! (`flight_dispatch_timeline_events`).

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::error::DomainError;

/// One timeline event row.
#[derive(Debug, Clone)]
pub struct FlightTimelineEvent {
    pub timeline_id: String,
    pub flight_id: String,
    pub milestone_code: String,
    pub occurred_at: DateTime<Utc>,
    pub leg_type: Option<String>,
    pub recorded_by: Option<String>,
    pub client_action_id: Option<String>,
    pub source: String,
    pub payload: Value,
    pub created_at: DateTime<Utc>,
}

/// Result of an idempotent timeline insert (ON CONFLICT client_action_id).
#[derive(Debug, Clone)]
pub struct FlightTimelineWriteResult {
    pub event: FlightTimelineEvent,
    pub inserted: bool,
}

/// Read/write port for flight dispatch timeline events.
///
/// Transactional writes use the generic `Tx` trait so the application layer can
/// pair inserts/deletes with outbox writes in the same sqlx transaction without
/// leaking sqlx into `fms-domain`.
#[async_trait]
pub trait FlightTimelineEventRepository: Send + Sync {
    async fn list_by_flight(&self, flight_id: &str, limit: i64) -> Result<Vec<FlightTimelineEvent>, DomainError>;

    /// Current milestone value per `(flight_id, milestone_code)`.
    ///
    /// Semantics: **last write wins** — the event with the greatest
    /// `created_at` (then `timeline_id`) for that milestone, not the maximum
    /// business `occurred_at`. Callers use this for optimistic checks and
    /// cell display of "current" times.
    async fn latest_snapshots(
        &self,
        flight_ids: &[String],
    ) -> Result<HashMap<String, HashMap<String, DateTime<Utc>>>, DomainError>;
}

/// Transactional write side of the timeline repository.
#[async_trait]
pub trait FlightTimelineEventTransactionalRepository<Tx: Send>: Send + Sync {
    /// Serialize writers for one `(flight_id, milestone_code)` until the
    /// caller's transaction completes.
    async fn lock_milestone_in_tx(&self, tx: &mut Tx, flight_id: &str, milestone_code: &str)
        -> Result<(), DomainError>;

    /// Read the current milestone value while holding the caller's lock.
    /// Current means last write by `(created_at, timeline_id)`, not the
    /// greatest business `occurred_at` value.
    async fn latest_occurred_at_in_tx(
        &self,
        tx: &mut Tx,
        flight_id: &str,
        milestone_code: &str,
    ) -> Result<Option<DateTime<Utc>>, DomainError>;

    async fn insert_in_tx(
        &self,
        tx: &mut Tx,
        event: &FlightTimelineEvent,
        client_action_id: Option<&str>,
    ) -> Result<FlightTimelineWriteResult, DomainError>;

    async fn delete_in_tx(&self, tx: &mut Tx, flight_id: &str, timeline_id: &str) -> Result<bool, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traits_are_object_safe() {
        fn assert_repo(_: &dyn FlightTimelineEventRepository) {}
        fn assert_tx_repo<Tx: Send>(_: &dyn FlightTimelineEventTransactionalRepository<Tx>) {}

        struct Stub;
        #[async_trait]
        impl FlightTimelineEventRepository for Stub {
            async fn list_by_flight(&self, _: &str, _: i64) -> Result<Vec<FlightTimelineEvent>, DomainError> {
                Ok(vec![])
            }
            async fn latest_snapshots(
                &self,
                _: &[String],
            ) -> Result<HashMap<String, HashMap<String, DateTime<Utc>>>, DomainError> {
                Ok(HashMap::new())
            }
        }
        #[async_trait]
        impl FlightTimelineEventTransactionalRepository<()> for Stub {
            async fn lock_milestone_in_tx(
                &self,
                _tx: &mut (),
                _flight_id: &str,
                _milestone_code: &str,
            ) -> Result<(), DomainError> {
                Ok(())
            }

            async fn latest_occurred_at_in_tx(
                &self,
                _tx: &mut (),
                _flight_id: &str,
                _milestone_code: &str,
            ) -> Result<Option<DateTime<Utc>>, DomainError> {
                Ok(None)
            }

            async fn insert_in_tx(
                &self,
                _: &mut (),
                _: &FlightTimelineEvent,
                _: Option<&str>,
            ) -> Result<FlightTimelineWriteResult, DomainError> {
                Err(DomainError::Internal("stub".into()))
            }
            async fn delete_in_tx(&self, _: &mut (), _: &str, _: &str) -> Result<bool, DomainError> {
                Ok(false)
            }
        }

        assert_repo(&Stub);
        assert_tx_repo::<()>(&Stub);
    }
}
