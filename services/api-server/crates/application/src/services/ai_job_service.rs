use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use thiserror::Error;

use fms_domain::models::ai_job::{AiJobRecord, AiJobStatus, AiRunEventRecord, AiRunRecord, AiRunStatus};
use fms_domain::ports::ai_job_repository::{AiJobRepository, AiJobRepositoryError};
use fms_domain::ports::ai_run_event_repository::{AiRunEventRepository, AiRunEventRepositoryError};
use fms_domain::ports::ai_run_repository::{AiRunRepository, AiRunRepositoryError};

use crate::services::ai_runtime_service::ai_execution_control_service::{
    AiExecutionControlService, RunInputCheckpointSummary,
};
use crate::sqlx_transactional_repositories::SqlxDomainEventOutboxTransactionalRepository;

#[derive(Debug, Error)]
pub enum AiJobServiceError {
    #[error("{0}")]
    Validation(String),
    #[error("job not found: {0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    /// The run was rejected because an AI run concurrency limit
    /// (per-entity or global) is already saturated. The message carries
    /// the machine-readable code `concurrency_limit_exceeded` plus the
    /// scope, current active count and configured limit.
    #[error("concurrency_limit_exceeded ({scope}): {current} active run(s) >= limit {limit}")]
    ConcurrencyLimitExceeded {
        scope: ConcurrencyLimitScope,
        current: i64,
        limit: i64,
    },
    #[error("{0}")]
    Internal(String),
}

/// Which concurrency limit rejected the run creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConcurrencyLimitScope {
    /// Per-entity limit (`max_concurrent_runs_per_entity`).
    Entity,
    /// Global limit across all entities (`max_concurrent_runs_global`).
    Global,
}

impl std::fmt::Display for ConcurrencyLimitScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Entity => "entity",
            Self::Global => "global",
        })
    }
}

/// Concurrency limits enforced on the run creation path
/// (`AiJobService::create_run`). Defaults follow the hybrid agent
/// architecture plan: 4 active runs per entity x 32 globally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiRunConcurrencyLimits {
    pub max_concurrent_runs_per_entity: i64,
    pub max_concurrent_runs_global: i64,
}

impl Default for AiRunConcurrencyLimits {
    fn default() -> Self {
        Self {
            max_concurrent_runs_per_entity: 4,
            max_concurrent_runs_global: 32,
        }
    }
}

impl From<AiJobRepositoryError> for AiJobServiceError {
    fn from(err: AiJobRepositoryError) -> Self {
        match err {
            AiJobRepositoryError::NotFound(id) => Self::NotFound(id),
            AiJobRepositoryError::Database(msg) | AiJobRepositoryError::Validation(msg) => Self::Internal(msg),
        }
    }
}

impl From<AiRunRepositoryError> for AiJobServiceError {
    fn from(err: AiRunRepositoryError) -> Self {
        match err {
            AiRunRepositoryError::NotFound(id) => Self::NotFound(id),
            AiRunRepositoryError::Database(msg) | AiRunRepositoryError::Validation(msg) => Self::Internal(msg),
        }
    }
}

impl From<AiRunEventRepositoryError> for AiJobServiceError {
    fn from(err: AiRunEventRepositoryError) -> Self {
        match err {
            AiRunEventRepositoryError::NotFound(id) => Self::NotFound(id),
            AiRunEventRepositoryError::Database(msg) | AiRunEventRepositoryError::Validation(msg) => {
                Self::Internal(msg)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiJob {
    pub job_id: String,
    pub job_type: String,
    pub status: String,
    pub requester_user_id: Option<String>,
    pub ontology_version: Option<String>,
    pub context_policy: Option<Value>,
    pub risk_ceiling: Option<String>,
    pub correlation_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub timeout_ms: Option<i64>,
    pub lease_owner: Option<String>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub expires_at: Option<DateTime<Utc>>,
}

impl From<AiJobRecord> for AiJob {
    fn from(r: AiJobRecord) -> Self {
        Self {
            job_id: r.job_id,
            job_type: r.job_type,
            status: r.status,
            requester_user_id: r.requester_user_id,
            ontology_version: r.ontology_version,
            context_policy: r.context_policy,
            risk_ceiling: r.risk_ceiling,
            correlation_id: r.correlation_id,
            created_at: r.created_at,
            started_at: r.started_at,
            finished_at: r.finished_at,
            cancelled_at: r.cancelled_at,
            error_code: r.error_code,
            error_message: r.error_message,
            timeout_ms: r.timeout_ms,
            lease_owner: r.lease_owner,
            lease_expires_at: r.lease_expires_at,
            last_heartbeat_at: r.last_heartbeat_at,
            attempt_count: r.attempt_count,
            max_attempts: r.max_attempts,
            expires_at: r.expires_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRun {
    pub run_id: String,
    pub job_id: String,
    pub runtime_engine: String,
    pub model_id: Option<String>,
    pub status: String,
    pub input_envelope: Option<Value>,
    pub output_raw: Option<Value>,
    pub output_validated: Option<Value>,
    pub token_usage: Option<Value>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl From<AiRunRecord> for AiRun {
    fn from(r: AiRunRecord) -> Self {
        Self {
            run_id: r.run_id,
            job_id: r.job_id,
            runtime_engine: r.runtime_engine,
            model_id: r.model_id,
            status: r.status,
            input_envelope: r.input_envelope,
            output_raw: r.output_raw,
            output_validated: r.output_validated,
            token_usage: r.token_usage,
            started_at: r.started_at,
            finished_at: r.finished_at,
            error_code: r.error_code,
            error_message: r.error_message,
            created_at: r.created_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRunEvent {
    pub event_id: i64,
    pub job_id: String,
    pub run_id: String,
    pub event_type: String,
    pub payload: Option<Value>,
    pub created_at: DateTime<Utc>,
}

impl From<AiRunEventRecord> for AiRunEvent {
    fn from(r: AiRunEventRecord) -> Self {
        Self {
            event_id: r.event_id,
            job_id: r.job_id,
            run_id: r.run_id,
            event_type: r.event_type,
            payload: r.payload,
            created_at: r.created_at,
        }
    }
}

pub struct AiJobService {
    job_repo: Arc<dyn AiJobRepository + Send + Sync>,
    run_repo: Arc<dyn AiRunRepository + Send + Sync>,
    event_repo: Arc<dyn AiRunEventRepository + Send + Sync>,
    control_service: Option<Arc<AiExecutionControlService>>,
    /// Transactional outbox repository. When `Some`, `complete_run` /
    /// `fail_run` / `cancel_job` / `timeout_job` write a domain event
    /// into `domain_event_outbox` so the CDC relay can fan-out the
    /// SSE event on the `ai_execution` topic.
    outbox_repo: Option<Arc<dyn SqlxDomainEventOutboxTransactionalRepository>>,
    /// Connection pool used to begin the outbox write transaction.
    /// Required when `outbox_repo` is `Some`.
    pool: Option<sqlx::PgPool>,
    /// Concurrency limits enforced in `create_run`.
    concurrency_limits: AiRunConcurrencyLimits,
}

impl AiJobService {
    pub fn new(
        job_repo: Arc<dyn AiJobRepository + Send + Sync>,
        run_repo: Arc<dyn AiRunRepository + Send + Sync>,
        event_repo: Arc<dyn AiRunEventRepository + Send + Sync>,
    ) -> Self {
        Self {
            job_repo,
            run_repo,
            event_repo,
            control_service: None,
            outbox_repo: None,
            pool: None,
            concurrency_limits: AiRunConcurrencyLimits::default(),
        }
    }

    /// Override the concurrency limits enforced in `create_run`.
    pub fn with_concurrency_limits(mut self, limits: AiRunConcurrencyLimits) -> Self {
        self.concurrency_limits = limits;
        self
    }

    pub fn with_control_service(mut self, control_service: Arc<AiExecutionControlService>) -> Self {
        self.control_service = Some(control_service);
        self
    }

    /// Wire the transactional outbox repository + pool so that terminal
    /// transitions (`complete_run`, `fail_run`, `cancel_job`,
    /// `timeout_job`) emit a `ai_job.*` domain event into the outbox.
    /// The CDC relay picks it up and pushes it to the SSE hub on the
    /// `ai_execution` topic.
    pub fn with_outbox_repository(
        mut self,
        outbox_repo: Arc<dyn SqlxDomainEventOutboxTransactionalRepository>,
        pool: sqlx::PgPool,
    ) -> Self {
        self.outbox_repo = Some(outbox_repo);
        self.pool = Some(pool);
        self
    }

    pub async fn create_job(
        &self,
        job_type: &str,
        requester_user_id: Option<&str>,
        correlation_id: Option<&str>,
        ontology_version: Option<&str>,
        risk_ceiling: Option<&str>,
    ) -> Result<AiJob, AiJobServiceError> {
        let job_id = format!("job_{}", uuid::Uuid::new_v4());
        let record = self
            .job_repo
            .insert(
                &job_id,
                job_type,
                requester_user_id,
                correlation_id,
                ontology_version,
                risk_ceiling,
            )
            .await?;
        Ok(record.into())
    }

    pub async fn get_job(&self, job_id: &str) -> Result<AiJob, AiJobServiceError> {
        self.job_repo
            .find_by_id(job_id)
            .await?
            .map(AiJob::from)
            .ok_or_else(|| AiJobServiceError::NotFound(job_id.to_string()))
    }

    pub async fn list_jobs(
        &self,
        status_filter: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AiJob>, AiJobServiceError> {
        let rows = self.job_repo.list(status_filter, limit, offset).await?;
        Ok(rows.into_iter().map(AiJob::from).collect())
    }

    pub async fn transition_job(&self, job_id: &str, new_status: AiJobStatus) -> Result<AiJob, AiJobServiceError> {
        let job = self.get_job(job_id).await?;
        let current = AiJobStatus::from_str(&job.status)
            .ok_or_else(|| AiJobServiceError::Internal(format!("invalid status in DB: {}", job.status)))?;
        if !current.can_transition_to(&new_status) {
            return Err(AiJobServiceError::Conflict(format!(
                "cannot transition job {} from {} to {}",
                job_id, current, new_status
            )));
        }
        let record = self.job_repo.update_status(job_id, new_status.as_str()).await?;
        Ok(record.into())
    }

    pub async fn cancel_job(&self, job_id: &str, error_message: Option<&str>) -> Result<AiJob, AiJobServiceError> {
        let job = self.transition_job(job_id, AiJobStatus::Cancelled).await?;
        if let Some(msg) = error_message {
            self.job_repo.set_error_message(job_id, msg).await?;
        }
        // Cancel any non-terminal runs for this job.
        let runs = self.run_repo.list_for_job(job_id).await?;
        for run in runs {
            if let Some(status) = AiRunStatus::from_str(&run.status) {
                if !status.is_terminal() {
                    let _ = self
                        .run_repo
                        .update_status(&run.run_id, AiRunStatus::Cancelled.as_str())
                        .await;
                    // D4: also enqueue a `cancel_run` runtime command so the
                    // worker owning the run stops it at the next round
                    // boundary (commands only go through ai_runtime_commands).
                    // The DB state above is already the source of truth, so
                    // enqueue failures are logged but never roll it back.
                    if let Some(control) = &self.control_service {
                        let requester = job.requester_user_id.as_deref().unwrap_or("system");
                        if let Err(error) = control
                            .enqueue_cancel_run(job_id, &run.run_id, requester)
                            .await
                        {
                            tracing::warn!(
                                target: "ai_job_service",
                                job_id = %job_id,
                                run_id = %run.run_id,
                                error = %error,
                                "cancel_job: failed to enqueue cancel_run command (DB state already cancelled)"
                            );
                        }
                    }
                }
            }
        }
        // Best-effort outbox event for SSE fan-out.
        self.emit_job_event(
            job_id,
            "",
            "ai_job.cancelled",
            None,
            error_message.map(|m| ("cancelled".to_string(), m.to_string())),
        )
        .await;
        Ok(job)
    }

    pub async fn create_run(
        &self,
        job_id: &str,
        runtime_engine: &str,
        model_id: Option<&str>,
        input_envelope: Option<Value>,
    ) -> Result<AiRun, AiJobServiceError> {
        self.enforce_concurrency_limits(input_envelope.as_ref()).await?;
        let run_id = format!("run_{}", uuid::Uuid::new_v4());
        let record = self
            .run_repo
            .insert(&run_id, job_id, runtime_engine, model_id, input_envelope)
            .await?;
        Ok(record.into())
    }

    /// Reject run creation when the per-entity or global active-run
    /// ceiling is already saturated. "Active" follows
    /// [`AiRunStatus::is_active`] (pending / claimed / running).
    ///
    /// The entity id is read from the input envelope using the same
    /// convention as `PgAiAuthContextLoader`: `entity_id` or
    /// `context.entity_id`. Runs created without an entity id only
    /// count towards (and are checked against) the global limit.
    async fn enforce_concurrency_limits(&self, input_envelope: Option<&Value>) -> Result<(), AiJobServiceError> {
        if let Some(entity_id) = input_envelope.and_then(extract_envelope_entity_id) {
            let current = self.run_repo.count_active(Some(entity_id)).await?;
            let limit = self.concurrency_limits.max_concurrent_runs_per_entity;
            if current >= limit {
                return Err(AiJobServiceError::ConcurrencyLimitExceeded {
                    scope: ConcurrencyLimitScope::Entity,
                    current,
                    limit,
                });
            }
        }
        let current = self.run_repo.count_active(None).await?;
        let limit = self.concurrency_limits.max_concurrent_runs_global;
        if current >= limit {
            return Err(AiJobServiceError::ConcurrencyLimitExceeded {
                scope: ConcurrencyLimitScope::Global,
                current,
                limit,
            });
        }
        Ok(())
    }

    pub async fn get_run(&self, run_id: &str) -> Result<AiRun, AiJobServiceError> {
        self.run_repo
            .find_by_id(run_id)
            .await?
            .map(AiRun::from)
            .ok_or_else(|| AiJobServiceError::NotFound(run_id.to_string()))
    }

    pub async fn list_runs_for_job(&self, job_id: &str) -> Result<Vec<AiRun>, AiJobServiceError> {
        let rows = self.run_repo.list_for_job(job_id).await?;
        Ok(rows.into_iter().map(AiRun::from).collect())
    }

    pub async fn transition_run(&self, run_id: &str, new_status: AiRunStatus) -> Result<AiRun, AiJobServiceError> {
        let run = self.get_run(run_id).await?;
        let current = AiRunStatus::from_str(&run.status)
            .ok_or_else(|| AiJobServiceError::Internal(format!("invalid run status in DB: {}", run.status)))?;
        if !current.can_transition_to(&new_status) {
            return Err(AiJobServiceError::Conflict(format!(
                "cannot transition run {} from {} to {}",
                run_id, current, new_status
            )));
        }
        let record = self.run_repo.update_status(run_id, new_status.as_str()).await?;
        Ok(record.into())
    }

    pub async fn update_run_input_envelope(
        &self,
        run_id: &str,
        input_envelope: Value,
    ) -> Result<AiRun, AiJobServiceError> {
        self.run_repo
            .update_input_envelope(run_id, input_envelope.clone())
            .await?;
        if let Some(control) = self.control_service.as_ref() {
            let run = self.get_run(run_id).await?;
            let summary = derive_run_input_summary(&run, &input_envelope);
            if let Err(error) = control
                .create_run_input_checkpoint(&run.job_id, &run.run_id, input_envelope, summary)
                .await
            {
                tracing::warn!(
                    target: "ai_job_service",
                    run_id = %run_id,
                    error = %error,
                    "failed to persist run_input checkpoint; run remains recoverable from input_envelope"
                );
            }
        }
        self.get_run(run_id).await
    }

    pub async fn complete_run(
        &self,
        run_id: &str,
        output_raw: Option<Value>,
        output_validated: Option<Value>,
        token_usage: Option<Value>,
    ) -> Result<AiRun, AiJobServiceError> {
        let run = self.get_run(run_id).await?;
        if let Some(status) = AiRunStatus::from_str(&run.status) {
            if status.is_terminal() {
                // Terminal already — but if output/error fields are NULL,
                // this is a retry from a failed complete_run (status was
                // written but output was not). Fill in the missing fields.
                if run.output_raw.is_none() && run.error_code.is_none() && run.error_message.is_none() {
                    self.run_repo
                        .fill_terminal_outputs(run_id, output_raw, output_validated, token_usage)
                        .await?;
                    return self.get_run(run_id).await;
                }
                tracing::debug!(run_id = %run_id, status = %run.status, "complete_run skipped: run already terminal with output");
                return Ok(run);
            }
        }
        let job_id = run.job_id.clone();
        // Single atomic UPDATE: status + output/error + finished_at in one transaction.
        self.run_repo
            .complete(run_id, output_raw.clone(), output_validated.clone(), token_usage)
            .await?;
        // Transition the job to Succeeded (best-effort — the run is the
        // authoritative record; a failed job update is logged but does
        // not roll back the run completion).
        if let Err(error) = self
            .job_repo
            .update_status(&job_id, AiJobStatus::Succeeded.as_str())
            .await
        {
            tracing::warn!(
                target: "ai_job_service",
                run_id = %run_id,
                job_id = %job_id,
                error = %error,
                "failed to transition job to succeeded after run completion"
            );
        }
        // Best-effort outbox event for SSE fan-out (CDC → SSE).
        self.emit_job_event(&job_id, run_id, "ai_job.succeeded", output_validated.clone(), None)
            .await;
        self.get_run(run_id).await
    }

    pub async fn fail_run(
        &self,
        run_id: &str,
        error_code: Option<&str>,
        error_message: Option<&str>,
        output_raw: Option<Value>,
    ) -> Result<AiRun, AiJobServiceError> {
        let run = self.get_run(run_id).await?;
        if let Some(status) = AiRunStatus::from_str(&run.status) {
            if status.is_terminal() {
                // Retry from a failed fail_run: fill in missing error fields.
                if run.error_code.is_none() && run.error_message.is_none() && run.output_raw.is_none() {
                    self.run_repo
                        .fill_terminal_error(run_id, error_code, error_message, output_raw)
                        .await?;
                    return self.get_run(run_id).await;
                }
                tracing::debug!(run_id = %run_id, status = %run.status, "fail_run skipped: run already terminal with error");
                return Ok(run);
            }
        }
        let job_id = run.job_id.clone();
        // Single atomic UPDATE: status + error + finished_at in one transaction.
        self.run_repo
            .fail(run_id, error_code, error_message, output_raw.clone())
            .await?;
        // Transition the job to FailedTerminal (best-effort).
        if let Err(error) = self
            .job_repo
            .update_status(&job_id, AiJobStatus::FailedTerminal.as_str())
            .await
        {
            tracing::warn!(
                target: "ai_job_service",
                run_id = %run_id,
                job_id = %job_id,
                error = %error,
                "failed to transition job to failed_terminal after run failure"
            );
        }
        if let Some(msg) = error_message {
            let _ = self.job_repo.set_error_message(&job_id, msg).await;
        }
        // Best-effort outbox event for SSE fan-out (CDC → SSE).
        self.emit_job_event(
            &job_id,
            run_id,
            "ai_job.failed",
            output_raw.clone(),
            error_code.map(|c| (c.to_string(), error_message.unwrap_or("").to_string())),
        )
        .await;
        self.get_run(run_id).await
    }

    pub async fn append_event(
        &self,
        job_id: &str,
        run_id: &str,
        event_type: &str,
        payload: Option<Value>,
    ) -> Result<AiRunEvent, AiJobServiceError> {
        let mut error_code_str = None;
        let mut duration_ms_val = None;
        if let Some(ref p) = payload {
            if let Some(ec) = p.get("error_code").and_then(|v| v.as_str()) {
                error_code_str = Some(ec.to_string());
            } else if let Some(ec) = p.get("error_message").and_then(|v| v.as_str()) {
                error_code_str = Some(ec.to_string());
            }
            if let Some(d) = p.get("duration_ms").and_then(|v| v.as_u64()) {
                duration_ms_val = Some(d);
            }
        }

        if let Some(ec) = error_code_str {
            if let Some(d) = duration_ms_val {
                tracing::info!(job_id = %job_id, run_id = %run_id, event_name = %event_type, error_code = %ec, duration_ms = d, "ai_run_event");
            } else {
                tracing::info!(job_id = %job_id, run_id = %run_id, event_name = %event_type, error_code = %ec, "ai_run_event");
            }
        } else if let Some(d) = duration_ms_val {
            tracing::info!(job_id = %job_id, run_id = %run_id, event_name = %event_type, duration_ms = d, "ai_run_event");
        } else {
            tracing::info!(job_id = %job_id, run_id = %run_id, event_name = %event_type, "ai_run_event");
        }

        let record = self.event_repo.insert(job_id, run_id, event_type, payload).await?;
        Ok(record.into())
    }

    pub async fn list_events_for_run(&self, run_id: &str, limit: i64) -> Result<Vec<AiRunEvent>, AiJobServiceError> {
        let rows = self.event_repo.list_for_run(run_id, limit).await?;
        Ok(rows.into_iter().map(AiRunEvent::from).collect())
    }

    pub async fn claim_pending_job(&self, job_type: Option<&str>) -> Result<Option<AiJob>, AiJobServiceError> {
        Ok(self.job_repo.claim_pending(job_type).await?.map(AiJob::from))
    }

    /// Lease a pending job with heartbeat semantics (async model).
    /// Returns the leased job or `None` if no pending job matched.
    pub async fn lease_job(
        &self,
        job_type: Option<&str>,
        lease_owner: &str,
        lease_seconds: i64,
    ) -> Result<Option<AiJob>, AiJobServiceError> {
        Ok(self
            .job_repo
            .lease_pending(job_type, lease_owner, lease_seconds)
            .await?
            .map(AiJob::from))
    }

    /// Extend the lease on a job owned by `lease_owner`.
    pub async fn heartbeat_job(
        &self,
        job_id: &str,
        lease_owner: &str,
        lease_seconds: i64,
    ) -> Result<bool, AiJobServiceError> {
        self.job_repo
            .heartbeat(job_id, lease_owner, lease_seconds)
            .await
            .map_err(Into::into)
    }

    /// Take over a job whose lease has expired (reaper path).
    pub async fn take_over_job(
        &self,
        job_id: &str,
        new_owner: &str,
        lease_seconds: i64,
    ) -> Result<Option<AiJob>, AiJobServiceError> {
        Ok(self
            .job_repo
            .take_over(job_id, new_owner, lease_seconds)
            .await?
            .map(AiJob::from))
    }

    /// Mark a job as timed out and fail any active run.
    /// Used by `AiJobTimeoutReaperService` when `attempt_count >= max_attempts`.
    pub async fn timeout_job(&self, job_id: &str, reason: &str) -> Result<AiJob, AiJobServiceError> {
        let job = self.get_job(job_id).await?;
        let current = AiJobStatus::from_str(&job.status)
            .ok_or_else(|| AiJobServiceError::Internal(format!("invalid status in DB: {}", job.status)))?;
        let target = AiJobStatus::TimedOut;
        if !current.can_transition_to(&target) {
            return Err(AiJobServiceError::Conflict(format!(
                "cannot transition job {} from {} to {}",
                job_id, current, target
            )));
        }
        self.job_repo
            .update_status(job_id, AiJobStatus::TimedOut.as_str())
            .await?;
        self.job_repo.set_error_message(job_id, reason).await?;
        // Fail any non-terminal runs for this job.
        let runs = self.run_repo.list_for_job(job_id).await?;
        for run in runs {
            if let Some(status) = AiRunStatus::from_str(&run.status) {
                if !status.is_terminal() {
                    let _ = self
                        .run_repo
                        .fail(&run.run_id, Some("timeout"), Some(reason), None)
                        .await;
                }
            }
        }
        // Best-effort outbox event for SSE fan-out.
        self.emit_job_event(
            job_id,
            "",
            "ai_job.timed_out",
            None,
            Some(("timeout".to_string(), reason.to_string())),
        )
        .await;
        self.get_job(job_id).await
    }

    /// List jobs whose lease has expired (reaper query).
    pub async fn list_expired_leases(&self, limit: i64) -> Result<Vec<AiJob>, AiJobServiceError> {
        let rows = self.job_repo.list_expired_leases(chrono::Utc::now(), limit).await?;
        Ok(rows.into_iter().map(AiJob::from).collect())
    }

    pub async fn get_job_stats(&self) -> Result<Value, AiJobServiceError> {
        let rows = self.job_repo.count_by_status().await?;
        let mut stats = serde_json::Map::new();
        let mut total: i64 = 0;
        for row in &rows {
            stats.insert(row.status.clone(), serde_json::json!(row.count));
            total += row.count;
        }
        stats.insert("total".to_string(), serde_json::json!(total));
        Ok(Value::Object(stats))
    }

    /// Write a `ai_job.*` domain event into the transactional outbox.
    ///
    /// This is **best-effort**: if the outbox is not wired (`outbox_repo`
    /// or `pool` is `None`) or the write fails, the method logs a warning
    /// and returns. The business state mutation (run/job status update)
    /// has already been committed at this point and is not rolled back.
    ///
    /// The CDC relay picks up the outbox row and publishes it to the
    /// `fms.domain-events` MQ topic; the `DomainEventSubscriberService`
    /// then dispatches it to `AiJobEventHandler` which broadcasts on the
    /// `ai_execution` SSE topic.
    async fn emit_job_event(
        &self,
        job_id: &str,
        run_id: &str,
        event_type: &str,
        output: Option<Value>,
        error: Option<(String, String)>,
    ) {
        let (Some(outbox_repo), Some(pool)) = (self.outbox_repo.as_ref(), self.pool.as_ref()) else {
            return; // outbox not configured — skip silently
        };
        let timestamp = chrono::Utc::now().to_rfc3339();
        let payload = json!({
            "job_id": job_id,
            "run_id": run_id,
            "event_type": event_type,
            "output": output,
            "error_code": error.as_ref().map(|(c, _)| c),
            "error_message": error.as_ref().map(|(_, m)| m),
            "timestamp": timestamp,
        });
        let source_change_id = if run_id.is_empty() {
            format!("ai_job_{}_{}", job_id, event_type)
        } else {
            format!("ai_job_{}_{}_{}", job_id, run_id, event_type)
        };
        let mut tx = match pool.begin().await {
            Ok(tx) => tx,
            Err(error) => {
                tracing::warn!(
                    target: "ai_job_service",
                    job_id = %job_id,
                    event_type = %event_type,
                    error = %error,
                    "failed to begin outbox transaction for ai_job event"
                );
                return;
            }
        };
        if let Err(error) = outbox_repo
            .insert_event_in_tx(&mut tx, "ai_job", job_id, event_type, payload, &source_change_id)
            .await
        {
            tracing::warn!(
                target: "ai_job_service",
                job_id = %job_id,
                event_type = %event_type,
                error = %error,
                "failed to insert ai_job outbox event"
            );
            return;
        }
        if let Err(error) = tx.commit().await {
            tracing::warn!(
                target: "ai_job_service",
                job_id = %job_id,
                event_type = %event_type,
                error = %error,
                "failed to commit ai_job outbox transaction"
            );
        }
    }
}

/// Extract the entity id from an input envelope, following the same
/// convention as `PgAiAuthContextLoader`: `entity_id` first, then
/// `context.entity_id`.
fn extract_envelope_entity_id(envelope: &Value) -> Option<&str> {
    envelope.get("entity_id").and_then(|v| v.as_str()).or_else(|| {
        envelope
            .get("context")
            .and_then(|c| c.get("entity_id"))
            .and_then(|v| v.as_str())
    })
}

fn derive_run_input_summary(run: &AiRun, input_envelope: &Value) -> RunInputCheckpointSummary {
    let mut summary = RunInputCheckpointSummary::default();
    summary.governance_hash = short_hash(&format!(
        "{}|{}|{}",
        run.job_id,
        run.runtime_engine,
        run.model_id.clone().unwrap_or_default()
    ));
    summary.tool_schema_hash = short_hash(&format!(
        "{}:tools:{}",
        run.job_id,
        input_envelope.get("tools").map(|t| t.to_string()).unwrap_or_default()
    ));
    summary.model_id = run.model_id.clone();
    summary.prompt_cache_key_hash = short_hash(
        input_envelope
            .get("prompt_cache_key")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    );
    summary
}

fn short_hash(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(input.as_bytes());
    let hex = hex::encode(digest);
    hex.chars().take(16).collect()
}
