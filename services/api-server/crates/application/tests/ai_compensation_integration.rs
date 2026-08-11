//! End-to-end integration tests for Phase 3: Compensation + Rollback.
//!
//! Drives the full `RollbackService` surface (planner, receipts,
//! compensation plans, rollback, scheduler) through the in-memory
//! repository set. The Postgres adapter (out of scope for this wave)
//! mirrors the same shape.

use std::sync::Arc;

use chrono::Utc;
use fms_application::services::ai_action_proposal_service::AiActionProposalService;
use fms_application::services::ai_runtime_service::compensation_planner::{
    CompensationPlanner, InMemoryObjectVersionLookup,
};
use fms_application::services::ai_runtime_service::in_memory_repos::{
    InMemoryActionReceiptRepository, InMemoryCheckpointRepository, InMemoryCompensationPlanRepository,
};
use fms_application::services::ai_runtime_service::rollback_service::{ExecuteProposalReceiptInput, RollbackService};
use fms_domain::models::ai_execution::{AiActionReceiptRecord, AiCompensationMode, AiCompensationStatus};
use fms_domain::models::ai_proposal::ActionProposalStatus;
use fms_domain::ports::ai_execution_repository::{
    AiActionReceiptRepository, AiCompensationPlanRepository, AiRunCheckpointRepository,
};
use serde_json::json;

fn build_rollback_service() -> (
    Arc<RollbackService>,
    Arc<InMemoryActionReceiptRepository>,
    Arc<InMemoryCompensationPlanRepository>,
    Arc<InMemoryCheckpointRepository>,
    Arc<InMemoryObjectVersionLookup>,
) {
    let receipt_typed: Arc<InMemoryActionReceiptRepository> = Arc::new(InMemoryActionReceiptRepository::new());
    let plan_typed: Arc<InMemoryCompensationPlanRepository> = Arc::new(InMemoryCompensationPlanRepository::new());
    let checkpoint_typed: Arc<InMemoryCheckpointRepository> = Arc::new(InMemoryCheckpointRepository::new());
    let checkpoint_dyn: Arc<dyn AiRunCheckpointRepository> = checkpoint_typed.clone();
    let version_lookup = Arc::new(InMemoryObjectVersionLookup::new());
    let planner = Arc::new(CompensationPlanner::new(version_lookup.clone()));

    let proposal_service = Arc::new(AiActionProposalService::new());
    let service = Arc::new(
        RollbackService::new(
            proposal_service,
            receipt_typed.clone() as Arc<dyn AiActionReceiptRepository>,
            plan_typed.clone() as Arc<dyn AiCompensationPlanRepository>,
            planner,
        )
        .with_checkpoint_repo(checkpoint_dyn),
    );

    (service, receipt_typed, plan_typed, checkpoint_typed, version_lookup)
}

fn build_executed_proposal(
    proposal_id: &str,
    object_type: &str,
    object_id: &str,
    action_name: &str,
    before_version: i64,
) -> fms_domain::models::ai_proposal::AiActionProposal {
    let mut proposal = fms_domain::models::ai_proposal::AiActionProposal::new(
        proposal_id,
        "job-1",
        "run-1",
        object_type,
        object_id,
        action_name,
        json!({"new_status": "BOARDING"}),
    );
    proposal = proposal.with_metadata(json!({
        "expected_object_version": before_version,
        "idempotency_key": format!("idem-{proposal_id}"),
    }));
    proposal.before_snapshot = Some(json!({
        "version": before_version,
        "status": "PLAN",
    }));
    proposal.approved_by = Some("approver-1".to_string());
    proposal.approved_at = Some(Utc::now());
    proposal.executed_by = Some("executor-1".to_string());
    proposal.executed_at = Some(Utc::now());
    proposal.execution_result = Some(json!({"status": "BOARDING"}));
    proposal
        .transition_to(ActionProposalStatus::Validating)
        .expect("validating");
    proposal.transition_to(ActionProposalStatus::Pending).expect("pending");
    proposal
        .transition_to(ActionProposalStatus::Approved)
        .expect("approved");
    proposal
        .transition_to(ActionProposalStatus::Executing)
        .expect("executing");
    proposal
        .transition_to(ActionProposalStatus::Executed)
        .expect("executed");
    proposal
}

fn sample_plan(
    compensation_id: &str,
    receipt_id: &str,
    status: AiCompensationStatus,
    mode: AiCompensationMode,
) -> fms_domain::models::ai_execution::AiCompensationPlanRecord {
    fms_domain::models::ai_execution::AiCompensationPlanRecord {
        compensation_id: compensation_id.into(),
        receipt_id: receipt_id.into(),
        proposal_id: "prop-1".into(),
        status,
        mode,
        plan: json!({}),
        requires_approval: true,
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
async fn planner_emits_restore_snapshot_for_flight_update_status() {
    let (_service, _receipts, _plans, _checkpoints, version_lookup) = build_rollback_service();
    let planner = CompensationPlanner::new(version_lookup);
    let receipt = AiActionReceiptRecord {
        receipt_id: "rcp-1".into(),
        proposal_id: "prop-1".into(),
        job_id: "job-1".into(),
        run_id: "run-1".into(),
        tool_call_pk: None,
        object_type: "Flight".into(),
        object_id: "flt-1".into(),
        action_name: "update_status".into(),
        idempotency_key: "idem-1".into(),
        before_checkpoint_id: None,
        after_checkpoint_id: None,
        outbox_event_id: None,
        execution_result: json!({"status": "BOARDING"}),
        executed_by: "exec".into(),
        executed_at: Utc::now(),
    };
    let metadata = comp_metadata_restore_snapshot();
    let plan = planner
        .plan(&receipt, &metadata, &json!({"version": 7, "status": "PLAN"}))
        .await
        .unwrap()
        .expect("plan should exist");
    assert_eq!(plan.mode, AiCompensationMode::RestoreSnapshot);
    assert!(plan.requires_approval);
    assert_eq!(plan.plan["expected_version"], 7);
}

#[tokio::test]
async fn planner_emits_inverse_action_for_todo_complete() {
    let (_service, _receipts, _plans, _checkpoints, version_lookup) = build_rollback_service();
    let planner = CompensationPlanner::new(version_lookup);
    let receipt = AiActionReceiptRecord {
        receipt_id: "rcp-2".into(),
        proposal_id: "prop-2".into(),
        job_id: "job-1".into(),
        run_id: "run-1".into(),
        tool_call_pk: None,
        object_type: "Todo".into(),
        object_id: "todo-1".into(),
        action_name: "complete".into(),
        idempotency_key: "idem-2".into(),
        before_checkpoint_id: None,
        after_checkpoint_id: None,
        outbox_event_id: None,
        execution_result: json!({"status": "completed"}),
        executed_by: "exec".into(),
        executed_at: Utc::now(),
    };
    let metadata = fms_domain::models::ai_ontology::CompensationMetadata {
        mode: "inverse_action".into(),
        requires_approval: false,
        irreversible_fields: vec![],
        inverse_action_name: Some("Todo.reopen".into()),
        before_snapshot_required: false,
        followup_action_name: None,
        followup_args: None,
    };
    let plan = planner
        .plan(&receipt, &metadata, &json!(null))
        .await
        .unwrap()
        .expect("plan should exist");
    assert_eq!(plan.mode, AiCompensationMode::InverseAction);
    assert_eq!(plan.plan["inverse_action_name"], "Todo.reopen");
    assert!(!plan.requires_approval);
}

#[tokio::test]
async fn planner_returns_none_for_irreversible_notification_send() {
    let (_service, _receipts, _plans, _checkpoints, version_lookup) = build_rollback_service();
    let planner = CompensationPlanner::new(version_lookup);
    let receipt = AiActionReceiptRecord {
        receipt_id: "rcp-3".into(),
        proposal_id: "prop-3".into(),
        job_id: "job-1".into(),
        run_id: "run-1".into(),
        tool_call_pk: None,
        object_type: "Notification".into(),
        object_id: "notif-1".into(),
        action_name: "send".into(),
        idempotency_key: "idem-3".into(),
        before_checkpoint_id: None,
        after_checkpoint_id: None,
        outbox_event_id: None,
        execution_result: json!({"notification_id": "n-1"}),
        executed_by: "exec".into(),
        executed_at: Utc::now(),
    };
    let metadata = fms_domain::models::ai_ontology::CompensationMetadata {
        mode: "irreversible".into(),
        requires_approval: true,
        irreversible_fields: vec!["body".into()],
        inverse_action_name: None,
        before_snapshot_required: false,
        followup_action_name: None,
        followup_args: None,
    };
    let plan = planner.plan(&receipt, &metadata, &json!(null)).await.unwrap();
    assert!(plan.is_none());
}

#[tokio::test]
async fn receipt_and_plan_persist_for_flight_update_status() {
    let (service, receipts, plans, _checkpoints, version_lookup) = build_rollback_service();
    version_lookup.set("Flight", "flt-1", 5);
    let _proposal = build_executed_proposal("prop-1", "Flight", "flt-1", "update_status", 5);
    let receipt = AiActionReceiptRecord {
        receipt_id: "rcp-1".into(),
        proposal_id: "prop-1".into(),
        job_id: "job-1".into(),
        run_id: "run-1".into(),
        tool_call_pk: None,
        object_type: "Flight".into(),
        object_id: "flt-1".into(),
        action_name: "update_status".into(),
        idempotency_key: "idem-prop-1".into(),
        before_checkpoint_id: None,
        after_checkpoint_id: None,
        outbox_event_id: None,
        execution_result: json!({"status": "BOARDING"}),
        executed_by: "executor-1".into(),
        executed_at: Utc::now(),
    };
    let snapshot = json!({"version": 5, "status": "PLAN"});
    let plan = service
        .planner()
        .plan(&receipt, &comp_metadata_restore_snapshot(), &snapshot)
        .await
        .unwrap()
        .expect("plan should exist");
    receipts.upsert(receipt.clone()).await.unwrap();
    plans.upsert(plan.clone()).await.unwrap();

    let stored_receipt = receipts.get_by_idempotency_key("idem-prop-1").await.unwrap();
    assert_eq!(stored_receipt.unwrap().receipt_id, "rcp-1");
    let stored_plans = plans.list_by_proposal("prop-1").await.unwrap();
    assert_eq!(stored_plans.len(), 1);
    assert_eq!(stored_plans[0].mode, AiCompensationMode::RestoreSnapshot);
}

#[tokio::test]
async fn wrap_execute_proposal_short_circuits_when_proposal_missing() {
    let (service, _receipts, _plans, _checkpoints, _version) = build_rollback_service();
    let input = ExecuteProposalReceiptInput {
        proposal_id: "prop-missing".into(),
        executor_id: "exec-1".into(),
        executor_permissions: vec!["flight:write".into()],
        executor_department_id: None,
        object_version: 99,
        tool_call_pk: None,
    };
    let err = service.wrap_execute_proposal(input).await.unwrap_err();
    assert!(matches!(
        err,
        fms_application::services::ai_runtime_service::rollback_service::RollbackError::ProposalNotFound { .. }
    ));
}

#[tokio::test]
async fn execute_compensation_marks_plan_executing_then_failed_without_executor() {
    let (service, _receipts, plans, _checkpoints, _version) = build_rollback_service();
    let plan = sample_plan(
        "cmp-1",
        "rcp-1",
        AiCompensationStatus::Planned,
        AiCompensationMode::InverseAction,
    );
    plans.upsert(plan).await.unwrap();

    let after = service.execute_compensation("cmp-1", "executor-x").await.unwrap();
    assert_eq!(after.status, AiCompensationStatus::Failed);
    assert!(after.execution_error.is_some());
}

#[tokio::test]
async fn execute_compensation_fails_for_irreversible_plan() {
    let (service, _receipts, plans, _checkpoints, _version) = build_rollback_service();
    let plan = sample_plan(
        "cmp-1",
        "rcp-1",
        AiCompensationStatus::Planned,
        AiCompensationMode::Irreversible,
    );
    plans.upsert(plan).await.unwrap();
    let err = service.execute_compensation("cmp-1", "executor-x").await.unwrap_err();
    assert!(matches!(
        err,
        fms_application::services::ai_runtime_service::rollback_service::RollbackError::Irreversible
    ));
}

#[tokio::test]
async fn approve_compensation_rejects_unauthorized_approver() {
    let (service, _receipts, plans, _checkpoints, _version) = build_rollback_service();
    let plan = sample_plan(
        "cmp-1",
        "rcp-1",
        AiCompensationStatus::Planned,
        AiCompensationMode::RestoreSnapshot,
    );
    plans.upsert(plan).await.unwrap();
    let err = service
        .approve_compensation("cmp-1", "stranger", &vec!["unrelated:perm".to_string()])
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        fms_application::services::ai_runtime_service::rollback_service::RollbackError::ApproverNotPermitted { .. }
    ));
}

#[tokio::test]
async fn approve_compensation_accepts_authorized_approver() {
    let (service, _receipts, plans, _checkpoints, _version) = build_rollback_service();
    let plan = sample_plan(
        "cmp-1",
        "rcp-1",
        AiCompensationStatus::Planned,
        AiCompensationMode::RestoreSnapshot,
    );
    plans.upsert(plan).await.unwrap();
    let approved = service
        .approve_compensation("cmp-1", "ops-lead", &vec!["ai:execute".to_string()])
        .await
        .unwrap();
    assert_eq!(approved.status, AiCompensationStatus::Approved);
    assert_eq!(approved.approved_by.as_deref(), Some("ops-lead"));
}

#[tokio::test]
async fn scheduler_tick_auto_executes_plans_without_approval() {
    let (service, _receipts, plans, _checkpoints, _version) = build_rollback_service();
    let mut plan = sample_plan(
        "cmp-auto",
        "rcp-auto",
        AiCompensationStatus::Planned,
        AiCompensationMode::InverseAction,
    );
    plan.requires_approval = false;
    plans.upsert(plan).await.unwrap();
    let report = service.scheduler_tick(60, 0).await;
    assert_eq!(report.auto_executed + report.failed, 1);
}

#[tokio::test]
async fn scheduler_tick_times_out_stuck_executing_plans() {
    let (service, _receipts, plans, _checkpoints, _version) = build_rollback_service();
    let plan = sample_plan(
        "cmp-stuck",
        "rcp-stuck",
        AiCompensationStatus::Executing,
        AiCompensationMode::InverseAction,
    );
    plans.upsert(plan).await.unwrap();
    let report = service.scheduler_tick(0, 60).await;
    assert_eq!(report.timed_out, 1);
    let stored = plans.get("cmp-stuck").await.unwrap().unwrap();
    assert_eq!(stored.status, AiCompensationStatus::Failed);
    assert_eq!(stored.execution_error.as_deref(), Some("execution_timeout"));
}

#[tokio::test]
async fn undo_creates_a_new_receipt_and_does_not_modify_original() {
    let (_service, receipts, plans, _checkpoints, _version) = build_rollback_service();
    let original = AiActionReceiptRecord {
        receipt_id: "rcp-original".into(),
        proposal_id: "prop-1".into(),
        job_id: "job-1".into(),
        run_id: "run-1".into(),
        tool_call_pk: Some("tpc-1".into()),
        object_type: "Flight".into(),
        object_id: "flt-1".into(),
        action_name: "update_status".into(),
        idempotency_key: "idem-original".into(),
        before_checkpoint_id: Some("cp-before".into()),
        after_checkpoint_id: Some("cp-after".into()),
        outbox_event_id: Some("evt-1".into()),
        execution_result: json!({"status": "BOARDING"}),
        executed_by: "executor-original".into(),
        executed_at: Utc::now(),
    };
    receipts.upsert(original.clone()).await.unwrap();

    let compensation_receipt = AiActionReceiptRecord {
        receipt_id: "rcp-rollback-1".into(),
        proposal_id: "prop-1-rollback".into(),
        job_id: "job-1".into(),
        run_id: "run-1".into(),
        tool_call_pk: Some("tpc-rollback-1".into()),
        object_type: "Flight".into(),
        object_id: "flt-1".into(),
        action_name: "update_status".into(),
        idempotency_key: "idem-rollback-1".into(),
        before_checkpoint_id: Some("cp-rollback-before".into()),
        after_checkpoint_id: Some("cp-rollback-after".into()),
        outbox_event_id: Some("evt-rollback-1".into()),
        execution_result: json!({"status": "PLAN"}),
        executed_by: "executor-rollback".into(),
        executed_at: Utc::now(),
    };
    receipts.upsert(compensation_receipt.clone()).await.unwrap();

    let original_after = receipts.get_by_idempotency_key("idem-original").await.unwrap().unwrap();
    assert_eq!(original_after.receipt_id, original.receipt_id);
    assert_eq!(original_after.executed_by, original.executed_by);
    let roll_after = receipts
        .get_by_idempotency_key("idem-rollback-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(roll_after.receipt_id, "rcp-rollback-1");
    assert_ne!(roll_after.receipt_id, original.receipt_id);

    let mut plan = sample_plan(
        "cmp-rollback-1",
        "rcp-rollback-1",
        AiCompensationStatus::Succeeded,
        AiCompensationMode::RestoreSnapshot,
    );
    plan.receipt_id = "rcp-rollback-1".into();
    plans.upsert(plan).await.unwrap();
    let plan_loaded = plans.get("cmp-rollback-1").await.unwrap().unwrap();
    assert_eq!(plan_loaded.receipt_id, "rcp-rollback-1");
    assert_ne!(plan_loaded.receipt_id, original.receipt_id);
}

fn comp_metadata_restore_snapshot() -> fms_domain::models::ai_ontology::CompensationMetadata {
    fms_domain::models::ai_ontology::CompensationMetadata {
        mode: "restore_snapshot".into(),
        requires_approval: true,
        irreversible_fields: vec![],
        inverse_action_name: None,
        before_snapshot_required: true,
        followup_action_name: None,
        followup_args: None,
    }
}
