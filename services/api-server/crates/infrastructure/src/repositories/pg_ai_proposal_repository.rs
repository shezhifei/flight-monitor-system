use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

use fms_domain::models::ai_proposal::{
    ActionProposalQuery, ActionProposalStats, ActionProposalStatus, AiActionProposal, ApprovalPolicy, ConstraintResult,
    RiskLevel,
};
use fms_domain::ports::ai_proposal_repository::{
    AiProposalRepository, AiProposalRepositoryError, SmokeProposalRow, SmokeProposalSummary,
};

use super::soft_delete_audit::record_soft_delete;

pub struct PgAiProposalRepository {
    pool: PgPool,
}

impl PgAiProposalRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn row_to_model(row: &sqlx::postgres::PgRow) -> Result<AiActionProposal, AiProposalRepositoryError> {
        let risk_code: i16 = row.try_get("risk_level").map_err(db_err)?;
        let approval_code: i16 = row.try_get("approval_policy").map_err(db_err)?;
        let status_code: i16 = row.try_get("status").map_err(db_err)?;
        let required_permissions_value: serde_json::Value = row.try_get("required_permissions").map_err(db_err)?;
        let constraint_results_value: serde_json::Value = row.try_get("constraint_results").map_err(db_err)?;

        let required_permissions = serde_json::from_value(required_permissions_value)
            .map_err(|err| AiProposalRepositoryError::validation(err.to_string()))?;
        let constraint_results: Vec<ConstraintResult> = serde_json::from_value(constraint_results_value)
            .map_err(|err| AiProposalRepositoryError::validation(err.to_string()))?;

        Ok(AiActionProposal {
            proposal_id: row.try_get("proposal_id").map_err(db_err)?,
            job_id: row.try_get("job_id").map_err(db_err)?,
            run_id: row.try_get("run_id").map_err(db_err)?,
            ontology_version: row.try_get("ontology_version").map_err(db_err)?,
            object_type: row.try_get("object_type").map_err(db_err)?,
            object_id: row.try_get("object_id").map_err(db_err)?,
            action_name: row.try_get("action_name").map_err(db_err)?,
            arguments: row.try_get("arguments").map_err(db_err)?,
            risk_level: RiskLevel::from_code(risk_code as i32)
                .ok_or_else(|| AiProposalRepositoryError::validation("invalid risk_level"))?,
            required_permissions,
            approval_policy: ApprovalPolicy::from_code(approval_code as i32)
                .ok_or_else(|| AiProposalRepositoryError::validation("invalid approval_policy"))?,
            before_snapshot: row.try_get("before_snapshot").map_err(db_err)?,
            after_preview: row.try_get("after_preview").map_err(db_err)?,
            constraint_results,
            confidence: row.try_get("confidence").map_err(db_err)?,
            reasoning: row.try_get("reasoning").map_err(db_err)?,
            status: ActionProposalStatus::from_code(status_code as i32)
                .ok_or_else(|| AiProposalRepositoryError::validation("invalid status"))?,
            pending_action_id: row.try_get("pending_action_id").map_err(db_err)?,
            approved_by: row.try_get("approved_by").map_err(db_err)?,
            approved_at: row.try_get("approved_at").map_err(db_err)?,
            rejected_by: row.try_get("rejected_by").map_err(db_err)?,
            rejected_reason: row.try_get("rejected_reason").map_err(db_err)?,
            rejected_at: row.try_get("rejected_at").map_err(db_err)?,
            executed_by: row.try_get("executed_by").map_err(db_err)?,
            executed_at: row.try_get("executed_at").map_err(db_err)?,
            execution_result: row.try_get("execution_result").map_err(db_err)?,
            execution_error: row.try_get("execution_error").map_err(db_err)?,
            created_at: row.try_get("created_at").map_err(db_err)?,
            updated_at: row.try_get("updated_at").map_err(db_err)?,
            expires_at: row.try_get("expires_at").map_err(db_err)?,
            correlation_id: row.try_get("correlation_id").map_err(db_err)?,
            metadata: row.try_get("metadata").map_err(db_err)?,
        })
    }
}

#[async_trait]
impl AiProposalRepository for PgAiProposalRepository {
    async fn save(&self, proposal: &AiActionProposal) -> Result<(), AiProposalRepositoryError> {
        let required_permissions = serde_json::to_value(&proposal.required_permissions)
            .map_err(|err| AiProposalRepositoryError::validation(err.to_string()))?;
        let constraint_results = serde_json::to_value(&proposal.constraint_results)
            .map_err(|err| AiProposalRepositoryError::validation(err.to_string()))?;

        sqlx::query(
            r#"
            INSERT INTO ai_action_proposals (
                proposal_id, job_id, run_id, ontology_version, object_type, object_id,
                action_name, arguments, risk_level, required_permissions, approval_policy,
                before_snapshot, after_preview, constraint_results, confidence, reasoning,
                status, pending_action_id, approved_by, approved_at, rejected_by,
                rejected_reason, rejected_at, executed_by, executed_at, execution_result,
                execution_error, created_at, updated_at, expires_at, correlation_id, metadata
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
                $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28,
                $29, $30, $31, $32
            )
            ON CONFLICT (proposal_id) DO UPDATE SET
                ontology_version = EXCLUDED.ontology_version,
                arguments = EXCLUDED.arguments,
                risk_level = EXCLUDED.risk_level,
                required_permissions = EXCLUDED.required_permissions,
                approval_policy = EXCLUDED.approval_policy,
                before_snapshot = EXCLUDED.before_snapshot,
                after_preview = EXCLUDED.after_preview,
                constraint_results = EXCLUDED.constraint_results,
                confidence = EXCLUDED.confidence,
                reasoning = EXCLUDED.reasoning,
                status = EXCLUDED.status,
                pending_action_id = EXCLUDED.pending_action_id,
                approved_by = EXCLUDED.approved_by,
                approved_at = EXCLUDED.approved_at,
                rejected_by = EXCLUDED.rejected_by,
                rejected_reason = EXCLUDED.rejected_reason,
                rejected_at = EXCLUDED.rejected_at,
                executed_by = EXCLUDED.executed_by,
                executed_at = EXCLUDED.executed_at,
                execution_result = EXCLUDED.execution_result,
                execution_error = EXCLUDED.execution_error,
                updated_at = EXCLUDED.updated_at,
                expires_at = EXCLUDED.expires_at,
                correlation_id = EXCLUDED.correlation_id,
                metadata = EXCLUDED.metadata,
                deleted_at = NULL
            "#,
        )
        .bind(&proposal.proposal_id)
        .bind(&proposal.job_id)
        .bind(&proposal.run_id)
        .bind(&proposal.ontology_version)
        .bind(&proposal.object_type)
        .bind(&proposal.object_id)
        .bind(&proposal.action_name)
        .bind(&proposal.arguments)
        .bind(proposal.risk_level.code() as i16)
        .bind(required_permissions)
        .bind(proposal.approval_policy.code() as i16)
        .bind(&proposal.before_snapshot)
        .bind(&proposal.after_preview)
        .bind(constraint_results)
        .bind(proposal.confidence)
        .bind(&proposal.reasoning)
        .bind(proposal.status.code() as i16)
        .bind(&proposal.pending_action_id)
        .bind(&proposal.approved_by)
        .bind(proposal.approved_at)
        .bind(&proposal.rejected_by)
        .bind(&proposal.rejected_reason)
        .bind(proposal.rejected_at)
        .bind(&proposal.executed_by)
        .bind(proposal.executed_at)
        .bind(&proposal.execution_result)
        .bind(&proposal.execution_error)
        .bind(proposal.created_at)
        .bind(proposal.updated_at)
        .bind(proposal.expires_at)
        .bind(&proposal.correlation_id)
        .bind(&proposal.metadata)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        Ok(())
    }

    async fn find_by_id(&self, proposal_id: &str) -> Result<Option<AiActionProposal>, AiProposalRepositoryError> {
        let row = sqlx::query(
            "SELECT * FROM ai_action_proposals WHERE proposal_id = $1 AND deleted_at IS NULL",
        )
            .bind(proposal_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_err)?;
        row.as_ref().map(Self::row_to_model).transpose()
    }

    async fn find_by_pending_action_id(
        &self,
        pending_action_id: &str,
    ) -> Result<Option<AiActionProposal>, AiProposalRepositoryError> {
        let row = sqlx::query(
            "SELECT * FROM ai_action_proposals WHERE pending_action_id = $1 AND deleted_at IS NULL",
        )
            .bind(pending_action_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_err)?;
        row.as_ref().map(Self::row_to_model).transpose()
    }

    async fn find_by_job_id(&self, job_id: &str) -> Result<Vec<AiActionProposal>, AiProposalRepositoryError> {
        self.search(&ActionProposalQuery {
            job_id: Some(job_id.to_string()),
            ..Default::default()
        })
        .await
    }

    async fn find_by_run_id(&self, run_id: &str) -> Result<Vec<AiActionProposal>, AiProposalRepositoryError> {
        self.search(&ActionProposalQuery {
            run_id: Some(run_id.to_string()),
            ..Default::default()
        })
        .await
    }

    async fn find_by_object(
        &self,
        object_type: &str,
        object_id: &str,
    ) -> Result<Vec<AiActionProposal>, AiProposalRepositoryError> {
        self.search(&ActionProposalQuery {
            object_type: Some(object_type.to_string()),
            object_id: Some(object_id.to_string()),
            ..Default::default()
        })
        .await
    }

    async fn search(&self, query: &ActionProposalQuery) -> Result<Vec<AiActionProposal>, AiProposalRepositoryError> {
        let limit = query.limit.unwrap_or(50).min(200) as i64;
        let offset = query.offset.unwrap_or(0) as i64;
        let status = query.status.map(|item| item.code() as i16);
        let risk = query.risk_level.map(|item| item.code() as i16);
        let approval = query.approval_policy.map(|item| item.code() as i16);

        let rows = sqlx::query(
            r#"
            SELECT * FROM ai_action_proposals
            WHERE ($1::text IS NULL OR job_id = $1)
              AND ($2::text IS NULL OR run_id = $2)
              AND ($3::text IS NULL OR object_type = $3)
              AND ($4::text IS NULL OR object_id = $4)
              AND ($5::text IS NULL OR action_name = $5)
              AND ($6::smallint IS NULL OR status = $6)
              AND ($7::smallint IS NULL OR risk_level = $7)
              AND ($8::smallint IS NULL OR approval_policy = $8)
              AND ($9::text IS NULL OR pending_action_id = $9)
              AND ($10::text IS NULL OR metadata->>'idempotency_key' = $10)
              AND ($11::timestamptz IS NULL OR created_at >= $11)
              AND ($12::timestamptz IS NULL OR created_at <= $12)
              AND deleted_at IS NULL
            ORDER BY created_at DESC
            LIMIT $13 OFFSET $14
            "#,
        )
        .bind(&query.job_id)
        .bind(&query.run_id)
        .bind(&query.object_type)
        .bind(&query.object_id)
        .bind(&query.action_name)
        .bind(status)
        .bind(risk)
        .bind(approval)
        .bind(&query.pending_action_id)
        .bind(&query.idempotency_key)
        .bind(query.created_after)
        .bind(query.created_before)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        rows.iter().map(Self::row_to_model).collect()
    }

    async fn find_pending(&self) -> Result<Vec<AiActionProposal>, AiProposalRepositoryError> {
        self.search(&ActionProposalQuery {
            status: Some(ActionProposalStatus::Pending),
            ..Default::default()
        })
        .await
    }

    async fn find_expired(&self) -> Result<Vec<AiActionProposal>, AiProposalRepositoryError> {
        let rows = sqlx::query(
            "SELECT * FROM ai_action_proposals \
             WHERE expires_at IS NOT NULL AND expires_at < now() AND deleted_at IS NULL",
        )
            .fetch_all(&self.pool)
            .await
            .map_err(db_err)?;
        rows.iter().map(Self::row_to_model).collect()
    }

    async fn count(&self, query: &ActionProposalQuery) -> Result<usize, AiProposalRepositoryError> {
        let status = query.status.map(|item| item.code() as i16);
        let risk = query.risk_level.map(|item| item.code() as i16);
        let approval = query.approval_policy.map(|item| item.code() as i16);

        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM ai_action_proposals
            WHERE ($1::text IS NULL OR job_id = $1)
              AND ($2::text IS NULL OR run_id = $2)
              AND ($3::text IS NULL OR object_type = $3)
              AND ($4::text IS NULL OR object_id = $4)
              AND ($5::text IS NULL OR action_name = $5)
              AND ($6::smallint IS NULL OR status = $6)
              AND ($7::smallint IS NULL OR risk_level = $7)
              AND ($8::smallint IS NULL OR approval_policy = $8)
              AND ($9::text IS NULL OR pending_action_id = $9)
              AND ($10::text IS NULL OR metadata->>'idempotency_key' = $10)
              AND ($11::timestamptz IS NULL OR created_at >= $11)
              AND ($12::timestamptz IS NULL OR created_at <= $12)
              AND deleted_at IS NULL
            "#,
        )
        .bind(&query.job_id)
        .bind(&query.run_id)
        .bind(&query.object_type)
        .bind(&query.object_id)
        .bind(&query.action_name)
        .bind(status)
        .bind(risk)
        .bind(approval)
        .bind(&query.pending_action_id)
        .bind(&query.idempotency_key)
        .bind(query.created_after)
        .bind(query.created_before)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;

        usize::try_from(count).map_err(|err| AiProposalRepositoryError::database(err.to_string()))
    }

    async fn get_stats(&self) -> Result<ActionProposalStats, AiProposalRepositoryError> {
        let totals = sqlx::query(
            r#"
            SELECT
                COUNT(*)::bigint AS total,
                COALESCE(AVG(confidence), 0)::double precision AS avg_confidence,
                COUNT(*) FILTER (WHERE status = $1)::bigint AS approved,
                COUNT(*) FILTER (WHERE status = $2)::bigint AS rejected,
                COUNT(*) FILTER (WHERE status = $3)::bigint AS executed,
                COUNT(*) FILTER (WHERE status = $4)::bigint AS failed
            FROM ai_action_proposals
            WHERE deleted_at IS NULL
            "#,
        )
        .bind(ActionProposalStatus::Approved.code() as i16)
        .bind(ActionProposalStatus::Rejected.code() as i16)
        .bind(ActionProposalStatus::Executed.code() as i16)
        .bind(ActionProposalStatus::Failed.code() as i16)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;

        let total: i64 = totals.try_get("total").map_err(db_err)?;
        let mut stats = ActionProposalStats {
            total: usize::try_from(total).map_err(|err| AiProposalRepositoryError::database(err.to_string()))?,
            avg_confidence: totals.try_get("avg_confidence").map_err(db_err)?,
            ..Default::default()
        };

        if stats.total == 0 {
            return Ok(stats);
        }

        let by_status = self
            .group_counts("status", |code| {
                ActionProposalStatus::from_code(code).map(|status| status.label().to_string())
            })
            .await?;
        let by_risk = self
            .group_counts("risk_level", |code| {
                RiskLevel::from_code(code).map(|risk| risk.label().to_string())
            })
            .await?;
        let by_object = self.group_object_counts().await?;

        let approved: i64 = totals.try_get("approved").map_err(db_err)?;
        let rejected: i64 = totals.try_get("rejected").map_err(db_err)?;
        let executed: i64 = totals.try_get("executed").map_err(db_err)?;
        let failed: i64 = totals.try_get("failed").map_err(db_err)?;
        let terminal = approved + rejected;
        let execution_terminal = executed + failed;
        stats.by_status = serde_json::Value::Object(by_status);
        stats.by_risk_level = serde_json::Value::Object(by_risk);
        stats.by_object_type = serde_json::Value::Object(by_object);
        stats.approval_rate = if terminal > 0 {
            approved as f64 / terminal as f64
        } else {
            0.0
        };
        stats.rejection_rate = if terminal > 0 {
            rejected as f64 / terminal as f64
        } else {
            0.0
        };
        stats.execution_success_rate = if execution_terminal > 0 {
            executed as f64 / execution_terminal as f64
        } else {
            0.0
        };
        Ok(stats)
    }

    async fn update_status(
        &self,
        proposal_id: &str,
        status: ActionProposalStatus,
    ) -> Result<(), AiProposalRepositoryError> {
        sqlx::query("UPDATE ai_action_proposals SET status = $2, updated_at = now() WHERE proposal_id = $1")
            .bind(proposal_id)
            .bind(status.code() as i16)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn link_pending_action(
        &self,
        proposal_id: &str,
        pending_action_id: &str,
    ) -> Result<(), AiProposalRepositoryError> {
        sqlx::query("UPDATE ai_action_proposals SET pending_action_id = $2, updated_at = now() WHERE proposal_id = $1")
            .bind(proposal_id)
            .bind(pending_action_id)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn unlink_pending_action(&self, pending_action_id: &str) -> Result<(), AiProposalRepositoryError> {
        sqlx::query(
            "UPDATE ai_action_proposals SET pending_action_id = NULL, updated_at = now() WHERE pending_action_id = $1",
        )
        .bind(pending_action_id)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn delete(&self, proposal_id: &str) -> Result<(), AiProposalRepositoryError> {
        // 审计要求软删除：仅标记 deleted_at，行保留
        let result = sqlx::query(
            "UPDATE ai_action_proposals SET deleted_at = NOW(), updated_at = NOW() \
             WHERE proposal_id = $1 AND deleted_at IS NULL",
        )
        .bind(proposal_id)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        if result.rows_affected() > 0 {
            record_soft_delete(&self.pool, "ai_action_proposal", proposal_id, "soft_delete").await;
        }
        Ok(())
    }

    async fn count_pending_by_risk(&self) -> Result<Vec<(i16, i64)>, AiProposalRepositoryError> {
        let rows: Vec<(i16, i64)> = sqlx::query_as(
            "SELECT risk_level, COUNT(*)::bigint FROM ai_action_proposals \
             WHERE status = 2 AND deleted_at IS NULL GROUP BY risk_level ORDER BY risk_level",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(rows)
    }

    async fn count_failed_since(&self, cutoff: DateTime<Utc>) -> Result<i64, AiProposalRepositoryError> {
        let count: (i64,) =
            sqlx::query_as(
                "SELECT COUNT(*)::bigint FROM ai_action_proposals \
                 WHERE status = 7 AND updated_at >= $1 AND deleted_at IS NULL",
            )
                .bind(cutoff)
                .fetch_one(&self.pool)
                .await
                .map_err(db_err)?;
        Ok(count.0)
    }

    async fn count_executed_since(&self, cutoff: DateTime<Utc>) -> Result<i64, AiProposalRepositoryError> {
        let count: (i64,) =
            sqlx::query_as(
                "SELECT COUNT(*)::bigint FROM ai_action_proposals \
                 WHERE status = 6 AND updated_at >= $1 AND deleted_at IS NULL",
            )
                .bind(cutoff)
                .fetch_one(&self.pool)
                .await
                .map_err(db_err)?;
        Ok(count.0)
    }

    async fn smoke_summary(&self) -> Result<Option<SmokeProposalSummary>, AiProposalRepositoryError> {
        let row: Option<(Option<DateTime<Utc>>, i64, i64, i64)> = sqlx::query_as(
            r#"
            SELECT
                MAX(created_at),
                COUNT(*)::bigint,
                COUNT(*) FILTER (WHERE status = 6)::bigint,
                COUNT(*) FILTER (WHERE status = 7)::bigint
            FROM ai_action_proposals
            WHERE metadata->>'smoke' = 'true'
            "#,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;

        let Some((last_run_at, total, succeeded, failed)) = row else {
            return Ok(None);
        };
        if total == 0 {
            return Ok(None);
        }
        Ok(Some(SmokeProposalSummary {
            last_run_at,
            total,
            succeeded,
            failed,
        }))
    }

    async fn find_smoke_older_than(
        &self,
        cutoff: DateTime<Utc>,
    ) -> Result<Vec<SmokeProposalRow>, AiProposalRepositoryError> {
        let rows: Vec<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
            r#"
            SELECT proposal_id, object_id,
                   metadata->>'job_id' as job_id,
                   metadata->>'run_id' as run_id
            FROM ai_action_proposals
            WHERE metadata->>'smoke' = 'true'
              AND created_at < $1
            "#,
        )
        .bind(cutoff)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        Ok(rows
            .into_iter()
            .map(|(proposal_id, object_id, job_id, run_id)| SmokeProposalRow {
                proposal_id,
                object_id,
                job_id,
                run_id,
            })
            .collect())
    }

    async fn delete_smoke_older_than(
        &self,
        proposal_ids: &[String],
        cutoff: DateTime<Utc>,
    ) -> Result<u64, AiProposalRepositoryError> {
        let result = sqlx::query(
            "DELETE FROM ai_action_proposals WHERE proposal_id = ANY($1) AND metadata->>'smoke' = 'true' AND created_at < $2",
        )
        .bind(proposal_ids)
        .bind(cutoff)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(result.rows_affected())
    }
}

impl PgAiProposalRepository {
    async fn group_counts<F>(
        &self,
        column: &str,
        label_for_code: F,
    ) -> Result<serde_json::Map<String, serde_json::Value>, AiProposalRepositoryError>
    where
        F: Fn(i32) -> Option<String>,
    {
        let sql = format!(
            "SELECT {column}::integer AS code, COUNT(*)::bigint AS count FROM ai_action_proposals \
             WHERE deleted_at IS NULL GROUP BY {column}"
        );
        let rows = sqlx::query(&sql).fetch_all(&self.pool).await.map_err(db_err)?;

        let mut result = serde_json::Map::new();
        for row in rows {
            let code: i32 = row.try_get("code").map_err(db_err)?;
            let count: i64 = row.try_get("count").map_err(db_err)?;
            if let Some(label) = label_for_code(code) {
                result.insert(label, serde_json::json!(count));
            }
        }
        Ok(result)
    }

    async fn group_object_counts(
        &self,
    ) -> Result<serde_json::Map<String, serde_json::Value>, AiProposalRepositoryError> {
        let rows = sqlx::query(
            "SELECT object_type, COUNT(*)::bigint AS count FROM ai_action_proposals \
             WHERE deleted_at IS NULL GROUP BY object_type",
        )
                .fetch_all(&self.pool)
                .await
                .map_err(db_err)?;

        let mut result = serde_json::Map::new();
        for row in rows {
            let object_type: String = row.try_get("object_type").map_err(db_err)?;
            let count: i64 = row.try_get("count").map_err(db_err)?;
            result.insert(object_type, serde_json::json!(count));
        }
        Ok(result)
    }
}

fn db_err(error: sqlx::Error) -> AiProposalRepositoryError {
    AiProposalRepositoryError::database(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::PgAiProposalRepository;
    use chrono::{Duration, Utc};
    use fms_domain::models::ai_proposal::{
        ActionProposalQuery, ActionProposalStatus, AiActionProposal, ApprovalPolicy, ConstraintResult, RiskLevel,
    };
    use fms_domain::ports::ai_proposal_repository::AiProposalRepository;
    use serde_json::json;
    use sqlx::postgres::PgPoolOptions;
    use ulid::Ulid;

    async fn repository_from_test_database() -> PgAiProposalRepository {
        let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("connect TEST_DATABASE_URL");
        sqlx::raw_sql(include_str!(
            "../../../../../../migrations/092_create_ai_action_proposals.sql"
        ))
        .execute(&pool)
        .await
        .expect("apply ai_action_proposals migration");
        sqlx::query("ALTER TABLE ai_action_proposals ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ")
            .execute(&pool)
            .await
            .expect("add soft delete column");
        PgAiProposalRepository::new(pool)
    }

    fn proposal(suffix: &str, status: ActionProposalStatus) -> AiActionProposal {
        let mut proposal = AiActionProposal::new(
            format!("repo-proposal-{suffix}"),
            format!("repo-job-{suffix}"),
            format!("repo-run-{suffix}"),
            "Flight",
            format!("repo-flight-{suffix}"),
            "change_stand",
            json!({
                "new_stand": "S02",
                "reason": "repository lifecycle regression",
                "idempotency_key": format!("repo-idem-{suffix}")
            }),
        )
        .with_ontology_version("flight-ops.v1")
        .with_risk_level(RiskLevel::Medium)
        .with_approval_policy(ApprovalPolicy::RequireApproval)
        .with_required_permissions(vec!["flight:write".to_string()])
        .with_before_snapshot(json!({"flight_id": format!("repo-flight-{suffix}"), "stand": "S01"}))
        .with_after_preview(json!({"flight_id": format!("repo-flight-{suffix}"), "stand": "S02"}))
        .with_constraint_results(vec![ConstraintResult::new("stand_available", "business", true)])
        .with_confidence(0.82)
        .with_reasoning("DB repository should round-trip canonical proposal fields")
        .with_correlation_id(format!("repo-correlation-{suffix}"))
        .with_metadata(json!({"test_suffix": suffix}));

        proposal.status = status;
        proposal.expires_at = Some(Utc::now() + Duration::minutes(10));
        proposal
    }

    async fn cleanup(repo: &PgAiProposalRepository, ids: &[String]) {
        for id in ids {
            let _ = repo.delete(id).await;
        }
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL and migrated ai_action_proposals table"]
    async fn pg_ai_proposal_repository_round_trips_and_filters_proposals() {
        let repo = repository_from_test_database().await;
        let suffix = Ulid::new().to_string();
        let proposal = proposal(&suffix, ActionProposalStatus::Pending);
        let ids = vec![proposal.proposal_id.clone()];
        cleanup(&repo, &ids).await;

        repo.save(&proposal).await.expect("save proposal");

        let loaded = repo
            .find_by_id(&proposal.proposal_id)
            .await
            .expect("find by id")
            .expect("proposal exists");
        assert_eq!(loaded.proposal_id, proposal.proposal_id);
        assert_eq!(loaded.job_id, proposal.job_id);
        assert_eq!(loaded.run_id, proposal.run_id);
        assert_eq!(loaded.ontology_version, "flight-ops.v1");
        assert_eq!(loaded.object_type, "Flight");
        assert_eq!(loaded.action_name, "change_stand");
        assert_eq!(loaded.risk_level, RiskLevel::Medium);
        assert_eq!(loaded.required_permissions, vec!["flight:write"]);
        assert_eq!(loaded.approval_policy, ApprovalPolicy::RequireApproval);
        assert_eq!(loaded.status, ActionProposalStatus::Pending);
        assert_eq!(loaded.arguments["new_stand"], "S02");
        assert_eq!(loaded.before_snapshot.as_ref().unwrap()["stand"], "S01");
        assert_eq!(loaded.after_preview.as_ref().unwrap()["stand"], "S02");
        assert_eq!(loaded.constraint_results.len(), 1);
        assert_eq!(loaded.constraint_results[0].constraint_name, "stand_available");
        assert_eq!(loaded.correlation_id, proposal.correlation_id);
        assert_eq!(loaded.metadata["test_suffix"], suffix);

        assert_eq!(
            repo.find_by_job_id(&proposal.job_id).await.expect("find by job").len(),
            1
        );
        assert_eq!(
            repo.find_by_run_id(&proposal.run_id).await.expect("find by run").len(),
            1
        );
        assert_eq!(
            repo.find_by_object(&proposal.object_type, &proposal.object_id)
                .await
                .expect("find by object")
                .len(),
            1
        );

        let filtered = repo
            .search(&ActionProposalQuery {
                job_id: Some(proposal.job_id.clone()),
                status: Some(ActionProposalStatus::Pending),
                risk_level: Some(RiskLevel::Medium),
                approval_policy: Some(ApprovalPolicy::RequireApproval),
                limit: Some(10),
                ..Default::default()
            })
            .await
            .expect("search");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].proposal_id, proposal.proposal_id);

        let count = repo
            .count(&ActionProposalQuery {
                job_id: Some(proposal.job_id.clone()),
                ..Default::default()
            })
            .await
            .expect("count");
        assert_eq!(count, 1);

        cleanup(&repo, &ids).await;
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL and migrated ai_action_proposals table"]
    async fn pg_ai_proposal_repository_updates_links_expiry_and_stats() {
        let repo = repository_from_test_database().await;
        let suffix = Ulid::new().to_string();
        let mut active = proposal(&format!("{suffix}-active"), ActionProposalStatus::Pending);
        let mut expired = proposal(&format!("{suffix}-expired"), ActionProposalStatus::Pending);
        expired.expires_at = Some(Utc::now() - Duration::minutes(1));
        active.confidence = 0.9;
        expired.confidence = 0.4;

        let ids = vec![active.proposal_id.clone(), expired.proposal_id.clone()];
        cleanup(&repo, &ids).await;
        repo.save(&active).await.expect("save active proposal");
        repo.save(&expired).await.expect("save expired proposal");

        repo.update_status(&active.proposal_id, ActionProposalStatus::Approved)
            .await
            .expect("approve via status update");
        repo.link_pending_action(&active.proposal_id, &format!("pending-{suffix}"))
            .await
            .expect("link pending action");

        let linked = repo
            .find_by_pending_action_id(&format!("pending-{suffix}"))
            .await
            .expect("find pending action link")
            .expect("linked proposal exists");
        assert_eq!(linked.proposal_id, active.proposal_id);
        assert_eq!(linked.status, ActionProposalStatus::Approved);

        repo.unlink_pending_action(&format!("pending-{suffix}"))
            .await
            .expect("unlink pending action");
        assert!(repo
            .find_by_pending_action_id(&format!("pending-{suffix}"))
            .await
            .expect("find unlinked pending action")
            .is_none());

        let expired_ids: Vec<String> = repo
            .find_expired()
            .await
            .expect("find expired")
            .into_iter()
            .map(|proposal| proposal.proposal_id)
            .collect();
        assert!(expired_ids.contains(&expired.proposal_id));

        let stats = repo.get_stats().await.expect("stats");
        assert!(stats.total >= 2);
        assert!(stats.by_status["approved"].as_i64().unwrap_or_default() >= 1);
        assert!(stats.by_status["pending"].as_i64().unwrap_or_default() >= 1);
        assert!(stats.by_object_type["Flight"].as_i64().unwrap_or_default() >= 2);
        assert!((0.0..=1.0).contains(&stats.avg_confidence));

        cleanup(&repo, &ids).await;
    }
}
