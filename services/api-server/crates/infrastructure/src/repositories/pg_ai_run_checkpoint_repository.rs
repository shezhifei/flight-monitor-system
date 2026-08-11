use async_trait::async_trait;
use sqlx::{PgPool, Row};

use fms_domain::models::ai_execution::{AiRunCheckpointRecord, AiRunCheckpointType};
use fms_domain::ports::ai_execution_repository::{
    assert_checkpoint_size_within_budget, AiExecutionRepositoryError, AiRunCheckpointRepository,
};

pub struct PgAiRunCheckpointRepository {
    pool: PgPool,
}

impl PgAiRunCheckpointRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn row_to_record(row: &sqlx::postgres::PgRow) -> Result<AiRunCheckpointRecord, AiExecutionRepositoryError> {
        let checkpoint_type_str: String = row.try_get("checkpoint_type").map_err(db_err)?;
        let snapshot: serde_json::Value = row.try_get("snapshot").map_err(db_err)?;

        let checkpoint_type = AiRunCheckpointType::from_str(&checkpoint_type_str).ok_or_else(|| {
            AiExecutionRepositoryError::validation(format!("invalid checkpoint_type: {checkpoint_type_str}"))
        })?;

        Ok(AiRunCheckpointRecord {
            checkpoint_id: row.try_get("checkpoint_id").map_err(db_err)?,
            job_id: row.try_get("job_id").map_err(db_err)?,
            run_id: row.try_get("run_id").map_err(db_err)?,
            sequence_no: row.try_get::<i64, _>("sequence_no").map_err(db_err)?,
            checkpoint_type,
            tool_call_pk: row.try_get("tool_call_pk").map_err(db_err)?,
            proposal_id: row.try_get("proposal_id").map_err(db_err)?,
            snapshot_hash: row.try_get("snapshot_hash").map_err(db_err)?,
            snapshot,
            snapshot_size_bytes: row.try_get::<i32, _>("snapshot_size_bytes").map_err(db_err)?,
            mq_message_id: row.try_get("mq_message_id").map_err(db_err)?,
            created_at: row.try_get("created_at").map_err(db_err)?,
        })
    }
}

#[async_trait]
impl AiRunCheckpointRepository for PgAiRunCheckpointRepository {
    async fn upsert(&self, record: AiRunCheckpointRecord) -> Result<bool, AiExecutionRepositoryError> {
        assert_checkpoint_size_within_budget(record.snapshot_size_bytes as u32)?;

        let result = sqlx::query(
            r#"
            INSERT INTO ai_run_checkpoints (
                checkpoint_id, job_id, run_id, sequence_no, checkpoint_type,
                tool_call_pk, proposal_id, snapshot_hash, snapshot, snapshot_size_bytes,
                mq_message_id, created_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12
            )
            ON CONFLICT (run_id, sequence_no) DO NOTHING
            "#,
        )
        .bind(&record.checkpoint_id)
        .bind(&record.job_id)
        .bind(&record.run_id)
        .bind(record.sequence_no)
        .bind(record.checkpoint_type.as_str())
        .bind(&record.tool_call_pk)
        .bind(&record.proposal_id)
        .bind(&record.snapshot_hash)
        .bind(&record.snapshot)
        .bind(record.snapshot_size_bytes)
        .bind(&record.mq_message_id)
        .bind(record.created_at)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        Ok(result.rows_affected() > 0)
    }

    async fn list_by_run(&self, run_id: &str) -> Result<Vec<AiRunCheckpointRecord>, AiExecutionRepositoryError> {
        let rows = sqlx::query(
            r#"
            SELECT checkpoint_id, job_id, run_id, sequence_no, checkpoint_type,
                   tool_call_pk, proposal_id, snapshot_hash, snapshot, snapshot_size_bytes,
                   mq_message_id, created_at
            FROM ai_run_checkpoints
            WHERE run_id = $1
            ORDER BY sequence_no ASC
            "#,
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        rows.iter().map(Self::row_to_record).collect()
    }

    async fn latest_recoverable(
        &self,
        run_id: &str,
    ) -> Result<Option<AiRunCheckpointRecord>, AiExecutionRepositoryError> {
        let row = sqlx::query(
            r#"
            SELECT checkpoint_id, job_id, run_id, sequence_no, checkpoint_type,
                   tool_call_pk, proposal_id, snapshot_hash, snapshot, snapshot_size_bytes,
                   mq_message_id, created_at
            FROM ai_run_checkpoints
            WHERE run_id = $1
              AND checkpoint_type IN ('before_tool', 'after_tool')
            ORDER BY sequence_no DESC
            LIMIT 1
            "#,
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;

        row.as_ref().map(Self::row_to_record).transpose()
    }

    async fn mark_superseded(&self, run_id: &str, before_sequence_no: u64) -> Result<u64, AiExecutionRepositoryError> {
        let result = sqlx::query(
            r#"
            SELECT COUNT(*) as cnt
            FROM ai_run_checkpoints
            WHERE run_id = $1
              AND sequence_no < $2
              AND checkpoint_type IN ('before_tool', 'after_tool')
            "#,
        )
        .bind(run_id)
        .bind(before_sequence_no as i64)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;

        let cnt: i64 = result.try_get("cnt").map_err(db_err)?;
        Ok(cnt as u64)
    }
}

fn db_err(error: sqlx::Error) -> AiExecutionRepositoryError {
    AiExecutionRepositoryError::Database(error.to_string())
}
