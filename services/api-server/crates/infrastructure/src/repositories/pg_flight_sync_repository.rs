use fms_domain::error::DomainError;
use fms_domain::ports::FlightSyncRepository;
use serde_json::Value;
use sqlx::PgPool;
use sqlx::Row;

pub struct PgFlightSyncRepository {
    pool: PgPool,
}

impl PgFlightSyncRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl FlightSyncRepository for PgFlightSyncRepository {
    async fn find_latest(&self, source_system: &str) -> Result<Option<Value>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT
                run_id, source_system, trigger, direction,
                window_start_date, window_end_date, status,
                processed_count, success_count, failure_count,
                created_count, updated_count,
                failure_samples, error_summary,
                started_at, completed_at
            FROM flight_sync_runs
            WHERE source_system = $1
            ORDER BY started_at DESC
            LIMIT 1
            "#,
        )
        .bind(source_system)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("failed to query flight sync status: {e}")))?;

        Ok(row.map(|r| flight_sync_row_to_payload(&r)))
    }

    async fn create_run(
        &self,
        run_id: &str,
        source_system: &str,
        trigger: &str,
        direction: &str,
        window_start: chrono::NaiveDate,
        window_end: chrono::NaiveDate,
        status: &str,
        started_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO flight_sync_runs (
                run_id, source_system, trigger, direction,
                window_start_date, window_end_date, status,
                started_at, completed_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NULL)
            "#,
        )
        .bind(run_id)
        .bind(source_system)
        .bind(trigger)
        .bind(direction)
        .bind(window_start)
        .bind(window_end)
        .bind(status)
        .bind(started_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("failed to create flight sync run: {e}")))?;
        Ok(())
    }

    async fn mark_completed(
        &self,
        run_id: &str,
        processed_count: i32,
        success_count: i32,
        failure_count: i32,
        created_count: i32,
        updated_count: i32,
        failure_samples: &[Value],
        error_summary: &[Value],
        completed_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            UPDATE flight_sync_runs
            SET
                status = $2, processed_count = $3, success_count = $4,
                failure_count = $5, created_count = $6, updated_count = $7,
                failure_samples = $8, error_summary = $9,
                completed_at = $10, updated_at = CURRENT_TIMESTAMP
            WHERE run_id = $1
            "#,
        )
        .bind(run_id)
        .bind("completed")
        .bind(processed_count)
        .bind(success_count)
        .bind(failure_count)
        .bind(created_count)
        .bind(updated_count)
        .bind(sqlx::types::Json(failure_samples))
        .bind(sqlx::types::Json(error_summary))
        .bind(completed_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("failed to complete flight sync run: {e}")))?;
        Ok(())
    }

    async fn mark_failed(
        &self,
        run_id: &str,
        failure_count: i32,
        failure_samples: &[Value],
        error_summary: &[Value],
        completed_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            UPDATE flight_sync_runs
            SET
                status = $2, failure_count = $3,
                failure_samples = $4, error_summary = $5,
                completed_at = $6, updated_at = CURRENT_TIMESTAMP
            WHERE run_id = $1
            "#,
        )
        .bind(run_id)
        .bind("failed")
        .bind(failure_count)
        .bind(sqlx::types::Json(failure_samples))
        .bind(sqlx::types::Json(error_summary))
        .bind(completed_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("failed to record flight sync failure: {e}")))?;
        Ok(())
    }

    async fn load_payload(&self, run_id: &str) -> Result<Value, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT
                run_id, source_system, trigger, direction,
                window_start_date, window_end_date, status,
                processed_count, success_count, failure_count,
                created_count, updated_count,
                failure_samples, error_summary,
                started_at, completed_at
            FROM flight_sync_runs
            WHERE run_id = $1
            "#,
        )
        .bind(run_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("failed to load flight sync payload: {e}")))?;

        Ok(flight_sync_row_to_payload(&row))
    }
}

fn flight_sync_row_to_payload(row: &sqlx::postgres::PgRow) -> Value {
    let run_id: String = row.get("run_id");
    let source_system: String = row.get("source_system");
    let trigger: String = row.get("trigger");
    let direction: String = row.get("direction");
    let window_start_date: chrono::NaiveDate = row.get("window_start_date");
    let window_end_date: chrono::NaiveDate = row.get("window_end_date");
    let status: String = row.get("status");
    let processed_count: i32 = row.get("processed_count");
    let success_count: i32 = row.get("success_count");
    let failure_count: i32 = row.get("failure_count");
    let created_count: i32 = row.get("created_count");
    let updated_count: i32 = row.get("updated_count");
    let failure_samples: Value = row.get::<sqlx::types::Json<Value>, _>("failure_samples").0;
    let error_summary: Value = row.get::<sqlx::types::Json<Value>, _>("error_summary").0;
    let started_at: chrono::DateTime<chrono::Utc> = row.get("started_at");
    let completed_at: Option<chrono::DateTime<chrono::Utc>> = row.get("completed_at");

    serde_json::json!({
        "run_id": run_id,
        "source_system": source_system,
        "trigger": trigger,
        "direction": direction,
        "window": {
            "anchor_date": window_start_date.to_string(),
            "start_date": window_start_date.to_string(),
            "end_date": window_end_date.to_string(),
        },
        "status": status,
        "started_at": started_at.to_rfc3339(),
        "completed_at": completed_at.map(|value| value.to_rfc3339()),
        "processed_count": processed_count,
        "success_count": success_count,
        "failure_count": failure_count,
        "created_count": created_count,
        "updated_count": updated_count,
        "official_record_count": 0,
        "registration_enriched_count": 0,
        "registration_ambiguous_count": 0,
        "registration_missing_count": 0,
        "stitched_turnaround_count": 0,
        "failure_samples": failure_samples,
        "error_summary": error_summary,
    })
}
