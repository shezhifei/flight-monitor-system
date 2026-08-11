use async_trait::async_trait;
use chrono::{DateTime, Utc};
use fms_domain::error::DomainError;
use fms_domain::ports::audit_log_repository::{AuditLogEntry, AuditLogRepository, NewFlightAuditLog};
use sqlx::{PgPool, Row};

#[derive(Debug, Clone)]
pub struct PgAuditLogRepository {
    pool: PgPool,
}

impl PgAuditLogRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn row_to_entry(row: &sqlx::postgres::PgRow) -> AuditLogEntry {
    AuditLogEntry {
        id: row.try_get::<String, _>("id").unwrap_or_default(),
        entity_type: row
            .try_get::<String, _>("entity_type")
            .unwrap_or_else(|_| "flight".to_string()),
        entity_id: row.try_get::<String, _>("entity_id").unwrap_or_default(),
        action: row
            .try_get::<String, _>("action")
            .unwrap_or_else(|_| "update".to_string()),
        changes: row
            .try_get::<serde_json::Value, _>("changes")
            .unwrap_or_else(|_| serde_json::json!({})),
        user_id: row.try_get::<Option<String>, _>("user_id").ok().flatten(),
        trace_id: row.try_get::<Option<String>, _>("trace_id").ok().flatten(),
        created_at: row.try_get::<Option<DateTime<Utc>>, _>("created_at").ok().flatten(),
    }
}

#[async_trait]
impl AuditLogRepository for PgAuditLogRepository {
    async fn insert_flight_audit(&self, entry: &NewFlightAuditLog) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO system_audit_logs (
                id, entity_type, entity_id, action, changes, user_id, trace_id, created_at
            ) VALUES ($1, 'flight', $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(entry.id)
        .bind(&entry.entity_id)
        .bind(&entry.action)
        .bind(&entry.changes)
        .bind(&entry.user_id)
        .bind(&entry.trace_id)
        .bind(entry.created_at)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(format!("failed to insert flight audit log: {error}")))?;
        Ok(())
    }

    async fn list_recent_flight_updates(
        &self,
        threshold: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<AuditLogEntry>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT id::text AS id, entity_type, entity_id, action, changes, user_id, trace_id, created_at
            FROM system_audit_logs
            WHERE entity_type = 'flight' AND created_at >= $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(threshold)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(format!("failed to list recent flight audit updates: {error}")))?;

        Ok(rows.iter().map(row_to_entry).collect())
    }

    async fn list_flight_history(&self, flight_id: &str, limit: i64) -> Result<Vec<AuditLogEntry>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT id::text AS id, entity_type, entity_id, action, changes, user_id, trace_id, created_at
            FROM system_audit_logs
            WHERE entity_type = 'flight' AND entity_id = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(flight_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| {
            DomainError::Internal(format!("failed to list flight audit history for {flight_id}: {error}"))
        })?;

        Ok(rows.iter().map(row_to_entry).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_type_exists() {
        let _ = std::any::type_name::<PgAuditLogRepository>();
    }
}
