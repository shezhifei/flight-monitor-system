//! System audit log persistence port (`system_audit_logs`).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::error::DomainError;

/// One audit-log row as returned to application services.
#[derive(Debug, Clone)]
pub struct AuditLogEntry {
    pub id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub action: String,
    pub changes: Value,
    pub user_id: Option<String>,
    pub trace_id: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

/// Insert payload for a flight audit entry.
#[derive(Debug, Clone)]
pub struct NewFlightAuditLog {
    pub id: Uuid,
    pub entity_id: String,
    pub action: String,
    pub changes: Value,
    pub user_id: String,
    pub trace_id: String,
    pub created_at: DateTime<Utc>,
}

/// Persistence port for `system_audit_logs` used by flight runtime.
#[async_trait]
pub trait AuditLogRepository: Send + Sync {
    async fn insert_flight_audit(&self, entry: &NewFlightAuditLog) -> Result<(), DomainError>;

    async fn list_recent_flight_updates(
        &self,
        threshold: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<AuditLogEntry>, DomainError>;

    async fn list_flight_history(&self, flight_id: &str, limit: i64) -> Result<Vec<AuditLogEntry>, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_is_object_safe() {
        fn assert_object_safe(_: &dyn AuditLogRepository) {}

        struct Stub;
        #[async_trait]
        impl AuditLogRepository for Stub {
            async fn insert_flight_audit(&self, _: &NewFlightAuditLog) -> Result<(), DomainError> {
                Ok(())
            }
            async fn list_recent_flight_updates(
                &self,
                _: DateTime<Utc>,
                _: i64,
            ) -> Result<Vec<AuditLogEntry>, DomainError> {
                Ok(vec![])
            }
            async fn list_flight_history(&self, _: &str, _: i64) -> Result<Vec<AuditLogEntry>, DomainError> {
                Ok(vec![])
            }
        }

        assert_object_safe(&Stub);
    }
}
