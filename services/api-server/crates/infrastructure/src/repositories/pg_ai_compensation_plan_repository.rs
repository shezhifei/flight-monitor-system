use async_trait::async_trait;
use chrono::{Duration, Utc};
use serde_json::Value;
use sqlx::{PgPool, Row};

use fms_domain::models::ai_execution::{AiCompensationMode, AiCompensationPlanRecord, AiCompensationStatus};
use fms_domain::ports::ai_execution_repository::{AiCompensationPlanRepository, AiExecutionRepositoryError};

pub struct PgAiCompensationPlanRepository {
    pool: PgPool,
}

impl PgAiCompensationPlanRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn row_to_record(row: &sqlx::postgres::PgRow) -> Result<AiCompensationPlanRecord, AiExecutionRepositoryError> {
        let status_str: String = row.try_get("status").map_err(db_err)?;
        let mode_str: String = row.try_get("mode").map_err(db_err)?;
        let plan: Value = row.try_get("plan").map_err(db_err)?;
        let execution_result: Option<Value> = row.try_get("execution_result").map_err(db_err)?;

        let status = AiCompensationStatus::from_str(&status_str).ok_or_else(|| {
            AiExecutionRepositoryError::validation(format!("invalid compensation status: {status_str}"))
        })?;
        let mode = AiCompensationMode::from_str(&mode_str)
            .ok_or_else(|| AiExecutionRepositoryError::validation(format!("invalid compensation mode: {mode_str}")))?;

        Ok(AiCompensationPlanRecord {
            compensation_id: row.try_get("compensation_id").map_err(db_err)?,
            receipt_id: row.try_get("receipt_id").map_err(db_err)?,
            proposal_id: row.try_get("proposal_id").map_err(db_err)?,
            status,
            mode,
            plan,
            requires_approval: row.try_get("requires_approval").map_err(db_err)?,
            approved_by: row.try_get("approved_by").map_err(db_err)?,
            approved_at: row.try_get("approved_at").map_err(db_err)?,
            executed_by: row.try_get("executed_by").map_err(db_err)?,
            executed_at: row.try_get("executed_at").map_err(db_err)?,
            execution_result,
            execution_error: row.try_get("execution_error").map_err(db_err)?,
            created_at: row.try_get("created_at").map_err(db_err)?,
            updated_at: row.try_get("updated_at").map_err(db_err)?,
        })
    }
}

#[async_trait]
impl AiCompensationPlanRepository for PgAiCompensationPlanRepository {
    async fn upsert(&self, plan: AiCompensationPlanRecord) -> Result<bool, AiExecutionRepositoryError> {
        let result = sqlx::query(
            r#"
            INSERT INTO ai_compensation_plans (
                compensation_id, receipt_id, proposal_id, status, mode, plan,
                requires_approval, approved_by, approved_at, executed_by, executed_at,
                execution_result, execution_error, created_at, updated_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15
            )
            ON CONFLICT (receipt_id, mode) DO NOTHING
            "#,
        )
        .bind(&plan.compensation_id)
        .bind(&plan.receipt_id)
        .bind(&plan.proposal_id)
        .bind(plan.status.as_str())
        .bind(plan.mode.as_str())
        .bind(&plan.plan)
        .bind(plan.requires_approval)
        .bind(&plan.approved_by)
        .bind(plan.approved_at)
        .bind(&plan.executed_by)
        .bind(plan.executed_at)
        .bind(&plan.execution_result)
        .bind(&plan.execution_error)
        .bind(plan.created_at)
        .bind(plan.updated_at)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        Ok(result.rows_affected() > 0)
    }

    async fn get(&self, compensation_id: &str) -> Result<Option<AiCompensationPlanRecord>, AiExecutionRepositoryError> {
        let row = sqlx::query(
            r#"
            SELECT compensation_id, receipt_id, proposal_id, status, mode, plan,
                   requires_approval, approved_by, approved_at, executed_by, executed_at,
                   execution_result, execution_error, created_at, updated_at
            FROM ai_compensation_plans WHERE compensation_id = $1
            "#,
        )
        .bind(compensation_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;

        row.as_ref().map(Self::row_to_record).transpose()
    }

    async fn list_by_receipt(
        &self,
        receipt_id: &str,
    ) -> Result<Vec<AiCompensationPlanRecord>, AiExecutionRepositoryError> {
        let rows = sqlx::query(
            r#"
            SELECT compensation_id, receipt_id, proposal_id, status, mode, plan,
                   requires_approval, approved_by, approved_at, executed_by, executed_at,
                   execution_result, execution_error, created_at, updated_at
            FROM ai_compensation_plans WHERE receipt_id = $1 ORDER BY created_at ASC
            "#,
        )
        .bind(receipt_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        rows.iter().map(Self::row_to_record).collect()
    }

    async fn list_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Vec<AiCompensationPlanRecord>, AiExecutionRepositoryError> {
        let rows = sqlx::query(
            r#"
            SELECT compensation_id, receipt_id, proposal_id, status, mode, plan,
                   requires_approval, approved_by, approved_at, executed_by, executed_at,
                   execution_result, execution_error, created_at, updated_at
            FROM ai_compensation_plans WHERE proposal_id = $1 ORDER BY created_at ASC
            "#,
        )
        .bind(proposal_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        rows.iter().map(Self::row_to_record).collect()
    }

    async fn mark_executing(
        &self,
        compensation_id: &str,
        executed_by: &str,
    ) -> Result<bool, AiExecutionRepositoryError> {
        let now = Utc::now();
        let mut tx = self.pool.begin().await.map_err(db_err)?;

        let row = sqlx::query(
            r#"
            SELECT compensation_id, status FROM ai_compensation_plans
            WHERE compensation_id = $1 AND status IN ('planned', 'approved')
            FOR UPDATE SKIP LOCKED
            "#,
        )
        .bind(compensation_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;

        if row.is_none() {
            tx.rollback().await.map_err(db_err)?;
            return Ok(false);
        }

        sqlx::query(
            r#"
            UPDATE ai_compensation_plans
            SET status = 'executing', executed_by = $2, executed_at = $3, updated_at = $3
            WHERE compensation_id = $1
            "#,
        )
        .bind(compensation_id)
        .bind(executed_by)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(true)
    }

    async fn mark_succeeded(
        &self,
        compensation_id: &str,
        executed_by: &str,
        result: Value,
    ) -> Result<(), AiExecutionRepositoryError> {
        let now = Utc::now();
        sqlx::query(
            r#"
            UPDATE ai_compensation_plans
            SET status = 'succeeded',
                executed_by = $2,
                executed_at = COALESCE(executed_at, $3),
                execution_result = $4,
                updated_at = $3
            WHERE compensation_id = $1
            "#,
        )
        .bind(compensation_id)
        .bind(executed_by)
        .bind(now)
        .bind(result)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn mark_failed(&self, compensation_id: &str, error: &str) -> Result<(), AiExecutionRepositoryError> {
        let now = Utc::now();
        sqlx::query(
            r#"
            UPDATE ai_compensation_plans
            SET status = 'failed', execution_error = $2, updated_at = $3
            WHERE compensation_id = $1
            "#,
        )
        .bind(compensation_id)
        .bind(error)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn list_pending_approval(
        &self,
        older_than_seconds: i64,
    ) -> Result<Vec<AiCompensationPlanRecord>, AiExecutionRepositoryError> {
        let cutoff = Utc::now() - Duration::seconds(older_than_seconds);
        let rows = sqlx::query(
            r#"
            SELECT compensation_id, receipt_id, proposal_id, status, mode, plan,
                   requires_approval, approved_by, approved_at, executed_by, executed_at,
                   execution_result, execution_error, created_at, updated_at
            FROM ai_compensation_plans
            WHERE status = 'planned' AND requires_approval = false AND created_at < $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(cutoff)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        rows.iter().map(Self::row_to_record).collect()
    }

    async fn list_executing_past_timeout(
        &self,
        timeout_seconds: i64,
    ) -> Result<Vec<AiCompensationPlanRecord>, AiExecutionRepositoryError> {
        let cutoff = Utc::now() - Duration::seconds(timeout_seconds);
        let rows = sqlx::query(
            r#"
            SELECT compensation_id, receipt_id, proposal_id, status, mode, plan,
                   requires_approval, approved_by, approved_at, executed_by, executed_at,
                   execution_result, execution_error, created_at, updated_at
            FROM ai_compensation_plans
            WHERE status = 'executing' AND updated_at < $1
            ORDER BY updated_at ASC
            "#,
        )
        .bind(cutoff)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        rows.iter().map(Self::row_to_record).collect()
    }

    async fn list_by_status(
        &self,
        status: AiCompensationStatus,
    ) -> Result<Vec<AiCompensationPlanRecord>, AiExecutionRepositoryError> {
        let rows = sqlx::query(
            r#"
            SELECT compensation_id, receipt_id, proposal_id, status, mode, plan,
                   requires_approval, approved_by, approved_at, executed_by, executed_at,
                   execution_result, execution_error, created_at, updated_at
            FROM ai_compensation_plans WHERE status = $1 ORDER BY created_at ASC
            "#,
        )
        .bind(status.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        rows.iter().map(Self::row_to_record).collect()
    }
}

fn db_err(error: sqlx::Error) -> AiExecutionRepositoryError {
    AiExecutionRepositoryError::Database(error.to_string())
}
