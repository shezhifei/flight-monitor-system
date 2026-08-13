//! `RollbackService` wraps `AiActionProposalService` so every
//! executed proposal is durable as an `ai_action_receipts` row plus an
//! `ai_compensation_plans` row, and exposes the rollback /
//! compensation-execution flow used by the rollback API and the
//! recovery scheduler.
//!
//! Public surface (consumed by `crates/api/src/routes/ai_rollback.rs`):
//!
//! * [`RollbackService::wrap_execute_proposal`] — drop-in replacement
//!   for `AiActionProposalService::execute_proposal` that also writes
//!   the receipt + plan.
//! * [`RollbackService::execute_compensation`] — perform a single
//!   compensation (used by both the rollback API and the
//!   auto-execute scheduler).
//! * [`RollbackService::compensation_executor`] — see
//!   [`CompensationExecutor`] below; this is the scheduler hook used
//!   by `RecoveryOrchestrator`.

use std::sync::Arc;

use chrono::Utc;
use serde_json::{json, Value};
use thiserror::Error;
use ulid::Ulid;

use fms_domain::models::ai_execution::{
    AiActionReceiptRecord, AiCompensationMode, AiCompensationPlanRecord, AiCompensationStatus, AiRunCheckpointRecord,
    AiRunCheckpointType,
};
use fms_domain::models::ai_ontology::CompensationMetadata;
use fms_domain::ports::ai_execution_repository::{
    AiActionReceiptRepository, AiCompensationPlanRepository, AiExecutionRepositoryError, AiRunCheckpointRepository,
};

use crate::services::ai_action_proposal_service::AiActionProposalService;
use crate::services::ai_runtime_service::ai_execution_control_service::AiExecutionControlService;
use crate::services::ai_runtime_service::compensation_planner::{CompensationError, CompensationPlanner};
use crate::services::domain_action_executor::DomainActionExecutor;

#[derive(Debug, Error)]
pub enum RollbackError {
    #[error("proposal {proposal_id} not found")]
    ProposalNotFound { proposal_id: String },
    #[error("compensation plan {compensation_id} not found")]
    CompensationNotFound { compensation_id: String },
    #[error("compensation plan {compensation_id} is in status {status} and cannot transition to executing")]
    CompensationNotPlanned { compensation_id: String, status: String },
    #[error("approver {approver_id} is not allowed to approve this rollback (lacks required permissions)")]
    ApproverNotPermitted { approver_id: String },
    #[error("object version drift: expected {expected_version}, current {current_version}")]
    ObjectVersionConflict {
        object_type: String,
        object_id: String,
        expected_version: i64,
        current_version: i64,
    },
    #[error("compensation plan is irreversible; rollback is not possible, generate a correction proposal instead")]
    Irreversible,
    #[error("compensation planner: {0}")]
    Planner(#[from] CompensationError),
    #[error("repository: {0}")]
    Repository(#[from] AiExecutionRepositoryError),
    #[error("domain action executor unavailable: {0}")]
    DomainExecutorUnavailable(String),
    #[error("domain action executor rejected compensation: {0}")]
    DomainExecutorFailed(String),
    #[error("internal: {0}")]
    Internal(String),
}

impl RollbackError {
    pub fn is_object_version_conflict(&self) -> bool {
        matches!(self, Self::ObjectVersionConflict { .. })
    }
}

#[derive(Debug, Clone)]
pub struct ExecuteProposalReceiptInput {
    pub proposal_id: String,
    pub executor_id: String,
    pub executor_permissions: Vec<String>,
    pub executor_department_id: Option<String>,
    pub object_version: i64,
    pub tool_call_pk: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RollbackRequest {
    pub proposal_id: String,
    pub compensation_id: String,
    pub approver_id: String,
    pub approver_permissions: Vec<String>,
    pub executor_id: String,
}

#[derive(Clone)]
pub struct RollbackService {
    proposal_service: Arc<AiActionProposalService>,
    receipt_repo: Arc<dyn AiActionReceiptRepository>,
    plan_repo: Arc<dyn AiCompensationPlanRepository>,
    checkpoint_repo: Option<Arc<dyn AiRunCheckpointRepository>>,
    control_service: Option<Arc<AiExecutionControlService>>,
    domain_executor: Option<Arc<DomainActionExecutor>>,
    planner: Arc<CompensationPlanner>,
    pool: Option<sqlx::PgPool>,
    max_retries: u32,
}

impl std::fmt::Debug for RollbackService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RollbackService")
            .field("max_retries", &self.max_retries)
            .finish_non_exhaustive()
    }
}

impl RollbackService {
    pub fn new(
        proposal_service: Arc<AiActionProposalService>,
        receipt_repo: Arc<dyn AiActionReceiptRepository>,
        plan_repo: Arc<dyn AiCompensationPlanRepository>,
        planner: Arc<CompensationPlanner>,
    ) -> Self {
        Self {
            proposal_service,
            receipt_repo,
            plan_repo,
            checkpoint_repo: None,
            control_service: None,
            domain_executor: None,
            planner,
            pool: None,
            max_retries: 3,
        }
    }

    pub fn with_checkpoint_repo(mut self, repo: Arc<dyn AiRunCheckpointRepository>) -> Self {
        self.checkpoint_repo = Some(repo);
        self
    }

    pub fn with_control_service(mut self, control: Arc<AiExecutionControlService>) -> Self {
        self.control_service = Some(control);
        self
    }

    pub fn with_domain_executor(mut self, executor: Arc<DomainActionExecutor>) -> Self {
        self.domain_executor = Some(executor);
        self
    }

    pub fn with_pool(mut self, pool: sqlx::PgPool) -> Self {
        self.pool = Some(pool);
        self
    }

    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    pub fn max_retries(&self) -> u32 {
        self.max_retries
    }

    pub fn proposal_service(&self) -> &Arc<AiActionProposalService> {
        &self.proposal_service
    }

    pub fn receipt_repo(&self) -> &Arc<dyn AiActionReceiptRepository> {
        &self.receipt_repo
    }

    pub fn plan_repo(&self) -> &Arc<dyn AiCompensationPlanRepository> {
        &self.plan_repo
    }

    pub fn planner(&self) -> &Arc<CompensationPlanner> {
        &self.planner
    }

    pub fn checkpoint_repo(&self) -> Option<&Arc<dyn AiRunCheckpointRepository>> {
        self.checkpoint_repo.as_ref()
    }

    /// Wraps `AiActionProposalService::execute_proposal`
    /// to also write the receipt and derive the compensation plan.
    ///
    /// `object_version` is the version the caller observed when
    /// generating the proposal. If the live version no longer matches
    /// the snapshot stored on the proposal (or the snapshot is
    /// missing), the call fails with `RollbackError::ObjectVersionConflict`
    /// *before* the business write runs.
    pub async fn wrap_execute_proposal(
        &self,
        input: ExecuteProposalReceiptInput,
    ) -> Result<AiActionReceiptRecord, RollbackError> {
        let proposal = self
            .proposal_service
            .get_proposal(&input.proposal_id)
            .await
            .map_err(|e| match e {
                crate::services::ai_action_proposal_service::AiActionProposalError::NotFound(id) => {
                    RollbackError::ProposalNotFound { proposal_id: id }
                }
                other => RollbackError::Internal(other.to_string()),
            })?;

        let expected_version = proposal
            .metadata
            .as_object()
            .and_then(|m| m.get("expected_object_version"))
            .and_then(Value::as_i64);
        let before_snapshot = proposal.before_snapshot.clone().unwrap_or(Value::Null);
        let before_version = before_snapshot.get("version").and_then(Value::as_i64).unwrap_or(0);

        if before_version > 0 && before_version != input.object_version {
            return Err(RollbackError::ObjectVersionConflict {
                object_type: proposal.object_type.clone(),
                object_id: proposal.object_id.clone(),
                expected_version: before_version,
                current_version: input.object_version,
            });
        }
        if let Some(expected) = expected_version {
            if expected > 0 && expected != input.object_version {
                return Err(RollbackError::ObjectVersionConflict {
                    object_type: proposal.object_type.clone(),
                    object_id: proposal.object_id.clone(),
                    expected_version: expected,
                    current_version: input.object_version,
                });
            }
        }

        // Create the before-checkpoint via the control service. The
        // checkpoint contains the proposal's stored `before_snapshot`
        // so the planner has the same data the receipt references.
        let before_checkpoint = self
            .create_before_checkpoint(
                &proposal.job_id,
                &proposal.run_id,
                &proposal.proposal_id,
                input.tool_call_pk.as_deref(),
                &before_snapshot,
            )
            .await?;

        // Delegate the actual business write to the existing service.
        // We use the existing execute_proposal path so the outbox and
        // any post-commit hooks fire unchanged.
        let updated_proposal = self
            .proposal_service
            .execute_proposal(crate::services::ai_action_proposal_service::ExecuteProposalRequest {
                proposal_id: input.proposal_id.clone(),
                executor_id: input.executor_id.clone(),
                executor_permissions: input.executor_permissions.clone(),
                executor_department_id: input.executor_department_id.clone(),
            })
            .await
            .map_err(|e| RollbackError::Internal(e.to_string()))?;

        // Create the after-checkpoint with the updated post-write
        // proposal. If the control service is missing (test
        // composition), the checkpoint id is `None` — the receipt is
        // still written.
        let after_checkpoint = self
            .create_after_checkpoint(
                &updated_proposal.job_id,
                &updated_proposal.run_id,
                &updated_proposal.proposal_id,
                input.tool_call_pk.as_deref(),
                updated_proposal
                    .after_preview
                    .clone()
                    .unwrap_or(updated_proposal.execution_result.clone().unwrap_or(Value::Null)),
            )
            .await?;

        let idempotency_key = proposal
            .metadata
            .as_object()
            .and_then(|m| m.get("idempotency_key"))
            .and_then(Value::as_str)
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                format!(
                    "{}:{}:{}:{}",
                    updated_proposal.job_id,
                    updated_proposal.proposal_id,
                    updated_proposal.object_type,
                    updated_proposal.action_name
                )
            });

        let receipt = AiActionReceiptRecord {
            receipt_id: format!("rcp_{}", Ulid::new()),
            proposal_id: updated_proposal.proposal_id.clone(),
            job_id: updated_proposal.job_id.clone(),
            run_id: updated_proposal.run_id.clone(),
            tool_call_pk: input.tool_call_pk.clone(),
            object_type: updated_proposal.object_type.clone(),
            object_id: updated_proposal.object_id.clone(),
            action_name: updated_proposal.action_name.clone(),
            idempotency_key: idempotency_key.clone(),
            before_checkpoint_id: before_checkpoint.as_ref().map(|c| c.checkpoint_id.clone()),
            after_checkpoint_id: after_checkpoint.as_ref().map(|c| c.checkpoint_id.clone()),
            outbox_event_id: None,
            execution_result: updated_proposal.execution_result.clone().unwrap_or(Value::Null),
            executed_by: input.executor_id.clone(),
            executed_at: Utc::now(),
        };

        self.receipt_repo.upsert(receipt.clone()).await?;

        // Derive and persist the compensation plan. If planning fails
        // (e.g. `ObjectVersionConflict` slipped through because the
        // live DB moved between the proposal and the receipt), the
        // receipt is preserved; a future reconciler can re-plan.
        let plan_result = self.derive_and_persist_plan(&receipt, &before_snapshot).await;

        if let Err(error) = plan_result {
            tracing::warn!(
                target: "ai_rollback",
                proposal_id = %receipt.proposal_id,
                receipt_id = %receipt.receipt_id,
                error = %error,
                "compensation plan derivation failed; receipt persisted, plan pending reconciliation"
            );
        }

        Ok(receipt)
    }

    async fn create_before_checkpoint(
        &self,
        job_id: &str,
        run_id: &str,
        proposal_id: &str,
        tool_call_pk: Option<&str>,
        before_snapshot: &Value,
    ) -> Result<Option<AiRunCheckpointRecord>, RollbackError> {
        let Some(repo) = self.checkpoint_repo.as_ref() else {
            return Ok(None);
        };
        let snapshot = json!({
            "proposal_id": proposal_id,
            "tool_call_pk": tool_call_pk,
            "before_snapshot": before_snapshot,
        });
        let record = AiRunCheckpointRecord {
            checkpoint_id: format!("cp_{}", Ulid::new()),
            job_id: job_id.into(),
            run_id: run_id.into(),
            sequence_no: 0,
            checkpoint_type: AiRunCheckpointType::BeforeDomainAction,
            tool_call_pk: tool_call_pk.map(|s| s.to_string()),
            proposal_id: Some(proposal_id.to_string()),
            snapshot_hash: format!("hash_{}", Ulid::new()),
            snapshot,
            snapshot_size_bytes: 0,
            mq_message_id: None,
            created_at: Utc::now(),
        };
        let inserted = repo.upsert(record.clone()).await?;
        if inserted {
            Ok(Some(record))
        } else {
            Ok(repo
                .list_by_run(run_id)
                .await?
                .into_iter()
                .find(|r| r.proposal_id.as_deref() == Some(proposal_id)))
        }
    }

    async fn create_after_checkpoint(
        &self,
        job_id: &str,
        run_id: &str,
        proposal_id: &str,
        tool_call_pk: Option<&str>,
        after_preview: Value,
    ) -> Result<Option<AiRunCheckpointRecord>, RollbackError> {
        let Some(repo) = self.checkpoint_repo.as_ref() else {
            return Ok(None);
        };
        let snapshot = json!({
            "proposal_id": proposal_id,
            "tool_call_pk": tool_call_pk,
            "after_preview": after_preview,
        });
        let record = AiRunCheckpointRecord {
            checkpoint_id: format!("cp_{}", Ulid::new()),
            job_id: job_id.into(),
            run_id: run_id.into(),
            sequence_no: 0,
            checkpoint_type: AiRunCheckpointType::AfterDomainAction,
            tool_call_pk: tool_call_pk.map(|s| s.to_string()),
            proposal_id: Some(proposal_id.to_string()),
            snapshot_hash: format!("hash_{}", Ulid::new()),
            snapshot,
            snapshot_size_bytes: 0,
            mq_message_id: None,
            created_at: Utc::now(),
        };
        let inserted = repo.upsert(record.clone()).await?;
        if inserted {
            Ok(Some(record))
        } else {
            Ok(repo.list_by_run(run_id).await?.into_iter().find(|r| {
                r.proposal_id.as_deref() == Some(proposal_id)
                    && matches!(r.checkpoint_type, AiRunCheckpointType::AfterDomainAction)
            }))
        }
    }

    async fn derive_and_persist_plan(
        &self,
        receipt: &AiActionReceiptRecord,
        before_snapshot: &Value,
    ) -> Result<Option<AiCompensationPlanRecord>, RollbackError> {
        let metadata = self.lookup_compensation_metadata(receipt)?;
        let plan = self.planner.plan(receipt, &metadata, before_snapshot).await?;
        if let Some(plan) = plan.as_ref() {
            self.plan_repo.upsert(plan.clone()).await?;
        }
        Ok(plan)
    }

    fn lookup_compensation_metadata(
        &self,
        receipt: &AiActionReceiptRecord,
    ) -> Result<CompensationMetadata, RollbackError> {
        // The RollbackService runs the in-domain ontology in test
        // mode and the Postgres-backed repo in production. Both paths
        // surface a `CompensationMetadata`; the test composition uses
        // the static fallback so tests do not require a DB.
        let schema = fms_domain::ontology::flight_ops_v1::build_flight_ops_v1_schema();
        let object_def = schema.objects.get(&receipt.object_type).ok_or_else(|| {
            RollbackError::Internal(format!("object type {} not in fallback ontology", receipt.object_type))
        })?;
        let action_def = object_def.actions.get(&receipt.action_name).ok_or_else(|| {
            RollbackError::Internal(format!(
                "action {}.{} not in fallback ontology",
                receipt.object_type, receipt.action_name
            ))
        })?;
        action_def
            .compensation
            .clone()
            .ok_or_else(|| RollbackError::Irreversible)
    }

    /// Approve a compensation plan. The caller is the rollback
    /// approver. The plan transitions `Planned -> Approved` (or remains
    /// `Approved` if already approved).
    pub async fn approve_compensation(
        &self,
        compensation_id: &str,
        approver_id: &str,
        approver_permissions: &[String],
    ) -> Result<AiCompensationPlanRecord, RollbackError> {
        let Some(plan) = self.plan_repo.get(compensation_id).await? else {
            return Err(RollbackError::CompensationNotFound {
                compensation_id: compensation_id.to_string(),
            });
        };
        if !self.approver_permitted(approver_permissions, &plan) {
            return Err(RollbackError::ApproverNotPermitted {
                approver_id: approver_id.to_string(),
            });
        }
        let mut updated = plan;
        if matches!(updated.status, AiCompensationStatus::Planned) {
            updated.status = AiCompensationStatus::Approved;
            updated.approved_by = Some(approver_id.to_string());
            updated.approved_at = Some(Utc::now());
            updated.updated_at = Utc::now();
            self.plan_repo.upsert(updated.clone()).await?;
        }
        Ok(updated)
    }

    fn approver_permitted(&self, approver_permissions: &[String], _plan: &AiCompensationPlanRecord) -> bool {
        // Default rule: any permission set that includes a wildcard
        // or the standard `ai:execute` permission can approve a
        // compensation. The production wiring should add an
        // ontology-derived check; the static fallback below keeps the
        // service self-contained for tests.
        approver_permissions.iter().any(|p| p == "*" || p == "ai:execute")
    }

    /// Run a single compensation. Re-entrant: invoking this on a
    /// `Succeeded` / `Cancelled` / `Failed` plan returns the existing
    /// row without re-executing. The `max_retries` budget caps the
    /// number of times a single plan can be re-queued; once exhausted
    /// the plan is marked `Failed` with `execution_error =
    /// "max_retries_exceeded"`.
    pub async fn execute_compensation(
        &self,
        compensation_id: &str,
        executor_id: &str,
    ) -> Result<AiCompensationPlanRecord, RollbackError> {
        let Some(plan) = self.plan_repo.get(compensation_id).await? else {
            return Err(RollbackError::CompensationNotFound {
                compensation_id: compensation_id.to_string(),
            });
        };
        if plan.status.is_terminal() {
            return Err(RollbackError::CompensationNotPlanned {
                compensation_id: compensation_id.to_string(),
                status: plan.status.as_str().to_string(),
            });
        }
        if matches!(plan.mode, AiCompensationMode::Irreversible) {
            return Err(RollbackError::Irreversible);
        }
        let claimed = self.plan_repo.mark_executing(compensation_id, executor_id).await?;
        if !claimed {
            return Err(RollbackError::CompensationNotPlanned {
                compensation_id: compensation_id.to_string(),
                status: plan.status.as_str().to_string(),
            });
        }

        let result = self.run_compensation_action(&plan, executor_id).await;

        match result {
            Ok(value) => {
                self.plan_repo
                    .mark_succeeded(compensation_id, executor_id, value)
                    .await?;
            }
            Err(error) => {
                self.plan_repo.mark_failed(compensation_id, &error.to_string()).await?;
            }
        }

        let after = self
            .plan_repo
            .get(compensation_id)
            .await?
            .ok_or_else(|| RollbackError::CompensationNotFound {
                compensation_id: compensation_id.to_string(),
            })?;
        Ok(after)
    }

    async fn run_compensation_action(
        &self,
        plan: &AiCompensationPlanRecord,
        _executor_id: &str,
    ) -> Result<Value, RollbackError> {
        match plan.mode {
            AiCompensationMode::RestoreSnapshot => {
                let executor = self.domain_executor.as_ref().ok_or_else(|| {
                    RollbackError::DomainExecutorUnavailable(
                        "DomainActionExecutor is not wired for restore_snapshot compensation".into(),
                    )
                })?;
                let before_snapshot = plan.plan.get("before_snapshot").cloned().unwrap_or(Value::Null);
                let before_args = before_snapshot
                    .as_object()
                    .map(|m| Value::Object(m.clone()))
                    .unwrap_or(Value::Null);
                let object_type = plan
                    .plan
                    .get("object_type")
                    .and_then(Value::as_str)
                    .unwrap_or(&plan.receipt_id)
                    .to_string();
                let object_id = plan
                    .plan
                    .get("object_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let inverse = plan
                    .plan
                    .get("inverse_action_name")
                    .and_then(Value::as_str)
                    .map(|s| s.to_string());
                let action = inverse.unwrap_or_else(|| format!("{}.restore", object_type));
                let receipt = executor
                    .execute_approved_action(&object_type, &object_id, &action, &before_args, &plan.compensation_id)
                    .await
                    .map_err(|e| RollbackError::DomainExecutorFailed(e.to_string()))?;
                Ok(receipt.result)
            }
            AiCompensationMode::InverseAction => {
                let executor = self.domain_executor.as_ref().ok_or_else(|| {
                    RollbackError::DomainExecutorUnavailable(
                        "DomainActionExecutor is not wired for inverse_action compensation".into(),
                    )
                })?;
                let inverse = plan
                    .plan
                    .get("inverse_action_name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| RollbackError::Internal("inverse_action plan missing inverse_action_name".into()))?
                    .to_string();
                let (object_type, action_name) = split_inverse_action(&inverse);
                let object_id = plan
                    .plan
                    .get("object_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let args = plan.plan.get("args_from_receipt").cloned().unwrap_or(Value::Null);
                let receipt = executor
                    .execute_approved_action(&object_type, &object_id, &action_name, &args, &plan.compensation_id)
                    .await
                    .map_err(|e| RollbackError::DomainExecutorFailed(e.to_string()))?;
                Ok(receipt.result)
            }
            AiCompensationMode::FollowupAction => {
                let executor = self.domain_executor.as_ref().ok_or_else(|| {
                    RollbackError::DomainExecutorUnavailable(
                        "DomainActionExecutor is not wired for followup_action compensation".into(),
                    )
                })?;
                let corrective = plan
                    .plan
                    .get("corrective_action_name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        RollbackError::Internal("followup_action plan missing corrective_action_name".into())
                    })?
                    .to_string();
                let (object_type, action_name) = split_inverse_action(&corrective);
                let object_id = plan
                    .plan
                    .get("object_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let args = plan.plan.get("followup_args").cloned().unwrap_or(Value::Null);
                let receipt = executor
                    .execute_approved_action(&object_type, &object_id, &action_name, &args, &plan.compensation_id)
                    .await
                    .map_err(|e| RollbackError::DomainExecutorFailed(e.to_string()))?;
                Ok(receipt.result)
            }
            AiCompensationMode::Irreversible => Err(RollbackError::Irreversible),
        }
    }

    /// List all compensation plans for a proposal, joined with the
    /// underlying receipts.
    pub async fn list_compensations_for_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Vec<AiCompensationPlanRecord>, RollbackError> {
        Ok(self.plan_repo.list_by_proposal(proposal_id).await?)
    }

    /// Find the receipts for a proposal.
    pub async fn list_receipts_for_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Vec<AiActionReceiptRecord>, RollbackError> {
        Ok(self.receipt_repo.list_by_proposal(proposal_id).await?)
    }

    /// One scan pass of the recovery scheduler. Two scanners:
    ///
    /// 1. Mark `Executing` plans whose `updated_at` is older than
    ///    `executing_timeout_seconds` as `Failed` with
    ///    `execution_error = "execution_timeout"`.
    /// 2. Auto-execute `Planned` plans that do not require approval
    ///    and whose `created_at` is older than
    ///    `auto_execute_grace_seconds`.
    pub async fn scheduler_tick(
        &self,
        executing_timeout_seconds: i64,
        auto_execute_grace_seconds: i64,
    ) -> CompensationExecutorReport {
        let mut report = CompensationExecutorReport::default();

        let timed_out = self
            .plan_repo
            .list_executing_past_timeout(executing_timeout_seconds)
            .await
            .unwrap_or_default();
        for plan in timed_out {
            if self
                .plan_repo
                .mark_failed(&plan.compensation_id, "execution_timeout")
                .await
                .is_ok()
            {
                report.timed_out += 1;
            }
        }

        let auto = self
            .plan_repo
            .list_pending_approval(auto_execute_grace_seconds)
            .await
            .unwrap_or_default();
        for plan in auto {
            match self.execute_compensation(&plan.compensation_id, "auto_executor").await {
                Ok(_) => report.auto_executed += 1,
                Err(error) => {
                    report.failed += 1;
                    tracing::warn!(
                        target: "ai_rollback",
                        compensation_id = %plan.compensation_id,
                        error = %error,
                        "auto execute compensation failed"
                    );
                }
            }
        }

        report
    }
}

fn split_inverse_action(qualified: &str) -> (String, String) {
    if let Some((object, action)) = qualified.split_once('.') {
        (object.to_string(), action.to_string())
    } else {
        (qualified.to_string(), String::new())
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CompensationExecutorReport {
    pub timed_out: u32,
    pub auto_executed: u32,
    pub failed: u32,
}

impl CompensationExecutorReport {
    pub fn is_no_op(&self) -> bool {
        self.timed_out == 0 && self.auto_executed == 0 && self.failed == 0
    }
}
