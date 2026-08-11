//! Postgres implementation of the `DatabaseMetadataPort`.
//!
//! Encapsulates catalog introspection queries so application services can
//! verify required relations exist and query the current database name
//! without issuing raw `sqlx::query*` calls themselves.

use async_trait::async_trait;
use sqlx::PgPool;

use fms_domain::error::DomainError;
use fms_domain::ports::database_metadata_port::DatabaseMetadataPort;

#[derive(Clone)]
pub struct PgDatabaseMetadataAdapter {
    pool: PgPool,
}

impl PgDatabaseMetadataAdapter {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DatabaseMetadataPort for PgDatabaseMetadataAdapter {
    async fn relation_exists(&self, qualified_name: &str) -> Result<bool, DomainError> {
        let exists: Option<String> = sqlx::query_scalar("SELECT to_regclass($1)::text")
            .bind(qualified_name)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(format!("relation_exists query failed: {e}")))?;

        Ok(exists.is_some())
    }

    async fn current_database_name(&self) -> Result<String, DomainError> {
        sqlx::query_scalar::<_, String>("SELECT current_database()")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(format!("current_database_name query failed: {e}")))
    }
}
