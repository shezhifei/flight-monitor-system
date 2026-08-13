//! Repository ports for the AI execution control plane.
//!
//! Five ports back the durable control plane:
//! * [`AiToolCallRepository`] — the per-tool-call ledger
//!   (`ai_tool_calls`).
//! * [`AiRuntimeCommandRepository`] — the Rust -> Python command queue
//!   (`ai_runtime_commands`).
//! * [`AiRunCheckpointRepository`] — the per-run checkpoint store
//!   (`ai_run_checkpoints`).
//! * [`AiActionReceiptRepository`] — the per-domain-action receipt
//!   store (`ai_action_receipts`).
//! * [`AiCompensationPlanRepository`] — the rollback plan store
//!   (`ai_compensation_plans`).
//!
//! The command lease path must use `FOR UPDATE SKIP LOCKED` to support
//! concurrent workers; the trait signature captures the lease contract
//! without leaking the SQL into the domain layer.

use async_trait::async_trait;

use crate::models::ai_execution::{
    AiActionReceiptRecord, AiCompensationPlanRecord, AiCompensationStatus, AiRunCheckpointRecord,
    AiRuntimeCommandRecord, AiToolCallError, AiToolCallRecord, AiToolCallResult, CHECKPOINT_SNAPSHOT_SIZE_BUDGET_BYTES,
};

#[async_trait]
pub trait AiToolCallRepository: Send + Sync {
    /// Idempotent insert for a freshly requested tool call.
    ///
    /// Returns `Ok(true)` when a new row was inserted, `Ok(false)` when
    /// a row with the same `(run_id, idempotency_key)` already
    /// existed. The unique constraint is the source of truth for
    /// deduplication; implementations should translate the conflict
    /// into `Ok(false)` rather than an error.
    async fn upsert_requested(&self, record: AiToolCallRecord) -> Result<bool, AiExecutionRepositoryError>;

    async fn mark_authorized(&self, tool_call_pk: &str) -> Result<(), AiExecutionRepositoryError>;

    async fn mark_running(&self, tool_call_pk: &str) -> Result<(), AiExecutionRepositoryError>;

    async fn mark_succeeded(
        &self,
        tool_call_pk: &str,
        result: AiToolCallResult,
    ) -> Result<(), AiExecutionRepositoryError>;

    async fn mark_failed(&self, tool_call_pk: &str, error: AiToolCallError) -> Result<(), AiExecutionRepositoryError>;

    async fn mark_cancelled(&self, tool_call_pk: &str) -> Result<(), AiExecutionRepositoryError>;

    async fn mark_expired(&self, tool_call_pk: &str) -> Result<(), AiExecutionRepositoryError>;

    async fn mark_proposal_only(&self, tool_call_pk: &str) -> Result<(), AiExecutionRepositoryError>;

    async fn mark_denied(
        &self,
        tool_call_pk: &str,
        code: &str,
        message: &str,
    ) -> Result<(), AiExecutionRepositoryError>;

    async fn heartbeat(&self, tool_call_pk: &str) -> Result<(), AiExecutionRepositoryError>;

    async fn get(&self, tool_call_pk: &str) -> Result<Option<AiToolCallRecord>, AiExecutionRepositoryError>;

    async fn list_by_run(&self, run_id: &str) -> Result<Vec<AiToolCallRecord>, AiExecutionRepositoryError>;
}

#[async_trait]
pub trait AiRuntimeCommandRepository: Send + Sync {
    async fn enqueue(&self, command: AiRuntimeCommandRecord) -> Result<(), AiExecutionRepositoryError>;

    /// Lease up to `batch_size` pending commands for the given owner.
    ///
    /// Implementations must use `FOR UPDATE SKIP LOCKED` so concurrent
    /// workers never see the same row. The lease is recorded by
    /// setting `status = leased`, `lease_owner = owner` and
    /// `lease_expires_at = now() + lease_seconds`. The DB transition
    /// and the row read happen in a single transaction.
    async fn lease_pending(
        &self,
        owner: &str,
        lease_seconds: u32,
        batch_size: u32,
    ) -> Result<Vec<AiRuntimeCommandRecord>, AiExecutionRepositoryError>;

    /// Lease pending commands while respecting per-run ownership.
    ///
    /// A command is eligible when:
    /// * `status = 'pending'`, or
    /// * `status = 'leased'`, `lease_expires_at < now()`, and the caller
    ///   is allowed to take it over (the run lock is unset or the old
    ///   lease owner matches `owner`).
    ///
    /// Commands whose `run_owner_lock` is set to another active owner
    /// are skipped unless their lease has expired. When a command is
    /// leased, `attempt_count` is incremented and capped at
    /// `max_attempts`; commands that have reached `max_attempts` are
    /// transitioned to `failed` instead of being leased.
    async fn lease_pending_with_owner_check(
        &self,
        owner: &str,
        lease_seconds: u32,
        batch_size: u32,
    ) -> Result<Vec<AiRuntimeCommandRecord>, AiExecutionRepositoryError>;

    async fn complete(&self, command_id: &str) -> Result<(), AiExecutionRepositoryError>;

    async fn fail(&self, command_id: &str, error: &str) -> Result<(), AiExecutionRepositoryError>;

    async fn get(&self, command_id: &str) -> Result<Option<AiRuntimeCommandRecord>, AiExecutionRepositoryError>;

    /// Refresh the lease heartbeat timestamp for a command.
    /// Implementations should only update rows with `status = 'leased'`.
    async fn heartbeat_command(&self, command_id: &str) -> Result<(), AiExecutionRepositoryError>;

    /// Atomically take over ownership of a run whose previous
    /// worker has crashed or let its lease expire.
    ///
    /// Returns the `start_run` command for `run_id` after setting
    /// `run_owner_lock = new_owner` on all pending commands for that
    /// run and leasing the `start_run` command to `new_owner`. Returns
    /// `None` when no take-overable `start_run` command exists.
    async fn take_over_run(
        &self,
        run_id: &str,
        new_owner: &str,
        lease_seconds: u32,
    ) -> Result<Option<AiRuntimeCommandRecord>, AiExecutionRepositoryError>;

    /// List leased commands whose `lease_expires_at` is in the
    /// past and whose `attempt_count < max_attempts`. Used by the
    /// recovery orchestrator to find work that needs to be retried or
    /// taken over.
    async fn list_expired_leases(
        &self,
        now: chrono::DateTime<chrono::Utc>,
        limit: u32,
    ) -> Result<Vec<AiRuntimeCommandRecord>, AiExecutionRepositoryError>;
}

#[async_trait]
pub trait AiRunCheckpointRepository: Send + Sync {
    /// Idempotent insert for a checkpoint row.
    ///
    /// Returns `Ok(true)` when a new row was inserted and `Ok(false)`
    /// when `(run_id, sequence_no)` already exists. Callers use the
    /// boolean to dedup duplicate MQ events without raising an error.
    async fn upsert(&self, record: AiRunCheckpointRecord) -> Result<bool, AiExecutionRepositoryError>;

    /// All checkpoints for a run in `sequence_no ASC` order. Used by
    /// the resume API and the read API.
    async fn list_by_run(&self, run_id: &str) -> Result<Vec<AiRunCheckpointRecord>, AiExecutionRepositoryError>;

    /// The latest `BeforeTool` / `AfterTool` checkpoint that is still
    /// `Persisted` (not `Superseded` and not `Resumed`). The
    /// `latest_recoverable` row is the recovery target the resume
    /// API hands back when no `from_checkpoint_id` is supplied.
    async fn latest_recoverable(
        &self,
        run_id: &str,
    ) -> Result<Option<AiRunCheckpointRecord>, AiExecutionRepositoryError>;

    /// Mark every `Persisted` checkpoint with `sequence_no <
    /// before_sequence_no` for the given run as `Superseded`. Returns
    /// the number of rows updated. The new checkpoint is the active
    /// one going forward.
    async fn mark_superseded(&self, run_id: &str, before_sequence_no: u64) -> Result<u64, AiExecutionRepositoryError>;
}

#[async_trait]
pub trait AiActionReceiptRepository: Send + Sync {
    /// Insert or fetch the receipt keyed by `idempotency_key`. Returns
    /// `Ok(true)` when a new row was inserted, `Ok(false)` when an
    /// existing row matched the idempotency key. Receipts are
    /// append-only; upsert on conflict keeps the original `receipt_id`
    /// and never overwrites audit fields.
    async fn upsert(&self, receipt: AiActionReceiptRecord) -> Result<bool, AiExecutionRepositoryError>;

    async fn get_by_idempotency_key(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<AiActionReceiptRecord>, AiExecutionRepositoryError>;

    async fn get(&self, receipt_id: &str) -> Result<Option<AiActionReceiptRecord>, AiExecutionRepositoryError>;

    async fn list_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Vec<AiActionReceiptRecord>, AiExecutionRepositoryError>;

    async fn list_by_run(&self, run_id: &str) -> Result<Vec<AiActionReceiptRecord>, AiExecutionRepositoryError>;
}

#[async_trait]
pub trait AiCompensationPlanRepository: Send + Sync {
    /// Insert or update the plan. The unique `(receipt_id, mode)`
    /// constraint is the source of truth for deduplication. When a
    /// plan with the same pair already exists the implementation
    /// returns `Ok(false)` and the caller treats the operation as a
    /// no-op (the original plan is preserved).
    async fn upsert(&self, plan: AiCompensationPlanRecord) -> Result<bool, AiExecutionRepositoryError>;

    async fn get(&self, compensation_id: &str) -> Result<Option<AiCompensationPlanRecord>, AiExecutionRepositoryError>;

    async fn list_by_receipt(
        &self,
        receipt_id: &str,
    ) -> Result<Vec<AiCompensationPlanRecord>, AiExecutionRepositoryError>;

    async fn list_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Vec<AiCompensationPlanRecord>, AiExecutionRepositoryError>;

    /// Atomically transition `Planned` / `Approved` to `Executing`
    /// and stamp `executed_by`. The Postgres implementation MUST use
    /// `SELECT ... FOR UPDATE SKIP LOCKED` so concurrent compensation
    /// workers cannot double-execute the same plan. Returns `Ok(true)`
    /// when the plan was claimed, `Ok(false)` when the plan was
    /// already in a terminal state or held by another worker.
    async fn mark_executing(
        &self,
        compensation_id: &str,
        executed_by: &str,
    ) -> Result<bool, AiExecutionRepositoryError>;

    async fn mark_succeeded(
        &self,
        compensation_id: &str,
        executed_by: &str,
        result: serde_json::Value,
    ) -> Result<(), AiExecutionRepositoryError>;

    async fn mark_failed(&self, compensation_id: &str, error: &str) -> Result<(), AiExecutionRepositoryError>;

    /// Find all plans currently in `Planned` status that do not
    /// require approval and are older than `older_than` (used by the
    /// compensation auto-execute scheduler).
    async fn list_pending_approval(
        &self,
        older_than_seconds: i64,
    ) -> Result<Vec<AiCompensationPlanRecord>, AiExecutionRepositoryError>;

    /// Find all plans currently in `Executing` status whose
    /// `updated_at` is older than `timeout_seconds` (used by the
    /// compensation timeout scanner).
    async fn list_executing_past_timeout(
        &self,
        timeout_seconds: i64,
    ) -> Result<Vec<AiCompensationPlanRecord>, AiExecutionRepositoryError>;

    /// Filter helper used by tests / recovery to enumerate plans in a
    /// given status (the production scheduler uses the more specific
    /// `list_pending_approval` / `list_executing_past_timeout` paths).
    async fn list_by_status(
        &self,
        status: AiCompensationStatus,
    ) -> Result<Vec<AiCompensationPlanRecord>, AiExecutionRepositoryError>;
}

/// Hard cap on a checkpoint snapshot's serialized size. The Rust
/// consumer rejects any payload that exceeds the budget before
/// touching the `ai_run_checkpoints` table. Exceeding the cap is a
/// poison-message condition (the producer must resend with a smaller
/// snapshot); the MQ consumer logs + acks the offending message to
/// avoid an infinite retry loop.
pub fn assert_checkpoint_size_within_budget(size_bytes: u32) -> Result<(), AiExecutionRepositoryError> {
    if size_bytes > CHECKPOINT_SNAPSHOT_SIZE_BUDGET_BYTES {
        return Err(AiExecutionRepositoryError::CheckpointTooLarge {
            size_bytes,
            budget_bytes: CHECKPOINT_SNAPSHOT_SIZE_BUDGET_BYTES,
        });
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub enum AiExecutionRepositoryError {
    /// A row referenced by the caller does not exist. Callers can
    /// treat this as a no-op for status transitions (idempotent).
    NotFound(String),
    /// Storage / SQL level failure.
    Database(String),
    /// Domain shape violation, e.g. invalid status transition.
    Validation(String),
    /// Checkpoint snapshot exceeded the 64 KB budget. The MQ
    /// consumer treats this as a poison message (log + ack) so the
    /// producer must resend with a smaller snapshot.
    CheckpointTooLarge { size_bytes: u32, budget_bytes: u32 },
    /// The object's optimistic-lock version drifted between the
    /// original action and the rollback. The plan/receipt was
    /// preserved; the caller should offer a correction proposal
    /// instead of overwriting the current state.
    ObjectVersionConflict {
        object_type: String,
        object_id: String,
        expected_version: i64,
        actual_version: i64,
    },
}

impl AiExecutionRepositoryError {
    pub fn not_found(id: impl Into<String>) -> Self {
        Self::NotFound(id.into())
    }

    pub fn database(message: impl Into<String>) -> Self {
        Self::Database(message.into())
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }

    pub fn checkpoint_too_large(size_bytes: u32, budget_bytes: u32) -> Self {
        Self::CheckpointTooLarge {
            size_bytes,
            budget_bytes,
        }
    }

    pub fn object_version_conflict(
        object_type: impl Into<String>,
        object_id: impl Into<String>,
        expected_version: i64,
        actual_version: i64,
    ) -> Self {
        Self::ObjectVersionConflict {
            object_type: object_type.into(),
            object_id: object_id.into(),
            expected_version,
            actual_version,
        }
    }
}

impl std::fmt::Display for AiExecutionRepositoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "ai execution record not found: {id}"),
            Self::Database(msg) => write!(f, "ai execution database error: {msg}"),
            Self::Validation(msg) => write!(f, "ai execution validation error: {msg}"),
            Self::CheckpointTooLarge {
                size_bytes,
                budget_bytes,
            } => write!(
                f,
                "checkpoint snapshot too large: {size_bytes} bytes exceeds budget {budget_bytes} bytes"
            ),
            Self::ObjectVersionConflict {
                object_type,
                object_id,
                expected_version,
                actual_version,
            } => write!(
                f,
                "object version conflict for {object_type} {object_id}: expected {expected_version}, got {actual_version}"
            ),
        }
    }
}

impl std::error::Error for AiExecutionRepositoryError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ai_execution::{
        AiRunCheckpointRecord, AiRunCheckpointStatus, AiRunCheckpointType, AiRuntimeCommandRecord,
        AiRuntimeCommandStatus, AiRuntimeCommandType, AiToolCallError, AiToolCallRecord, AiToolCallResult,
        AiToolCallStatus, AiToolCallType, CHECKPOINT_SNAPSHOT_SIZE_BUDGET_BYTES,
    };
    use chrono::Utc;
    use serde_json::json;
    use std::sync::Mutex;

    fn sample_tool_call(tool_call_pk: &str) -> AiToolCallRecord {
        AiToolCallRecord {
            tool_call_pk: tool_call_pk.into(),
            job_id: "job-1".into(),
            run_id: "run-1".into(),
            parent_tool_call_pk: None,
            root_tool_call_pk: None,
            depth: 0,
            round_index: 0,
            tool_call_id: "call-1".into(),
            tool_name: "weather_at_airport".into(),
            tool_type: AiToolCallType::Builtin,
            status: AiToolCallStatus::Requested,
            args_hash: "hash-1".into(),
            args_summary: json!({"airport_code": "PEK"}),
            result_hash: None,
            result_summary: None,
            error_code: None,
            error_message: None,
            retry_count: 0,
            max_retries: 2,
            timeout_seconds: 30,
            last_heartbeat_at: None,
            idempotency_key: format!("run-1:0:{}:weather_at_airport:hash-1", tool_call_pk),
            mq_message_id: None,
            mq_offset: None,
            created_at: Utc::now(),
            started_at: None,
            finished_at: None,
            metadata: json!({}),
        }
    }

    fn sample_command(command_id: &str) -> AiRuntimeCommandRecord {
        AiRuntimeCommandRecord {
            command_id: command_id.into(),
            run_id: "run-1".into(),
            command_type: AiRuntimeCommandType::StartRun,
            command_sequence: 1,
            tool_call_pk: None,
            payload: json!({}),
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
        }
    }

    /// In-memory mock used to demonstrate that the trait surface is
    /// implementable. Other code can pattern-match on this shape to
    /// drive the application service tests until the Postgres adapter
    /// lands.
    #[derive(Default)]
    struct InMemoryToolCallRepo {
        rows: Mutex<Vec<AiToolCallRecord>>,
    }

    impl InMemoryToolCallRepo {
        fn find_mut(
            &self,
            tool_call_pk: &str,
        ) -> Result<std::sync::MutexGuard<'_, Vec<AiToolCallRecord>>, AiExecutionRepositoryError> {
            let guard = self.rows.lock().unwrap();
            if guard.iter().any(|r| r.tool_call_pk == tool_call_pk) {
                Ok(guard)
            } else {
                Err(AiExecutionRepositoryError::not_found(tool_call_pk))
            }
        }
    }

    #[async_trait]
    impl AiToolCallRepository for InMemoryToolCallRepo {
        async fn upsert_requested(&self, record: AiToolCallRecord) -> Result<bool, AiExecutionRepositoryError> {
            let mut rows = self.rows.lock().unwrap();
            if rows.iter().any(|r| r.idempotency_key == record.idempotency_key) {
                return Ok(false);
            }
            rows.push(record);
            Ok(true)
        }

        async fn mark_authorized(&self, tool_call_pk: &str) -> Result<(), AiExecutionRepositoryError> {
            let mut rows = self.find_mut(tool_call_pk)?;
            let row = rows.iter_mut().find(|r| r.tool_call_pk == tool_call_pk).unwrap();
            row.status = AiToolCallStatus::Authorized;
            row.started_at.get_or_insert(Utc::now());
            Ok(())
        }

        async fn mark_running(&self, tool_call_pk: &str) -> Result<(), AiExecutionRepositoryError> {
            let mut rows = self.find_mut(tool_call_pk)?;
            let row = rows.iter_mut().find(|r| r.tool_call_pk == tool_call_pk).unwrap();
            row.status = AiToolCallStatus::Running;
            Ok(())
        }

        async fn mark_succeeded(
            &self,
            tool_call_pk: &str,
            result: AiToolCallResult,
        ) -> Result<(), AiExecutionRepositoryError> {
            let mut rows = self.find_mut(tool_call_pk)?;
            let row = rows.iter_mut().find(|r| r.tool_call_pk == tool_call_pk).unwrap();
            row.status = AiToolCallStatus::Succeeded;
            row.result_hash = result.result_hash;
            row.result_summary = result.result_summary;
            row.finished_at = Some(Utc::now());
            Ok(())
        }

        async fn mark_failed(
            &self,
            tool_call_pk: &str,
            error: AiToolCallError,
        ) -> Result<(), AiExecutionRepositoryError> {
            let mut rows = self.find_mut(tool_call_pk)?;
            let row = rows.iter_mut().find(|r| r.tool_call_pk == tool_call_pk).unwrap();
            row.status = if error.retryable {
                AiToolCallStatus::FailedRetryable
            } else {
                AiToolCallStatus::FailedTerminal
            };
            row.error_code = Some(error.code);
            row.error_message = Some(error.message);
            row.finished_at = Some(Utc::now());
            Ok(())
        }

        async fn mark_cancelled(&self, tool_call_pk: &str) -> Result<(), AiExecutionRepositoryError> {
            let mut rows = self.find_mut(tool_call_pk)?;
            let row = rows.iter_mut().find(|r| r.tool_call_pk == tool_call_pk).unwrap();
            row.status = AiToolCallStatus::Cancelled;
            row.finished_at = Some(Utc::now());
            Ok(())
        }

        async fn mark_expired(&self, tool_call_pk: &str) -> Result<(), AiExecutionRepositoryError> {
            let mut rows = self.find_mut(tool_call_pk)?;
            let row = rows.iter_mut().find(|r| r.tool_call_pk == tool_call_pk).unwrap();
            row.status = AiToolCallStatus::Expired;
            row.finished_at = Some(Utc::now());
            Ok(())
        }

        async fn mark_proposal_only(&self, tool_call_pk: &str) -> Result<(), AiExecutionRepositoryError> {
            let mut rows = self.find_mut(tool_call_pk)?;
            let row = rows.iter_mut().find(|r| r.tool_call_pk == tool_call_pk).unwrap();
            row.status = AiToolCallStatus::ProposalOnly;
            row.finished_at = Some(Utc::now());
            Ok(())
        }

        async fn mark_denied(
            &self,
            tool_call_pk: &str,
            code: &str,
            message: &str,
        ) -> Result<(), AiExecutionRepositoryError> {
            let mut rows = self.find_mut(tool_call_pk)?;
            let row = rows.iter_mut().find(|r| r.tool_call_pk == tool_call_pk).unwrap();
            row.status = AiToolCallStatus::Denied;
            row.error_code = Some(code.into());
            row.error_message = Some(message.into());
            row.finished_at = Some(Utc::now());
            Ok(())
        }

        async fn heartbeat(&self, tool_call_pk: &str) -> Result<(), AiExecutionRepositoryError> {
            let mut rows = self.find_mut(tool_call_pk)?;
            let row = rows.iter_mut().find(|r| r.tool_call_pk == tool_call_pk).unwrap();
            row.status = AiToolCallStatus::Running;
            row.last_heartbeat_at = Some(Utc::now());
            Ok(())
        }

        async fn get(&self, tool_call_pk: &str) -> Result<Option<AiToolCallRecord>, AiExecutionRepositoryError> {
            let rows = self.rows.lock().unwrap();
            Ok(rows.iter().find(|r| r.tool_call_pk == tool_call_pk).cloned())
        }

        async fn list_by_run(&self, run_id: &str) -> Result<Vec<AiToolCallRecord>, AiExecutionRepositoryError> {
            let rows = self.rows.lock().unwrap();
            Ok(rows.iter().filter(|r| r.run_id == run_id).cloned().collect())
        }
    }

    #[derive(Default)]
    struct InMemoryCommandRepo {
        rows: Mutex<Vec<AiRuntimeCommandRecord>>,
    }

    #[async_trait]
    impl AiRuntimeCommandRepository for InMemoryCommandRepo {
        async fn enqueue(&self, command: AiRuntimeCommandRecord) -> Result<(), AiExecutionRepositoryError> {
            self.rows.lock().unwrap().push(command);
            Ok(())
        }

        async fn lease_pending(
            &self,
            owner: &str,
            lease_seconds: u32,
            batch_size: u32,
        ) -> Result<Vec<AiRuntimeCommandRecord>, AiExecutionRepositoryError> {
            let mut rows = self.rows.lock().unwrap();
            let now = Utc::now();
            let mut leased = Vec::new();
            for row in rows.iter_mut() {
                if leased.len() as u32 >= batch_size {
                    break;
                }
                if row.status != AiRuntimeCommandStatus::Pending {
                    continue;
                }
                row.status = AiRuntimeCommandStatus::Leased;
                row.lease_owner = Some(owner.into());
                row.lease_expires_at = Some(now + chrono::Duration::seconds(lease_seconds as i64));
                leased.push(row.clone());
            }
            Ok(leased)
        }

        async fn complete(&self, command_id: &str) -> Result<(), AiExecutionRepositoryError> {
            let mut rows = self.rows.lock().unwrap();
            let row = rows
                .iter_mut()
                .find(|r| r.command_id == command_id)
                .ok_or_else(|| AiExecutionRepositoryError::not_found(command_id))?;
            row.status = AiRuntimeCommandStatus::Completed;
            row.processed_at = Some(Utc::now());
            Ok(())
        }

        async fn fail(&self, command_id: &str, _error: &str) -> Result<(), AiExecutionRepositoryError> {
            let mut rows = self.rows.lock().unwrap();
            let row = rows
                .iter_mut()
                .find(|r| r.command_id == command_id)
                .ok_or_else(|| AiExecutionRepositoryError::not_found(command_id))?;
            row.status = AiRuntimeCommandStatus::Failed;
            row.processed_at = Some(Utc::now());
            Ok(())
        }

        async fn get(&self, command_id: &str) -> Result<Option<AiRuntimeCommandRecord>, AiExecutionRepositoryError> {
            let rows = self.rows.lock().unwrap();
            Ok(rows.iter().find(|r| r.command_id == command_id).cloned())
        }

        async fn lease_pending_with_owner_check(
            &self,
            owner: &str,
            lease_seconds: u32,
            batch_size: u32,
        ) -> Result<Vec<AiRuntimeCommandRecord>, AiExecutionRepositoryError> {
            let mut rows = self.rows.lock().unwrap();
            let now = Utc::now();
            let mut leased = Vec::new();
            for row in rows.iter_mut() {
                if leased.len() as u32 >= batch_size {
                    break;
                }
                let is_pending = row.status == AiRuntimeCommandStatus::Pending;
                let is_expired_lease = row.status == AiRuntimeCommandStatus::Leased
                    && row.lease_expires_at.map(|exp| exp < now).unwrap_or(false);
                if !is_pending && !is_expired_lease {
                    continue;
                }
                if let Some(lock) = row.run_owner_lock.as_ref() {
                    if lock != owner && !is_expired_lease {
                        continue;
                    }
                }
                if row.attempt_count >= row.max_attempts {
                    row.status = AiRuntimeCommandStatus::Failed;
                    row.processed_at = Some(now);
                    continue;
                }
                row.attempt_count += 1;
                row.status = AiRuntimeCommandStatus::Leased;
                row.lease_owner = Some(owner.into());
                row.lease_expires_at = Some(now + chrono::Duration::seconds(lease_seconds as i64));
                row.last_heartbeat_at = Some(now);
                if row.run_owner_lock.is_none() {
                    row.run_owner_lock = Some(owner.into());
                }
                leased.push(row.clone());
            }
            Ok(leased)
        }

        async fn heartbeat_command(&self, command_id: &str) -> Result<(), AiExecutionRepositoryError> {
            let mut rows = self.rows.lock().unwrap();
            let row = rows
                .iter_mut()
                .find(|r| r.command_id == command_id)
                .ok_or_else(|| AiExecutionRepositoryError::not_found(command_id))?;
            if row.status != AiRuntimeCommandStatus::Leased {
                return Err(AiExecutionRepositoryError::validation(format!(
                    "command {} is not leased",
                    command_id
                )));
            }
            row.last_heartbeat_at = Some(Utc::now());
            Ok(())
        }

        async fn take_over_run(
            &self,
            run_id: &str,
            new_owner: &str,
            lease_seconds: u32,
        ) -> Result<Option<AiRuntimeCommandRecord>, AiExecutionRepositoryError> {
            let mut rows = self.rows.lock().unwrap();
            let now = Utc::now();
            for row in rows.iter_mut() {
                if row.run_id != run_id {
                    continue;
                }
                if row.status == AiRuntimeCommandStatus::Leased || row.status == AiRuntimeCommandStatus::Pending {
                    row.run_owner_lock = Some(new_owner.into());
                }
            }
            for row in rows.iter_mut() {
                if row.run_id != run_id {
                    continue;
                }
                if row.command_type == AiRuntimeCommandType::StartRun
                    && (row.status == AiRuntimeCommandStatus::Pending || row.status == AiRuntimeCommandStatus::Leased)
                {
                    row.attempt_count += 1;
                    row.status = AiRuntimeCommandStatus::Leased;
                    row.lease_owner = Some(new_owner.into());
                    row.lease_expires_at = Some(now + chrono::Duration::seconds(lease_seconds as i64));
                    row.last_heartbeat_at = Some(now);
                    row.run_owner_lock = Some(new_owner.into());
                    return Ok(Some(row.clone()));
                }
            }
            Ok(None)
        }

        async fn list_expired_leases(
            &self,
            now: chrono::DateTime<chrono::Utc>,
            limit: u32,
        ) -> Result<Vec<AiRuntimeCommandRecord>, AiExecutionRepositoryError> {
            let rows = self.rows.lock().unwrap();
            let mut out: Vec<AiRuntimeCommandRecord> = rows
                .iter()
                .filter(|r| {
                    r.status == AiRuntimeCommandStatus::Leased
                        && r.lease_expires_at.map(|exp| exp < now).unwrap_or(false)
                        && r.attempt_count < r.max_attempts
                })
                .cloned()
                .collect();
            out.truncate(limit as usize);
            Ok(out)
        }
    }

    #[tokio::test]
    async fn upsert_requested_is_idempotent_on_idempotency_key() {
        let repo = InMemoryToolCallRepo::default();
        let first = repo.upsert_requested(sample_tool_call("tpc-1")).await.unwrap();
        let second = repo.upsert_requested(sample_tool_call("tpc-1")).await.unwrap();
        assert!(first);
        assert!(!second);
    }

    #[tokio::test]
    async fn status_transitions_round_trip_through_repo() {
        let repo = InMemoryToolCallRepo::default();
        repo.upsert_requested(sample_tool_call("tpc-1")).await.unwrap();
        repo.mark_authorized("tpc-1").await.unwrap();
        repo.mark_running("tpc-1").await.unwrap();
        repo.mark_succeeded(
            "tpc-1",
            AiToolCallResult {
                result_hash: Some("rh".into()),
                result_summary: Some(json!({"ok": true})),
                proposal_ids: vec![],
                duration_ms: 12,
            },
        )
        .await
        .unwrap();
        let row = repo.get("tpc-1").await.unwrap().unwrap();
        assert_eq!(row.status, AiToolCallStatus::Succeeded);
        assert_eq!(row.result_hash.as_deref(), Some("rh"));
    }

    #[tokio::test]
    async fn mark_failed_uses_retryable_flag_to_pick_state() {
        let repo = InMemoryToolCallRepo::default();
        repo.upsert_requested(sample_tool_call("tpc-retry")).await.unwrap();
        repo.upsert_requested(sample_tool_call("tpc-term")).await.unwrap();
        repo.mark_failed(
            "tpc-retry",
            AiToolCallError {
                code: "TIMEOUT".into(),
                message: "x".into(),
                retryable: true,
            },
        )
        .await
        .unwrap();
        repo.mark_failed(
            "tpc-term",
            AiToolCallError {
                code: "SCHEMA".into(),
                message: "x".into(),
                retryable: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            repo.get("tpc-retry").await.unwrap().unwrap().status,
            AiToolCallStatus::FailedRetryable
        );
        assert_eq!(
            repo.get("tpc-term").await.unwrap().unwrap().status,
            AiToolCallStatus::FailedTerminal
        );
    }

    #[tokio::test]
    async fn command_enqueue_and_lease_pending() {
        let repo = InMemoryCommandRepo::default();
        repo.enqueue(sample_command("c-1")).await.unwrap();
        repo.enqueue(sample_command("c-2")).await.unwrap();

        let leased = repo.lease_pending("worker-a", 30, 10).await.unwrap();
        assert_eq!(leased.len(), 2);
        assert!(leased.iter().all(|c| c.status == AiRuntimeCommandStatus::Leased));
        assert!(leased.iter().all(|c| c.lease_owner.as_deref() == Some("worker-a")));

        // No pending rows left.
        let again = repo.lease_pending("worker-a", 30, 10).await.unwrap();
        assert!(again.is_empty());

        repo.complete("c-1").await.unwrap();
        let row = repo.get("c-1").await.unwrap().unwrap();
        assert_eq!(row.status, AiRuntimeCommandStatus::Completed);
        assert!(row.processed_at.is_some());

        repo.fail("c-2", "boom").await.unwrap();
        let row = repo.get("c-2").await.unwrap().unwrap();
        assert_eq!(row.status, AiRuntimeCommandStatus::Failed);
    }

    #[test]
    fn error_display_strings_are_stable() {
        assert_eq!(
            AiExecutionRepositoryError::not_found("tpc-1").to_string(),
            "ai execution record not found: tpc-1"
        );
        assert_eq!(
            AiExecutionRepositoryError::database("boom").to_string(),
            "ai execution database error: boom"
        );
        assert_eq!(
            AiExecutionRepositoryError::validation("bad").to_string(),
            "ai execution validation error: bad"
        );
        assert_eq!(
            AiExecutionRepositoryError::checkpoint_too_large(100, 64).to_string(),
            "checkpoint snapshot too large: 100 bytes exceeds budget 64 bytes"
        );
    }

    #[test]
    fn checkpoint_size_guard_accepts_under_budget() {
        assert!(assert_checkpoint_size_within_budget(0).is_ok());
        assert!(assert_checkpoint_size_within_budget(CHECKPOINT_SNAPSHOT_SIZE_BUDGET_BYTES).is_ok());
    }

    #[test]
    fn checkpoint_size_guard_rejects_oversize() {
        let err = assert_checkpoint_size_within_budget(CHECKPOINT_SNAPSHOT_SIZE_BUDGET_BYTES + 1)
            .expect_err("oversize must fail");
        assert!(matches!(err, AiExecutionRepositoryError::CheckpointTooLarge { .. }));
    }

    fn sample_checkpoint(seq: u64, kind: AiRunCheckpointType) -> AiRunCheckpointRecord {
        AiRunCheckpointRecord {
            checkpoint_id: format!("cp-{seq}"),
            job_id: "job-1".into(),
            run_id: "run-1".into(),
            sequence_no: seq as i64,
            checkpoint_type: kind,
            tool_call_pk: None,
            proposal_id: None,
            snapshot_hash: format!("h-{seq}"),
            snapshot: json!({"seq": seq}),
            snapshot_size_bytes: 16,
            mq_message_id: None,
            created_at: Utc::now(),
        }
    }

    #[derive(Default)]
    struct InMemoryCheckpointRepo {
        rows: Mutex<Vec<AiRunCheckpointRecord>>,
        statuses: Mutex<std::collections::HashMap<String, AiRunCheckpointStatus>>,
    }

    #[async_trait]
    impl AiRunCheckpointRepository for InMemoryCheckpointRepo {
        async fn upsert(&self, record: AiRunCheckpointRecord) -> Result<bool, AiExecutionRepositoryError> {
            let mut rows = self.rows.lock().unwrap();
            if rows
                .iter()
                .any(|r| r.run_id == record.run_id && r.sequence_no == record.sequence_no)
            {
                return Ok(false);
            }
            let key = format!("{}:{}", record.run_id, record.sequence_no);
            self.statuses
                .lock()
                .unwrap()
                .insert(key, AiRunCheckpointStatus::Persisted);
            rows.push(record);
            Ok(true)
        }

        async fn list_by_run(&self, run_id: &str) -> Result<Vec<AiRunCheckpointRecord>, AiExecutionRepositoryError> {
            let rows = self.rows.lock().unwrap();
            let mut out: Vec<_> = rows.iter().filter(|r| r.run_id == run_id).cloned().collect();
            out.sort_by_key(|r| r.sequence_no);
            Ok(out)
        }

        async fn latest_recoverable(
            &self,
            run_id: &str,
        ) -> Result<Option<AiRunCheckpointRecord>, AiExecutionRepositoryError> {
            let rows = self.rows.lock().unwrap();
            let statuses = self.statuses.lock().unwrap();
            let mut best: Option<AiRunCheckpointRecord> = None;
            for row in rows.iter() {
                if row.run_id != run_id {
                    continue;
                }
                if !matches!(
                    row.checkpoint_type,
                    AiRunCheckpointType::BeforeTool | AiRunCheckpointType::AfterTool
                ) {
                    continue;
                }
                let status = statuses
                    .get(&format!("{}:{}", row.run_id, row.sequence_no))
                    .copied()
                    .unwrap_or(AiRunCheckpointStatus::Persisted);
                if status != AiRunCheckpointStatus::Persisted {
                    continue;
                }
                if best
                    .as_ref()
                    .map(|prev| row.sequence_no > prev.sequence_no)
                    .unwrap_or(true)
                {
                    best = Some(row.clone());
                }
            }
            Ok(best)
        }

        async fn mark_superseded(
            &self,
            run_id: &str,
            before_sequence_no: u64,
        ) -> Result<u64, AiExecutionRepositoryError> {
            let mut count = 0u64;
            let mut statuses = self.statuses.lock().unwrap();
            for row in self.rows.lock().unwrap().iter() {
                if row.run_id == run_id
                    && (row.sequence_no as u64) < before_sequence_no
                    && matches!(
                        row.checkpoint_type,
                        AiRunCheckpointType::BeforeTool | AiRunCheckpointType::AfterTool
                    )
                {
                    statuses.insert(
                        format!("{}:{}", row.run_id, row.sequence_no),
                        AiRunCheckpointStatus::Superseded,
                    );
                    count += 1;
                }
            }
            Ok(count)
        }
    }

    #[tokio::test]
    async fn checkpoint_upsert_is_idempotent_on_sequence() {
        let repo = InMemoryCheckpointRepo::default();
        let first = repo
            .upsert(sample_checkpoint(1, AiRunCheckpointType::BeforeTool))
            .await
            .unwrap();
        let second = repo
            .upsert(sample_checkpoint(1, AiRunCheckpointType::BeforeTool))
            .await
            .unwrap();
        assert!(first);
        assert!(!second);
    }

    #[tokio::test]
    async fn checkpoint_latest_recoverable_picks_highest_sequence() {
        let repo = InMemoryCheckpointRepo::default();
        repo.upsert(sample_checkpoint(1, AiRunCheckpointType::RunInput))
            .await
            .unwrap();
        repo.upsert(sample_checkpoint(2, AiRunCheckpointType::BeforeTool))
            .await
            .unwrap();
        repo.upsert(sample_checkpoint(3, AiRunCheckpointType::AfterTool))
            .await
            .unwrap();
        let best = repo.latest_recoverable("run-1").await.unwrap().unwrap();
        assert_eq!(best.sequence_no, 3);
        assert_eq!(best.checkpoint_type, AiRunCheckpointType::AfterTool);
    }

    #[tokio::test]
    async fn checkpoint_mark_superseded_only_affects_before_tool_and_after_tool() {
        let repo = InMemoryCheckpointRepo::default();
        repo.upsert(sample_checkpoint(1, AiRunCheckpointType::RunInput))
            .await
            .unwrap();
        repo.upsert(sample_checkpoint(2, AiRunCheckpointType::BeforeTool))
            .await
            .unwrap();
        repo.upsert(sample_checkpoint(3, AiRunCheckpointType::AfterTool))
            .await
            .unwrap();
        let superseded = repo.mark_superseded("run-1", 3).await.unwrap();
        assert_eq!(superseded, 1, "only seq 2 (BeforeTool) is < 3");
        // After seq 2 was superseded, the latest_recoverable is seq 3 (AfterTool)
        let best = repo.latest_recoverable("run-1").await.unwrap().unwrap();
        assert_eq!(best.sequence_no, 3);
    }

    #[tokio::test]
    async fn checkpoint_latest_recoverable_returns_none_when_no_tool_checkpoint_exists() {
        let repo = InMemoryCheckpointRepo::default();
        repo.upsert(sample_checkpoint(1, AiRunCheckpointType::RunInput))
            .await
            .unwrap();
        let best = repo.latest_recoverable("run-1").await.unwrap();
        assert!(best.is_none());
    }
}
