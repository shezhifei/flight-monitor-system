use async_trait::async_trait;
use serde_json::Value;
use sqlx::{PgPool, Row};

use fms_domain::models::ai_job::AiRunRecord;
use fms_domain::ports::ai_run_repository::{AiRunRepository, AiRunRepositoryError};

const RUN_SELECT: &str = "run_id, job_id, runtime_engine, model_id, status, input_envelope, output_raw, output_validated, token_usage, started_at, finished_at, error_code, error_message, created_at";

pub struct PgAiRunRepository {
    pool: PgPool,
}

impl PgAiRunRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn row_to_run(row: &sqlx::postgres::PgRow) -> Result<AiRunRecord, AiRunRepositoryError> {
        Ok(AiRunRecord {
            run_id: row.try_get("run_id").map_err(db_err)?,
            job_id: row.try_get("job_id").map_err(db_err)?,
            runtime_engine: row.try_get("runtime_engine").map_err(db_err)?,
            model_id: row.try_get("model_id").map_err(db_err)?,
            status: row.try_get("status").map_err(db_err)?,
            input_envelope: row.try_get("input_envelope").map_err(db_err)?,
            output_raw: row.try_get("output_raw").map_err(db_err)?,
            output_validated: row.try_get("output_validated").map_err(db_err)?,
            token_usage: row.try_get("token_usage").map_err(db_err)?,
            started_at: row.try_get("started_at").map_err(db_err)?,
            finished_at: row.try_get("finished_at").map_err(db_err)?,
            error_code: row.try_get("error_code").map_err(db_err)?,
            error_message: row.try_get("error_message").map_err(db_err)?,
            created_at: row.try_get("created_at").map_err(db_err)?,
        })
    }
}

#[async_trait]
impl AiRunRepository for PgAiRunRepository {
    async fn insert(
        &self,
        run_id: &str,
        job_id: &str,
        runtime_engine: &str,
        model_id: Option<&str>,
        input_envelope: Option<Value>,
    ) -> Result<AiRunRecord, AiRunRepositoryError> {
        let sql = format!(
            "INSERT INTO ai_runs (run_id, job_id, runtime_engine, model_id, status, input_envelope)
             VALUES ($1, $2, $3, $4, 'pending', $5)
             RETURNING {RUN_SELECT}"
        );
        let row = sqlx::query(&sql)
            .bind(run_id)
            .bind(job_id)
            .bind(runtime_engine)
            .bind(model_id)
            .bind(input_envelope)
            .fetch_one(&self.pool)
            .await
            .map_err(db_err)?;
        Self::row_to_run(&row)
    }

    async fn find_by_id(&self, run_id: &str) -> Result<Option<AiRunRecord>, AiRunRepositoryError> {
        let sql = format!("SELECT {RUN_SELECT} FROM ai_runs WHERE run_id = $1");
        let row = sqlx::query(&sql)
            .bind(run_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_err)?;
        match row {
            Some(r) => Ok(Some(Self::row_to_run(&r)?)),
            None => Ok(None),
        }
    }

    async fn list_for_job(&self, job_id: &str) -> Result<Vec<AiRunRecord>, AiRunRepositoryError> {
        let sql = format!("SELECT {RUN_SELECT} FROM ai_runs WHERE job_id = $1 ORDER BY created_at DESC");
        let rows = sqlx::query(&sql)
            .bind(job_id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_err)?;
        rows.iter().map(Self::row_to_run).collect()
    }

    async fn count_active(&self, entity_id: Option<&str>) -> Result<i64, AiRunRepositoryError> {
        // Active statuses mirror AiRunStatus::active_statuses().
        // Entity id follows the same convention as PgAiAuthContextLoader:
        // `input_envelope.entity_id` or `input_envelope.context.entity_id`.
        let count: i64 = match entity_id {
            Some(entity_id) => sqlx::query_scalar(
                "SELECT COUNT(*) FROM ai_runs
                     WHERE status IN ('pending', 'claimed', 'running')
                       AND COALESCE(input_envelope->>'entity_id', input_envelope->'context'->>'entity_id') = $1",
            )
            .bind(entity_id)
            .fetch_one(&self.pool)
            .await
            .map_err(db_err)?,
            None => {
                sqlx::query_scalar("SELECT COUNT(*) FROM ai_runs WHERE status IN ('pending', 'claimed', 'running')")
                    .fetch_one(&self.pool)
                    .await
                    .map_err(db_err)?
            }
        };
        Ok(count)
    }

    async fn update_status(&self, run_id: &str, new_status: &str) -> Result<AiRunRecord, AiRunRepositoryError> {
        let sql = format!(
            "UPDATE ai_runs SET
                 status = $2,
                 started_at = CASE WHEN $2 = 'running' AND started_at IS NULL THEN now() ELSE started_at END,
                 finished_at = CASE WHEN $2 IN ('succeeded', 'failed_terminal') THEN now() ELSE finished_at END
             WHERE run_id = $1
             RETURNING {RUN_SELECT}"
        );
        let row = sqlx::query(&sql)
            .bind(run_id)
            .bind(new_status)
            .fetch_one(&self.pool)
            .await
            .map_err(db_err)?;
        Self::row_to_run(&row)
    }

    async fn update_input_envelope(&self, run_id: &str, input_envelope: Value) -> Result<(), AiRunRepositoryError> {
        sqlx::query("UPDATE ai_runs SET input_envelope = $2 WHERE run_id = $1")
            .bind(run_id)
            .bind(input_envelope)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn fill_terminal_outputs(
        &self,
        run_id: &str,
        output_raw: Option<Value>,
        output_validated: Option<Value>,
        token_usage: Option<Value>,
    ) -> Result<(), AiRunRepositoryError> {
        sqlx::query(
            "UPDATE ai_runs SET output_raw = $2, output_validated = $3, token_usage = $4, finished_at = COALESCE(finished_at, NOW()) WHERE run_id = $1",
        )
        .bind(run_id)
        .bind(output_raw)
        .bind(output_validated)
        .bind(token_usage)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn complete(
        &self,
        run_id: &str,
        output_raw: Option<Value>,
        output_validated: Option<Value>,
        token_usage: Option<Value>,
    ) -> Result<(), AiRunRepositoryError> {
        sqlx::query(
            "UPDATE ai_runs SET status = $2, output_raw = $3, output_validated = $4, token_usage = $5, finished_at = NOW() WHERE run_id = $1",
        )
        .bind(run_id)
        .bind("succeeded")
        .bind(output_raw)
        .bind(output_validated)
        .bind(token_usage)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn fill_terminal_error(
        &self,
        run_id: &str,
        error_code: Option<&str>,
        error_message: Option<&str>,
        output_raw: Option<Value>,
    ) -> Result<(), AiRunRepositoryError> {
        sqlx::query(
            "UPDATE ai_runs SET error_code = $2, error_message = $3, output_raw = $4, finished_at = COALESCE(finished_at, NOW()) WHERE run_id = $1",
        )
        .bind(run_id)
        .bind(error_code)
        .bind(error_message)
        .bind(output_raw)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn fail(
        &self,
        run_id: &str,
        error_code: Option<&str>,
        error_message: Option<&str>,
        output_raw: Option<Value>,
    ) -> Result<(), AiRunRepositoryError> {
        sqlx::query(
            "UPDATE ai_runs SET status = $2, error_code = $3, error_message = $4, output_raw = $5, finished_at = NOW() WHERE run_id = $1",
        )
        .bind(run_id)
        .bind("failed_terminal")
        .bind(error_code)
        .bind(error_message)
        .bind(output_raw)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }
}

fn db_err(err: impl ToString) -> AiRunRepositoryError {
    AiRunRepositoryError::database(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor_builds() {
        let _ = std::any::type_name::<PgAiRunRepository>();
    }
}
