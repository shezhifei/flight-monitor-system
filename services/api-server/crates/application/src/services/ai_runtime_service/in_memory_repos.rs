//! Hand-rolled in-memory implementations of the AI execution control
//! plane repositories.
//!
//! These exist so the application services can be exercised without a
//! live Postgres instance. The behavior mirrors the trait surface
//! defined in [`fms_domain::ports::ai_execution_repository`]; the
//! Postgres adapter (delivered with the persistence layer) is the
//! production source of truth.
//!
//! The consumer, control service, and recovery scheduler share this
//! trait set.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::Utc;

use fms_domain::models::ai_execution::{
    AiActionReceiptRecord, AiCompensationPlanRecord, AiCompensationStatus, AiRunCheckpointRecord,
    AiRunCheckpointStatus, AiRunCheckpointType, AiRuntimeCommandRecord, AiRuntimeCommandStatus, AiToolCallError,
    AiToolCallRecord, AiToolCallResult,
};
use fms_domain::ports::ai_execution_repository::{
    AiActionReceiptRepository, AiCompensationPlanRepository, AiExecutionRepositoryError, AiRunCheckpointRepository,
    AiRuntimeCommandRepository, AiToolCallRepository,
};

#[derive(Debug, Default)]
pub struct InMemoryToolCallRepository {
    rows: Mutex<Vec<AiToolCallRecord>>,
}

impl InMemoryToolCallRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.rows.lock().expect("tool call repo poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn snapshot(&self) -> Vec<AiToolCallRecord> {
        self.rows
            .lock()
            .expect("tool call repo poisoned")
            .iter()
            .cloned()
            .collect()
    }

    fn find_mut<'a>(
        rows: &'a mut Vec<AiToolCallRecord>,
        tool_call_pk: &str,
    ) -> Result<&'a mut AiToolCallRecord, AiExecutionRepositoryError> {
        rows.iter_mut()
            .find(|r| r.tool_call_pk == tool_call_pk)
            .ok_or_else(|| AiExecutionRepositoryError::not_found(tool_call_pk))
    }
}

#[async_trait]
impl AiToolCallRepository for InMemoryToolCallRepository {
    async fn upsert_requested(&self, record: AiToolCallRecord) -> Result<bool, AiExecutionRepositoryError> {
        let mut rows = self.rows.lock().expect("tool call repo poisoned");
        if rows.iter().any(|r| r.idempotency_key == record.idempotency_key) {
            return Ok(false);
        }
        rows.push(record);
        Ok(true)
    }

    async fn mark_authorized(&self, tool_call_pk: &str) -> Result<(), AiExecutionRepositoryError> {
        let mut rows = self.rows.lock().expect("tool call repo poisoned");
        let row = Self::find_mut(&mut rows, tool_call_pk)?;
        row.status = fms_domain::models::ai_execution::AiToolCallStatus::Authorized;
        row.started_at.get_or_insert(Utc::now());
        Ok(())
    }

    async fn mark_running(&self, tool_call_pk: &str) -> Result<(), AiExecutionRepositoryError> {
        let mut rows = self.rows.lock().expect("tool call repo poisoned");
        let row = Self::find_mut(&mut rows, tool_call_pk)?;
        row.status = fms_domain::models::ai_execution::AiToolCallStatus::Running;
        if row.started_at.is_none() {
            row.started_at = Some(Utc::now());
        }
        Ok(())
    }

    async fn mark_succeeded(
        &self,
        tool_call_pk: &str,
        result: AiToolCallResult,
    ) -> Result<(), AiExecutionRepositoryError> {
        let mut rows = self.rows.lock().expect("tool call repo poisoned");
        let row = Self::find_mut(&mut rows, tool_call_pk)?;
        row.status = fms_domain::models::ai_execution::AiToolCallStatus::Succeeded;
        row.result_hash = result.result_hash;
        row.result_summary = result.result_summary;
        row.finished_at = Some(Utc::now());
        Ok(())
    }

    async fn mark_failed(&self, tool_call_pk: &str, error: AiToolCallError) -> Result<(), AiExecutionRepositoryError> {
        let mut rows = self.rows.lock().expect("tool call repo poisoned");
        let row = Self::find_mut(&mut rows, tool_call_pk)?;
        row.status = if error.retryable {
            fms_domain::models::ai_execution::AiToolCallStatus::FailedRetryable
        } else {
            fms_domain::models::ai_execution::AiToolCallStatus::FailedTerminal
        };
        row.error_code = Some(error.code);
        row.error_message = Some(error.message);
        row.finished_at = Some(Utc::now());
        Ok(())
    }

    async fn mark_cancelled(&self, tool_call_pk: &str) -> Result<(), AiExecutionRepositoryError> {
        let mut rows = self.rows.lock().expect("tool call repo poisoned");
        let row = Self::find_mut(&mut rows, tool_call_pk)?;
        row.status = fms_domain::models::ai_execution::AiToolCallStatus::Cancelled;
        row.finished_at = Some(Utc::now());
        Ok(())
    }

    async fn mark_expired(&self, tool_call_pk: &str) -> Result<(), AiExecutionRepositoryError> {
        let mut rows = self.rows.lock().expect("tool call repo poisoned");
        let row = Self::find_mut(&mut rows, tool_call_pk)?;
        row.status = fms_domain::models::ai_execution::AiToolCallStatus::Expired;
        row.finished_at = Some(Utc::now());
        Ok(())
    }

    async fn mark_proposal_only(&self, tool_call_pk: &str) -> Result<(), AiExecutionRepositoryError> {
        let mut rows = self.rows.lock().expect("tool call repo poisoned");
        let row = Self::find_mut(&mut rows, tool_call_pk)?;
        row.status = fms_domain::models::ai_execution::AiToolCallStatus::ProposalOnly;
        row.finished_at = Some(Utc::now());
        Ok(())
    }

    async fn mark_denied(
        &self,
        tool_call_pk: &str,
        code: &str,
        message: &str,
    ) -> Result<(), AiExecutionRepositoryError> {
        let mut rows = self.rows.lock().expect("tool call repo poisoned");
        let row = Self::find_mut(&mut rows, tool_call_pk)?;
        row.status = fms_domain::models::ai_execution::AiToolCallStatus::Denied;
        row.error_code = Some(code.to_string());
        row.error_message = Some(message.to_string());
        row.finished_at = Some(Utc::now());
        Ok(())
    }

    async fn heartbeat(&self, tool_call_pk: &str) -> Result<(), AiExecutionRepositoryError> {
        let mut rows = self.rows.lock().expect("tool call repo poisoned");
        let row = Self::find_mut(&mut rows, tool_call_pk)?;
        if !matches!(
            row.status,
            fms_domain::models::ai_execution::AiToolCallStatus::Running
                | fms_domain::models::ai_execution::AiToolCallStatus::Authorized
        ) {
            row.status = fms_domain::models::ai_execution::AiToolCallStatus::Running;
        }
        row.last_heartbeat_at = Some(Utc::now());
        Ok(())
    }

    async fn get(&self, tool_call_pk: &str) -> Result<Option<AiToolCallRecord>, AiExecutionRepositoryError> {
        let rows = self.rows.lock().expect("tool call repo poisoned");
        Ok(rows.iter().find(|r| r.tool_call_pk == tool_call_pk).cloned())
    }

    async fn list_by_run(&self, run_id: &str) -> Result<Vec<AiToolCallRecord>, AiExecutionRepositoryError> {
        let rows = self.rows.lock().expect("tool call repo poisoned");
        Ok(rows.iter().filter(|r| r.run_id == run_id).cloned().collect())
    }
}

#[derive(Debug, Default)]
pub struct InMemoryRuntimeCommandRepository {
    rows: Mutex<Vec<AiRuntimeCommandRecord>>,
    sequences: Mutex<HashMap<String, i64>>,
}

impl InMemoryRuntimeCommandRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.rows.lock().expect("command repo poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn snapshot(&self) -> Vec<AiRuntimeCommandRecord> {
        self.rows
            .lock()
            .expect("command repo poisoned")
            .iter()
            .cloned()
            .collect()
    }

    /// Allocate the next per-run command sequence. Real Postgres uses
    /// `UNIQUE(run_id, command_sequence)`; this mock mirrors the
    /// contract so the application service cannot accidentally emit
    /// two commands with the same sequence number.
    pub fn next_sequence(&self, run_id: &str) -> i64 {
        let mut sequences = self.sequences.lock().expect("sequences poisoned");
        let counter = sequences.entry(run_id.to_string()).or_insert(0);
        *counter += 1;
        *counter
    }
}

#[async_trait]
impl AiRuntimeCommandRepository for InMemoryRuntimeCommandRepository {
    async fn enqueue(&self, command: AiRuntimeCommandRecord) -> Result<(), AiExecutionRepositoryError> {
        let mut rows = self.rows.lock().expect("command repo poisoned");
        if rows
            .iter()
            .any(|r| r.run_id == command.run_id && r.command_sequence == command.command_sequence)
        {
            return Err(AiExecutionRepositoryError::validation(format!(
                "duplicate command_sequence {} for run {}",
                command.command_sequence, command.run_id
            )));
        }
        rows.push(command);
        Ok(())
    }

    async fn lease_pending(
        &self,
        owner: &str,
        lease_seconds: u32,
        batch_size: u32,
    ) -> Result<Vec<AiRuntimeCommandRecord>, AiExecutionRepositoryError> {
        let mut rows = self.rows.lock().expect("command repo poisoned");
        let now = Utc::now();
        let mut leased = Vec::new();
        for row in rows.iter_mut() {
            if leased.len() as u32 >= batch_size {
                break;
            }
            if row.status != fms_domain::models::ai_execution::AiRuntimeCommandStatus::Pending {
                continue;
            }
            row.status = fms_domain::models::ai_execution::AiRuntimeCommandStatus::Leased;
            row.lease_owner = Some(owner.to_string());
            row.lease_expires_at = Some(now + chrono::Duration::seconds(lease_seconds as i64));
            leased.push(row.clone());
        }
        Ok(leased)
    }

    async fn complete(&self, command_id: &str) -> Result<(), AiExecutionRepositoryError> {
        let mut rows = self.rows.lock().expect("command repo poisoned");
        let row = rows
            .iter_mut()
            .find(|r| r.command_id == command_id)
            .ok_or_else(|| AiExecutionRepositoryError::not_found(command_id))?;
        row.status = fms_domain::models::ai_execution::AiRuntimeCommandStatus::Completed;
        row.processed_at = Some(Utc::now());
        Ok(())
    }

    async fn fail(&self, command_id: &str, _error: &str) -> Result<(), AiExecutionRepositoryError> {
        let mut rows = self.rows.lock().expect("command repo poisoned");
        let row = rows
            .iter_mut()
            .find(|r| r.command_id == command_id)
            .ok_or_else(|| AiExecutionRepositoryError::not_found(command_id))?;
        row.status = fms_domain::models::ai_execution::AiRuntimeCommandStatus::Failed;
        row.processed_at = Some(Utc::now());
        Ok(())
    }

    async fn get(&self, command_id: &str) -> Result<Option<AiRuntimeCommandRecord>, AiExecutionRepositoryError> {
        let rows = self.rows.lock().expect("command repo poisoned");
        Ok(rows.iter().find(|r| r.command_id == command_id).cloned())
    }

    async fn lease_pending_with_owner_check(
        &self,
        owner: &str,
        lease_seconds: u32,
        batch_size: u32,
    ) -> Result<Vec<AiRuntimeCommandRecord>, AiExecutionRepositoryError> {
        let mut rows = self.rows.lock().expect("command repo poisoned");
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
            row.lease_owner = Some(owner.to_string());
            row.lease_expires_at = Some(now + chrono::Duration::seconds(lease_seconds as i64));
            row.last_heartbeat_at = Some(now);
            if row.run_owner_lock.is_none() {
                row.run_owner_lock = Some(owner.to_string());
            }
            leased.push(row.clone());
        }
        Ok(leased)
    }

    async fn heartbeat_command(&self, command_id: &str) -> Result<(), AiExecutionRepositoryError> {
        let mut rows = self.rows.lock().expect("command repo poisoned");
        let row = rows
            .iter_mut()
            .find(|r| r.command_id == command_id)
            .ok_or_else(|| AiExecutionRepositoryError::not_found(command_id))?;
        if row.status != AiRuntimeCommandStatus::Leased {
            return Err(AiExecutionRepositoryError::validation(format!(
                "command {} is not leased (status={})",
                command_id, row.status
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
        let mut rows = self.rows.lock().expect("command repo poisoned");
        let now = Utc::now();
        let mut start_run: Option<AiRuntimeCommandRecord> = None;
        for row in rows.iter_mut() {
            if row.run_id != run_id {
                continue;
            }
            if row.status == AiRuntimeCommandStatus::Leased || row.status == AiRuntimeCommandStatus::Pending {
                row.run_owner_lock = Some(new_owner.to_string());
            }
        }
        for row in rows.iter_mut() {
            if row.run_id != run_id {
                continue;
            }
            if row.command_type == fms_domain::models::ai_execution::AiRuntimeCommandType::StartRun
                && (row.status == AiRuntimeCommandStatus::Pending || row.status == AiRuntimeCommandStatus::Leased)
            {
                row.attempt_count += 1;
                row.status = AiRuntimeCommandStatus::Leased;
                row.lease_owner = Some(new_owner.to_string());
                row.lease_expires_at = Some(now + chrono::Duration::seconds(lease_seconds as i64));
                row.last_heartbeat_at = Some(now);
                row.run_owner_lock = Some(new_owner.to_string());
                start_run = Some(row.clone());
                break;
            }
        }
        Ok(start_run)
    }

    async fn list_expired_leases(
        &self,
        now: chrono::DateTime<chrono::Utc>,
        limit: u32,
    ) -> Result<Vec<AiRuntimeCommandRecord>, AiExecutionRepositoryError> {
        let rows = self.rows.lock().expect("command repo poisoned");
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use fms_domain::models::ai_execution::{
        AiRuntimeCommandRecord, AiRuntimeCommandStatus, AiRuntimeCommandType, AiToolCallRecord, AiToolCallStatus,
        AiToolCallType,
    };
    use serde_json::json;

    fn sample_tool_call(pk: &str) -> AiToolCallRecord {
        AiToolCallRecord {
            tool_call_pk: pk.into(),
            job_id: "job-1".into(),
            run_id: "run-1".into(),
            parent_tool_call_pk: None,
            root_tool_call_pk: None,
            depth: 0,
            round_index: 0,
            tool_call_id: format!("call-{pk}"),
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
            idempotency_key: format!("run-1:0:call-{pk}:weather_at_airport:hash-1"),
            mq_message_id: None,
            mq_offset: None,
            created_at: Utc::now(),
            started_at: None,
            finished_at: None,
            metadata: json!({}),
        }
    }

    fn sample_command(command_id: &str, run_id: &str, sequence: i64) -> AiRuntimeCommandRecord {
        AiRuntimeCommandRecord {
            command_id: command_id.into(),
            run_id: run_id.into(),
            command_type: AiRuntimeCommandType::ToolLease,
            command_sequence: sequence,
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

    #[tokio::test]
    async fn tool_call_repo_rejects_duplicate_idempotency_key() {
        let repo = InMemoryToolCallRepository::new();
        assert!(repo.upsert_requested(sample_tool_call("tpc-1")).await.unwrap());
        assert!(!repo.upsert_requested(sample_tool_call("tpc-1")).await.unwrap());
        assert_eq!(repo.len(), 1);
    }

    #[tokio::test]
    async fn tool_call_repo_status_transitions_round_trip() {
        let repo = InMemoryToolCallRepository::new();
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
    async fn command_repo_next_sequence_is_per_run_monotonic() {
        let repo = InMemoryRuntimeCommandRepository::new();
        assert_eq!(repo.next_sequence("run-1"), 1);
        assert_eq!(repo.next_sequence("run-1"), 2);
        assert_eq!(repo.next_sequence("run-2"), 1);
        assert_eq!(repo.next_sequence("run-1"), 3);
    }

    #[tokio::test]
    async fn command_repo_rejects_duplicate_run_sequence_pair() {
        let repo = InMemoryRuntimeCommandRepository::new();
        repo.enqueue(sample_command("c-1", "run-1", 1)).await.unwrap();
        let err = repo
            .enqueue(sample_command("c-2", "run-1", 1))
            .await
            .expect_err("duplicate sequence should fail");
        assert!(matches!(err, AiExecutionRepositoryError::Validation(_)));
        assert_eq!(repo.len(), 1);
    }

    #[tokio::test]
    async fn command_repo_lease_pending_skips_already_leased_rows() {
        let repo = InMemoryRuntimeCommandRepository::new();
        repo.enqueue(sample_command("c-1", "run-1", 1)).await.unwrap();
        repo.enqueue(sample_command("c-2", "run-1", 2)).await.unwrap();
        let leased = repo.lease_pending("worker-a", 30, 10).await.unwrap();
        assert_eq!(leased.len(), 2);
        let again = repo.lease_pending("worker-b", 30, 10).await.unwrap();
        assert!(again.is_empty());
    }

    #[tokio::test]
    async fn checkpoint_repo_upsert_is_idempotent_on_sequence() {
        let repo = InMemoryCheckpointRepository::new();
        let cp = AiRunCheckpointRecord {
            checkpoint_id: "cp-1".into(),
            job_id: "job-1".into(),
            run_id: "run-1".into(),
            sequence_no: 1,
            checkpoint_type: AiRunCheckpointType::BeforeTool,
            tool_call_pk: None,
            proposal_id: None,
            snapshot_hash: "h".into(),
            snapshot: serde_json::json!({}),
            snapshot_size_bytes: 2,
            mq_message_id: None,
            created_at: Utc::now(),
        };
        assert!(repo.upsert(cp.clone()).await.unwrap());
        assert!(!repo.upsert(cp).await.unwrap());
    }

    fn sample_receipt(receipt_id: &str, key: &str) -> AiActionReceiptRecord {
        AiActionReceiptRecord {
            receipt_id: receipt_id.into(),
            proposal_id: "prop-1".into(),
            job_id: "job-1".into(),
            run_id: "run-1".into(),
            tool_call_pk: Some("tpc-1".into()),
            object_type: "Flight".into(),
            object_id: "flt-1".into(),
            action_name: "update_status".into(),
            idempotency_key: key.into(),
            before_checkpoint_id: Some("cp-before".into()),
            after_checkpoint_id: Some("cp-after".into()),
            outbox_event_id: Some("evt-1".into()),
            execution_result: serde_json::json!({"status": "BOARDING"}),
            executed_by: "executor-1".into(),
            executed_at: Utc::now(),
        }
    }

    fn sample_plan(compensation_id: &str, receipt_id: &str, status: AiCompensationStatus) -> AiCompensationPlanRecord {
        AiCompensationPlanRecord {
            compensation_id: compensation_id.into(),
            receipt_id: receipt_id.into(),
            proposal_id: "prop-1".into(),
            status,
            mode: fms_domain::models::ai_execution::AiCompensationMode::RestoreSnapshot,
            plan: serde_json::json!({}),
            requires_approval: false,
            approved_by: None,
            approved_at: None,
            executed_by: None,
            executed_at: None,
            execution_result: None,
            execution_error: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn receipt_repo_upsert_is_idempotent_on_idempotency_key() {
        let repo = InMemoryActionReceiptRepository::new();
        assert!(repo.upsert(sample_receipt("rcp-1", "idem-1")).await.unwrap());
        let again = repo.upsert(sample_receipt("rcp-1", "idem-1")).await.unwrap();
        assert!(!again);
        assert_eq!(repo.len(), 1);
    }

    #[tokio::test]
    async fn receipt_repo_upsert_rejects_mismatched_receipt_id_on_same_key() {
        let repo = InMemoryActionReceiptRepository::new();
        assert!(repo.upsert(sample_receipt("rcp-1", "idem-1")).await.unwrap());
        let err = repo
            .upsert(sample_receipt("rcp-2", "idem-1"))
            .await
            .expect_err("collision must fail validation");
        assert!(matches!(err, AiExecutionRepositoryError::Validation(_)));
    }

    #[tokio::test]
    async fn receipt_repo_list_by_proposal_filters_correctly() {
        let repo = InMemoryActionReceiptRepository::new();
        let mut a = sample_receipt("rcp-1", "idem-1");
        a.proposal_id = "prop-A".into();
        let mut b = sample_receipt("rcp-2", "idem-2");
        b.proposal_id = "prop-B".into();
        repo.upsert(a).await.unwrap();
        repo.upsert(b).await.unwrap();
        let a_rows = repo.list_by_proposal("prop-A").await.unwrap();
        assert_eq!(a_rows.len(), 1);
        assert_eq!(a_rows[0].receipt_id, "rcp-1");
    }

    #[tokio::test]
    async fn compensation_repo_upsert_is_idempotent_on_receipt_mode_pair() {
        let repo = InMemoryCompensationPlanRepository::new();
        let p = sample_plan("cmp-1", "rcp-1", AiCompensationStatus::Planned);
        assert!(repo.upsert(p.clone()).await.unwrap());
        assert!(!repo.upsert(p).await.unwrap());
        assert_eq!(repo.len(), 1);
    }

    #[tokio::test]
    async fn compensation_repo_mark_executing_succeeded_failed_transition() {
        let repo = InMemoryCompensationPlanRepository::new();
        repo.upsert(sample_plan("cmp-1", "rcp-1", AiCompensationStatus::Planned))
            .await
            .unwrap();
        assert!(repo.mark_executing("cmp-1", "executor-1").await.unwrap());
        let mid = repo.get("cmp-1").await.unwrap().unwrap();
        assert_eq!(mid.status, AiCompensationStatus::Executing);
        assert_eq!(mid.executed_by.as_deref(), Some("executor-1"));
        repo.mark_succeeded("cmp-1", "executor-1", serde_json::json!({"ok": true}))
            .await
            .unwrap();
        let after = repo.get("cmp-1").await.unwrap().unwrap();
        assert_eq!(after.status, AiCompensationStatus::Succeeded);
        assert!(after.executed_at.is_some());
    }

    #[tokio::test]
    async fn compensation_repo_mark_executing_refuses_terminal_status() {
        let repo = InMemoryCompensationPlanRepository::new();
        repo.upsert(sample_plan("cmp-1", "rcp-1", AiCompensationStatus::Succeeded))
            .await
            .unwrap();
        let claimed = repo.mark_executing("cmp-1", "executor-1").await.unwrap();
        assert!(!claimed);
    }

    #[tokio::test]
    async fn compensation_repo_mark_failed_transitions_to_failed() {
        let repo = InMemoryCompensationPlanRepository::new();
        repo.upsert(sample_plan("cmp-1", "rcp-1", AiCompensationStatus::Executing))
            .await
            .unwrap();
        repo.mark_failed("cmp-1", "boom").await.unwrap();
        let row = repo.get("cmp-1").await.unwrap().unwrap();
        assert_eq!(row.status, AiCompensationStatus::Failed);
        assert_eq!(row.execution_error.as_deref(), Some("boom"));
    }

    fn owned_command(
        command_id: &str,
        run_id: &str,
        sequence: i64,
        owner: &str,
        lease_expires_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> AiRuntimeCommandRecord {
        let mut cmd = sample_command(command_id, run_id, sequence);
        cmd.status = AiRuntimeCommandStatus::Leased;
        cmd.lease_owner = Some(owner.into());
        cmd.lease_expires_at = lease_expires_at;
        cmd.run_owner_lock = Some(owner.into());
        cmd.last_heartbeat_at = Some(Utc::now());
        cmd
    }

    #[tokio::test]
    async fn lease_pending_with_owner_check_skips_run_owned_by_another() {
        let repo = InMemoryRuntimeCommandRepository::new();
        let mut cmd = sample_command("c-1", "run-1", 1);
        cmd.run_owner_lock = Some("worker-alpha".into());
        repo.enqueue(cmd).await.unwrap();
        let leased = repo
            .lease_pending_with_owner_check("worker-beta", 30, 10)
            .await
            .unwrap();
        assert!(
            leased.is_empty(),
            "worker-beta must not steal run locked to worker-alpha"
        );
        let row = repo.get("c-1").await.unwrap().unwrap();
        assert_eq!(row.status, AiRuntimeCommandStatus::Pending);
    }

    #[tokio::test]
    async fn lease_pending_with_owner_check_reclaims_expired_lease() {
        let repo = InMemoryRuntimeCommandRepository::new();
        let past = Utc::now() - chrono::Duration::seconds(60);
        let cmd = owned_command("c-1", "run-1", 1, "worker-alpha", Some(past));
        repo.enqueue(cmd).await.unwrap();
        let leased = repo
            .lease_pending_with_owner_check("worker-beta", 30, 10)
            .await
            .unwrap();
        assert_eq!(leased.len(), 1);
        let row = repo.get("c-1").await.unwrap().unwrap();
        assert_eq!(row.status, AiRuntimeCommandStatus::Leased);
        assert_eq!(row.lease_owner.as_deref(), Some("worker-beta"));
        assert_eq!(row.attempt_count, 1);
    }

    #[tokio::test]
    async fn heartbeat_command_updates_last_heartbeat_at() {
        let repo = InMemoryRuntimeCommandRepository::new();
        let mut cmd = sample_command("c-1", "run-1", 1);
        cmd.status = AiRuntimeCommandStatus::Leased;
        cmd.lease_owner = Some("worker-a".into());
        cmd.last_heartbeat_at = Some(Utc::now() - chrono::Duration::seconds(30));
        let old_hb = cmd.last_heartbeat_at;
        repo.enqueue(cmd).await.unwrap();
        repo.heartbeat_command("c-1").await.unwrap();
        let row = repo.get("c-1").await.unwrap().unwrap();
        assert!(row.last_heartbeat_at > old_hb, "heartbeat must advance timestamp");
    }

    #[tokio::test]
    async fn heartbeat_command_rejects_non_leased_command() {
        let repo = InMemoryRuntimeCommandRepository::new();
        repo.enqueue(sample_command("c-1", "run-1", 1)).await.unwrap();
        let err = repo
            .heartbeat_command("c-1")
            .await
            .expect_err("pending command cannot heartbeat");
        assert!(matches!(err, AiExecutionRepositoryError::Validation(_)));
    }

    #[tokio::test]
    async fn take_over_run_reassigns_all_leased_commands() {
        let repo = InMemoryRuntimeCommandRepository::new();
        let mut start = sample_command("c-start", "run-1", 1);
        start.command_type = AiRuntimeCommandType::StartRun;
        start.status = AiRuntimeCommandStatus::Leased;
        start.lease_owner = Some("worker-a".into());
        start.run_owner_lock = Some("worker-a".into());
        let mut tool = sample_command("c-tool", "run-1", 2);
        tool.status = AiRuntimeCommandStatus::Leased;
        tool.lease_owner = Some("worker-a".into());
        tool.run_owner_lock = Some("worker-a".into());
        repo.enqueue(start).await.unwrap();
        repo.enqueue(tool).await.unwrap();

        let claimed = repo.take_over_run("run-1", "worker-b", 30).await.unwrap();
        assert!(claimed.is_some(), "take_over_run must reclaim the StartRun command");
        let claimed = claimed.unwrap();
        assert_eq!(claimed.lease_owner.as_deref(), Some("worker-b"));
        assert_eq!(claimed.run_owner_lock.as_deref(), Some("worker-b"));

        let tool_after = repo.get("c-tool").await.unwrap().unwrap();
        assert_eq!(tool_after.run_owner_lock.as_deref(), Some("worker-b"));
    }

    #[tokio::test]
    async fn list_expired_leases_returns_only_expired_leased_rows() {
        let repo = InMemoryRuntimeCommandRepository::new();
        let now = Utc::now();
        let expired = owned_command("c-1", "run-1", 1, "worker-a", Some(now - chrono::Duration::seconds(60)));
        let live = owned_command("c-2", "run-1", 2, "worker-a", Some(now + chrono::Duration::seconds(60)));
        repo.enqueue(expired).await.unwrap();
        repo.enqueue(live).await.unwrap();
        let rows = repo.list_expired_leases(now, 10).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].command_id, "c-1");
    }

    #[tokio::test]
    async fn lease_pending_with_owner_check_fails_command_past_max_attempts() {
        let repo = InMemoryRuntimeCommandRepository::new();
        let mut cmd = sample_command("c-1", "run-1", 1);
        cmd.attempt_count = 3;
        cmd.max_attempts = 3;
        repo.enqueue(cmd).await.unwrap();
        let leased = repo.lease_pending_with_owner_check("worker-a", 30, 10).await.unwrap();
        assert!(leased.is_empty());
        let row = repo.get("c-1").await.unwrap().unwrap();
        assert_eq!(row.status, AiRuntimeCommandStatus::Failed);
    }
}

#[derive(Debug, Default)]
pub struct InMemoryCheckpointRepository {
    rows: Mutex<Vec<AiRunCheckpointRecord>>,
    statuses: Mutex<HashMap<String, AiRunCheckpointStatus>>,
}

impl InMemoryCheckpointRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.rows.lock().expect("checkpoint repo poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn snapshot(&self) -> Vec<AiRunCheckpointRecord> {
        self.rows
            .lock()
            .expect("checkpoint repo poisoned")
            .iter()
            .cloned()
            .collect()
    }

    fn key(run_id: &str, sequence_no: i64) -> String {
        format!("{run_id}:{sequence_no}")
    }
}

#[async_trait]
impl AiRunCheckpointRepository for InMemoryCheckpointRepository {
    async fn upsert(&self, record: AiRunCheckpointRecord) -> Result<bool, AiExecutionRepositoryError> {
        let mut rows = self.rows.lock().expect("checkpoint repo poisoned");
        if rows
            .iter()
            .any(|r| r.run_id == record.run_id && r.sequence_no == record.sequence_no)
        {
            return Ok(false);
        }
        self.statuses.lock().expect("checkpoint statuses poisoned").insert(
            Self::key(&record.run_id, record.sequence_no),
            AiRunCheckpointStatus::Persisted,
        );
        rows.push(record);
        Ok(true)
    }

    async fn list_by_run(&self, run_id: &str) -> Result<Vec<AiRunCheckpointRecord>, AiExecutionRepositoryError> {
        let rows = self.rows.lock().expect("checkpoint repo poisoned");
        let mut out: Vec<_> = rows.iter().filter(|r| r.run_id == run_id).cloned().collect();
        out.sort_by_key(|r| r.sequence_no);
        Ok(out)
    }

    async fn latest_recoverable(
        &self,
        run_id: &str,
    ) -> Result<Option<AiRunCheckpointRecord>, AiExecutionRepositoryError> {
        let rows = self.rows.lock().expect("checkpoint repo poisoned");
        let statuses = self.statuses.lock().expect("checkpoint statuses poisoned");
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
                .get(&Self::key(&row.run_id, row.sequence_no))
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

    async fn mark_superseded(&self, run_id: &str, before_sequence_no: u64) -> Result<u64, AiExecutionRepositoryError> {
        let mut count = 0u64;
        {
            let rows = self.rows.lock().expect("checkpoint repo poisoned");
            let mut statuses = self.statuses.lock().expect("checkpoint statuses poisoned");
            for row in rows.iter() {
                if row.run_id == run_id
                    && (row.sequence_no as u64) < before_sequence_no
                    && matches!(
                        row.checkpoint_type,
                        AiRunCheckpointType::BeforeTool | AiRunCheckpointType::AfterTool
                    )
                {
                    statuses.insert(
                        Self::key(&row.run_id, row.sequence_no),
                        AiRunCheckpointStatus::Superseded,
                    );
                    count += 1;
                }
            }
        }
        Ok(count)
    }
}

// ---------------------------------------------------------------------------
// Action receipts
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct InMemoryActionReceiptRepository {
    rows: Mutex<Vec<AiActionReceiptRecord>>,
}

impl InMemoryActionReceiptRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.rows.lock().expect("receipt repo poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn snapshot(&self) -> Vec<AiActionReceiptRecord> {
        self.rows
            .lock()
            .expect("receipt repo poisoned")
            .iter()
            .cloned()
            .collect()
    }
}

#[async_trait]
impl AiActionReceiptRepository for InMemoryActionReceiptRepository {
    async fn upsert(&self, receipt: AiActionReceiptRecord) -> Result<bool, AiExecutionRepositoryError> {
        let mut rows = self.rows.lock().expect("receipt repo poisoned");
        if let Some(existing) = rows.iter_mut().find(|r| r.idempotency_key == receipt.idempotency_key) {
            if existing.receipt_id != receipt.receipt_id {
                return Err(AiExecutionRepositoryError::validation(format!(
                    "idempotency_key {} already bound to receipt {} (new attempt: {})",
                    receipt.idempotency_key, existing.receipt_id, receipt.receipt_id
                )));
            }
            return Ok(false);
        }
        rows.push(receipt);
        Ok(true)
    }

    async fn get_by_idempotency_key(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<AiActionReceiptRecord>, AiExecutionRepositoryError> {
        let rows = self.rows.lock().expect("receipt repo poisoned");
        Ok(rows.iter().find(|r| r.idempotency_key == idempotency_key).cloned())
    }

    async fn get(&self, receipt_id: &str) -> Result<Option<AiActionReceiptRecord>, AiExecutionRepositoryError> {
        let rows = self.rows.lock().expect("receipt repo poisoned");
        Ok(rows.iter().find(|r| r.receipt_id == receipt_id).cloned())
    }

    async fn list_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Vec<AiActionReceiptRecord>, AiExecutionRepositoryError> {
        let rows = self.rows.lock().expect("receipt repo poisoned");
        Ok(rows.iter().filter(|r| r.proposal_id == proposal_id).cloned().collect())
    }

    async fn list_by_run(&self, run_id: &str) -> Result<Vec<AiActionReceiptRecord>, AiExecutionRepositoryError> {
        let rows = self.rows.lock().expect("receipt repo poisoned");
        Ok(rows.iter().filter(|r| r.run_id == run_id).cloned().collect())
    }
}

// ---------------------------------------------------------------------------
// Compensation plans
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct InMemoryCompensationPlanRepository {
    rows: Mutex<Vec<AiCompensationPlanRecord>>,
}

impl InMemoryCompensationPlanRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.rows.lock().expect("compensation repo poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn snapshot(&self) -> Vec<AiCompensationPlanRecord> {
        self.rows
            .lock()
            .expect("compensation repo poisoned")
            .iter()
            .cloned()
            .collect()
    }
}

#[async_trait]
impl AiCompensationPlanRepository for InMemoryCompensationPlanRepository {
    async fn upsert(&self, plan: AiCompensationPlanRecord) -> Result<bool, AiExecutionRepositoryError> {
        let mut rows = self.rows.lock().expect("compensation repo poisoned");
        if rows
            .iter()
            .any(|r| r.receipt_id == plan.receipt_id && r.mode == plan.mode)
        {
            return Ok(false);
        }
        rows.push(plan);
        Ok(true)
    }

    async fn get(&self, compensation_id: &str) -> Result<Option<AiCompensationPlanRecord>, AiExecutionRepositoryError> {
        let rows = self.rows.lock().expect("compensation repo poisoned");
        Ok(rows.iter().find(|r| r.compensation_id == compensation_id).cloned())
    }

    async fn list_by_receipt(
        &self,
        receipt_id: &str,
    ) -> Result<Vec<AiCompensationPlanRecord>, AiExecutionRepositoryError> {
        let rows = self.rows.lock().expect("compensation repo poisoned");
        Ok(rows.iter().filter(|r| r.receipt_id == receipt_id).cloned().collect())
    }

    async fn list_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Vec<AiCompensationPlanRecord>, AiExecutionRepositoryError> {
        let rows = self.rows.lock().expect("compensation repo poisoned");
        Ok(rows.iter().filter(|r| r.proposal_id == proposal_id).cloned().collect())
    }

    async fn mark_executing(
        &self,
        compensation_id: &str,
        executed_by: &str,
    ) -> Result<bool, AiExecutionRepositoryError> {
        let mut rows = self.rows.lock().expect("compensation repo poisoned");
        let row = rows
            .iter_mut()
            .find(|r| r.compensation_id == compensation_id)
            .ok_or_else(|| AiExecutionRepositoryError::not_found(compensation_id))?;
        if !matches!(
            row.status,
            AiCompensationStatus::Planned | AiCompensationStatus::Approved
        ) {
            return Ok(false);
        }
        row.status = AiCompensationStatus::Executing;
        row.executed_by = Some(executed_by.to_string());
        row.updated_at = Utc::now();
        Ok(true)
    }

    async fn mark_succeeded(
        &self,
        compensation_id: &str,
        executed_by: &str,
        result: serde_json::Value,
    ) -> Result<(), AiExecutionRepositoryError> {
        let mut rows = self.rows.lock().expect("compensation repo poisoned");
        let row = rows
            .iter_mut()
            .find(|r| r.compensation_id == compensation_id)
            .ok_or_else(|| AiExecutionRepositoryError::not_found(compensation_id))?;
        row.status = AiCompensationStatus::Succeeded;
        row.executed_by = Some(executed_by.to_string());
        row.executed_at = Some(Utc::now());
        row.execution_result = Some(result);
        row.updated_at = Utc::now();
        Ok(())
    }

    async fn mark_failed(&self, compensation_id: &str, error: &str) -> Result<(), AiExecutionRepositoryError> {
        let mut rows = self.rows.lock().expect("compensation repo poisoned");
        let row = rows
            .iter_mut()
            .find(|r| r.compensation_id == compensation_id)
            .ok_or_else(|| AiExecutionRepositoryError::not_found(compensation_id))?;
        row.status = AiCompensationStatus::Failed;
        row.execution_error = Some(error.to_string());
        row.updated_at = Utc::now();
        Ok(())
    }

    async fn list_pending_approval(
        &self,
        older_than_seconds: i64,
    ) -> Result<Vec<AiCompensationPlanRecord>, AiExecutionRepositoryError> {
        let now = Utc::now();
        let threshold = now - chrono::Duration::seconds(older_than_seconds);
        let rows = self.rows.lock().expect("compensation repo poisoned");
        Ok(rows
            .iter()
            .filter(|r| r.status == AiCompensationStatus::Planned && !r.requires_approval && r.created_at < threshold)
            .cloned()
            .collect())
    }

    async fn list_executing_past_timeout(
        &self,
        timeout_seconds: i64,
    ) -> Result<Vec<AiCompensationPlanRecord>, AiExecutionRepositoryError> {
        let now = Utc::now();
        let threshold = now - chrono::Duration::seconds(timeout_seconds);
        let rows = self.rows.lock().expect("compensation repo poisoned");
        Ok(rows
            .iter()
            .filter(|r| r.status == AiCompensationStatus::Executing && r.updated_at < threshold)
            .cloned()
            .collect())
    }

    async fn list_by_status(
        &self,
        status: AiCompensationStatus,
    ) -> Result<Vec<AiCompensationPlanRecord>, AiExecutionRepositoryError> {
        let rows = self.rows.lock().expect("compensation repo poisoned");
        Ok(rows.iter().filter(|r| r.status == status).cloned().collect())
    }
}
