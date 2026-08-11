//! PostgreSQL 航班归档仓储实现。

use async_trait::async_trait;
use sqlx::{PgPool, Row};

use fms_domain::error::DomainError;
use fms_domain::ports::flight_archive_repository::FlightArchiveRepository;

pub struct PgFlightArchiveRepository {
    pool: PgPool,
}

impl PgFlightArchiveRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl FlightArchiveRepository for PgFlightArchiveRepository {
    async fn find_archived_flights(&self, limit: i64, offset: i64) -> Result<Vec<serde_json::Value>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT row_to_json(af) AS data
            FROM (
                SELECT *
                FROM archived_flights
                ORDER BY archived_at DESC
                LIMIT $1 OFFSET $2
            ) af
            "#,
        )
        .bind(limit.max(1))
        .bind(offset.max(0))
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        Ok(rows
            .into_iter()
            .filter_map(|row| row.try_get::<Option<serde_json::Value>, _>("data").ok().flatten())
            .collect())
    }

    async fn find_archived_flight_by_id(&self, flight_id: &str) -> Result<Option<serde_json::Value>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT row_to_json(af) AS data
            FROM archived_flights af
            WHERE flight_id = $1
            "#,
        )
        .bind(flight_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        Ok(row.and_then(|item| item.try_get::<Option<serde_json::Value>, _>("data").ok().flatten()))
    }

    async fn get_archive_stats(&self) -> Result<serde_json::Value, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT json_build_object(
                'total_archived_flights', (SELECT COUNT(*) FROM archived_flights),
                'total_archived_changes', (SELECT COUNT(*) FROM archived_flight_state_changes),
                'last_archived_at', (SELECT MAX(archived_at) FROM archived_flights)
            ) AS data
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        Ok(row
            .try_get::<Option<serde_json::Value>, _>("data")
            .ok()
            .flatten()
            .unwrap_or_else(|| serde_json::json!({})))
    }

    async fn trigger_archive(
        &self,
        cutoff_date: Option<&str>,
        target_date: Option<&str>,
    ) -> Result<serde_json::Value, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT to_jsonb(archive_flight_data($1, $2)) AS data
            "#,
        )
        .bind(cutoff_date)
        .bind(target_date)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        Ok(row
            .try_get::<Option<serde_json::Value>, _>("data")
            .ok()
            .flatten()
            .unwrap_or(serde_json::Value::Null))
    }
}
