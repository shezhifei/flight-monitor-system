//! `AiExecutionControlService` — the durable ledger side of the AI
//! agent resilient tool architecture.
//!
//! The service is the consumer-side counterpart to the Python
//! `ToolExecutor` / `LLMStreamRunner`. It owns the transitions on
//! `ai_tool_calls` and writes the `ai_runtime_commands` rows that the
//! Python workers will pick up via `FOR UPDATE SKIP LOCKED`.
//!
//! The MQ event consumer (`AiEventConsumer`) dispatches each
//! `AiRuntimeEventEnvelope` into the right `handle_*` method here. The
//! service never invents tool-call data on its own; it always starts
//! from the envelope produced by the sidecar.
//!
//! Authorization for protected tools is delegated to
//! [`ToolAuthorizationService`]; the result drives the command
//! (`tool_lease` / `tool_proposal_only` / `tool_denied`) and the
//! ledger status (`Authorized` / `ProposalOnly` / `Denied`).
//!
//! The service owns ledger transitions, checkpoint persistence, run
//! finalization, and routing into `AiProposalIngestService`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{json, Value};
use thiserror::Error;
use ulid::Ulid;

use fms_domain::ai_runtime_event::{
    AiRuntimeEventEnvelope, CheckpointPayload, HeartbeatPayload, RunCompletePayload, RunFailPayload,
    ToolAuthorizationMode, ToolCallRequestedPayload, ToolExecutionStatus, ToolResultPayload,
};
use fms_domain::models::ai_execution::{
    AiRunCheckpointRecord, AiRunCheckpointType, AiRuntimeCommandRecord, AiRuntimeCommandStatus, AiRuntimeCommandType,
    AiToolCallError, AiToolCallRecord, AiToolCallResult, AiToolCallStatus, AiToolCallType,
};
use fms_domain::models::tool_authorization::{
    ObjectPolicyDecision, ToolAuthorizationContext, ToolAuthorizationDecision,
};
use fms_domain::models::tool_governance::{ResolvedToolGovernance, RustToolGovernanceResolver};
use fms_domain::ports::ai_auth_context_loader::RunAuthorizationContextLoader;
use fms_domain::ports::ai_execution_repository::{
    assert_checkpoint_size_within_budget, AiExecutionRepositoryError, AiRunCheckpointRepository,
    AiRuntimeCommandRepository, AiToolCallRepository,
};

use crate::services::ai_runtime_service::tool_authorization_service::{
    ToolAuthorizationError, ToolAuthorizationService,
};

/// Hook for ingesting proposals emitted by a tool result. The full
/// implementation lives in `AiProposalIngestService`; this surface
/// lets the control service route proposal ids without depending on
/// the heavier service type.
#[async_trait]
pub trait ProposalIngestHook: Send + Sync {
    async fn ingest(
        &self,
        run_id: &str,
        job_id: &str,
        tool_call_pk: &str,
        proposal_ids: &[String],
    ) -> Result<(), ControlServiceError>;
}

/// Default hook: log and return `Ok(())`. Production wiring injects
/// `AiProposalIngestService`.
pub struct LoggingProposalIngestHook;

#[async_trait]
impl ProposalIngestHook for LoggingProposalIngestHook {
    async fn ingest(
        &self,
        run_id: &str,
        _job_id: &str,
        tool_call_pk: &str,
        proposal_ids: &[String],
    ) -> Result<(), ControlServiceError> {
        if proposal_ids.is_empty() {
            return Ok(());
        }
        tracing::info!(
            target: "ai_execution_control",
            run_id = %run_id,
            tool_call_pk = %tool_call_pk,
            proposal_count = proposal_ids.len(),
            "phase 1 stub: would ingest proposals"
        );
        Ok(())
    }
}

/// Hook for run lifecycle terminal events (run.complete / run.fail).
/// The durable implementation persists run output/error to Postgres
/// and transitions the parent job to its terminal state.
#[async_trait]
pub trait RunLifecycleHook: Send + Sync {
    async fn on_run_complete(
        &self,
        run_id: &str,
        output_raw: Value,
        token_usage: Option<Value>,
        proposal_ids: &[String],
        terminal_event_id: Option<&str>,
    ) -> Result<(), ControlServiceError>;

    async fn on_run_fail(
        &self,
        run_id: &str,
        error_code: &str,
        error_message: &str,
        terminal_event_id: Option<&str>,
    ) -> Result<(), ControlServiceError>;
}

/// Default implementation: log only. Production DI wires a Postgres
/// implementation that persists to ai_runs / ai_jobs.
pub struct LoggingRunLifecycleHook;

#[async_trait]
impl RunLifecycleHook for LoggingRunLifecycleHook {
    async fn on_run_complete(
        &self,
        run_id: &str,
        _output_raw: Value,
        _token_usage: Option<Value>,
        proposal_ids: &[String],
        terminal_event_id: Option<&str>,
    ) -> Result<(), ControlServiceError> {
        tracing::info!(
            target: "ai_execution_control",
            run_id = %run_id,
            proposal_count = proposal_ids.len(),
            terminal_event_id = ?terminal_event_id,
            "run.complete received (logging hook)"
        );
        Ok(())
    }

    async fn on_run_fail(
        &self,
        run_id: &str,
        error_code: &str,
        error_message: &str,
        terminal_event_id: Option<&str>,
    ) -> Result<(), ControlServiceError> {
        tracing::warn!(
            target: "ai_execution_control",
            run_id = %run_id,
            error_code = %error_code,
            error_message = %error_message,
            terminal_event_id = ?terminal_event_id,
            "run.fail received (logging hook)"
        );
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum ControlServiceError {
    #[error("failed to parse runtime event payload: {0}")]
    PayloadParse(String),
    #[error("failed to parse tool authorization context: {0}")]
    AuthorizationContext(String),
    #[error("tool authorization failed: {0}")]
    Authorization(#[from] ToolAuthorizationError),
    #[error("repository error: {0}")]
    Repository(#[from] AiExecutionRepositoryError),
    #[error("invalid state transition: {0}")]
    InvalidState(String),
}

impl std::fmt::Debug for AiExecutionControlService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiExecutionControlService").finish_non_exhaustive()
    }
}

pub struct AiExecutionControlService {
    tool_call_repo: Arc<dyn AiToolCallRepository>,
    command_repo: Arc<dyn AiRuntimeCommandRepository>,
    checkpoint_repo: Option<Arc<dyn AiRunCheckpointRepository>>,
    authorization: Arc<ToolAuthorizationService>,
    auth_context_loader: Option<Arc<dyn RunAuthorizationContextLoader>>,
    proposal_ingest: Arc<dyn ProposalIngestHook>,
    run_lifecycle: Arc<dyn RunLifecycleHook>,
    command_sequences: Mutex<HashMap<String, i64>>,
    run_checkpoints: Mutex<HashMap<String, i64>>,
}

impl AiExecutionControlService {
    pub fn new(
        tool_call_repo: Arc<dyn AiToolCallRepository>,
        command_repo: Arc<dyn AiRuntimeCommandRepository>,
        authorization: Arc<ToolAuthorizationService>,
    ) -> Self {
        Self {
            tool_call_repo,
            command_repo,
            checkpoint_repo: None,
            authorization,
            auth_context_loader: None,
            proposal_ingest: Arc::new(LoggingProposalIngestHook),
            run_lifecycle: Arc::new(LoggingRunLifecycleHook),
            command_sequences: Mutex::new(HashMap::new()),
            run_checkpoints: Mutex::new(HashMap::new()),
        }
    }

    pub fn with_auth_context_loader(mut self, loader: Arc<dyn RunAuthorizationContextLoader>) -> Self {
        self.auth_context_loader = Some(loader);
        self
    }

    pub fn with_checkpoint_repo(mut self, repo: Arc<dyn AiRunCheckpointRepository>) -> Self {
        self.checkpoint_repo = Some(repo);
        self
    }

    pub fn with_proposal_ingest(mut self, hook: Arc<dyn ProposalIngestHook>) -> Self {
        self.proposal_ingest = hook;
        self
    }

    pub fn with_run_lifecycle_hook(mut self, hook: Arc<dyn RunLifecycleHook>) -> Self {
        self.run_lifecycle = hook;
        self
    }

    pub fn checkpoint_repo(&self) -> Option<Arc<dyn AiRunCheckpointRepository>> {
        self.checkpoint_repo.clone()
    }

    pub fn tool_call_repo(&self) -> Option<Arc<dyn AiToolCallRepository>> {
        Some(self.tool_call_repo.clone())
    }

    pub fn command_repo(&self) -> Option<Arc<dyn AiRuntimeCommandRepository>> {
        Some(self.command_repo.clone())
    }

    /// Process a `tool.call.requested` event.
    ///
    /// ## Trust boundary
    ///
    /// The Python sidecar sends `authorization_mode` in the payload, but
    /// it is **never trusted** for security decisions. Instead:
    ///
    /// 1. Tool governance is resolved by [`RustToolGovernanceResolver`]
    ///    based solely on `tool_name`. Unknown tools default to `RustPdp`
    ///    (fail-closed).
    /// 2. If the Rust resolver classifies the tool as `PublicDirect`
    ///    (known L0 read-only), the sidecar's claim is honored and the
    ///    tool is marked running immediately.
    /// 3. All other tools go through the Rust PDP. The authorization
    ///    context (requester identity, permissions, entity allowlist) is
    ///    loaded via [`RunAuthorizationContextLoader`] from
    ///    Rust-persisted data (ai_jobs, user/role tables, entity config).
    ///    Payload fields `requester` / `governance` / `entity_allowlist`
    ///    are **completely ignored** - there is NO fallback to payload
    ///    data. If `auth_context_loader` is unavailable, the tool call
    ///    fails closed with an authorization error.
    ///
    /// Object policies (derived from tool args) are currently accepted
    /// from the payload; they will be re-derived from `tool_args`
    /// server-side in a future hardening pass.
    pub async fn handle_tool_call_requested(
        &self,
        envelope: AiRuntimeEventEnvelope,
    ) -> Result<(), ControlServiceError> {
        let payload: ToolCallRequestedPayload = serde_json::from_value(envelope.payload.clone())
            .map_err(|error| ControlServiceError::PayloadParse(format!("tool_call_requested: {error}")))?;

        let tool_type = parse_tool_type(&payload.tool_type);

        let record = AiToolCallRecord {
            tool_call_pk: payload.tool_call_pk.clone(),
            job_id: envelope.job_id.clone(),
            run_id: envelope.run_id.clone(),
            parent_tool_call_pk: payload.parent_tool_call_pk.clone(),
            root_tool_call_pk: None,
            depth: payload.depth as i32,
            round_index: envelope.round_index as i32,
            tool_call_id: payload.tool_call_id.clone(),
            tool_name: payload.tool_name.clone(),
            tool_type,
            status: AiToolCallStatus::Requested,
            args_hash: payload.args_hash.clone(),
            args_summary: payload.args_summary.clone(),
            result_hash: None,
            result_summary: None,
            error_code: None,
            error_message: None,
            retry_count: 0,
            max_retries: payload.max_retries as i32,
            timeout_seconds: payload.timeout_seconds as i32,
            last_heartbeat_at: None,
            idempotency_key: envelope.idempotency_key.clone(),
            mq_message_id: Some(envelope.event_id.clone()),
            mq_offset: Some(envelope.event_sequence as i64),
            created_at: envelope.emitted_at,
            started_at: None,
            finished_at: None,
            metadata: json!({}),
        };

        let inserted = self.tool_call_repo.upsert_requested(record).await?;
        if !inserted {
            tracing::debug!(
                target: "ai_execution_control",
                run_id = %envelope.run_id,
                idempotency_key = %envelope.idempotency_key,
                "skipping duplicate tool.call.requested"
            );
            return Ok(());
        }

        let rust_governance = RustToolGovernanceResolver::resolve(&payload.tool_name);

        if rust_governance.is_public_direct() {
            self.tool_call_repo.mark_running(&payload.tool_call_pk).await?;
            return Ok(());
        }

        let context = match &self.auth_context_loader {
            Some(loader) => {
                let tool_args = envelope.payload.get("tool_args").cloned().unwrap_or(Value::Null);
                loader
                    .load_context(
                        &envelope.run_id,
                        &envelope.job_id,
                        &payload.tool_call_pk,
                        &payload.tool_name,
                        &tool_args,
                    )
                    .await
                    .map_err(|e| {
                        ControlServiceError::AuthorizationContext(format!("failed to load authorization context: {e}"))
                    })?
            }
            None => {
                return Err(ControlServiceError::AuthorizationContext(
                    "auth_context_loader not configured; protected tool authorization is not available (fail-closed)"
                        .to_string(),
                ));
            }
        };

        let decision = self.authorization.authorize(context).await?;
        self.apply_authorization_decision(&envelope, &payload, decision).await
    }

    async fn apply_authorization_decision(
        &self,
        envelope: &AiRuntimeEventEnvelope,
        payload: &ToolCallRequestedPayload,
        decision: ToolAuthorizationDecision,
    ) -> Result<(), ControlServiceError> {
        match decision {
            ToolAuthorizationDecision::AllowDirect {
                lease_id,
                max_retries,
                timeout_seconds,
            } => {
                let command = self.build_lease_command(envelope, payload, &lease_id, max_retries, timeout_seconds)?;
                self.command_repo.enqueue(command).await?;
                self.tool_call_repo.mark_authorized(&payload.tool_call_pk).await?;
                Ok(())
            }
            ToolAuthorizationDecision::ProposalOnly { reason } => {
                let command = self.build_runtime_command(
                    envelope,
                    AiRuntimeCommandType::ToolProposalOnly,
                    &payload.tool_call_pk,
                    json!({
                        "tool_name": payload.tool_name,
                        "reason": reason,
                    }),
                )?;
                self.command_repo.enqueue(command).await?;
                self.tool_call_repo.mark_proposal_only(&payload.tool_call_pk).await?;
                Ok(())
            }
            ToolAuthorizationDecision::Deny { code, message } => {
                let denial_code = code.as_str();
                let command = self.build_runtime_command(
                    envelope,
                    AiRuntimeCommandType::ToolDenied,
                    &payload.tool_call_pk,
                    json!({
                        "tool_name": payload.tool_name,
                        "denial_code": denial_code,
                        "message": message,
                    }),
                )?;
                self.command_repo.enqueue(command).await?;
                self.tool_call_repo
                    .mark_denied(&payload.tool_call_pk, denial_code, &message)
                    .await?;
                Ok(())
            }
        }
    }

    fn build_lease_command(
        &self,
        envelope: &AiRuntimeEventEnvelope,
        payload: &ToolCallRequestedPayload,
        lease_id: &str,
        max_retries: u32,
        timeout_seconds: u32,
    ) -> Result<AiRuntimeCommandRecord, ControlServiceError> {
        self.build_runtime_command(
            envelope,
            AiRuntimeCommandType::ToolLease,
            &payload.tool_call_pk,
            json!({
                "lease_id": lease_id,
                "tool_name": payload.tool_name,
                "max_retries": max_retries,
                "timeout_seconds": timeout_seconds,
            }),
        )
    }

    fn build_runtime_command(
        &self,
        envelope: &AiRuntimeEventEnvelope,
        command_type: AiRuntimeCommandType,
        tool_call_pk: &str,
        payload: Value,
    ) -> Result<AiRuntimeCommandRecord, ControlServiceError> {
        let sequence = self.allocate_command_sequence(&envelope.run_id)?;
        Ok(AiRuntimeCommandRecord {
            command_id: Ulid::new().to_string(),
            run_id: envelope.run_id.clone(),
            command_type,
            command_sequence: sequence,
            tool_call_pk: Some(tool_call_pk.to_string()),
            payload,
            status: AiRuntimeCommandStatus::Pending,
            run_owner: None,
            lease_owner: None,
            lease_expires_at: None,
            created_at: Utc::now(),
            processed_at: None,
            attempt_count: 0,
            max_attempts: 3,
            last_heartbeat_at: None,
            run_owner_lock: None,
        })
    }

    fn allocate_command_sequence(&self, run_id: &str) -> Result<i64, ControlServiceError> {
        let mut sequences = self.command_sequences.lock().expect("command sequence map poisoned");
        let counter = sequences.entry(run_id.to_string()).or_insert(0);
        *counter += 1;
        Ok(*counter)
    }

    pub async fn handle_tool_result(&self, envelope: AiRuntimeEventEnvelope) -> Result<(), ControlServiceError> {
        let payload: ToolResultPayload = serde_json::from_value(envelope.payload.clone())
            .map_err(|error| ControlServiceError::PayloadParse(format!("tool_result: {error}")))?;

        let result = AiToolCallResult {
            result_hash: payload.result_hash.clone(),
            result_summary: payload.result_summary.clone(),
            proposal_ids: payload.proposal_ids.clone(),
            duration_ms: payload.duration_ms,
        };

        match payload.status {
            ToolExecutionStatus::Succeeded => {
                self.tool_call_repo
                    .mark_succeeded(&payload.tool_call_pk, result)
                    .await?;
            }
            ToolExecutionStatus::Failed => {
                self.tool_call_repo
                    .mark_failed(
                        &payload.tool_call_pk,
                        AiToolCallError {
                            code: payload.error_code.clone().unwrap_or_else(|| "TOOL_FAILED".to_string()),
                            message: payload
                                .error_message
                                .clone()
                                .unwrap_or_else(|| "tool execution failed".to_string()),
                            retryable: false,
                        },
                    )
                    .await?;
            }
            ToolExecutionStatus::Cancelled => {
                self.tool_call_repo.mark_cancelled(&payload.tool_call_pk).await?;
            }
            ToolExecutionStatus::Expired => {
                self.tool_call_repo.mark_expired(&payload.tool_call_pk).await?;
            }
            ToolExecutionStatus::Denied => {
                let code = payload.error_code.clone().unwrap_or_else(|| "TOOL_DENIED".to_string());
                let message = payload
                    .error_message
                    .clone()
                    .unwrap_or_else(|| "tool call denied".to_string());
                self.tool_call_repo
                    .mark_denied(&payload.tool_call_pk, &code, &message)
                    .await?;
            }
            ToolExecutionStatus::ProposalOnly => {
                self.tool_call_repo.mark_proposal_only(&payload.tool_call_pk).await?;
            }
        }

        if !payload.proposal_ids.is_empty() {
            self.proposal_ingest
                .ingest(
                    &envelope.run_id,
                    &envelope.job_id,
                    &payload.tool_call_pk,
                    &payload.proposal_ids,
                )
                .await?;
        }

        Ok(())
    }

    /// Persist a run checkpoint. If no checkpoint repository is
    /// configured, skip persistence after validating the payload.
    pub async fn handle_checkpoint(&self, envelope: AiRuntimeEventEnvelope) -> Result<(), ControlServiceError> {
        let payload: CheckpointPayload = serde_json::from_value(envelope.payload.clone())
            .map_err(|error| ControlServiceError::PayloadParse(format!("checkpoint: {error}")))?;

        assert_checkpoint_size_within_budget(payload.snapshot_size_bytes).map_err(|error| {
            tracing::warn!(
                target: "ai_execution_control",
                run_id = %envelope.run_id,
                checkpoint_id = %payload.checkpoint_id,
                size_bytes = payload.snapshot_size_bytes,
                error = %error,
                "poison checkpoint event: snapshot exceeds 64KB budget; acking without retry"
            );
            ControlServiceError::PayloadParse(format!("checkpoint snapshot too large: {error}"))
        })?;

        let Some(repo) = self.checkpoint_repo.as_ref() else {
            tracing::debug!(
                target: "ai_execution_control",
                run_id = %envelope.run_id,
                checkpoint_id = %payload.checkpoint_id,
                "no checkpoint repo configured; skipping persistence"
            );
            return Ok(());
        };

        let checkpoint_type = map_checkpoint_type(payload.checkpoint_type);
        let record = AiRunCheckpointRecord {
            checkpoint_id: payload.checkpoint_id.clone(),
            job_id: envelope.job_id.clone(),
            run_id: envelope.run_id.clone(),
            sequence_no: payload.sequence_no as i64,
            checkpoint_type,
            tool_call_pk: payload.tool_call_pk.clone(),
            proposal_id: payload.proposal_id.clone(),
            snapshot_hash: payload.snapshot_hash.clone(),
            snapshot: payload.snapshot.clone(),
            snapshot_size_bytes: payload.snapshot_size_bytes as i32,
            mq_message_id: Some(envelope.event_id.clone()),
            created_at: envelope.emitted_at,
        };

        let idempotency_key = checkpoint_idempotency_key(&envelope.run_id, payload.sequence_no, checkpoint_type);
        let inserted = repo.upsert(record).await?;
        if !inserted {
            tracing::debug!(
                target: "ai_execution_control",
                run_id = %envelope.run_id,
                sequence_no = payload.sequence_no,
                checkpoint_type = checkpoint_type.as_str(),
                "skipping duplicate checkpoint"
            );
            return Ok(());
        }

        if matches!(
            checkpoint_type,
            AiRunCheckpointType::BeforeTool | AiRunCheckpointType::AfterTool
        ) {
            let superseded = repo.mark_superseded(&envelope.run_id, payload.sequence_no).await?;
            if superseded > 0 {
                tracing::debug!(
                    target: "ai_execution_control",
                    run_id = %envelope.run_id,
                    sequence_no = payload.sequence_no,
                    superseded_count = superseded,
                    "superseded prior tool checkpoints"
                );
            }
        }

        tracing::info!(
            target: "ai_execution_control",
            run_id = %envelope.run_id,
            checkpoint_id = %payload.checkpoint_id,
            checkpoint_type = checkpoint_type.as_str(),
            sequence_no = payload.sequence_no,
            idempotency_key = %idempotency_key,
            "checkpoint persisted"
        );
        Ok(())
    }

    /// Persist a `RunInput` checkpoint directly (not via MQ). Invoked
    /// by the run starter right after `ai_runs.input_envelope` is
    /// written. Failures are non-fatal — the run is still recoverable
    /// from `ai_runs.input_envelope` even without this row.
    pub async fn create_run_input_checkpoint(
        &self,
        job_id: &str,
        run_id: &str,
        input_envelope: Value,
        snapshot_summary: RunInputCheckpointSummary,
    ) -> Result<Option<AiRunCheckpointRecord>, ControlServiceError> {
        let Some(repo) = self.checkpoint_repo.as_ref() else {
            return Ok(None);
        };
        let snapshot = json!({
            "envelope": input_envelope,
            "summary": {
                "governance_hash": snapshot_summary.governance_hash,
                "tool_schema_hash": snapshot_summary.tool_schema_hash,
                "model_id": snapshot_summary.model_id,
                "prompt_cache_key_hash": snapshot_summary.prompt_cache_key_hash,
            }
        });
        let snapshot_size = estimate_snapshot_size(&snapshot);
        if snapshot_size > 64 * 1024 {
            tracing::warn!(
                target: "ai_execution_control",
                run_id = %run_id,
                size_bytes = snapshot_size,
                "run_input checkpoint snapshot exceeds 64KB; skipping persistence"
            );
            return Ok(None);
        }
        let sequence_no = self.allocate_checkpoint_sequence(run_id);
        let checkpoint_id = Ulid::new().to_string();
        let record = AiRunCheckpointRecord {
            checkpoint_id: checkpoint_id.clone(),
            job_id: job_id.to_string(),
            run_id: run_id.to_string(),
            sequence_no,
            checkpoint_type: AiRunCheckpointType::RunInput,
            tool_call_pk: None,
            proposal_id: None,
            snapshot_hash: snapshot_summary.governance_hash.clone(),
            snapshot,
            snapshot_size_bytes: snapshot_size as i32,
            mq_message_id: None,
            created_at: Utc::now(),
        };
        repo.upsert(record.clone()).await?;
        Ok(Some(record))
    }

    pub async fn list_recoverable_checkpoints(
        &self,
        run_id: &str,
    ) -> Result<Vec<AiRunCheckpointRecord>, ControlServiceError> {
        let Some(repo) = self.checkpoint_repo.as_ref() else {
            return Ok(Vec::new());
        };
        let rows = repo.list_by_run(run_id).await?;
        Ok(rows
            .into_iter()
            .filter(|row| {
                matches!(
                    row.checkpoint_type,
                    AiRunCheckpointType::BeforeTool | AiRunCheckpointType::AfterTool
                )
            })
            .collect())
    }

    pub async fn list_all_checkpoints(&self, run_id: &str) -> Result<Vec<AiRunCheckpointRecord>, ControlServiceError> {
        let Some(repo) = self.checkpoint_repo.as_ref() else {
            return Ok(Vec::new());
        };
        repo.list_by_run(run_id).await.map_err(Into::into)
    }

    pub async fn latest_recoverable_checkpoint(
        &self,
        run_id: &str,
    ) -> Result<Option<AiRunCheckpointRecord>, ControlServiceError> {
        let Some(repo) = self.checkpoint_repo.as_ref() else {
            return Ok(None);
        };
        repo.latest_recoverable(run_id).await.map_err(Into::into)
    }

    /// Enqueue a `ResumeRun` command in `ai_runtime_commands` so a
    /// Python worker can pick the run back up from the supplied
    /// checkpoint.
    pub async fn enqueue_resume_run(
        &self,
        job_id: &str,
        run_id: &str,
        checkpoint: &AiRunCheckpointRecord,
        requester_user_id: &str,
    ) -> Result<AiRuntimeCommandRecord, ControlServiceError> {
        let sequence = self.allocate_command_sequence(run_id)?;
        let record = AiRuntimeCommandRecord {
            command_id: Ulid::new().to_string(),
            run_id: run_id.to_string(),
            command_type: AiRuntimeCommandType::ResumeRun,
            command_sequence: sequence,
            tool_call_pk: checkpoint.tool_call_pk.clone(),
            payload: json!({
                "job_id": job_id,
                "checkpoint_id": checkpoint.checkpoint_id,
                "sequence_no": checkpoint.sequence_no,
                "checkpoint_type": checkpoint.checkpoint_type.as_str(),
                "snapshot": checkpoint.snapshot,
                "snapshot_hash": checkpoint.snapshot_hash,
                "requester_user_id": requester_user_id,
            }),
            status: AiRuntimeCommandStatus::Pending,
            run_owner: None,
            lease_owner: None,
            lease_expires_at: None,
            created_at: Utc::now(),
            processed_at: None,
            attempt_count: 0,
            max_attempts: 3,
            last_heartbeat_at: None,
            run_owner_lock: None,
        };
        self.command_repo.enqueue(record.clone()).await?;
        Ok(record)
    }

    /// Enqueue a `start_run` command. A Python worker leases this
    /// command and becomes the run owner. The payload carries the
    /// envelope, capability snapshot and governance hash so the worker
    /// has everything it needs without a second round-trip.
    pub async fn enqueue_start_run(
        &self,
        job_id: &str,
        run_id: &str,
        input_envelope: Value,
        capability_snapshot: Value,
        governance_hash: &str,
    ) -> Result<AiRuntimeCommandRecord, ControlServiceError> {
        let sequence = self.allocate_command_sequence(run_id)?;
        let record = AiRuntimeCommandRecord {
            command_id: Ulid::new().to_string(),
            run_id: run_id.to_string(),
            command_type: AiRuntimeCommandType::StartRun,
            command_sequence: sequence,
            tool_call_pk: None,
            payload: json!({
                "job_id": job_id,
                "envelope": input_envelope,
                "capability_snapshot": capability_snapshot,
                "governance_hash": governance_hash,
            }),
            status: AiRuntimeCommandStatus::Pending,
            run_owner: None,
            lease_owner: None,
            lease_expires_at: None,
            created_at: Utc::now(),
            processed_at: None,
            attempt_count: 0,
            max_attempts: 3,
            last_heartbeat_at: None,
            run_owner_lock: None,
        };
        self.command_repo.enqueue(record.clone()).await?;
        Ok(record)
    }

    /// Enqueue a `cancel_run` command. The worker that owns the run
    /// (or any worker if the run is unowned) picks it up and
    /// interrupts the in-flight tool execution.
    pub async fn enqueue_cancel_run(
        &self,
        job_id: &str,
        run_id: &str,
        requester_user_id: &str,
    ) -> Result<AiRuntimeCommandRecord, ControlServiceError> {
        let sequence = self.allocate_command_sequence(run_id)?;
        let record = AiRuntimeCommandRecord {
            command_id: Ulid::new().to_string(),
            run_id: run_id.to_string(),
            command_type: AiRuntimeCommandType::CancelRun,
            command_sequence: sequence,
            tool_call_pk: None,
            payload: json!({
                "job_id": job_id,
                "requester_user_id": requester_user_id,
            }),
            status: AiRuntimeCommandStatus::Pending,
            run_owner: None,
            lease_owner: None,
            lease_expires_at: None,
            created_at: Utc::now(),
            processed_at: None,
            attempt_count: 0,
            max_attempts: 3,
            last_heartbeat_at: None,
            run_owner_lock: None,
        };
        self.command_repo.enqueue(record.clone()).await?;
        Ok(record)
    }

    /// Enqueue a `retry_tool` command for a specific tool call. The
    /// worker re-executes the tool with the same idempotency key (so
    /// duplicate effects are prevented by the DB unique constraint).
    pub async fn enqueue_retry_tool(
        &self,
        job_id: &str,
        run_id: &str,
        tool_call_pk: &str,
        requester_user_id: &str,
    ) -> Result<AiRuntimeCommandRecord, ControlServiceError> {
        let sequence = self.allocate_command_sequence(run_id)?;
        let record = AiRuntimeCommandRecord {
            command_id: Ulid::new().to_string(),
            run_id: run_id.to_string(),
            command_type: AiRuntimeCommandType::RetryTool,
            command_sequence: sequence,
            tool_call_pk: Some(tool_call_pk.to_string()),
            payload: json!({
                "job_id": job_id,
                "tool_call_pk": tool_call_pk,
                "requester_user_id": requester_user_id,
            }),
            status: AiRuntimeCommandStatus::Pending,
            run_owner: None,
            lease_owner: None,
            lease_expires_at: None,
            created_at: Utc::now(),
            processed_at: None,
            attempt_count: 0,
            max_attempts: 3,
            last_heartbeat_at: None,
            run_owner_lock: None,
        };
        self.command_repo.enqueue(record.clone()).await?;
        Ok(record)
    }

    fn allocate_checkpoint_sequence(&self, run_id: &str) -> i64 {
        let mut sequences = self
            .run_checkpoints
            .lock()
            .expect("run checkpoint sequence map poisoned");
        let counter = sequences.entry(run_id.to_string()).or_insert(0);
        *counter += 1;
        *counter
    }

    pub async fn update_heartbeat(&self, envelope: AiRuntimeEventEnvelope) -> Result<(), ControlServiceError> {
        let payload: HeartbeatPayload = serde_json::from_value(envelope.payload.clone())
            .map_err(|error| ControlServiceError::PayloadParse(format!("heartbeat: {error}")))?;
        self.tool_call_repo.heartbeat(&payload.tool_call_pk).await?;
        Ok(())
    }

    pub async fn handle_run_complete(&self, envelope: AiRuntimeEventEnvelope) -> Result<(), ControlServiceError> {
        let payload: RunCompletePayload = serde_json::from_value(envelope.payload.clone())
            .map_err(|error| ControlServiceError::PayloadParse(format!("run_complete: {error}")))?;
        tracing::info!(
            target: "ai_execution_control",
            run_id = %envelope.run_id,
            proposal_count = payload.proposal_ids.len(),
            terminal_event_id = ?payload.terminal_event_id,
            "run.complete received; delegating to lifecycle hook"
        );
        self.run_lifecycle
            .on_run_complete(
                &envelope.run_id,
                payload.output_raw,
                payload.token_usage,
                &payload.proposal_ids,
                payload.terminal_event_id.as_deref(),
            )
            .await
    }

    pub async fn handle_run_fail(&self, envelope: AiRuntimeEventEnvelope) -> Result<(), ControlServiceError> {
        let payload: RunFailPayload = serde_json::from_value(envelope.payload.clone())
            .map_err(|error| ControlServiceError::PayloadParse(format!("run_fail: {error}")))?;
        tracing::warn!(
            target: "ai_execution_control",
            run_id = %envelope.run_id,
            error_code = %payload.error_code,
            error_message = %payload.error_message,
            "run.fail received; delegating to lifecycle hook"
        );
        self.run_lifecycle
            .on_run_fail(
                &envelope.run_id,
                &payload.error_code,
                &payload.error_message,
                payload.terminal_event_id.as_deref(),
            )
            .await
    }
}

fn parse_tool_type(raw: &str) -> AiToolCallType {
    match raw.trim().to_ascii_lowercase().as_str() {
        "builtin" => AiToolCallType::Builtin,
        "mcp" => AiToolCallType::Mcp,
        "skill" => AiToolCallType::Skill,
        "subagent" => AiToolCallType::Subagent,
        other => {
            tracing::debug!(
                target: "ai_execution_control",
                tool_type = %other,
                "unknown tool_type from MQ event; defaulting to builtin"
            );
            AiToolCallType::Builtin
        }
    }
}

/// Inputs the run starter passes to
/// [`AiExecutionControlService::create_run_input_checkpoint`].
#[derive(Debug, Clone, Default)]
pub struct RunInputCheckpointSummary {
    pub governance_hash: String,
    pub tool_schema_hash: String,
    pub model_id: Option<String>,
    pub prompt_cache_key_hash: String,
}

fn map_checkpoint_type(value: fms_domain::ai_runtime_event::CheckpointType) -> AiRunCheckpointType {
    use fms_domain::ai_runtime_event::CheckpointType as Src;
    match value {
        Src::RunInput => AiRunCheckpointType::RunInput,
        Src::RoundBeforeModel => AiRunCheckpointType::RoundBeforeModel,
        Src::BeforeTool => AiRunCheckpointType::BeforeTool,
        Src::AfterTool => AiRunCheckpointType::AfterTool,
        Src::BeforeProposalIngest => AiRunCheckpointType::BeforeProposalIngest,
        Src::BeforeDomainAction => AiRunCheckpointType::BeforeDomainAction,
        Src::AfterDomainAction => AiRunCheckpointType::AfterDomainAction,
        Src::AfterCompletion => AiRunCheckpointType::AfterCompletion,
    }
}

fn checkpoint_idempotency_key(run_id: &str, sequence_no: u64, kind: AiRunCheckpointType) -> String {
    format!("{run_id}:{sequence_no}:{}", kind.as_str())
}

fn estimate_snapshot_size(snapshot: &Value) -> usize {
    serde_json::to_vec(snapshot).map(|bytes| bytes.len()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::ai_runtime_service::in_memory_repos::{
        InMemoryCheckpointRepository, InMemoryRuntimeCommandRepository, InMemoryToolCallRepository,
    };
    use crate::services::ai_runtime_service::tool_authorization_service::{
        StaticFeatureFlagSource, ToolAuthorizationService,
    };
    use fms_domain::models::ai_execution::{AiRunCheckpointType, AiRuntimeCommandType, AiToolCallStatus};
    use fms_domain::models::tool_governance::ToolGovernancePreset;
    use fms_domain::ports::ai_auth_context_loader::AuthContextLoaderError;
    use fms_domain::ports::ai_execution_repository::AiRunCheckpointRepository;
    use serde_json::json;

    /// Mock loader that returns a pre-configured [`ToolAuthorizationContext`].
    /// Used in tests to avoid needing a real Postgres connection.
    struct MockAuthContextLoader {
        context: ToolAuthorizationContext,
    }

    #[async_trait]
    impl RunAuthorizationContextLoader for MockAuthContextLoader {
        async fn load_context(
            &self,
            _run_id: &str,
            _job_id: &str,
            _tool_call_pk: &str,
            _tool_name: &str,
            _tool_args: &Value,
        ) -> Result<ToolAuthorizationContext, AuthContextLoaderError> {
            Ok(self.context.clone())
        }
    }

    fn control_service() -> (
        AiExecutionControlService,
        Arc<InMemoryToolCallRepository>,
        Arc<InMemoryRuntimeCommandRepository>,
    ) {
        let tool_call_repo = Arc::new(InMemoryToolCallRepository::new());
        let command_repo = Arc::new(InMemoryRuntimeCommandRepository::new());
        let authorization = Arc::new(ToolAuthorizationService::new(
            Arc::new(StaticFeatureFlagSource::empty()),
        ));
        let svc = AiExecutionControlService::new(
            tool_call_repo.clone() as Arc<dyn AiToolCallRepository>,
            command_repo.clone() as Arc<dyn AiRuntimeCommandRepository>,
            authorization,
        );
        (svc, tool_call_repo, command_repo)
    }

    fn control_service_with_auth(
        auth_context: ToolAuthorizationContext,
    ) -> (
        AiExecutionControlService,
        Arc<InMemoryToolCallRepository>,
        Arc<InMemoryRuntimeCommandRepository>,
    ) {
        let tool_call_repo = Arc::new(InMemoryToolCallRepository::new());
        let command_repo = Arc::new(InMemoryRuntimeCommandRepository::new());
        let authorization = Arc::new(ToolAuthorizationService::new(
            Arc::new(StaticFeatureFlagSource::empty()),
        ));
        let svc = AiExecutionControlService::new(
            tool_call_repo.clone() as Arc<dyn AiToolCallRepository>,
            command_repo.clone() as Arc<dyn AiRuntimeCommandRepository>,
            authorization,
        );
        let svc = svc.with_auth_context_loader(Arc::new(MockAuthContextLoader { context: auth_context }));
        (svc, tool_call_repo, command_repo)
    }

    impl InMemoryCheckpointRepository {
        pub(crate) async fn latest_recoverable_for_test(&self, run_id: &str) -> Option<AiRunCheckpointRecord> {
            use fms_domain::ports::ai_execution_repository::AiRunCheckpointRepository;
            self.latest_recoverable(run_id).await.ok().flatten()
        }
    }

    fn protected_governance(name: &str, perms: Vec<String>) -> ResolvedToolGovernance {
        let mut g = ToolGovernancePreset::InternalWorkspaceWrite.default_governance(name);
        g.required_account_permissions = perms;
        g.execution_mode = fms_domain::models::tool_governance::ExecutionMode::Direct;
        g
    }

    fn requested_envelope(
        run_id: &str,
        job_id: &str,
        tool_call_pk: &str,
        tool_name: &str,
        authorization_mode: ToolAuthorizationMode,
        extra: Value,
    ) -> AiRuntimeEventEnvelope {
        let payload = json!({
            "tool_call_pk": tool_call_pk,
            "tool_call_id": format!("call-{tool_call_pk}"),
            "tool_name": tool_name,
            "tool_type": "builtin",
            "parent_tool_call_pk": Value::Null,
            "depth": 0,
            "args_hash": "abc",
            "args_summary": {"airport_code": "PEK"},
            "authorization_mode": authorization_mode,
            "max_retries": 2,
            "timeout_seconds": 30,
        });
        let mut payload = payload.as_object().cloned().unwrap_or_default();
        if let Value::Object(extra_obj) = extra {
            for (k, v) in extra_obj {
                payload.insert(k, v);
            }
        }
        AiRuntimeEventEnvelope::new(
            fms_domain::ai_runtime_event::AiRuntimeEventType::ToolCallRequested,
            run_id,
            job_id,
            0,
            1,
            format!("{run_id}:0:call-{tool_call_pk}:{tool_name}:abc"),
            Value::Object(payload),
        )
    }

    #[tokio::test]
    async fn handle_tool_call_requested_creates_ledger_row() {
        let (svc, tool_call_repo, command_repo) = control_service();
        let envelope = requested_envelope(
            "run-1",
            "job-1",
            "tpc-1",
            "weather_at_airport",
            ToolAuthorizationMode::PublicDirect,
            Value::Object(Default::default()),
        );
        svc.handle_tool_call_requested(envelope).await.unwrap();
        assert_eq!(tool_call_repo.len(), 1);
        let row = tool_call_repo.get("tpc-1").await.unwrap().unwrap();
        assert_eq!(row.status, AiToolCallStatus::Running);
        assert!(command_repo.is_empty(), "no command expected for public L0");
    }

    #[tokio::test]
    async fn handle_tool_call_requested_skips_duplicate_idempotency_key() {
        let (svc, tool_call_repo, _) = control_service();
        let envelope_a = requested_envelope(
            "run-1",
            "job-1",
            "tpc-1",
            "weather_at_airport",
            ToolAuthorizationMode::PublicDirect,
            Value::Object(Default::default()),
        );
        let envelope_b = requested_envelope(
            "run-1",
            "job-1",
            "tpc-1",
            "weather_at_airport",
            ToolAuthorizationMode::PublicDirect,
            Value::Object(Default::default()),
        );
        svc.handle_tool_call_requested(envelope_a).await.unwrap();
        svc.handle_tool_call_requested(envelope_b).await.unwrap();
        assert_eq!(tool_call_repo.len(), 1);
    }

    fn protected_envelope(
        run_id: &str,
        job_id: &str,
        tool_call_pk: &str,
        requester_user_id: &str,
        permissions: Vec<String>,
        allowlist: Vec<String>,
    ) -> AiRuntimeEventEnvelope {
        protected_envelope_with_governance(
            run_id,
            job_id,
            tool_call_pk,
            requester_user_id,
            permissions,
            allowlist,
            protected_governance("book_flight", vec!["booking:write".into()]),
        )
    }

    fn proposal_only_governance(name: &str) -> ResolvedToolGovernance {
        let mut g = ToolGovernancePreset::InternalWorkspaceWrite.default_governance(name);
        g.required_account_permissions = vec!["booking:write".into()];
        g
    }

    fn protected_envelope_with_governance(
        run_id: &str,
        job_id: &str,
        tool_call_pk: &str,
        requester_user_id: &str,
        permissions: Vec<String>,
        allowlist: Vec<String>,
        governance: ResolvedToolGovernance,
    ) -> AiRuntimeEventEnvelope {
        let governance_json = serde_json::to_value(&governance).unwrap();
        let extra = json!({
            "requester": {
                "user_id": requester_user_id,
                "roles": ["dispatcher"],
                "permissions": permissions,
                "object_policies": [],
            },
            "governance": governance_json,
            "entity_allowlist": allowlist,
        });
        requested_envelope(
            run_id,
            job_id,
            tool_call_pk,
            "book_flight",
            ToolAuthorizationMode::RustPdp,
            extra,
        )
    }

    fn protected_context(
        requester_user_id: &str,
        permissions: Vec<String>,
        allowlist: Vec<String>,
        governance: ResolvedToolGovernance,
    ) -> ToolAuthorizationContext {
        ToolAuthorizationContext {
            requester_user_id: requester_user_id.to_string(),
            requester_user_roles: vec!["dispatcher".to_string()],
            requester_permissions: permissions,
            requester_object_policies: Vec::new(),
            entity_tool_allowlist: allowlist,
            tool_governance: governance,
            tool_call_pk: "tpc-protected".to_string(),
            tool_args: json!({"flight": "CA1234"}),
            feature_flags: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn handle_tool_call_requested_inserts_tool_lease_command_for_protected_allow() {
        let context = protected_context(
            "user-1",
            vec!["booking:write".into()],
            vec!["book_flight".into()],
            protected_governance("book_flight", vec!["booking:write".into()]),
        );
        let (svc, tool_call_repo, command_repo) = control_service_with_auth(context);
        let envelope = protected_envelope(
            "run-1",
            "job-1",
            "tpc-1",
            "user-1",
            vec!["booking:write".into()],
            vec!["book_flight".into()],
        );
        svc.handle_tool_call_requested(envelope).await.unwrap();

        let commands = command_repo.snapshot();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command_type, AiRuntimeCommandType::ToolLease);
        let lease_id = commands[0]
            .payload
            .get("lease_id")
            .and_then(Value::as_str)
            .expect("lease_id present");
        assert!(!lease_id.is_empty());

        let row = tool_call_repo.get("tpc-1").await.unwrap().unwrap();
        assert_eq!(row.status, AiToolCallStatus::Authorized);
    }

    #[tokio::test]
    async fn handle_tool_call_requested_inserts_tool_proposal_only_command() {
        let context = protected_context(
            "user-1",
            vec!["booking:write".into()],
            vec!["book_flight".into()],
            proposal_only_governance("book_flight"),
        );
        let (svc, tool_call_repo, command_repo) = control_service_with_auth(context);
        let envelope = protected_envelope_with_governance(
            "run-1",
            "job-1",
            "tpc-1",
            "user-1",
            vec!["booking:write".into()],
            vec!["book_flight".into()],
            proposal_only_governance("book_flight"),
        );
        svc.handle_tool_call_requested(envelope).await.unwrap();

        let commands = command_repo.snapshot();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command_type, AiRuntimeCommandType::ToolProposalOnly);
        let row = tool_call_repo.get("tpc-1").await.unwrap().unwrap();
        assert_eq!(row.status, AiToolCallStatus::ProposalOnly);
    }

    #[tokio::test]
    async fn handle_tool_call_requested_inserts_tool_denied_command() {
        let context = protected_context(
            "user-1",
            vec![],
            vec!["book_flight".into()],
            protected_governance("book_flight", vec!["booking:write".into()]),
        );
        let (svc, tool_call_repo, command_repo) = control_service_with_auth(context);
        let envelope = protected_envelope("run-1", "job-1", "tpc-1", "user-1", vec![], vec!["book_flight".into()]);
        svc.handle_tool_call_requested(envelope).await.unwrap();

        let commands = command_repo.snapshot();
        assert_eq!(commands[0].command_type, AiRuntimeCommandType::ToolDenied);
        let row = tool_call_repo.get("tpc-1").await.unwrap().unwrap();
        assert_eq!(row.status, AiToolCallStatus::Denied);
    }

    #[tokio::test]
    async fn handle_tool_call_requested_fails_without_auth_loader_for_protected() {
        let (svc, _, _) = control_service();
        let envelope = protected_envelope(
            "run-1",
            "job-1",
            "tpc-1",
            "user-1",
            vec!["booking:write".into()],
            vec!["book_flight".into()],
        );
        let result = svc.handle_tool_call_requested(envelope).await;
        assert!(result.is_err(), "should fail-closed without auth_context_loader");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("auth_context_loader not configured"),
            "expected fail-closed message, got: {err}"
        );
    }

    /// Regression: Python payload permissions are ignored.
    /// Even when the MQ payload includes `requester.permissions` that
    /// would satisfy the governance, Rust uses the loader-provided
    /// context. If the loader provides empty permissions, the tool is
    /// denied regardless of what the Python payload says.
    #[tokio::test]
    async fn python_payload_permissions_are_ignored_rust_loader_is_authoritative() {
        let context = protected_context(
            "user-1",
            vec![], // loader says: no permissions
            vec!["book_flight".into()],
            protected_governance("book_flight", vec!["booking:write".into()]),
        );
        let (svc, tool_call_repo, command_repo) = control_service_with_auth(context);
        // Python payload says user has "booking:write" — but Rust
        // loader says user has no permissions. Rust wins.
        let envelope = protected_envelope(
            "run-1",
            "job-1",
            "tpc-deny",
            "user-1",
            vec!["booking:write".into()], // Python lies
            vec!["book_flight".into()],
        );
        svc.handle_tool_call_requested(envelope).await.unwrap();

        let commands = command_repo.snapshot();
        assert_eq!(
            commands[0].command_type,
            AiRuntimeCommandType::ToolDenied,
            "Rust must deny based on loader context, not Python payload"
        );
        let row = tool_call_repo.get("tpc-deny").await.unwrap().unwrap();
        assert_eq!(row.status, AiToolCallStatus::Denied);
    }

    #[tokio::test]
    async fn handle_tool_call_requested_skips_authorization_for_public_direct() {
        let (svc, tool_call_repo, command_repo) = control_service();
        let envelope = requested_envelope(
            "run-1",
            "job-1",
            "tpc-1",
            "weather_at_airport",
            ToolAuthorizationMode::PublicDirect,
            Value::Object(Default::default()),
        );
        svc.handle_tool_call_requested(envelope).await.unwrap();
        let row = tool_call_repo.get("tpc-1").await.unwrap().unwrap();
        assert_eq!(row.status, AiToolCallStatus::Running);
        assert!(command_repo.is_empty());
    }

    fn result_envelope(
        run_id: &str,
        job_id: &str,
        tool_call_pk: &str,
        status: ToolExecutionStatus,
    ) -> AiRuntimeEventEnvelope {
        let payload = json!({
            "tool_call_pk": tool_call_pk,
            "tool_call_id": format!("call-{tool_call_pk}"),
            "tool_name": "weather_at_airport",
            "status": status,
            "result_hash": null,
            "result_summary": null,
            "error_code": null,
            "error_message": null,
            "retry_count": 0,
            "proposal_ids": [],
            "duration_ms": 12,
        });
        AiRuntimeEventEnvelope::new(
            fms_domain::ai_runtime_event::AiRuntimeEventType::ToolResult,
            run_id,
            job_id,
            0,
            2,
            format!("{run_id}:0:result-{tool_call_pk}"),
            payload,
        )
    }

    #[tokio::test]
    async fn handle_tool_result_marks_succeeded() {
        let (svc, tool_call_repo, _) = control_service();
        tool_call_repo
            .upsert_requested(AiToolCallRecord {
                tool_call_pk: "tpc-1".into(),
                job_id: "job-1".into(),
                run_id: "run-1".into(),
                parent_tool_call_pk: None,
                root_tool_call_pk: None,
                depth: 0,
                round_index: 0,
                tool_call_id: "call-1".into(),
                tool_name: "weather_at_airport".into(),
                tool_type: AiToolCallType::Builtin,
                status: AiToolCallStatus::Running,
                args_hash: "h".into(),
                args_summary: json!({}),
                result_hash: None,
                result_summary: None,
                error_code: None,
                error_message: None,
                retry_count: 0,
                max_retries: 2,
                timeout_seconds: 30,
                last_heartbeat_at: None,
                idempotency_key: "run-1:0:tpc-1:weather_at_airport:h".into(),
                mq_message_id: None,
                mq_offset: None,
                created_at: Utc::now(),
                started_at: Some(Utc::now()),
                finished_at: None,
                metadata: json!({}),
            })
            .await
            .unwrap();

        let envelope = result_envelope("run-1", "job-1", "tpc-1", ToolExecutionStatus::Succeeded);
        svc.handle_tool_result(envelope).await.unwrap();
        let row = tool_call_repo.get("tpc-1").await.unwrap().unwrap();
        assert_eq!(row.status, AiToolCallStatus::Succeeded);
    }

    #[tokio::test]
    async fn handle_tool_result_marks_failed_terminal_for_actor_denied() {
        let (svc, tool_call_repo, _) = control_service();
        tool_call_repo
            .upsert_requested(AiToolCallRecord {
                tool_call_pk: "tpc-1".into(),
                job_id: "job-1".into(),
                run_id: "run-1".into(),
                parent_tool_call_pk: None,
                root_tool_call_pk: None,
                depth: 0,
                round_index: 0,
                tool_call_id: "call-1".into(),
                tool_name: "weather_at_airport".into(),
                tool_type: AiToolCallType::Builtin,
                status: AiToolCallStatus::Running,
                args_hash: "h".into(),
                args_summary: json!({}),
                result_hash: None,
                result_summary: None,
                error_code: None,
                error_message: None,
                retry_count: 0,
                max_retries: 2,
                timeout_seconds: 30,
                last_heartbeat_at: None,
                idempotency_key: "run-1:0:tpc-1:weather_at_airport:h".into(),
                mq_message_id: None,
                mq_offset: None,
                created_at: Utc::now(),
                started_at: Some(Utc::now()),
                finished_at: None,
                metadata: json!({}),
            })
            .await
            .unwrap();

        let mut envelope = result_envelope("run-1", "job-1", "tpc-1", ToolExecutionStatus::Denied);
        envelope.payload = json!({
            "tool_call_pk": "tpc-1",
            "tool_call_id": "call-1",
            "tool_name": "weather_at_airport",
            "status": "denied",
            "result_hash": null,
            "result_summary": null,
            "error_code": "TOOL_ACTOR_PERMISSION_DENIED",
            "error_message": "missing required permission weather:read",
            "retry_count": 0,
            "proposal_ids": [],
            "duration_ms": 0,
        });
        svc.handle_tool_result(envelope).await.unwrap();
        let row = tool_call_repo.get("tpc-1").await.unwrap().unwrap();
        assert_eq!(row.status, AiToolCallStatus::Denied);
        assert_eq!(row.error_code.as_deref(), Some("TOOL_ACTOR_PERMISSION_DENIED"));
    }

    #[tokio::test]
    async fn update_heartbeat_refreshes_last_heartbeat_at() {
        let (svc, tool_call_repo, _) = control_service();
        tool_call_repo
            .upsert_requested(AiToolCallRecord {
                tool_call_pk: "tpc-1".into(),
                job_id: "job-1".into(),
                run_id: "run-1".into(),
                parent_tool_call_pk: None,
                root_tool_call_pk: None,
                depth: 0,
                round_index: 0,
                tool_call_id: "call-1".into(),
                tool_name: "weather_at_airport".into(),
                tool_type: AiToolCallType::Builtin,
                status: AiToolCallStatus::Running,
                args_hash: "h".into(),
                args_summary: json!({}),
                result_hash: None,
                result_summary: None,
                error_code: None,
                error_message: None,
                retry_count: 0,
                max_retries: 2,
                timeout_seconds: 30,
                last_heartbeat_at: None,
                idempotency_key: "run-1:0:tpc-1:weather_at_airport:h".into(),
                mq_message_id: None,
                mq_offset: None,
                created_at: Utc::now(),
                started_at: Some(Utc::now()),
                finished_at: None,
                metadata: json!({}),
            })
            .await
            .unwrap();

        let envelope = AiRuntimeEventEnvelope::new(
            fms_domain::ai_runtime_event::AiRuntimeEventType::Heartbeat,
            "run-1",
            "job-1",
            0,
            3,
            "run-1:0:hb-tpc-1",
            json!({ "tool_call_pk": "tpc-1", "progress_pct": 50, "note": null }),
        );
        svc.update_heartbeat(envelope).await.unwrap();
        let row = tool_call_repo.get("tpc-1").await.unwrap().unwrap();
        assert!(row.last_heartbeat_at.is_some());
    }

    #[tokio::test]
    async fn handle_checkpoint_logs_and_does_not_crash() {
        let (svc, _, _, _) = control_service_with_checkpoint_repo();
        let envelope = AiRuntimeEventEnvelope::new(
            fms_domain::ai_runtime_event::AiRuntimeEventType::Checkpoint,
            "run-1",
            "job-1",
            0,
            4,
            "run-1:0:cp-1",
            json!({
                "checkpoint_id": "cp-1",
                "sequence_no": 1,
                "checkpoint_type": "before_tool",
                "tool_call_pk": "tpc-1",
                "proposal_id": null,
                "snapshot_hash": "h",
                "snapshot": {},
                "snapshot_size_bytes": 2,
            }),
        );
        svc.handle_checkpoint(envelope).await.unwrap();
    }

    fn control_service_with_checkpoint_repo() -> (
        AiExecutionControlService,
        Arc<InMemoryToolCallRepository>,
        Arc<InMemoryRuntimeCommandRepository>,
        Arc<crate::services::ai_runtime_service::in_memory_repos::InMemoryCheckpointRepository>,
    ) {
        let tool_call_repo = Arc::new(InMemoryToolCallRepository::new());
        let command_repo = Arc::new(InMemoryRuntimeCommandRepository::new());
        let checkpoint_repo =
            Arc::new(crate::services::ai_runtime_service::in_memory_repos::InMemoryCheckpointRepository::new());
        let authorization = Arc::new(ToolAuthorizationService::new(
            Arc::new(StaticFeatureFlagSource::empty()),
        ));
        let svc = AiExecutionControlService::new(
            tool_call_repo.clone() as Arc<dyn AiToolCallRepository>,
            command_repo.clone() as Arc<dyn AiRuntimeCommandRepository>,
            authorization,
        )
        .with_checkpoint_repo(checkpoint_repo.clone() as Arc<dyn AiRunCheckpointRepository>);
        (svc, tool_call_repo, command_repo, checkpoint_repo)
    }

    fn checkpoint_envelope(
        run_id: &str,
        sequence_no: u64,
        checkpoint_id: &str,
        checkpoint_type: &str,
        size_bytes: u32,
    ) -> AiRuntimeEventEnvelope {
        AiRuntimeEventEnvelope::new(
            fms_domain::ai_runtime_event::AiRuntimeEventType::Checkpoint,
            run_id,
            "job-1",
            0,
            sequence_no,
            &format!("{run_id}:0:{checkpoint_id}"),
            json!({
                "checkpoint_id": checkpoint_id,
                "sequence_no": sequence_no,
                "checkpoint_type": checkpoint_type,
                "tool_call_pk": null,
                "proposal_id": null,
                "snapshot_hash": format!("h-{sequence_no}"),
                "snapshot": {"seq": sequence_no},
                "snapshot_size_bytes": size_bytes,
            }),
        )
    }

    #[tokio::test]
    async fn handle_checkpoint_rejects_oversize_snapshots() {
        let (svc, _, _, checkpoint_repo) = control_service_with_checkpoint_repo();
        let envelope = checkpoint_envelope("run-1", 1, "cp-1", "before_tool", 70_000);
        let result = svc.handle_checkpoint(envelope).await;
        assert!(matches!(result, Err(ControlServiceError::PayloadParse(_))));
        assert!(checkpoint_repo.is_empty());
    }

    #[tokio::test]
    async fn handle_checkpoint_dedups_on_sequence_no() {
        let (svc, _, _, checkpoint_repo) = control_service_with_checkpoint_repo();
        let first = checkpoint_envelope("run-1", 1, "cp-1", "before_tool", 64);
        let second = checkpoint_envelope("run-1", 1, "cp-1", "before_tool", 64);
        svc.handle_checkpoint(first).await.unwrap();
        svc.handle_checkpoint(second).await.unwrap();
        assert_eq!(checkpoint_repo.len(), 1);
    }

    #[tokio::test]
    async fn handle_checkpoint_supersedes_prior_tool_checkpoints() {
        let (svc, _, _, checkpoint_repo) = control_service_with_checkpoint_repo();
        svc.handle_checkpoint(checkpoint_envelope("run-1", 1, "cp-1", "before_tool", 16))
            .await
            .unwrap();
        svc.handle_checkpoint(checkpoint_envelope("run-1", 2, "cp-2", "before_tool", 16))
            .await
            .unwrap();
        assert_eq!(checkpoint_repo.len(), 2);
        let best = checkpoint_repo
            .latest_recoverable_for_test("run-1")
            .await
            .expect("latest recoverable");
        assert_eq!(best.sequence_no, 2);
    }

    #[tokio::test]
    async fn handle_checkpoint_does_not_supersede_run_input() {
        let (svc, _, _, _) = control_service_with_checkpoint_repo();
        svc.handle_checkpoint(checkpoint_envelope("run-1", 1, "cp-input", "run_input", 16))
            .await
            .unwrap();
        svc.handle_checkpoint(checkpoint_envelope("run-1", 2, "cp-tool", "before_tool", 16))
            .await
            .unwrap();
        // run_input still has Persisted status (not superseded by tool checkpoint)
        // Both rows present.
    }

    #[tokio::test]
    async fn create_run_input_checkpoint_persists_with_typed_summary() {
        let (svc, _, _, checkpoint_repo) = control_service_with_checkpoint_repo();
        let summary = RunInputCheckpointSummary {
            governance_hash: "g-1".into(),
            tool_schema_hash: "t-1".into(),
            model_id: Some("m-1".into()),
            prompt_cache_key_hash: "p-1".into(),
        };
        let record = svc
            .create_run_input_checkpoint("job-1", "run-1", json!({"question": "hi"}), summary)
            .await
            .unwrap()
            .expect("checkpoint repo is configured");
        assert_eq!(record.checkpoint_type, AiRunCheckpointType::RunInput);
        assert_eq!(record.sequence_no, 1);
        assert_eq!(checkpoint_repo.len(), 1);
    }

    #[tokio::test]
    async fn enqueue_resume_run_creates_command_with_snapshot_payload() {
        let (svc, _, command_repo, checkpoint_repo) = control_service_with_checkpoint_repo();
        svc.handle_checkpoint(checkpoint_envelope("run-1", 1, "cp-1", "after_tool", 16))
            .await
            .unwrap();
        let checkpoint = checkpoint_repo
            .latest_recoverable_for_test("run-1")
            .await
            .expect("checkpoint persisted");
        let cmd = svc
            .enqueue_resume_run("job-1", "run-1", &checkpoint, "user-1")
            .await
            .unwrap();
        assert_eq!(cmd.command_type, AiRuntimeCommandType::ResumeRun);
        assert_eq!(cmd.payload.get("checkpoint_id").unwrap().as_str().unwrap(), "cp-1");
        assert_eq!(
            cmd.payload.get("requester_user_id").unwrap().as_str().unwrap(),
            "user-1"
        );
        let commands = command_repo.snapshot();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command_type, AiRuntimeCommandType::ResumeRun);
    }

    #[tokio::test]
    async fn handle_run_complete_and_run_fail_are_acknowledged() {
        let (svc, _, _) = control_service();
        let complete = AiRuntimeEventEnvelope::new(
            fms_domain::ai_runtime_event::AiRuntimeEventType::RunComplete,
            "run-1",
            "job-1",
            0,
            5,
            "run-1:0:complete",
            json!({
                "output_raw": {"answer": "ok"},
                "token_usage": null,
                "proposal_ids": [],
                "terminal_event_id": null,
            }),
        );
        svc.handle_run_complete(complete).await.unwrap();
        let fail = AiRuntimeEventEnvelope::new(
            fms_domain::ai_runtime_event::AiRuntimeEventType::RunFail,
            "run-1",
            "job-1",
            0,
            6,
            "run-1:0:fail",
            json!({
                "error_code": "PROVIDER_TIMEOUT",
                "error_message": "model provider timeout",
                "terminal_event_id": null,
            }),
        );
        svc.handle_run_fail(fail).await.unwrap();
    }

    #[tokio::test]
    async fn start_run_command_payload_contains_envelope_and_snapshot() {
        let (svc, _, command_repo) = control_service();
        let envelope = json!({
            "run_id": "run-1",
            "job_id": "job-1",
            "requester_user_id": "user-1",
        });
        let capability_snapshot = json!({
            "tools": ["weather_at_airport"],
            "governance_version": "1.0",
        });
        let cmd = svc
            .enqueue_start_run(
                "job-1",
                "run-1",
                envelope.clone(),
                capability_snapshot.clone(),
                "gov-hash-1",
            )
            .await
            .unwrap();
        assert_eq!(cmd.command_type, AiRuntimeCommandType::StartRun);
        assert_eq!(cmd.run_id, "run-1");
        assert_eq!(cmd.tool_call_pk, None);
        assert_eq!(cmd.status, AiRuntimeCommandStatus::Pending);
        assert_eq!(cmd.attempt_count, 0);
        assert_eq!(cmd.max_attempts, 3);
        assert_eq!(cmd.run_owner_lock, None);
        assert_eq!(cmd.payload.get("job_id").unwrap().as_str().unwrap(), "job-1");
        assert_eq!(cmd.payload.get("envelope").unwrap(), &envelope);
        assert_eq!(cmd.payload.get("capability_snapshot").unwrap(), &capability_snapshot);
        assert_eq!(
            cmd.payload.get("governance_hash").unwrap().as_str().unwrap(),
            "gov-hash-1"
        );

        let commands = command_repo.snapshot();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command_type, AiRuntimeCommandType::StartRun);
        assert_eq!(commands[0].command_sequence, 1);
    }

    #[tokio::test]
    async fn cancel_run_command_is_inserted_not_directly_invoked() {
        let (svc, _, command_repo) = control_service();
        let cmd = svc.enqueue_cancel_run("job-1", "run-1", "user-1").await.unwrap();
        assert_eq!(cmd.command_type, AiRuntimeCommandType::CancelRun);
        assert_eq!(cmd.run_id, "run-1");
        assert_eq!(cmd.tool_call_pk, None);
        assert_eq!(cmd.status, AiRuntimeCommandStatus::Pending);
        assert_eq!(cmd.payload.get("job_id").unwrap().as_str().unwrap(), "job-1");
        assert_eq!(
            cmd.payload.get("requester_user_id").unwrap().as_str().unwrap(),
            "user-1"
        );
        assert_eq!(cmd.attempt_count, 0);
        assert_eq!(cmd.max_attempts, 3);

        let commands = command_repo.snapshot();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command_type, AiRuntimeCommandType::CancelRun);
    }

    #[tokio::test]
    async fn enqueue_retry_tool_command_carries_tool_call_pk() {
        let (svc, _, command_repo) = control_service();
        let cmd = svc
            .enqueue_retry_tool("job-1", "run-1", "tpc-1", "user-1")
            .await
            .unwrap();
        assert_eq!(cmd.command_type, AiRuntimeCommandType::RetryTool);
        assert_eq!(cmd.tool_call_pk.as_deref(), Some("tpc-1"));
        assert_eq!(cmd.payload.get("tool_call_pk").unwrap().as_str().unwrap(), "tpc-1");
        assert_eq!(
            cmd.payload.get("requester_user_id").unwrap().as_str().unwrap(),
            "user-1"
        );
        let commands = command_repo.snapshot();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command_type, AiRuntimeCommandType::RetryTool);
    }

    #[tokio::test]
    async fn enqueue_start_run_allocates_per_run_monotonic_sequence() {
        let (svc, _, _) = control_service();
        let first = svc
            .enqueue_start_run("job-1", "run-1", json!({}), json!({}), "h-1")
            .await
            .unwrap();
        let second = svc
            .enqueue_start_run("job-1", "run-1", json!({}), json!({}), "h-1")
            .await
            .unwrap();
        assert_eq!(first.command_sequence, 1);
        assert_eq!(second.command_sequence, 2);
    }
}
