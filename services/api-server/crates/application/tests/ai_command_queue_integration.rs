//! Command queue lifecycle from
//! `start_run` through worker competition, heartbeat, crash, and
//! take-over.
//!
//! The scenario exercises the hardened lease contract end-to-end
//! using the in-memory repositories and the real
//! `AiExecutionControlService` + `RecoveryOrchestrator`:
//!
//! 1. `enqueue_start_run` inserts a `start_run` command.
//! 2. Two fake workers race to lease it via
//!    `lease_pending_with_owner_check`. The winner becomes the
//!    `run_owner_lock`; the loser gets nothing.
//! 3. The winner sends a heartbeat; the lease stays alive.
//! 4. The winner "crashes" — we fast-forward time and the recovery
//!    orchestrator's `expire_lost_command_leases` fails the lost
//!    command with `worker_lease_lost`.
//! 5. A second StartRun command is enqueued (simulating the
//!    orchestrator-driven re-queue) and another worker leases it,
//!    completing the run.

use std::sync::Arc;

use chrono::Utc;

use fms_application::services::ai_runtime_service::ai_execution_control_service::AiExecutionControlService;
use fms_application::services::ai_runtime_service::in_memory_repos::{
    InMemoryCheckpointRepository, InMemoryRuntimeCommandRepository, InMemoryToolCallRepository,
};
use fms_application::services::ai_runtime_service::recovery_orchestrator::{
    RecoveryOrchestrator, RecoveryOrchestratorConfig, RecoveryOrchestratorDeps,
};
use fms_application::services::ai_runtime_service::tool_authorization_service::{
    StaticFeatureFlagSource, ToolAuthorizationService,
};
use fms_domain::models::ai_execution::{AiRuntimeCommandStatus, AiRuntimeCommandType};
use fms_domain::ports::ai_execution_repository::{
    AiRunCheckpointRepository, AiRuntimeCommandRepository, AiToolCallRepository,
};
use serde_json::json;

fn build_control_service() -> (
    Arc<AiExecutionControlService>,
    Arc<InMemoryToolCallRepository>,
    Arc<InMemoryRuntimeCommandRepository>,
    Arc<InMemoryCheckpointRepository>,
) {
    let tool_call_repo = Arc::new(InMemoryToolCallRepository::new());
    let command_repo = Arc::new(InMemoryRuntimeCommandRepository::new());
    let checkpoint_repo = Arc::new(InMemoryCheckpointRepository::new());
    let authorization = Arc::new(ToolAuthorizationService::new(
        Arc::new(StaticFeatureFlagSource::empty()),
    ));
    let control = Arc::new(
        AiExecutionControlService::new(
            tool_call_repo.clone() as Arc<dyn AiToolCallRepository>,
            command_repo.clone() as Arc<dyn AiRuntimeCommandRepository>,
            authorization,
        )
        .with_checkpoint_repo(checkpoint_repo.clone() as Arc<dyn AiRunCheckpointRepository>),
    );
    (control, tool_call_repo, command_repo, checkpoint_repo)
}

fn build_orchestrator(control: Arc<AiExecutionControlService>) -> Arc<RecoveryOrchestrator> {
    let deps = RecoveryOrchestratorDeps {
        tool_call_repo: control.tool_call_repo().unwrap(),
        command_repo: control.command_repo().unwrap(),
        checkpoint_repo: control.checkpoint_repo(),
        rollback_service: None,
        compensation_executing_timeout_seconds: 60,
        compensation_auto_execute_grace_seconds: 5,
    };
    Arc::new(RecoveryOrchestrator::new(deps, RecoveryOrchestratorConfig::default()))
}

#[tokio::test]
async fn start_run_two_workers_compete_winner_heartbeat_crash_takeover_completes() {
    let (control, _tool_call_repo, command_repo, _checkpoint_repo) = build_control_service();
    let orchestrator = build_orchestrator(control.clone());

    let envelope = json!({
        "run_id": "run-phase4",
        "job_id": "job-phase4",
        "requester_user_id": "user-1",
    });
    let capability_snapshot = json!({
        "tools": ["weather_at_airport"],
        "governance_version": "1.0",
    });
    let start_cmd = control
        .enqueue_start_run("job-phase4", "run-phase4", envelope, capability_snapshot, "gov-hash")
        .await
        .unwrap();
    assert_eq!(start_cmd.command_type, AiRuntimeCommandType::StartRun);
    assert_eq!(start_cmd.status, AiRuntimeCommandStatus::Pending);

    let worker_a_leased = command_repo
        .lease_pending_with_owner_check("worker-a", 30, 10)
        .await
        .unwrap();
    let worker_b_leased = command_repo
        .lease_pending_with_owner_check("worker-b", 30, 10)
        .await
        .unwrap();
    assert_eq!(worker_a_leased.len(), 1, "worker-a leases the StartRun");
    assert!(
        worker_b_leased.is_empty(),
        "worker-b must not steal run locked to worker-a"
    );
    let leased_cmd = &worker_a_leased[0];
    assert_eq!(leased_cmd.lease_owner.as_deref(), Some("worker-a"));
    assert_eq!(leased_cmd.run_owner_lock.as_deref(), Some("worker-a"));
    assert_eq!(leased_cmd.attempt_count, 1);

    command_repo.heartbeat_command(&leased_cmd.command_id).await.unwrap();
    let after_hb = command_repo.get(&leased_cmd.command_id).await.unwrap().unwrap();
    assert!(after_hb.last_heartbeat_at > leased_cmd.last_heartbeat_at);

    let future_now = Utc::now() + chrono::Duration::seconds(120);
    let lost_count = orchestrator.expire_lost_command_leases(future_now).await.unwrap();
    assert_eq!(lost_count, 1, "crashed worker's lease must be reaped");
    let failed = command_repo.get(&leased_cmd.command_id).await.unwrap().unwrap();
    assert_eq!(failed.status, AiRuntimeCommandStatus::Failed);

    let retry_cmd = control
        .enqueue_start_run(
            "job-phase4",
            "run-phase4",
            json!({"run_id": "run-phase4", "job_id": "job-phase4"}),
            json!({"tools": ["weather_at_airport"]}),
            "gov-hash",
        )
        .await
        .unwrap();
    assert_eq!(retry_cmd.command_sequence, 2);

    let worker_c_leased = command_repo
        .lease_pending_with_owner_check("worker-c", 30, 10)
        .await
        .unwrap();
    assert_eq!(worker_c_leased.len(), 1, "worker-c leases the re-queued StartRun");
    let reclaimed = &worker_c_leased[0];
    assert_eq!(reclaimed.lease_owner.as_deref(), Some("worker-c"));
    assert_eq!(reclaimed.run_owner_lock.as_deref(), Some("worker-c"));

    command_repo.complete(&reclaimed.command_id).await.unwrap();
    let completed = command_repo.get(&reclaimed.command_id).await.unwrap().unwrap();
    assert_eq!(completed.status, AiRuntimeCommandStatus::Completed);
}

#[tokio::test]
async fn cancel_run_command_is_consumed_by_run_owner() {
    let (control, _, command_repo, _) = build_control_service();
    control
        .enqueue_start_run("job-1", "run-1", json!({}), json!({}), "h")
        .await
        .unwrap();
    let leased = command_repo
        .lease_pending_with_owner_check("worker-a", 30, 10)
        .await
        .unwrap();
    assert_eq!(leased.len(), 1);

    let cancel_cmd = control.enqueue_cancel_run("job-1", "run-1", "user-1").await.unwrap();
    assert_eq!(cancel_cmd.command_type, AiRuntimeCommandType::CancelRun);

    let cancel_leased = command_repo
        .lease_pending_with_owner_check("worker-a", 30, 10)
        .await
        .unwrap();
    assert_eq!(cancel_leased.len(), 1);
    assert_eq!(cancel_leased[0].command_type, AiRuntimeCommandType::CancelRun);
    assert_eq!(cancel_leased[0].lease_owner.as_deref(), Some("worker-a"));

    let competitor = command_repo
        .lease_pending_with_owner_check("worker-b", 30, 10)
        .await
        .unwrap();
    assert!(
        competitor.is_empty(),
        "worker-b must not see commands owned by worker-a"
    );
}

#[tokio::test]
async fn heartbeat_keeps_lease_alive_through_recovery_scan() {
    let (control, _, command_repo, _) = build_control_service();
    let orchestrator = build_orchestrator(control.clone());

    control
        .enqueue_start_run("job-1", "run-1", json!({}), json!({}), "h")
        .await
        .unwrap();
    let leased = command_repo
        .lease_pending_with_owner_check("worker-a", 30, 10)
        .await
        .unwrap();
    assert_eq!(leased.len(), 1);

    command_repo.heartbeat_command(&leased[0].command_id).await.unwrap();

    let now = Utc::now();
    let report = orchestrator.scan_once().await;
    assert_eq!(
        report.lost_command_leases, 0,
        "fresh heartbeat must keep the lease alive through a recovery scan"
    );
    let still_leased = command_repo.get(&leased[0].command_id).await.unwrap().unwrap();
    assert_eq!(still_leased.status, AiRuntimeCommandStatus::Leased);
    let _ = now;
}

// ---------------------------------------------------------------------------
// Task D4: job cancel -> ai_runtime_commands.cancel_run
// ---------------------------------------------------------------------------

use async_trait::async_trait;
use fms_application::services::ai_job_service::AiJobService;
use fms_domain::models::ai_job::{AiJobRecord, AiJobStatusCount, AiRunEventRecord, AiRunRecord, AiRunStatus};
use fms_domain::ports::ai_job_repository::{AiJobRepository, AiJobRepositoryError};
use fms_domain::ports::ai_run_event_repository::{AiRunEventRepository, AiRunEventRepositoryError};
use fms_domain::ports::ai_run_repository::{AiRunRepository, AiRunRepositoryError};
use serde_json::Value;
use std::sync::Mutex;

/// Functional in-memory job repo for the cancel path (insert / find /
/// update_status / set_error_message); everything else fails loudly.
#[derive(Default)]
struct InMemoryJobRepository {
    jobs: Mutex<Vec<AiJobRecord>>,
}

#[async_trait]
impl AiJobRepository for InMemoryJobRepository {
    async fn insert(
        &self,
        job_id: &str,
        job_type: &str,
        requester_user_id: Option<&str>,
        correlation_id: Option<&str>,
        ontology_version: Option<&str>,
        risk_ceiling: Option<&str>,
    ) -> Result<AiJobRecord, AiJobRepositoryError> {
        let record = AiJobRecord {
            job_id: job_id.to_string(),
            job_type: job_type.to_string(),
            status: "pending".to_string(),
            requester_user_id: requester_user_id.map(str::to_string),
            ontology_version: ontology_version.map(str::to_string),
            context_policy: None,
            risk_ceiling: risk_ceiling.map(str::to_string),
            correlation_id: correlation_id.map(str::to_string),
            created_at: Utc::now(),
            started_at: None,
            finished_at: None,
            cancelled_at: None,
            error_code: None,
            error_message: None,
            timeout_ms: None,
            lease_owner: None,
            lease_expires_at: None,
            last_heartbeat_at: None,
            attempt_count: 0,
            max_attempts: 3,
            expires_at: None,
        };
        self.jobs.lock().unwrap().push(record.clone());
        Ok(record)
    }

    async fn find_by_id(&self, job_id: &str) -> Result<Option<AiJobRecord>, AiJobRepositoryError> {
        Ok(self.jobs.lock().unwrap().iter().find(|j| j.job_id == job_id).cloned())
    }

    async fn list(
        &self,
        _status_filter: Option<&str>,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<AiJobRecord>, AiJobRepositoryError> {
        Ok(self.jobs.lock().unwrap().clone())
    }

    async fn update_status(&self, job_id: &str, new_status: &str) -> Result<AiJobRecord, AiJobRepositoryError> {
        let mut jobs = self.jobs.lock().unwrap();
        let record = jobs
            .iter_mut()
            .find(|j| j.job_id == job_id)
            .ok_or_else(|| AiJobRepositoryError::Database(format!("job {job_id} not found")))?;
        record.status = new_status.to_string();
        Ok(record.clone())
    }

    async fn set_error_message(&self, job_id: &str, error_message: &str) -> Result<(), AiJobRepositoryError> {
        let mut jobs = self.jobs.lock().unwrap();
        if let Some(record) = jobs.iter_mut().find(|j| j.job_id == job_id) {
            record.error_message = Some(error_message.to_string());
        }
        Ok(())
    }

    async fn claim_pending(&self, _job_type: Option<&str>) -> Result<Option<AiJobRecord>, AiJobRepositoryError> {
        Err(AiJobRepositoryError::Database("unused".into()))
    }

    async fn lease_pending(
        &self,
        _job_type: Option<&str>,
        _lease_owner: &str,
        _lease_seconds: i64,
    ) -> Result<Option<AiJobRecord>, AiJobRepositoryError> {
        Err(AiJobRepositoryError::Database("unused".into()))
    }

    async fn list_expired_leases(
        &self,
        _now: chrono::DateTime<Utc>,
        _limit: i64,
    ) -> Result<Vec<AiJobRecord>, AiJobRepositoryError> {
        Err(AiJobRepositoryError::Database("unused".into()))
    }

    async fn heartbeat(
        &self,
        _job_id: &str,
        _lease_owner: &str,
        _lease_seconds: i64,
    ) -> Result<bool, AiJobRepositoryError> {
        Err(AiJobRepositoryError::Database("unused".into()))
    }

    async fn take_over(
        &self,
        _job_id: &str,
        _new_owner: &str,
        _lease_seconds: i64,
    ) -> Result<Option<AiJobRecord>, AiJobRepositoryError> {
        Err(AiJobRepositoryError::Database("unused".into()))
    }

    async fn count_by_status(&self) -> Result<Vec<AiJobStatusCount>, AiJobRepositoryError> {
        Err(AiJobRepositoryError::Database("unused".into()))
    }
}

#[derive(Default)]
struct InMemoryRunRepository {
    runs: Mutex<Vec<AiRunRecord>>,
}

#[async_trait]
impl AiRunRepository for InMemoryRunRepository {
    async fn insert(
        &self,
        run_id: &str,
        job_id: &str,
        runtime_engine: &str,
        model_id: Option<&str>,
        input_envelope: Option<Value>,
    ) -> Result<AiRunRecord, AiRunRepositoryError> {
        let record = AiRunRecord {
            run_id: run_id.to_string(),
            job_id: job_id.to_string(),
            runtime_engine: runtime_engine.to_string(),
            model_id: model_id.map(str::to_string),
            status: AiRunStatus::Pending.as_str().to_string(),
            input_envelope,
            output_raw: None,
            output_validated: None,
            token_usage: None,
            started_at: None,
            finished_at: None,
            error_code: None,
            error_message: None,
            created_at: Utc::now(),
        };
        self.runs.lock().unwrap().push(record.clone());
        Ok(record)
    }

    async fn find_by_id(&self, run_id: &str) -> Result<Option<AiRunRecord>, AiRunRepositoryError> {
        Ok(self.runs.lock().unwrap().iter().find(|r| r.run_id == run_id).cloned())
    }

    async fn list_for_job(&self, job_id: &str) -> Result<Vec<AiRunRecord>, AiRunRepositoryError> {
        Ok(self
            .runs
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.job_id == job_id)
            .cloned()
            .collect())
    }

    async fn count_active(&self, _entity_id: Option<&str>) -> Result<i64, AiRunRepositoryError> {
        Ok(0)
    }

    async fn update_status(&self, run_id: &str, new_status: &str) -> Result<AiRunRecord, AiRunRepositoryError> {
        let mut runs = self.runs.lock().unwrap();
        let record = runs
            .iter_mut()
            .find(|r| r.run_id == run_id)
            .ok_or_else(|| AiRunRepositoryError::not_found(run_id))?;
        record.status = new_status.to_string();
        Ok(record.clone())
    }

    async fn update_input_envelope(&self, _run_id: &str, _input_envelope: Value) -> Result<(), AiRunRepositoryError> {
        Ok(())
    }

    async fn fill_terminal_outputs(
        &self,
        _run_id: &str,
        _output_raw: Option<Value>,
        _output_validated: Option<Value>,
        _token_usage: Option<Value>,
    ) -> Result<(), AiRunRepositoryError> {
        Ok(())
    }

    async fn complete(
        &self,
        _run_id: &str,
        _output_raw: Option<Value>,
        _output_validated: Option<Value>,
        _token_usage: Option<Value>,
    ) -> Result<(), AiRunRepositoryError> {
        Ok(())
    }

    async fn fill_terminal_error(
        &self,
        _run_id: &str,
        _error_code: Option<&str>,
        _error_message: Option<&str>,
        _output_raw: Option<Value>,
    ) -> Result<(), AiRunRepositoryError> {
        Ok(())
    }

    async fn fail(
        &self,
        _run_id: &str,
        _error_code: Option<&str>,
        _error_message: Option<&str>,
        _output_raw: Option<Value>,
    ) -> Result<(), AiRunRepositoryError> {
        Ok(())
    }
}

struct StubRunEventRepository;

#[async_trait]
impl AiRunEventRepository for StubRunEventRepository {
    async fn insert(
        &self,
        _job_id: &str,
        _run_id: &str,
        _event_type: &str,
        _payload: Option<Value>,
    ) -> Result<AiRunEventRecord, AiRunEventRepositoryError> {
        Err(AiRunEventRepositoryError::Database("unused".into()))
    }

    async fn insert_fire_and_forget(
        &self,
        _job_id: &str,
        _run_id: &str,
        _event_type: &str,
        _payload: Option<Value>,
    ) -> Result<(), AiRunEventRepositoryError> {
        Ok(())
    }

    async fn list_for_run(
        &self,
        _run_id: &str,
        _limit: i64,
    ) -> Result<Vec<AiRunEventRecord>, AiRunEventRepositoryError> {
        Ok(Vec::new())
    }

    async fn count_by_job_ids_before(
        &self,
        _job_ids: &[String],
        _older_than: chrono::DateTime<Utc>,
    ) -> Result<i64, AiRunEventRepositoryError> {
        Ok(0)
    }

    async fn delete_by_job_ids_before(
        &self,
        _job_ids: &[String],
        _older_than: chrono::DateTime<Utc>,
    ) -> Result<u64, AiRunEventRepositoryError> {
        Ok(0)
    }

    async fn count_smoke_readiness_blocks(&self, _event_type: &str) -> Result<i64, AiRunEventRepositoryError> {
        Ok(0)
    }
}

#[tokio::test]
async fn cancel_job_enqueues_cancel_run_command_for_active_runs() {
    let (control, _tool_call_repo, command_repo, _checkpoint_repo) = build_control_service();
    let job_repo = Arc::new(InMemoryJobRepository::default());
    let run_repo = Arc::new(InMemoryRunRepository::default());
    let job_service = AiJobService::new(job_repo.clone(), run_repo.clone(), Arc::new(StubRunEventRepository))
        .with_control_service(control);

    let job = job_service
        .create_job("chat", Some("user-1"), None, None, None)
        .await
        .unwrap();
    let run = job_service
        .create_run(
            &job.job_id,
            "python-ai-runtime",
            None,
            Some(json!({"entity_id": "query_ops"})),
        )
        .await
        .unwrap();

    job_service
        .cancel_job(&job.job_id, Some("operator cancel"))
        .await
        .expect("cancel_job succeeds");

    // DB semantics preserved: job + run are Cancelled.
    let job_after = job_repo.find_by_id(&job.job_id).await.unwrap().unwrap();
    assert_eq!(job_after.status, "cancelled");
    let run_after = run_repo.find_by_id(&run.run_id).await.unwrap().unwrap();
    assert_eq!(run_after.status, AiRunStatus::Cancelled.as_str());

    // D4: a cancel_run command was enqueued for the owning worker.
    let commands = command_repo.snapshot();
    let cancel_cmds: Vec<_> = commands
        .iter()
        .filter(|c| c.command_type == AiRuntimeCommandType::CancelRun)
        .collect();
    assert_eq!(cancel_cmds.len(), 1, "cancel_job must enqueue cancel_run");
    assert_eq!(cancel_cmds[0].run_id, run.run_id);
    assert_eq!(
        cancel_cmds[0].payload.get("job_id").and_then(|v| v.as_str()),
        Some(job.job_id.as_str())
    );
    assert_eq!(
        cancel_cmds[0].payload.get("requester_user_id").and_then(|v| v.as_str()),
        Some("user-1")
    );
    assert_eq!(cancel_cmds[0].status, AiRuntimeCommandStatus::Pending);
}
