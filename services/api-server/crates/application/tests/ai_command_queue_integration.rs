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
