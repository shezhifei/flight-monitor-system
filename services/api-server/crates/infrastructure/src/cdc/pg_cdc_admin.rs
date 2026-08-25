//! PostgreSQL logical-replication admin helpers (publication + slot).
//!
//! Keeps pg_catalog / admin SQL out of application services.

use async_trait::async_trait;
use fms_domain::error::DomainError;
use fms_domain::ports::cdc_admin_port::CdcAdminPort;
use sqlx::{PgPool, Row};

#[derive(Debug, Clone)]
pub struct PgCdcAdmin {
    pool: PgPool,
}

impl PgCdcAdmin {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn ensure_publication_exists(&self, publication_name: &str) -> Result<(), DomainError> {
        let existing = sqlx::query("SELECT 1 FROM pg_publication WHERE pubname = $1")
            .bind(publication_name)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| {
                DomainError::Internal(format!("failed to query logical replication publication: {error}"))
            })?;

        if existing.is_some() {
            return Ok(());
        }

        sqlx::query(&format!(
            "CREATE PUBLICATION {} FOR TABLE domain_event_outbox WITH (publish = 'insert')",
            publication_name
        ))
        .execute(&self.pool)
        .await
        .map_err(|error| {
            DomainError::Internal(format!(
                "failed to create logical replication publication {publication_name}: {error}"
            ))
        })?;

        Ok(())
    }

    pub async fn ensure_replication_slot(&self, slot_name: &str, expected_database: &str) -> Result<(), DomainError> {
        let existing = sqlx::query(
            r#"
            SELECT slot_name, plugin, database
            FROM pg_replication_slots
            WHERE slot_name = $1
            "#,
        )
        .bind(slot_name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(format!("failed to query logical replication slots: {error}")))?;

        if let Some(existing) = existing {
            let plugin = existing
                .try_get::<Option<String>, _>("plugin")
                .map_err(|error| {
                    DomainError::Internal(format!("failed to decode logical replication slot metadata: {error}"))
                })?
                .unwrap_or_default();
            let slot_database = existing
                .try_get::<Option<String>, _>("database")
                .map_err(|error| {
                    DomainError::Internal(format!(
                        "failed to decode logical replication slot database metadata: {error}"
                    ))
                })?
                .unwrap_or_default();

            if !slot_database.trim().is_empty() && slot_database.trim() != expected_database.trim() {
                return Err(DomainError::Internal(format!(
                    "replication slot {slot_name} belongs to database {slot_database}, but replication config targets {expected_database}"
                )));
            }

            if plugin.trim().is_empty() || plugin == "pgoutput" {
                return Ok(());
            }
            return Err(DomainError::Internal(format!(
                "replication slot {slot_name} uses plugin {plugin}, expected pgoutput"
            )));
        }

        sqlx::query("SELECT * FROM pg_create_logical_replication_slot($1, 'pgoutput')")
            .bind(slot_name)
            .execute(&self.pool)
            .await
            .map_err(|error| {
                DomainError::Internal(format!(
                    "failed to create logical replication slot {slot_name}: {error}"
                ))
            })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_exists() {
        let _ = std::any::type_name::<PgCdcAdmin>();
    }
}

#[async_trait]
impl CdcAdminPort for PgCdcAdmin {
    async fn ensure_publication_exists(&self, publication_name: &str) -> Result<(), DomainError> {
        PgCdcAdmin::ensure_publication_exists(self, publication_name).await
    }

    async fn ensure_replication_slot(&self, slot_name: &str, expected_database: &str) -> Result<(), DomainError> {
        PgCdcAdmin::ensure_replication_slot(self, slot_name, expected_database).await
    }
}
