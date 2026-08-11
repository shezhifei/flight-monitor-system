//! AI Copilot draft-batch repository port.

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::error::DomainError;
use crate::models::ai_copilot::{AiCopilotBatchStatus, AiCopilotBusinessCaseBatch, AiCopilotOperationalMetrics};

/// Result of an atomic commit-attempt on a batch.
#[derive(Debug, Clone)]
pub enum BeginCommitResult {
    /// Caller acquired the lock; batch is now in `committing` state.
    Acquired(AiCopilotBusinessCaseBatch),
    /// Another request already committed this batch.
    AlreadyCommitted(AiCopilotBusinessCaseBatch),
    /// Another request is currently committing; caller should retry later or return 409.
    Conflict(AiCopilotBusinessCaseBatch),
    /// Batch not found.
    NotFound,
}

#[async_trait]
pub trait AiCopilotBusinessCaseBatchRepository {
    async fn save(&self, batch: &AiCopilotBusinessCaseBatch) -> Result<(), DomainError>;

    async fn find_by_id(&self, batch_id: &str) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError>;

    async fn list(
        &self,
        status: Option<AiCopilotBatchStatus>,
        workflow_dispatch_status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AiCopilotBusinessCaseBatch>, DomainError>;

    async fn list_due_workflow_dispatch_retries(
        &self,
        limit: i64,
        max_attempts: i32,
    ) -> Result<Vec<AiCopilotBusinessCaseBatch>, DomainError>;

    /// Recover committed workflow dispatches that have been stuck in `pending`
    /// longer than the caller's stale threshold.
    ///
    /// Recovered batches are marked `failed` with a structured stale-pending
    /// error, keep their request snapshot, keep their attempt count, and have
    /// `workflow_dispatch_next_retry_at = NULL` so the normal failed retry
    /// acquisition path can pick them up.
    async fn recover_stale_workflow_dispatch_pending(
        &self,
        stale_before: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<AiCopilotBusinessCaseBatch>, DomainError>;

    async fn operational_metrics(
        &self,
        max_workflow_dispatch_attempts: i32,
        recent_error_limit: i64,
    ) -> Result<AiCopilotOperationalMetrics, DomainError>;

    /// Atomically transition batch from `draft` -> `committing`.
    /// Uses `UPDATE ... WHERE status = 'draft'` to prevent concurrent commits.
    async fn try_begin_commit(&self, batch_id: &str) -> Result<BeginCommitResult, DomainError>;

    /// Atomically transition batch from `draft` -> `committing` while persisting
    /// the canonical commit request snapshot needed for durable recovery.
    async fn try_begin_commit_with_request(
        &self,
        batch_id: &str,
        commit_request: &serde_json::Value,
        next_recovery_at: Option<DateTime<Utc>>,
    ) -> Result<BeginCommitResult, DomainError>;

    /// Record the business case created for an approved action during commit.
    async fn record_created_action_case(
        &self,
        batch_id: &str,
        action_id: &str,
        case_id: &str,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError>;

    /// Atomically claim stale `committing` batches for commit-saga recovery.
    ///
    /// The implementation must use row locking (for example
    /// `FOR UPDATE SKIP LOCKED`) and must not mark the batch `committed`.
    /// Legacy rows without `commit_request` should still be returned so the
    /// application service can fail them conservatively instead of creating
    /// business cases without a durable request snapshot.
    async fn recover_stale_committing(
        &self,
        stale_before: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<AiCopilotBusinessCaseBatch>, DomainError>;

    async fn mark_committed(
        &self,
        batch_id: &str,
        case_ids: &[String],
        notification_groups: &serde_json::Value,
        idempotency_key: Option<&str>,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError>;

    /// Atomically mark the commit saga as committed and enqueue workflow dispatch.
    ///
    /// Implementations must persist `status = committed`,
    /// `workflow_dispatch_status = pending`, and the dispatch request in the
    /// same write so recovery never observes a committed batch without a
    /// pending dispatch request.
    async fn mark_committed_with_workflow_dispatch_request(
        &self,
        batch_id: &str,
        case_ids: &[String],
        notification_groups: &serde_json::Value,
        idempotency_key: Option<&str>,
        workflow_dispatch_request: &serde_json::Value,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError>;

    async fn mark_commit_failed(
        &self,
        batch_id: &str,
        case_ids: &[String],
        error: &serde_json::Value,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError>;

    async fn mark_workflow_dispatch_pending(
        &self,
        batch_id: &str,
        request: &serde_json::Value,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError>;

    /// Atomically acquire a committed batch whose workflow dispatch previously failed.
    /// Returns `None` when another retry already acquired it or the state changed.
    async fn try_begin_workflow_dispatch_retry(
        &self,
        batch_id: &str,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError>;

    async fn mark_workflow_dispatch_failed(
        &self,
        batch_id: &str,
        error: &serde_json::Value,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError>;

    async fn mark_workflow_dispatch_succeeded(
        &self,
        batch_id: &str,
        notification_groups: &serde_json::Value,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError>;

    async fn reset_commit_to_draft(&self, batch_id: &str) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError>;

    async fn reset_failed_to_draft(
        &self,
        batch_id: &str,
        resolution: &serde_json::Value,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError>;

    async fn mark_failed_resolved(
        &self,
        batch_id: &str,
        resolution: &serde_json::Value,
    ) -> Result<Option<AiCopilotBusinessCaseBatch>, DomainError>;
}
