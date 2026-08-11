use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Row};

use fms_domain::models::ai_job::AiRunEventRecord;
use fms_domain::ports::ai_run_event_repository::{AiRunEventRepository, AiRunEventRepositoryError};

const EVENT_SELECT: &str = "event_id, job_id, run_id, event_type, payload, created_at";

pub struct PgAiRunEventRepository {
    pool: PgPool,
}

impl PgAiRunEventRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn row_to_event(row: &sqlx::postgres::PgRow) -> Result<AiRunEventRecord, AiRunEventRepositoryError> {
        Ok(AiRunEventRecord {
            event_id: row.try_get("event_id").map_err(db_err)?,
            job_id: row.try_get("job_id").map_err(db_err)?,
            run_id: row.try_get("run_id").map_err(db_err)?,
            event_type: row.try_get("event_type").map_err(db_err)?,
            payload: row.try_get("payload").map_err(db_err)?,
            created_at: row.try_get("created_at").map_err(db_err)?,
        })
    }
}

#[async_trait]
impl AiRunEventRepository for PgAiRunEventRepository {
    async fn insert(
        &self,
        job_id: &str,
        run_id: &str,
        event_type: &str,
        payload: Option<Value>,
    ) -> Result<AiRunEventRecord, AiRunEventRepositoryError> {
        let sql = format!(
            "INSERT INTO ai_run_events (job_id, run_id, event_type, payload)
             VALUES ($1, $2, $3, $4)
             RETURNING {EVENT_SELECT}"
        );
        let row = sqlx::query(&sql)
            .bind(job_id)
            .bind(run_id)
            .bind(event_type)
            .bind(payload)
            .fetch_one(&self.pool)
            .await
            .map_err(db_err)?;
        Self::row_to_event(&row)
    }

    async fn insert_fire_and_forget(
        &self,
        job_id: &str,
        run_id: &str,
        event_type: &str,
        payload: Option<Value>,
    ) -> Result<(), AiRunEventRepositoryError> {
        sqlx::query(
            "INSERT INTO ai_run_events (job_id, run_id, event_type, payload)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(job_id)
        .bind(run_id)
        .bind(event_type)
        .bind(payload)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn list_for_run(&self, run_id: &str, limit: i64) -> Result<Vec<AiRunEventRecord>, AiRunEventRepositoryError> {
        let sql =
            format!("SELECT {EVENT_SELECT} FROM ai_run_events WHERE run_id = $1 ORDER BY created_at ASC LIMIT $2");
        let rows = sqlx::query(&sql)
            .bind(run_id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(db_err)?;
        rows.iter().map(Self::row_to_event).collect()
    }

    async fn count_by_job_ids_before(
        &self,
        job_ids: &[String],
        older_than: DateTime<Utc>,
    ) -> Result<i64, AiRunEventRepositoryError> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*)::bigint FROM ai_run_events WHERE job_id = ANY($1) AND created_at < $2")
                .bind(job_ids)
                .bind(older_than)
                .fetch_one(&self.pool)
                .await
                .map_err(db_err)?;
        Ok(count)
    }

    async fn delete_by_job_ids_before(
        &self,
        job_ids: &[String],
        older_than: DateTime<Utc>,
    ) -> Result<u64, AiRunEventRepositoryError> {
        let result = sqlx::query("DELETE FROM ai_run_events WHERE job_id = ANY($1) AND created_at < $2")
            .bind(job_ids)
            .bind(older_than)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(result.rows_affected())
    }

    async fn count_smoke_readiness_blocks(&self, event_type: &str) -> Result<i64, AiRunEventRepositoryError> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)::bigint
            FROM ai_run_events e
            WHERE e.event_type = $1
              AND (e.job_id LIKE 'smoke_job_%' OR e.job_id LIKE 'api_smoke_job_%')
            "#,
        )
        .bind(event_type)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(count)
    }
}

fn db_err(err: impl ToString) -> AiRunEventRepositoryError {
    AiRunEventRepositoryError::database(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor_builds() {
        let _ = std::any::type_name::<PgAiRunEventRepository>();
    }
}
