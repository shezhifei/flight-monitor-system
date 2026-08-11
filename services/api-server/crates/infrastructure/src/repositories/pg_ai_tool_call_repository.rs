use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Row};

use fms_domain::models::ai_execution::{
    AiToolCallError, AiToolCallRecord, AiToolCallResult, AiToolCallStatus, AiToolCallType,
};
use fms_domain::ports::ai_execution_repository::{AiExecutionRepositoryError, AiToolCallRepository};

pub struct PgAiToolCallRepository {
    pool: PgPool,
}

impl PgAiToolCallRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn row_to_record(row: &sqlx::postgres::PgRow) -> Result<AiToolCallRecord, AiExecutionRepositoryError> {
        let status_str: String = row.try_get("status").map_err(db_err)?;
        let tool_type_str: String = row.try_get("tool_type").map_err(db_err)?;
        let args_summary: serde_json::Value = row.try_get("args_summary").map_err(db_err)?;
        let result_summary: Option<serde_json::Value> = row.try_get("result_summary").map_err(db_err)?;
        let metadata: serde_json::Value = row.try_get("metadata").map_err(db_err)?;

        let status = AiToolCallStatus::from_str(&status_str)
            .ok_or_else(|| AiExecutionRepositoryError::validation(format!("invalid tool_call status: {status_str}")))?;
        let tool_type = AiToolCallType::from_str(&tool_type_str)
            .ok_or_else(|| AiExecutionRepositoryError::validation(format!("invalid tool_type: {tool_type_str}")))?;

        Ok(AiToolCallRecord {
            tool_call_pk: row.try_get("tool_call_pk").map_err(db_err)?,
            job_id: row.try_get("job_id").map_err(db_err)?,
            run_id: row.try_get("run_id").map_err(db_err)?,
            parent_tool_call_pk: row.try_get("parent_tool_call_pk").map_err(db_err)?,
            root_tool_call_pk: row.try_get("root_tool_call_pk").map_err(db_err)?,
            depth: row.try_get::<i32, _>("depth").map_err(db_err)?,
            round_index: row.try_get::<i32, _>("round_index").map_err(db_err)?,
            tool_call_id: row.try_get("tool_call_id").map_err(db_err)?,
            tool_name: row.try_get("tool_name").map_err(db_err)?,
            tool_type,
            status,
            args_hash: row.try_get("args_hash").map_err(db_err)?,
            args_summary,
            result_hash: row.try_get("result_hash").map_err(db_err)?,
            result_summary,
            error_code: row.try_get("error_code").map_err(db_err)?,
            error_message: row.try_get("error_message").map_err(db_err)?,
            retry_count: row.try_get::<i32, _>("retry_count").map_err(db_err)?,
            max_retries: row.try_get::<i32, _>("max_retries").map_err(db_err)?,
            timeout_seconds: row.try_get::<i32, _>("timeout_seconds").map_err(db_err)?,
            last_heartbeat_at: row.try_get("last_heartbeat_at").map_err(db_err)?,
            idempotency_key: row.try_get("idempotency_key").map_err(db_err)?,
            mq_message_id: row.try_get("mq_message_id").map_err(db_err)?,
            mq_offset: row.try_get::<Option<i64>, _>("mq_offset").map_err(db_err)?,
            created_at: row.try_get("created_at").map_err(db_err)?,
            started_at: row.try_get("started_at").map_err(db_err)?,
            finished_at: row.try_get("finished_at").map_err(db_err)?,
            metadata,
        })
    }
}

#[async_trait]
impl AiToolCallRepository for PgAiToolCallRepository {
    async fn upsert_requested(&self, record: AiToolCallRecord) -> Result<bool, AiExecutionRepositoryError> {
        let result = sqlx::query(
            r#"
            INSERT INTO ai_tool_calls (
                tool_call_pk, job_id, run_id, parent_tool_call_pk, root_tool_call_pk,
                depth, round_index, tool_call_id, tool_name, tool_type, status,
                args_hash, args_summary, result_hash, result_summary, error_code, error_message,
                retry_count, max_retries, timeout_seconds, last_heartbeat_at,
                idempotency_key, mq_message_id, mq_offset, created_at, started_at, finished_at, metadata
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
                $12, $13, $14, $15, $16, $17, $18, $19, $20, $21,
                $22, $23, $24, $25, $26, $27, $28
            )
            ON CONFLICT (run_id, idempotency_key) DO NOTHING
            "#,
        )
        .bind(&record.tool_call_pk)
        .bind(&record.job_id)
        .bind(&record.run_id)
        .bind(&record.parent_tool_call_pk)
        .bind(&record.root_tool_call_pk)
        .bind(record.depth)
        .bind(record.round_index)
        .bind(&record.tool_call_id)
        .bind(&record.tool_name)
        .bind(record.tool_type.as_str())
        .bind(record.status.as_str())
        .bind(&record.args_hash)
        .bind(&record.args_summary)
        .bind(&record.result_hash)
        .bind(&record.result_summary)
        .bind(&record.error_code)
        .bind(&record.error_message)
        .bind(record.retry_count)
        .bind(record.max_retries)
        .bind(record.timeout_seconds)
        .bind(record.last_heartbeat_at)
        .bind(&record.idempotency_key)
        .bind(&record.mq_message_id)
        .bind(record.mq_offset)
        .bind(record.created_at)
        .bind(record.started_at)
        .bind(record.finished_at)
        .bind(&record.metadata)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        Ok(result.rows_affected() > 0)
    }

    async fn mark_authorized(&self, tool_call_pk: &str) -> Result<(), AiExecutionRepositoryError> {
        let now = Utc::now();
        sqlx::query(
            r#"
            UPDATE ai_tool_calls
            SET status = 'authorized', started_at = COALESCE(started_at, $2)
            WHERE tool_call_pk = $1
            "#,
        )
        .bind(tool_call_pk)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn mark_running(&self, tool_call_pk: &str) -> Result<(), AiExecutionRepositoryError> {
        let now = Utc::now();
        sqlx::query(
            r#"
            UPDATE ai_tool_calls
            SET status = 'running', started_at = COALESCE(started_at, $2)
            WHERE tool_call_pk = $1
            "#,
        )
        .bind(tool_call_pk)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn mark_succeeded(
        &self,
        tool_call_pk: &str,
        result: AiToolCallResult,
    ) -> Result<(), AiExecutionRepositoryError> {
        let now = Utc::now();
        sqlx::query(
            r#"
            UPDATE ai_tool_calls
            SET status = 'succeeded',
                result_hash = $2,
                result_summary = $3,
                finished_at = $4
            WHERE tool_call_pk = $1
            "#,
        )
        .bind(tool_call_pk)
        .bind(&result.result_hash)
        .bind(&result.result_summary)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn mark_failed(&self, tool_call_pk: &str, error: AiToolCallError) -> Result<(), AiExecutionRepositoryError> {
        let now = Utc::now();
        let status = if error.retryable {
            "failed_retryable"
        } else {
            "failed_terminal"
        };
        sqlx::query(
            r#"
            UPDATE ai_tool_calls
            SET status = $2,
                error_code = $3,
                error_message = $4,
                finished_at = $5
            WHERE tool_call_pk = $1
            "#,
        )
        .bind(tool_call_pk)
        .bind(status)
        .bind(&error.code)
        .bind(&error.message)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn mark_cancelled(&self, tool_call_pk: &str) -> Result<(), AiExecutionRepositoryError> {
        let now = Utc::now();
        sqlx::query("UPDATE ai_tool_calls SET status = 'cancelled', finished_at = $2 WHERE tool_call_pk = $1")
            .bind(tool_call_pk)
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn mark_expired(&self, tool_call_pk: &str) -> Result<(), AiExecutionRepositoryError> {
        let now = Utc::now();
        sqlx::query("UPDATE ai_tool_calls SET status = 'expired', finished_at = $2 WHERE tool_call_pk = $1")
            .bind(tool_call_pk)
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn mark_proposal_only(&self, tool_call_pk: &str) -> Result<(), AiExecutionRepositoryError> {
        let now = Utc::now();
        sqlx::query("UPDATE ai_tool_calls SET status = 'proposal_only', finished_at = $2 WHERE tool_call_pk = $1")
            .bind(tool_call_pk)
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn mark_denied(
        &self,
        tool_call_pk: &str,
        code: &str,
        message: &str,
    ) -> Result<(), AiExecutionRepositoryError> {
        let now = Utc::now();
        sqlx::query(
            r#"
            UPDATE ai_tool_calls
            SET status = 'denied', error_code = $2, error_message = $3, finished_at = $4
            WHERE tool_call_pk = $1
            "#,
        )
        .bind(tool_call_pk)
        .bind(code)
        .bind(message)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn heartbeat(&self, tool_call_pk: &str) -> Result<(), AiExecutionRepositoryError> {
        let now = Utc::now();
        sqlx::query(
            r#"
            UPDATE ai_tool_calls
            SET last_heartbeat_at = $2
            WHERE tool_call_pk = $1 AND status = 'running'
            "#,
        )
        .bind(tool_call_pk)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn get(&self, tool_call_pk: &str) -> Result<Option<AiToolCallRecord>, AiExecutionRepositoryError> {
        let row = sqlx::query(
            r#"
            SELECT tool_call_pk, job_id, run_id, parent_tool_call_pk, root_tool_call_pk,
                   depth, round_index, tool_call_id, tool_name, tool_type, status,
                   args_hash, args_summary, result_hash, result_summary, error_code, error_message,
                   retry_count, max_retries, timeout_seconds, last_heartbeat_at,
                   idempotency_key, mq_message_id, mq_offset, created_at, started_at, finished_at, metadata
            FROM ai_tool_calls WHERE tool_call_pk = $1
            "#,
        )
        .bind(tool_call_pk)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;

        row.as_ref().map(Self::row_to_record).transpose()
    }

    async fn list_by_run(&self, run_id: &str) -> Result<Vec<AiToolCallRecord>, AiExecutionRepositoryError> {
        let rows = sqlx::query(
            r#"
            SELECT tool_call_pk, job_id, run_id, parent_tool_call_pk, root_tool_call_pk,
                   depth, round_index, tool_call_id, tool_name, tool_type, status,
                   args_hash, args_summary, result_hash, result_summary, error_code, error_message,
                   retry_count, max_retries, timeout_seconds, last_heartbeat_at,
                   idempotency_key, mq_message_id, mq_offset, created_at, started_at, finished_at, metadata
            FROM ai_tool_calls WHERE run_id = $1 ORDER BY created_at ASC
            "#,
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        rows.iter().map(Self::row_to_record).collect()
    }
}

fn db_err(error: sqlx::Error) -> AiExecutionRepositoryError {
    AiExecutionRepositoryError::Database(error.to_string())
}
