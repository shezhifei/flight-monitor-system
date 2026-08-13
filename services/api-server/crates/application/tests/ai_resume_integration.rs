//! End-to-end integration test for checkpoints and resume.
//!
//! Drives the resume API path through:
//! 1. Insert a `run_input` + `before_tool` + `after_tool` checkpoint
//!    sequence into the in-memory checkpoint repo.
//! 2. Call the resume handler logic (without spinning up a full
//!    Actix test server) by invoking
//!    [`AiExecutionControlService::enqueue_resume_run`] and
//!    [`AiExecutionControlService::latest_recoverable_checkpoint`]
//!    directly. The route itself is exercised in `crates/api`.
//! 3. Verify the resulting `ResumeRun` command is queued with the
//!    checkpoint payload.

use std::sync::Arc;

use fms_application::services::ai_runtime_service::ai_execution_control_service::{
    AiExecutionControlService, RunInputCheckpointSummary,
};
use fms_application::services::ai_runtime_service::in_memory_repos::{
    InMemoryCheckpointRepository, InMemoryRuntimeCommandRepository, InMemoryToolCallRepository,
};
use fms_application::services::ai_runtime_service::tool_authorization_service::{
    StaticFeatureFlagSource, ToolAuthorizationService,
};
use fms_domain::models::ai_execution::{AiRunCheckpointRecord, AiRunCheckpointType, AiRuntimeCommandType};
use fms_domain::ports::ai_execution_repository::AiRunCheckpointRepository;
use serde_json::json;

fn build_control_service() -> (
    AiExecutionControlService,
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
    let control = AiExecutionControlService::new(
        tool_call_repo.clone() as Arc<dyn fms_domain::ports::ai_execution_repository::AiToolCallRepository>,
        command_repo.clone() as Arc<dyn fms_domain::ports::ai_execution_repository::AiRuntimeCommandRepository>,
        authorization,
    )
    .with_checkpoint_repo(checkpoint_repo.clone() as Arc<dyn AiRunCheckpointRepository>);
    (control, tool_call_repo, command_repo, checkpoint_repo)
}

async fn seed_three_checkpoints(checkpoint_repo: &InMemoryCheckpointRepository, run_id: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let samples: [(AiRunCheckpointType, &str); 3] = [
        (AiRunCheckpointType::RunInput, "run_input"),
        (AiRunCheckpointType::BeforeTool, "before_tool"),
        (AiRunCheckpointType::AfterTool, "after_tool"),
    ];
    for (i, (kind, _label)) in samples.into_iter().enumerate() {
        let id = format!("cp-{i}");
        ids.push(id.clone());
        let record = AiRunCheckpointRecord {
            checkpoint_id: id,
            job_id: "job-1".into(),
            run_id: run_id.into(),
            sequence_no: (i as i64) + 1,
            checkpoint_type: kind,
            tool_call_pk: None,
            proposal_id: None,
            snapshot_hash: format!("h-{i}"),
            snapshot: json!({"seq": i}),
            snapshot_size_bytes: 16,
            mq_message_id: None,
            created_at: chrono::Utc::now(),
        };
        let inserted = checkpoint_repo.upsert(record).await.unwrap();
        assert!(inserted, "checkpoint seq={} should be inserted", i);
    }
    ids
}

#[tokio::test]
async fn resume_after_three_checkpoints_enqueues_resume_run_command() {
    let (control, _, command_repo, checkpoint_repo) = build_control_service();
    let ids = seed_three_checkpoints(&checkpoint_repo, "run-1").await;
    assert_eq!(ids.len(), 3);

    // Latest recoverable should be the AfterTool checkpoint (sequence 3).
    let latest = control
        .latest_recoverable_checkpoint("run-1")
        .await
        .unwrap()
        .expect("latest recoverable exists");
    assert_eq!(latest.sequence_no, 3);
    assert_eq!(latest.checkpoint_type, AiRunCheckpointType::AfterTool);

    let command = control
        .enqueue_resume_run("job-1", "run-1", &latest, "user-1")
        .await
        .unwrap();
    assert_eq!(command.command_type, AiRuntimeCommandType::ResumeRun);
    assert_eq!(
        command.payload.get("checkpoint_id").and_then(|v| v.as_str()),
        Some("cp-2")
    );
    assert_eq!(
        command.payload.get("requester_user_id").and_then(|v| v.as_str()),
        Some("user-1")
    );

    let commands = command_repo.snapshot();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].command_type, AiRuntimeCommandType::ResumeRun);
}

#[tokio::test]
async fn resume_with_explicit_checkpoint_id_picks_that_row() {
    let (control, _, command_repo, checkpoint_repo) = build_control_service();
    let _ = seed_three_checkpoints(&checkpoint_repo, "run-2").await;

    let all = control.list_all_checkpoints("run-2").await.unwrap();
    let explicit = all
        .iter()
        .find(|row| row.checkpoint_id == "cp-1")
        .expect("cp-1 exists")
        .clone();

    let command = control
        .enqueue_resume_run("job-1", "run-2", &explicit, "user-1")
        .await
        .unwrap();
    assert_eq!(
        command.payload.get("checkpoint_id").and_then(|v| v.as_str()),
        Some("cp-1")
    );
    assert_eq!(command_repo.snapshot().len(), 1);
}

#[tokio::test]
async fn resume_run_input_checkpoint_persists_via_direct_call() {
    let (control, _, _, checkpoint_repo) = build_control_service();
    let record = control
        .create_run_input_checkpoint(
            "job-1",
            "run-3",
            json!({"question": "hi"}),
            RunInputCheckpointSummary {
                governance_hash: "g-1".into(),
                tool_schema_hash: "t-1".into(),
                model_id: Some("m-1".into()),
                prompt_cache_key_hash: "p-1".into(),
            },
        )
        .await
        .unwrap()
        .expect("checkpoint repo is configured");
    assert_eq!(record.checkpoint_type, AiRunCheckpointType::RunInput);
    assert_eq!(checkpoint_repo.len(), 1);
    let all = control.list_all_checkpoints("run-3").await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].checkpoint_type, AiRunCheckpointType::RunInput);
}

#[tokio::test]
async fn resume_returns_404_when_no_recoverable_checkpoint() {
    let (control, _, _, checkpoint_repo) = build_control_service();
    let record = AiRunCheckpointRecord {
        checkpoint_id: "cp-only-input".into(),
        job_id: "job-1".into(),
        run_id: "run-4".into(),
        sequence_no: 1,
        checkpoint_type: AiRunCheckpointType::RunInput,
        tool_call_pk: None,
        proposal_id: None,
        snapshot_hash: "h".into(),
        snapshot: json!({}),
        snapshot_size_bytes: 2,
        mq_message_id: None,
        created_at: chrono::Utc::now(),
    };
    checkpoint_repo.upsert(record).await.unwrap();
    let latest = control.latest_recoverable_checkpoint("run-4").await.unwrap();
    assert!(latest.is_none());
}
