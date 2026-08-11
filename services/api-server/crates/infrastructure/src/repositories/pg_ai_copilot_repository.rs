//! PostgreSQL AI Copilot draft-batch repository.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

use fms_domain::error::DomainError;
use fms_domain::models::ai_copilot::{
    AiCopilotBatchStatus, AiCopilotBatchStatusMetrics, AiCopilotBusinessCaseBatch, AiCopilotOperationalError,
    AiCopilotOperationalMetrics, AiCopilotWorkflowDispatchMetrics,
};
use fms_domain::ports::ai_copilot_repository::{AiCopilotBusinessCaseBatchRepository, BeginCommitResult};

pub struct PgAiCopilotBusinessCaseBatchRepository {
    pool: PgPool,
}

impl PgAiCopilotBusinessCaseBatchRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AiCopilotBusinessCaseBatchRepository for PgAiCopilotBusinessCaseBatchRepository {
    async fn save(&self, batch: &AiCopilotBusinessCaseBatch) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO ai_copilot_business_case_batches (
                batch_id, entity_id, source_page, transcript_summary, transcript_text,
                draft_actions, status, created_by, committed_case_ids, idempotency_key,
                notification_groups, commit_request, created_action_case_ids,
                commit_error, commit_started_at, commit_attempts, commit_next_recovery_at,
                committed_at,
                workflow_dispatch_status, workflow_dispatch_request, workflow_dispatch_error,
                workflow_dispatch_attempts, workflow_dispatch_next_retry_at, workflow_dispatched_at,
                created_at, updated_at, expires_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27)
            ON CONFLICT (batch_id) DO UPDATE SET
                entity_id = EXCLUDED.entity_id,
                source_page = EXCLUDED.source_page,
                transcript_summary = EXCLUDED.transcript_summary,
                transcript_text = EXCLUDED.transcript_text,
                draft_actions = EXCLUDED.draft_actions,
                status = EXCLUDED.status,
                committed_case_ids = EXCLUDED.committed_case_ids,
                idempotency_key = EXCLUDED.idempotency_key,
                notification_groups = EXCLUDED.notification_groups,
                commit_request = EXCLUDED.commit_request,
                created_action_case_ids = EXCLUDED.created_action_case_ids,
                commit_error = EXCLUDED.commit_error,
                commit_started_at = EXCLUDED.commit_started_at,
                commit_attempts = EXCLUDED.commit_attempts,
                commit_next_recovery_at = EXCLUDED.commit_next_recovery_at,
                committed_at = EXCLUDED.committed_at,
                workflow_dispatch_status = EXCLUDED.workflow_dispatch_status,
                workflow_dispatch_request = EXCLUDED.workflow_dispatch_request,
                workflow_dispatch_error = EXCLUDED.workflow_dispatch_error,
                workflow_dispatch_attempts = EXCLUDED.workflow_dispatch_attempts,
                workflow_dispatch_next_retry_at = EXCLUDED.workflow_dispatch_next_retry_at,
                workflow_dispatched_at = EXCLUDED.workflow_dispatched_at,
                updated_at = EXCLUDED.updated_at,
                expires_at = EXCLUDED.expires_at
            "#,
        )
        .bind(&batch.batch_id)
        .bind(&batch.entity_id)
        .bind(&batch.source_page)
        .bind(&batch.transcript_summary)
        .bind(&batch.transcript_text)
        .bind(&batch.draft_actions)
        .bind(batch.status.as_str())
        .bind(&batch.created_by)
        .bind(&batch.committed_case_ids)
        .bind(&batch.idempotency_key)
        .bind(&batch.notification_groups)
        .bind(&batch.commit_request)
        .bind(&batch.created_action_case_ids)
        .bind(&batch.commit_error)
        .bind(batch.commit_started_at)
        .bind(batch.commit_attempts)
        .bind(batch.commit_next_recovery_at)
        .bind(batch.committed_at)
        .bind(&batch.workflow_dispatch_status)
        .bind(&batch.workflow_dispatch_request)
        .bind(&batch.workflow_dispatch_error)
        .bind(batch.workflow_dispatch_attempts)
        .bind(batch.workflow_dispatch_next_retry_at)
        .bind(batch.workflow_dispatched_at)
        .bind(batch.created_at)
        .bind(batch.updated_at)
        .bind(batch.expires_at)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, batch_id: &str) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT * FROM ai_copilot_business_case_batches
            WHERE batch_id = $1
            "#,
        )
        .bind(batch_id.trim())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        row.as_ref().map(row_to_batch).transpose()
    }

    async fn list(
        &self,
        status: Option<AiCopilotBatchStatus>,
        workflow_dispatch_status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AiCopilotBusinessCaseBatch>, DomainError> {
        let limit = limit.clamp(1, 200);
        let offset = offset.max(0);
        let status = status.map(|value| value.as_str().to_string());
        let workflow_dispatch_status = workflow_dispatch_status
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let rows = sqlx::query(
            r#"
            SELECT * FROM ai_copilot_business_case_batches
            WHERE ($1::text IS NULL OR status = $1)
              AND ($2::text IS NULL OR workflow_dispatch_status = $2)
            ORDER BY updated_at DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(status.as_deref())
        .bind(workflow_dispatch_status.as_deref())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        rows_to_batches(&rows)
    }

    async fn list_due_workflow_dispatch_retries(
        &self,
        limit: i64,
        max_attempts: i32,
    ) -> Result<Vec<AiCopilotBusinessCaseBatch>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT * FROM ai_copilot_business_case_batches
            WHERE status = 'committed'
              AND workflow_dispatch_status = 'failed'
              AND workflow_dispatch_request IS NOT NULL
              AND workflow_dispatch_attempts < $2
              AND (
                    workflow_dispatch_next_retry_at IS NULL
                    OR workflow_dispatch_next_retry_at <= NOW()
                  )
            ORDER BY COALESCE(workflow_dispatch_next_retry_at, updated_at) ASC, updated_at ASC
            LIMIT $1
            "#,
        )
        .bind(limit.clamp(1, 200))
        .bind(max_attempts.max(1))
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        rows_to_batches(&rows)
    }

    async fn recover_stale_workflow_dispatch_pending(
        &self,
        stale_before: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<AiCopilotBusinessCaseBatch>, DomainError> {
        let rows = sqlx::query(
            r#"
            WITH stale AS (
                SELECT batch_id, updated_at
                FROM ai_copilot_business_case_batches
                WHERE status = 'committed'
                  AND workflow_dispatch_status = 'pending'
                  AND workflow_dispatch_request IS NOT NULL
                  AND updated_at <= $1
                ORDER BY updated_at ASC
                LIMIT $2
                FOR UPDATE SKIP LOCKED
            )
            UPDATE ai_copilot_business_case_batches AS batch
            SET workflow_dispatch_status = 'failed',
                workflow_dispatch_error = jsonb_build_object(
                    'stage', 'workflow_dispatch_stale_pending',
                    'message', 'workflow dispatch remained pending past stale threshold',
                    'pending_updated_at', stale.updated_at,
                    'stale_before', $1,
                    'recorded_at', NOW()
                ),
                workflow_dispatch_next_retry_at = NULL,
                updated_at = NOW()
            FROM stale
            WHERE batch.batch_id = stale.batch_id
            RETURNING batch.*
            "#,
        )
        .bind(stale_before)
        .bind(limit.clamp(1, 200))
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        rows_to_batches(&rows)
    }

    async fn operational_metrics(
        &self,
        max_workflow_dispatch_attempts: i32,
        recent_error_limit: i64,
    ) -> Result<AiCopilotOperationalMetrics, DomainError> {
        let max_attempts = max_workflow_dispatch_attempts.max(1);
        let row = sqlx::query(
            r#"
            SELECT
                COUNT(*)::BIGINT AS total,
                COUNT(*) FILTER (WHERE status = 'draft')::BIGINT AS draft,
                COUNT(*) FILTER (WHERE status = 'committing')::BIGINT AS committing,
                COUNT(*) FILTER (WHERE status = 'committed')::BIGINT AS committed,
                COUNT(*) FILTER (WHERE status = 'failed')::BIGINT AS failed,
                COUNT(*) FILTER (WHERE status = 'failed_resolved')::BIGINT AS failed_resolved,
                COUNT(*) FILTER (WHERE status = 'expired')::BIGINT AS expired,
                COUNT(*) FILTER (WHERE workflow_dispatch_status = 'not_required')::BIGINT AS workflow_not_required,
                COUNT(*) FILTER (WHERE workflow_dispatch_status = 'pending')::BIGINT AS workflow_pending,
                COUNT(*) FILTER (WHERE workflow_dispatch_status = 'succeeded')::BIGINT AS workflow_succeeded,
                COUNT(*) FILTER (WHERE workflow_dispatch_status = 'failed')::BIGINT AS workflow_failed,
                COUNT(*) FILTER (
                    WHERE status = 'committed'
                      AND workflow_dispatch_status = 'failed'
                      AND workflow_dispatch_request IS NOT NULL
                      AND workflow_dispatch_attempts < $1
                      AND (
                            workflow_dispatch_next_retry_at IS NULL
                            OR workflow_dispatch_next_retry_at <= NOW()
                          )
                )::BIGINT AS workflow_retry_due,
                COUNT(*) FILTER (
                    WHERE status = 'committed'
                      AND workflow_dispatch_status = 'failed'
                      AND workflow_dispatch_attempts >= $1
                )::BIGINT AS workflow_retry_exhausted
            FROM ai_copilot_business_case_batches
            "#,
        )
        .bind(max_attempts)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        let error_rows = sqlx::query(
            r#"
            SELECT batch_id, status, workflow_dispatch_status, commit_error,
                   workflow_dispatch_error, workflow_dispatch_attempts,
                   workflow_dispatch_next_retry_at, updated_at
            FROM ai_copilot_business_case_batches
            WHERE status = 'failed'
               OR workflow_dispatch_status = 'failed'
            ORDER BY updated_at DESC
            LIMIT $1
            "#,
        )
        .bind(recent_error_limit.clamp(1, 50))
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        Ok(AiCopilotOperationalMetrics {
            generated_at: chrono::Utc::now(),
            batch_status: AiCopilotBatchStatusMetrics {
                total: row.try_get("total").unwrap_or(0),
                draft: row.try_get("draft").unwrap_or(0),
                committing: row.try_get("committing").unwrap_or(0),
                committed: row.try_get("committed").unwrap_or(0),
                failed: row.try_get("failed").unwrap_or(0),
                failed_resolved: row.try_get("failed_resolved").unwrap_or(0),
                expired: row.try_get("expired").unwrap_or(0),
            },
            workflow_dispatch: AiCopilotWorkflowDispatchMetrics {
                not_required: row.try_get("workflow_not_required").unwrap_or(0),
                pending: row.try_get("workflow_pending").unwrap_or(0),
                succeeded: row.try_get("workflow_succeeded").unwrap_or(0),
                failed: row.try_get("workflow_failed").unwrap_or(0),
                retry_due: row.try_get("workflow_retry_due").unwrap_or(0),
                retry_exhausted: row.try_get("workflow_retry_exhausted").unwrap_or(0),
                max_attempts,
            },
            recent_errors: error_rows.iter().map(row_to_operational_error).collect(),
        })
    }

    async fn try_begin_commit(&self, batch_id: &str) -> Result<BeginCommitResult, DomainError> {
        // Atomically transition draft -> committing.
        // Only one concurrent request can succeed because WHERE status='draft' acts as a guard.
        let updated = sqlx::query(
            r#"
            UPDATE ai_copilot_business_case_batches
            SET status = 'committing',
                commit_started_at = NOW(),
                commit_attempts = commit_attempts + 1,
                commit_next_recovery_at = NULL,
                updated_at = NOW()
            WHERE batch_id = $1 AND status = 'draft'
            RETURNING *
            "#,
        )
        .bind(batch_id.trim())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        if let Some(row) = updated {
            return Ok(BeginCommitResult::Acquired(row_to_batch(&row)?));
        }

        // Lock not acquired; read current state to determine why.
        let current = self.find_by_id(batch_id).await?;
        match current {
            None => Ok(BeginCommitResult::NotFound),
            Some(batch) => match batch.status {
                AiCopilotBatchStatus::Committed => Ok(BeginCommitResult::AlreadyCommitted(batch)),
                AiCopilotBatchStatus::Committing => Ok(BeginCommitResult::Conflict(batch)),
                AiCopilotBatchStatus::Failed => Ok(BeginCommitResult::Conflict(batch)),
                AiCopilotBatchStatus::FailedResolved => Ok(BeginCommitResult::Conflict(batch)),
                AiCopilotBatchStatus::Expired => Ok(BeginCommitResult::Conflict(batch)),
                // Should not happen (we just failed to acquire), but handle defensively.
                AiCopilotBatchStatus::Draft => Ok(BeginCommitResult::Conflict(batch)),
            },
        }
    }

    async fn try_begin_commit_with_request(
        &self,
        batch_id: &str,
        commit_request: &serde_json::Value,
        next_recovery_at: Option<DateTime<Utc>>,
    ) -> Result<BeginCommitResult, DomainError> {
        let updated = sqlx::query(
            r#"
            UPDATE ai_copilot_business_case_batches
            SET status = 'committing',
                commit_request = $2,
                created_action_case_ids = '{}'::jsonb,
                commit_error = NULL,
                commit_started_at = NOW(),
                commit_attempts = commit_attempts + 1,
                commit_next_recovery_at = $3,
                updated_at = NOW()
            WHERE batch_id = $1 AND status = 'draft'
            RETURNING *
            "#,
        )
        .bind(batch_id.trim())
        .bind(commit_request)
        .bind(next_recovery_at)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        if let Some(row) = updated {
            return Ok(BeginCommitResult::Acquired(row_to_batch(&row)?));
        }

        let current = self.find_by_id(batch_id).await?;
        match current {
            None => Ok(BeginCommitResult::NotFound),
            Some(batch) => match batch.status {
                AiCopilotBatchStatus::Committed => Ok(BeginCommitResult::AlreadyCommitted(batch)),
                AiCopilotBatchStatus::Committing => Ok(BeginCommitResult::Conflict(batch)),
                AiCopilotBatchStatus::Failed => Ok(BeginCommitResult::Conflict(batch)),
                AiCopilotBatchStatus::FailedResolved => Ok(BeginCommitResult::Conflict(batch)),
                AiCopilotBatchStatus::Expired => Ok(BeginCommitResult::Conflict(batch)),
                AiCopilotBatchStatus::Draft => Ok(BeginCommitResult::Conflict(batch)),
            },
        }
    }

    async fn record_created_action_case(
        &self,
        batch_id: &str,
        action_id: &str,
        case_id: &str,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError> {
        let action_id = action_id.trim();
        let case_id = case_id.trim();
        if action_id.is_empty() || case_id.is_empty() {
            return Err(DomainError::ValidationError(
                "action_id and case_id are required".into(),
            ));
        }

        let row = sqlx::query(
            r#"
            UPDATE ai_copilot_business_case_batches
            SET created_action_case_ids = jsonb_set(
                    created_action_case_ids,
                    ARRAY[$2],
                    to_jsonb($3::text),
                    true
                ),
                updated_at = NOW()
            WHERE batch_id = $1 AND status = 'committing'
              AND jsonb_typeof(created_action_case_ids) = 'object'
              AND (
                    NOT created_action_case_ids ? $2
                    OR created_action_case_ids->>$2 = $3
                  )
            RETURNING *
            "#,
        )
        .bind(batch_id.trim())
        .bind(action_id)
        .bind(case_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        row.as_ref().map(row_to_batch).transpose()
    }

    async fn recover_stale_committing(
        &self,
        stale_before: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<AiCopilotBusinessCaseBatch>, DomainError> {
        let rows = sqlx::query(
            r#"
            WITH stale AS (
                SELECT batch_id
                FROM ai_copilot_business_case_batches
                WHERE status = 'committing'
                  AND commit_started_at IS NOT NULL
                  AND commit_started_at <= $1
                  AND (
                        commit_next_recovery_at IS NULL
                        OR commit_next_recovery_at <= NOW()
                      )
                ORDER BY COALESCE(commit_next_recovery_at, commit_started_at) ASC,
                         commit_started_at ASC
                LIMIT $2
                FOR UPDATE SKIP LOCKED
            )
            UPDATE ai_copilot_business_case_batches AS batch
            SET commit_attempts = commit_attempts + 1,
                commit_next_recovery_at = NOW() + make_interval(secs => LEAST(3600, (60 * POWER(2, LEAST(GREATEST(commit_attempts, 0), 5)))::integer)),
                updated_at = NOW()
            FROM stale
            WHERE batch.batch_id = stale.batch_id
            RETURNING batch.*
            "#,
        )
        .bind(stale_before)
        .bind(limit.clamp(1, 200))
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        rows_to_batches(&rows)
    }

    async fn mark_committed(
        &self,
        batch_id: &str,
        case_ids: &[String],
        notification_groups: &serde_json::Value,
        idempotency_key: Option<&str>,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError> {
        let row = sqlx::query(
            r#"
            UPDATE ai_copilot_business_case_batches
            SET status = 'committed',
                committed_case_ids = $2,
                notification_groups = $3,
                commit_error = NULL,
                commit_next_recovery_at = NULL,
                committed_at = NOW(),
                idempotency_key = $4,
                updated_at = NOW()
            WHERE batch_id = $1 AND status = 'committing'
            RETURNING *
            "#,
        )
        .bind(batch_id.trim())
        .bind(case_ids)
        .bind(notification_groups)
        .bind(idempotency_key.map(str::trim).filter(|value| !value.is_empty()))
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        row.as_ref().map(row_to_batch).transpose()
    }

    async fn mark_committed_with_workflow_dispatch_request(
        &self,
        batch_id: &str,
        case_ids: &[String],
        notification_groups: &serde_json::Value,
        idempotency_key: Option<&str>,
        workflow_dispatch_request: &serde_json::Value,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError> {
        let row = sqlx::query(
            r#"
            UPDATE ai_copilot_business_case_batches
            SET status = 'committed',
                committed_case_ids = $2,
                notification_groups = $3,
                commit_error = NULL,
                commit_next_recovery_at = NULL,
                committed_at = NOW(),
                idempotency_key = $4,
                workflow_dispatch_status = 'pending',
                workflow_dispatch_request = $5,
                workflow_dispatch_error = NULL,
                workflow_dispatch_next_retry_at = NULL,
                updated_at = NOW()
            WHERE batch_id = $1 AND status = 'committing'
            RETURNING *
            "#,
        )
        .bind(batch_id.trim())
        .bind(case_ids)
        .bind(notification_groups)
        .bind(idempotency_key.map(str::trim).filter(|value| !value.is_empty()))
        .bind(workflow_dispatch_request)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        row.as_ref().map(row_to_batch).transpose()
    }

    async fn mark_commit_failed(
        &self,
        batch_id: &str,
        case_ids: &[String],
        error: &serde_json::Value,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError> {
        let row = sqlx::query(
            r#"
            UPDATE ai_copilot_business_case_batches
            SET status = 'failed',
                committed_case_ids = $2,
                commit_error = $3,
                commit_next_recovery_at = NULL,
                updated_at = NOW()
            WHERE batch_id = $1 AND status = 'committing'
            RETURNING *
            "#,
        )
        .bind(batch_id.trim())
        .bind(case_ids)
        .bind(error)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        row.as_ref().map(row_to_batch).transpose()
    }

    async fn mark_workflow_dispatch_pending(
        &self,
        batch_id: &str,
        request: &serde_json::Value,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError> {
        let row = sqlx::query(
            r#"
            UPDATE ai_copilot_business_case_batches
            SET workflow_dispatch_status = 'pending',
                workflow_dispatch_request = $2,
                workflow_dispatch_error = NULL,
                workflow_dispatch_next_retry_at = NULL,
                updated_at = NOW()
            WHERE batch_id = $1 AND status IN ('committing', 'committed')
            RETURNING *
            "#,
        )
        .bind(batch_id.trim())
        .bind(request)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        row.as_ref().map(row_to_batch).transpose()
    }

    async fn try_begin_workflow_dispatch_retry(
        &self,
        batch_id: &str,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError> {
        let row = sqlx::query(
            r#"
            UPDATE ai_copilot_business_case_batches
            SET workflow_dispatch_status = 'pending',
                workflow_dispatch_error = NULL,
                workflow_dispatch_next_retry_at = NULL,
                updated_at = NOW()
            WHERE batch_id = $1
              AND status = 'committed'
              AND workflow_dispatch_status = 'failed'
              AND workflow_dispatch_request IS NOT NULL
            RETURNING *
            "#,
        )
        .bind(batch_id.trim())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        row.as_ref().map(row_to_batch).transpose()
    }

    async fn mark_workflow_dispatch_failed(
        &self,
        batch_id: &str,
        error: &serde_json::Value,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError> {
        let row = sqlx::query(
            r#"
            UPDATE ai_copilot_business_case_batches
            SET workflow_dispatch_status = 'failed',
                workflow_dispatch_error = $2,
                workflow_dispatch_attempts = workflow_dispatch_attempts + 1,
                workflow_dispatch_next_retry_at = NOW() + make_interval(secs => LEAST(3600, (60 * POWER(2, workflow_dispatch_attempts))::integer)),
                updated_at = NOW()
            WHERE batch_id = $1 AND status IN ('committing', 'committed')
            RETURNING *
            "#,
        )
        .bind(batch_id.trim())
        .bind(error)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        row.as_ref().map(row_to_batch).transpose()
    }

    async fn mark_workflow_dispatch_succeeded(
        &self,
        batch_id: &str,
        notification_groups: &serde_json::Value,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError> {
        let row = sqlx::query(
            r#"
            UPDATE ai_copilot_business_case_batches
            SET workflow_dispatch_status = 'succeeded',
                workflow_dispatch_error = NULL,
                workflow_dispatch_attempts = workflow_dispatch_attempts + 1,
                workflow_dispatch_next_retry_at = NULL,
                workflow_dispatched_at = NOW(),
                notification_groups = $2,
                updated_at = NOW()
            WHERE batch_id = $1 AND status IN ('committing', 'committed')
            RETURNING *
            "#,
        )
        .bind(batch_id.trim())
        .bind(notification_groups)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        row.as_ref().map(row_to_batch).transpose()
    }

    async fn reset_commit_to_draft(&self, batch_id: &str) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError> {
        let row = sqlx::query(
            r#"
            UPDATE ai_copilot_business_case_batches
            SET status = 'draft',
                committed_case_ids = ARRAY[]::TEXT[],
                notification_groups = '[]'::jsonb,
                commit_request = NULL,
                created_action_case_ids = '{}'::jsonb,
                commit_error = NULL,
                commit_started_at = NULL,
                commit_attempts = 0,
                commit_next_recovery_at = NULL,
                committed_at = NULL,
                workflow_dispatch_status = 'not_required',
                workflow_dispatch_request = NULL,
                workflow_dispatch_error = NULL,
                workflow_dispatch_attempts = 0,
                workflow_dispatch_next_retry_at = NULL,
                workflow_dispatched_at = NULL,
                updated_at = NOW()
            WHERE batch_id = $1 AND status = 'committing'
            RETURNING *
            "#,
        )
        .bind(batch_id.trim())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        row.as_ref().map(row_to_batch).transpose()
    }

    async fn reset_failed_to_draft(
        &self,
        batch_id: &str,
        resolution: &serde_json::Value,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError> {
        let row = sqlx::query(
            r#"
            UPDATE ai_copilot_business_case_batches
            SET status = 'draft',
                committed_case_ids = ARRAY[]::TEXT[],
                notification_groups = '[]'::jsonb,
                commit_request = NULL,
                created_action_case_ids = '{}'::jsonb,
                commit_error = $2,
                commit_started_at = NULL,
                commit_attempts = 0,
                commit_next_recovery_at = NULL,
                committed_at = NULL,
                workflow_dispatch_status = 'not_required',
                workflow_dispatch_request = NULL,
                workflow_dispatch_error = NULL,
                workflow_dispatch_attempts = 0,
                workflow_dispatch_next_retry_at = NULL,
                workflow_dispatched_at = NULL,
                updated_at = NOW()
            WHERE batch_id = $1 AND status = 'failed'
            RETURNING *
            "#,
        )
        .bind(batch_id.trim())
        .bind(resolution)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        row.as_ref().map(row_to_batch).transpose()
    }

    async fn mark_failed_resolved(
        &self,
        batch_id: &str,
        resolution: &serde_json::Value,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError> {
        let row = sqlx::query(
            r#"
            UPDATE ai_copilot_business_case_batches
            SET status = 'failed_resolved',
                commit_error = $2,
                updated_at = NOW()
            WHERE batch_id = $1 AND status = 'failed'
            RETURNING *
            "#,
        )
        .bind(batch_id.trim())
        .bind(resolution)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        row.as_ref().map(row_to_batch).transpose()
    }
}

fn rows_to_batches(rows: &[sqlx::postgres::PgRow]) -> Result<Vec<AiCopilotBusinessCaseBatch>, DomainError> {
    rows.iter().map(row_to_batch).collect()
}

fn required_column<T>(row: &sqlx::postgres::PgRow, column: &str) -> Result<T, DomainError>
where
    T: for<'r> sqlx::Decode<'r, sqlx::Postgres> + sqlx::Type<sqlx::Postgres>,
{
    row.try_get(column)
        .map_err(|error| DomainError::Internal(format!("failed to decode required column '{column}': {error}")))
}

fn row_to_batch(row: &sqlx::postgres::PgRow) -> Result<AiCopilotBusinessCaseBatch, DomainError> {
    let status: String = required_column(row, "status")?;
    Ok(AiCopilotBusinessCaseBatch {
        batch_id: required_column(row, "batch_id")?,
        entity_id: required_column(row, "entity_id")?,
        source_page: required_column(row, "source_page")?,
        transcript_summary: required_column(row, "transcript_summary")?,
        transcript_text: required_column(row, "transcript_text")?,
        draft_actions: required_column(row, "draft_actions")?,
        status: AiCopilotBatchStatus::from_str(&status),
        created_by: required_column(row, "created_by")?,
        committed_case_ids: required_column(row, "committed_case_ids")?,
        idempotency_key: row.try_get("idempotency_key").ok(),
        notification_groups: row
            .try_get("notification_groups")
            .unwrap_or_else(|_| serde_json::json!([])),
        commit_request: row.try_get("commit_request").ok(),
        created_action_case_ids: row
            .try_get("created_action_case_ids")
            .unwrap_or_else(|_| serde_json::json!({})),
        commit_error: row.try_get("commit_error").ok(),
        commit_started_at: row.try_get("commit_started_at").ok(),
        commit_attempts: row.try_get("commit_attempts").unwrap_or(0),
        commit_next_recovery_at: row.try_get("commit_next_recovery_at").ok(),
        committed_at: row.try_get("committed_at").ok(),
        workflow_dispatch_status: row
            .try_get("workflow_dispatch_status")
            .unwrap_or_else(|_| "not_required".to_string()),
        workflow_dispatch_request: row.try_get("workflow_dispatch_request").ok(),
        workflow_dispatch_error: row.try_get("workflow_dispatch_error").ok(),
        workflow_dispatch_attempts: row.try_get("workflow_dispatch_attempts").unwrap_or(0),
        workflow_dispatch_next_retry_at: row.try_get("workflow_dispatch_next_retry_at").ok(),
        workflow_dispatched_at: row.try_get("workflow_dispatched_at").ok(),
        created_at: required_column(row, "created_at")?,
        updated_at: required_column(row, "updated_at")?,
        expires_at: required_column(row, "expires_at")?,
    })
}

fn row_to_operational_error(row: &sqlx::postgres::PgRow) -> AiCopilotOperationalError {
    let status: String = row.try_get("status").unwrap_or_else(|_| "draft".to_string());
    let commit_error: Option<serde_json::Value> = row.try_get("commit_error").ok();
    let workflow_error: Option<serde_json::Value> = row.try_get("workflow_dispatch_error").ok();
    let error = workflow_error.as_ref().or(commit_error.as_ref());
    AiCopilotOperationalError {
        batch_id: row.try_get("batch_id").unwrap_or_default(),
        status: AiCopilotBatchStatus::from_str(&status),
        workflow_dispatch_status: row
            .try_get("workflow_dispatch_status")
            .unwrap_or_else(|_| "not_required".to_string()),
        stage: error.and_then(|value| {
            value
                .get("stage")
                .or_else(|| value.get("previous_error").and_then(|prev| prev.get("stage")))
                .and_then(|value| value.as_str())
                .map(str::to_string)
        }),
        message: error.and_then(|value| {
            value
                .get("message")
                .or_else(|| value.get("previous_error").and_then(|prev| prev.get("message")))
                .and_then(|value| value.as_str())
                .map(str::to_string)
        }),
        attempts: row.try_get("workflow_dispatch_attempts").unwrap_or(0),
        next_retry_at: row.try_get("workflow_dispatch_next_retry_at").ok(),
        updated_at: row.try_get("updated_at").unwrap_or_else(|_| chrono::Utc::now()),
    }
}
