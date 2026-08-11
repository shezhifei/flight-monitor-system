use async_trait::async_trait;
use sqlx::{PgPool, Row};

use fms_domain::models::ai_execution::AiActionReceiptRecord;
use fms_domain::ports::ai_execution_repository::{AiActionReceiptRepository, AiExecutionRepositoryError};

pub struct PgAiActionReceiptRepository {
    pool: PgPool,
}

impl PgAiActionReceiptRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn row_to_record(row: &sqlx::postgres::PgRow) -> Result<AiActionReceiptRecord, AiExecutionRepositoryError> {
        let execution_result: serde_json::Value = row.try_get("execution_result").map_err(db_err)?;

        Ok(AiActionReceiptRecord {
            receipt_id: row.try_get("receipt_id").map_err(db_err)?,
            proposal_id: row.try_get("proposal_id").map_err(db_err)?,
            job_id: row.try_get("job_id").map_err(db_err)?,
            run_id: row.try_get("run_id").map_err(db_err)?,
            tool_call_pk: row.try_get("tool_call_pk").map_err(db_err)?,
            object_type: row.try_get("object_type").map_err(db_err)?,
            object_id: row.try_get("object_id").map_err(db_err)?,
            action_name: row.try_get("action_name").map_err(db_err)?,
            idempotency_key: row.try_get("idempotency_key").map_err(db_err)?,
            before_checkpoint_id: row.try_get("before_checkpoint_id").map_err(db_err)?,
            after_checkpoint_id: row.try_get("after_checkpoint_id").map_err(db_err)?,
            outbox_event_id: row.try_get("outbox_event_id").map_err(db_err)?,
            execution_result,
            executed_by: row.try_get("executed_by").map_err(db_err)?,
            executed_at: row.try_get("executed_at").map_err(db_err)?,
        })
    }
}

#[async_trait]
impl AiActionReceiptRepository for PgAiActionReceiptRepository {
    async fn upsert(&self, receipt: AiActionReceiptRecord) -> Result<bool, AiExecutionRepositoryError> {
        let result = sqlx::query(
            r#"
            INSERT INTO ai_action_receipts (
                receipt_id, proposal_id, job_id, run_id, tool_call_pk,
                object_type, object_id, action_name, idempotency_key,
                before_checkpoint_id, after_checkpoint_id, outbox_event_id,
                execution_result, executed_by, executed_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15
            )
            ON CONFLICT (idempotency_key) DO NOTHING
            "#,
        )
        .bind(&receipt.receipt_id)
        .bind(&receipt.proposal_id)
        .bind(&receipt.job_id)
        .bind(&receipt.run_id)
        .bind(&receipt.tool_call_pk)
        .bind(&receipt.object_type)
        .bind(&receipt.object_id)
        .bind(&receipt.action_name)
        .bind(&receipt.idempotency_key)
        .bind(&receipt.before_checkpoint_id)
        .bind(&receipt.after_checkpoint_id)
        .bind(&receipt.outbox_event_id)
        .bind(&receipt.execution_result)
        .bind(&receipt.executed_by)
        .bind(receipt.executed_at)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        Ok(result.rows_affected() > 0)
    }

    async fn get_by_idempotency_key(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<AiActionReceiptRecord>, AiExecutionRepositoryError> {
        let row = sqlx::query(
            r#"
            SELECT receipt_id, proposal_id, job_id, run_id, tool_call_pk,
                   object_type, object_id, action_name, idempotency_key,
                   before_checkpoint_id, after_checkpoint_id, outbox_event_id,
                   execution_result, executed_by, executed_at
            FROM ai_action_receipts WHERE idempotency_key = $1
            "#,
        )
        .bind(idempotency_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;

        row.as_ref().map(Self::row_to_record).transpose()
    }

    async fn get(&self, receipt_id: &str) -> Result<Option<AiActionReceiptRecord>, AiExecutionRepositoryError> {
        let row = sqlx::query(
            r#"
            SELECT receipt_id, proposal_id, job_id, run_id, tool_call_pk,
                   object_type, object_id, action_name, idempotency_key,
                   before_checkpoint_id, after_checkpoint_id, outbox_event_id,
                   execution_result, executed_by, executed_at
            FROM ai_action_receipts WHERE receipt_id = $1
            "#,
        )
        .bind(receipt_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;

        row.as_ref().map(Self::row_to_record).transpose()
    }

    async fn list_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Vec<AiActionReceiptRecord>, AiExecutionRepositoryError> {
        let rows = sqlx::query(
            r#"
            SELECT receipt_id, proposal_id, job_id, run_id, tool_call_pk,
                   object_type, object_id, action_name, idempotency_key,
                   before_checkpoint_id, after_checkpoint_id, outbox_event_id,
                   execution_result, executed_by, executed_at
            FROM ai_action_receipts WHERE proposal_id = $1 ORDER BY executed_at ASC
            "#,
        )
        .bind(proposal_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        rows.iter().map(Self::row_to_record).collect()
    }

    async fn list_by_run(&self, run_id: &str) -> Result<Vec<AiActionReceiptRecord>, AiExecutionRepositoryError> {
        let rows = sqlx::query(
            r#"
            SELECT receipt_id, proposal_id, job_id, run_id, tool_call_pk,
                   object_type, object_id, action_name, idempotency_key,
                   before_checkpoint_id, after_checkpoint_id, outbox_event_id,
                   execution_result, executed_by, executed_at
            FROM ai_action_receipts WHERE run_id = $1 ORDER BY executed_at ASC
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
