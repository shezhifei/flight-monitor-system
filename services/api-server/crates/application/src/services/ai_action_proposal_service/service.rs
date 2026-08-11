//! AI 动作建议服务 (AiActionProposalService)

use chrono::Utc;
use metrics::counter;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use ulid::Ulid;

use fms_domain::models::ai_proposal::{
    ActionProposalQuery, ActionProposalStats, ActionProposalStatus, AiActionProposal, ApprovalPolicy, ConstraintResult,
    RiskLevel,
};
use fms_domain::ports::ai_object_policy_repository::{
    AiObjectAccessDecision, AiObjectAccessRequest, AiObjectPolicyRepository, AiObjectPolicySubject,
};
use fms_domain::ports::ai_proposal_repository::{AiProposalRepository, AiProposalRepositoryError};
use fms_domain::ports::anomaly_repository::AnomalyRepository;
use fms_domain::ports::dispatch_repository::StandRepository;
use fms_domain::ports::flight_repository::FlightRepository;

use crate::services::ai_execution_allowlist::ExecutionAllowlist;
use crate::services::ai_execution_readiness_service::AiExecutionReadinessService;
use crate::services::ai_proposal_audit_recorder::{AiProposalAuditEventRecorder, ProposalAuditEvent};
use crate::services::ai_runtime_service::{AiRuntimeError, AiRuntimeService, AiToolExecutionSpec};
use crate::services::authorization_service::AuthorizationService;
use crate::services::notification_service::NotificationCreate;
use crate::types::ConcreteNotificationService;

use super::error::AiActionProposalError;
use super::helpers::{feature_enabled, normalize_policy_for_risk, object_policy_subject};
use super::schemas::{
    ApproveProposalRequest, ExecuteProposalRequest, GenerateProposalRequest, RejectProposalRequest,
    SubmitProposalRequest, ValidateProposalRequest,
};
// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

const PROPOSAL_EXPIRATION_MINUTES: i64 = 60;
const MAX_ACTIVE_PROPOSALS_PER_JOB: usize = 32;
const AI_ACTION_PROPOSAL_GENERATED_METRIC: &str = "ai_action_proposal_generated_total";
const AI_ACTION_PROPOSAL_VALIDATED_METRIC: &str = "ai_action_proposal_validated_total";
const AI_ACTION_PROPOSAL_SUBMITTED_METRIC: &str = "ai_action_proposal_submitted_total";
const AI_ACTION_PROPOSAL_APPROVED_METRIC: &str = "ai_action_proposal_approved_total";
const AI_ACTION_PROPOSAL_REJECTED_METRIC: &str = "ai_action_proposal_rejected_total";
const AI_ACTION_PROPOSAL_EXECUTED_METRIC: &str = "ai_action_proposal_executed_total";
const AI_ACTION_PROPOSAL_FAILED_METRIC: &str = "ai_action_proposal_failed_total";
const AI_ACTION_PROPOSAL_EXPIRED_METRIC: &str = "ai_action_proposal_expired_total";
// ---------------------------------------------------------------------------
// 内部状态
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct ProposalServiceState {
    active_proposals: HashMap<String, AiActionProposal>,
}

// ---------------------------------------------------------------------------
// AiActionProposalService
// ---------------------------------------------------------------------------

pub struct AiActionProposalService {
    state: RwLock<ProposalServiceState>,
    repository: Option<Arc<dyn AiProposalRepository + Send + Sync>>,
    ai_runtime_service: Option<Arc<AiRuntimeService>>,
    notification_service: Option<Arc<ConcreteNotificationService>>,
    domain_action_executor: Option<Arc<crate::services::domain_action_executor::DomainActionExecutor>>,
    object_policy_repository: Option<Arc<dyn AiObjectPolicyRepository + Send + Sync>>,
    ontology_repository: Option<Arc<dyn fms_domain::ports::ai_ontology_repository::AiOntologyRepository + Send + Sync>>,
    #[allow(dead_code)] // retained for API/DI compatibility; SQL now goes through ports
    pool: Option<sqlx::PgPool>,
    flight_repository: Option<Arc<dyn FlightRepository + Send + Sync>>,
    anomaly_repository: Option<Arc<dyn AnomalyRepository + Send + Sync>>,
    stand_repository: Option<Arc<dyn StandRepository + Send + Sync>>,
    readiness_service: Option<Arc<AiExecutionReadinessService>>,
    audit_recorder: Option<Arc<dyn AiProposalAuditEventRecorder>>,
    proposal_execution_enabled_override: Option<bool>,
}
impl AiActionProposalService {
    pub fn new() -> Self {
        Self {
            state: RwLock::new(ProposalServiceState::default()),
            repository: None,
            ai_runtime_service: None,
            notification_service: None,
            domain_action_executor: None,
            object_policy_repository: None,
            ontology_repository: None,
            pool: None,
            flight_repository: None,
            anomaly_repository: None,
            stand_repository: None,
            readiness_service: None,
            audit_recorder: None,
            proposal_execution_enabled_override: None,
        }
    }

    pub fn with_repository(mut self, repository: Arc<dyn AiProposalRepository + Send + Sync>) -> Self {
        self.repository = Some(repository);
        self
    }

    pub fn with_ai_runtime_service(mut self, service: Arc<AiRuntimeService>) -> Self {
        self.ai_runtime_service = Some(service);
        self
    }

    pub fn with_notification_service(mut self, service: Arc<ConcreteNotificationService>) -> Self {
        self.notification_service = Some(service);
        self
    }

    pub fn with_domain_action_executor(
        mut self,
        executor: Arc<crate::services::domain_action_executor::DomainActionExecutor>,
    ) -> Self {
        self.domain_action_executor = Some(executor);
        self
    }

    pub fn with_object_policy_repository(
        mut self,
        repository: Arc<dyn AiObjectPolicyRepository + Send + Sync>,
    ) -> Self {
        self.object_policy_repository = Some(repository);
        self
    }

    pub fn with_ontology_repository(
        mut self,
        repository: Arc<dyn fms_domain::ports::ai_ontology_repository::AiOntologyRepository + Send + Sync>,
    ) -> Self {
        self.ontology_repository = Some(repository);
        self
    }

    pub fn with_pool(mut self, pool: sqlx::PgPool) -> Self {
        self.pool = Some(pool);
        self
    }

    pub fn with_flight_repository(mut self, repository: Arc<dyn FlightRepository + Send + Sync>) -> Self {
        self.flight_repository = Some(repository);
        self
    }

    pub fn with_anomaly_repository(mut self, repository: Arc<dyn AnomalyRepository + Send + Sync>) -> Self {
        self.anomaly_repository = Some(repository);
        self
    }

    pub fn with_stand_repository(mut self, repository: Arc<dyn StandRepository + Send + Sync>) -> Self {
        self.stand_repository = Some(repository);
        self
    }

    pub fn with_readiness_service(mut self, service: Arc<AiExecutionReadinessService>) -> Self {
        self.readiness_service = Some(service);
        self
    }

    pub fn with_audit_recorder(mut self, recorder: Arc<dyn AiProposalAuditEventRecorder>) -> Self {
        self.audit_recorder = Some(recorder);
        self
    }

    #[cfg(test)]
    pub(crate) fn with_proposal_execution_enabled_for_test(mut self, enabled: bool) -> Self {
        self.proposal_execution_enabled_override = Some(enabled);
        self
    }

    #[cfg(test)]
    pub(crate) fn test_repository(&self) -> Option<&Arc<dyn AiProposalRepository + Send + Sync>> {
        self.repository.as_ref()
    }

    fn execution_allowlist(&self) -> ExecutionAllowlist {
        if let Some(enabled) = self.proposal_execution_enabled_override {
            return if enabled {
                ExecutionAllowlist::AllowAll
            } else {
                ExecutionAllowlist::Disabled
            };
        }
        ExecutionAllowlist::from_env()
    }

    // -----------------------------------------------------------------------
    // 1. 生成建议 (Generate)
    // -----------------------------------------------------------------------

    pub async fn generate_proposal(
        &self,
        req: GenerateProposalRequest,
    ) -> Result<AiActionProposal, AiActionProposalError> {
        let proposal_id = format!("prop_{}", Ulid::new());
        let now = Utc::now();
        let expires_at = now + chrono::Duration::minutes(PROPOSAL_EXPIRATION_MINUTES);

        let mut proposal = AiActionProposal::new(
            &proposal_id,
            &req.job_id,
            &req.run_id,
            &req.object_type,
            &req.object_id,
            &req.action_name,
            req.arguments.clone(),
        )
        .with_expires_at(expires_at)
        .with_correlation_id(req.correlation_id.unwrap_or_else(|| Ulid::new().to_string()));

        if let Some(version) = req.ontology_version {
            proposal = proposal.with_ontology_version(version);
        }

        let mut metadata = serde_json::Map::new();
        if let Some(idempotency_key) = req.idempotency_key {
            metadata.insert("idempotency_key".to_string(), Value::String(idempotency_key));
        }
        if let Some(expected_object_version) = req.expected_object_version {
            metadata.insert(
                "expected_object_version".to_string(),
                serde_json::json!(expected_object_version),
            );
        }
        if !metadata.is_empty() {
            proposal = proposal.with_metadata(Value::Object(metadata));
        }

        if let Some(reasoning) = req.reasoning {
            proposal = proposal.with_reasoning(reasoning);
        }

        if let Some(confidence) = req.confidence {
            proposal = proposal.with_confidence(confidence);
        }

        // 根据 action_name 和 object_type 推断风险等级和审批策略
        let (inferred_risk_level, inferred_approval_policy) =
            self.infer_risk_and_policy(&req.object_type, &req.action_name, &req.arguments);
        let risk_level = req.risk_level.unwrap_or(inferred_risk_level);
        let approval_policy =
            normalize_policy_for_risk(risk_level, req.approval_policy.unwrap_or(inferred_approval_policy));
        proposal.risk_level = risk_level;
        proposal.approval_policy = approval_policy;
        proposal.required_permissions = req
            .required_permissions
            .unwrap_or_else(|| self.infer_required_permissions(&req.object_type, &req.action_name));

        let requester_id = req.requester_user_id.as_deref().unwrap_or("unknown_user").to_string();
        self.ensure_action_permissions("generate", &proposal, &requester_id, &req.requester_user_roles)?;
        self.ensure_object_policy_access(
            "generate",
            &proposal,
            &requester_id,
            &req.requester_user_roles,
            req.requester_department_id.as_deref(),
        )
        .await?;

        // 写入内存缓存
        {
            let mut state = self.state.write().await;
            let job_proposals: Vec<_> = state
                .active_proposals
                .values()
                .filter(|p| p.job_id == req.job_id && !p.status.is_terminal())
                .cloned()
                .collect();
            if job_proposals.len() >= MAX_ACTIVE_PROPOSALS_PER_JOB {
                return Err(AiActionProposalError::validation(format!(
                    "job {} has too many active proposals (max {})",
                    req.job_id, MAX_ACTIVE_PROPOSALS_PER_JOB
                )));
            }
            state.active_proposals.insert(proposal_id.clone(), proposal.clone());
        }

        // 持久化
        if let Some(repo) = &self.repository {
            repo.save(&proposal).await?;
        }

        let _ = counter!(AI_ACTION_PROPOSAL_GENERATED_METRIC,
            "object_type" => req.object_type.clone(),
            "action_name" => req.action_name.clone(),
            "risk_level" => risk_level.label()
        );

        Ok(proposal)
    }

    // -----------------------------------------------------------------------
    // 2. 验证建议 (Validate)
    // -----------------------------------------------------------------------

    pub async fn validate_proposal(
        &self,
        req: ValidateProposalRequest,
    ) -> Result<AiActionProposal, AiActionProposalError> {
        let mut proposal = self.get_proposal(&req.proposal_id).await?;

        if proposal.status != ActionProposalStatus::Draft {
            return Err(AiActionProposalError::validation(format!(
                "proposal {} is not in draft status (current: {})",
                req.proposal_id,
                proposal.status.label()
            )));
        }

        // 应用验证数据
        if let Some(snapshot) = req.before_snapshot {
            proposal.before_snapshot = Some(snapshot);
        }
        if let Some(preview) = req.after_preview {
            proposal.after_preview = Some(preview);
        }
        if let Some(results) = req.constraint_results {
            proposal.constraint_results = results;
        }

        // 自动校验约束
        if !proposal.all_constraints_passed() {
            let failed = proposal
                .failed_constraints()
                .into_iter()
                .map(|c| format!("{}: {}", c.constraint_name, c.message.as_deref().unwrap_or("failed")))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(AiActionProposalError::validation(format!(
                "constraint validation failed: {}",
                failed
            )));
        }

        // 状态迁移: Draft -> Validating -> Pending
        proposal
            .transition_to(ActionProposalStatus::Validating)
            .map_err(AiActionProposalError::validation)?;
        proposal
            .transition_to(ActionProposalStatus::Pending)
            .map_err(AiActionProposalError::validation)?;

        // 根据审批策略自动处理
        if proposal.approval_policy.is_auto() && proposal.risk_level == RiskLevel::Low {
            // 低风险 + 自动执行策略 = 自动批准
            proposal
                .approve("system_auto")
                .map_err(AiActionProposalError::validation)?;
        }

        self.persist_proposal(&proposal).await?;

        let _ = counter!(AI_ACTION_PROPOSAL_VALIDATED_METRIC,
            "object_type" => proposal.object_type.clone(),
            "action_name" => proposal.action_name.clone(),
            "auto_approved" => if proposal.status == ActionProposalStatus::Approved { "true" } else { "false" }
        );

        Ok(proposal)
    }

    // -----------------------------------------------------------------------
    // 3. 提交建议 (Submit) — 供外部系统或手动调用
    // -----------------------------------------------------------------------

    pub async fn submit_proposal(&self, req: SubmitProposalRequest) -> Result<AiActionProposal, AiActionProposalError> {
        let proposal = self.get_proposal(&req.proposal_id).await?;

        if proposal.status != ActionProposalStatus::Draft {
            return Err(AiActionProposalError::validation(format!(
                "proposal {} cannot be submitted from status {}",
                req.proposal_id,
                proposal.status.label()
            )));
        }

        // 直接调用 validate 完成提交
        let validated = self
            .validate_proposal(ValidateProposalRequest {
                proposal_id: req.proposal_id,
                before_snapshot: proposal.before_snapshot.clone(),
                after_preview: proposal.after_preview.clone(),
                constraint_results: Some(proposal.constraint_results.clone()),
            })
            .await?;

        let _ = counter!(AI_ACTION_PROPOSAL_SUBMITTED_METRIC,
            "object_type" => validated.object_type.clone(),
            "action_name" => validated.action_name.clone()
        );

        Ok(validated)
    }

    // -----------------------------------------------------------------------
    // 4. 审批建议 (Approve)
    // -----------------------------------------------------------------------

    pub async fn approve_proposal(
        &self,
        req: ApproveProposalRequest,
    ) -> Result<AiActionProposal, AiActionProposalError> {
        let mut proposal = self.get_proposal(&req.proposal_id).await?;

        if !proposal.status.can_approve() {
            return Err(AiActionProposalError::conflict(format!(
                "proposal {} cannot be approved in status {}",
                req.proposal_id,
                proposal.status.label()
            )));
        }

        self.ensure_not_expired(&proposal)?;
        self.ensure_action_permissions("approve", &proposal, &req.approver_id, &req.approver_permissions)?;
        self.ensure_object_policy_access(
            "approve",
            &proposal,
            &req.approver_id,
            &req.approver_permissions,
            req.approver_department_id.as_deref(),
        )
        .await?;
        self.check_approval_permission(&proposal, &req.approver_id, &req.approver_permissions)?;

        // 如果提供了修改后的参数，更新 arguments
        if let Some(modified) = req.modified_arguments {
            proposal.arguments = modified;
        }

        proposal
            .approve(&req.approver_id)
            .map_err(AiActionProposalError::validation)?;

        self.persist_proposal(&proposal).await?;

        // 如果已链接到 pending action，也更新 pending action 状态
        if let Some(pending_id) = &proposal.pending_action_id {
            if let Some(runtime) = &self.ai_runtime_service {
                let _ = runtime
                    .approve_pending_action(pending_id, &req.approver_id, Some(proposal.arguments.clone()))
                    .await;
            }
        }

        let _ = counter!(AI_ACTION_PROPOSAL_APPROVED_METRIC,
            "object_type" => proposal.object_type.clone(),
            "action_name" => proposal.action_name.clone(),
            "risk_level" => proposal.risk_level.label()
        );

        Ok(proposal)
    }

    // -----------------------------------------------------------------------
    // 5. 拒绝建议 (Reject)
    // -----------------------------------------------------------------------

    pub async fn reject_proposal(&self, req: RejectProposalRequest) -> Result<AiActionProposal, AiActionProposalError> {
        let mut proposal = self.get_proposal(&req.proposal_id).await?;

        if !proposal.status.can_reject() {
            return Err(AiActionProposalError::conflict(format!(
                "proposal {} cannot be rejected in status {}",
                req.proposal_id,
                proposal.status.label()
            )));
        }

        proposal
            .reject(&req.rejecter_id, &req.reason)
            .map_err(AiActionProposalError::validation)?;

        self.persist_proposal(&proposal).await?;

        // 如果已链接到 pending action，也拒绝 pending action
        if let Some(pending_id) = &proposal.pending_action_id {
            if let Some(runtime) = &self.ai_runtime_service {
                let reason_str = req.reason.as_str();
                let _ = runtime
                    .reject_pending_action(pending_id, &req.rejecter_id, Some(reason_str))
                    .await;
            }
        }

        let _ = counter!(AI_ACTION_PROPOSAL_REJECTED_METRIC,
            "object_type" => proposal.object_type.clone(),
            "action_name" => proposal.action_name.clone()
        );

        Ok(proposal)
    }

    // -----------------------------------------------------------------------
    // 6. 执行建议 (Execute)
    // -----------------------------------------------------------------------

    pub async fn execute_proposal(
        &self,
        req: ExecuteProposalRequest,
    ) -> Result<AiActionProposal, AiActionProposalError> {
        let mut proposal = self.get_proposal(&req.proposal_id).await?;

        if let Some(recorder) = &self.audit_recorder {
            let _ = recorder
                .record_execution_event(&ProposalAuditEvent {
                    proposal_id: proposal.proposal_id.clone(),
                    job_id: proposal.job_id.clone(),
                    run_id: proposal.run_id.clone(),
                    event_type: "proposal.execution_requested".to_string(),
                    payload: Some(serde_json::json!({
                        "proposal_id": proposal.proposal_id,
                        "executor_id": req.executor_id,
                        "object_type": proposal.object_type,
                        "action_name": proposal.action_name,
                    })),
                })
                .await;
        }

        if !proposal.status.can_execute() {
            return Err(AiActionProposalError::conflict(format!(
                "proposal {} cannot be executed in status {}",
                req.proposal_id,
                proposal.status.label()
            )));
        }

        self.ensure_not_expired(&proposal)?;
        self.ensure_action_permissions("execute", &proposal, &req.executor_id, &req.executor_permissions)?;
        self.ensure_object_policy_access(
            "execute",
            &proposal,
            &req.executor_id,
            &req.executor_permissions,
            req.executor_department_id.as_deref(),
        )
        .await?;

        let allowlist = self.execution_allowlist();
        if !allowlist.allows(&proposal.object_type, &proposal.action_name) {
            return Err(AiActionProposalError::conflict(format!(
                "AI proposal execution is not enabled for {}.{} by FMS_AI_PROPOSAL_EXECUTION_ENABLED",
                proposal.object_type, proposal.action_name
            )));
        }

        if let Some(readiness) = &self.readiness_service {
            let report = readiness.evaluate().await;
            if !report.is_ready() {
                let failed: Vec<String> = report
                    .failed_checks()
                    .iter()
                    .map(|check| format!("{}: {}", check.name, check.message))
                    .collect();
                let error_message = format!("execution readiness check failed: {}", failed.join("; "));
                if let Some(recorder) = &self.audit_recorder {
                    let _ = recorder
                        .record_execution_event(&ProposalAuditEvent {
                            proposal_id: proposal.proposal_id.clone(),
                            job_id: proposal.job_id.clone(),
                            run_id: proposal.run_id.clone(),
                            event_type: "proposal.execution_blocked_readiness".to_string(),
                            payload: Some(serde_json::json!({
                                "proposal_id": proposal.proposal_id,
                                "error": error_message,
                                "failed_checks": failed,
                            })),
                        })
                        .await;
                }
                return Err(AiActionProposalError::forbidden(error_message));
            }
        }

        // 1. Expected Object Version Check
        if let Some(metadata_obj) = proposal.metadata.as_object() {
            if let Some(expected_version_val) = metadata_obj.get("expected_object_version") {
                if let Some(expected_version) = expected_version_val.as_i64() {
                    if let Some(flight_repository) = &self.flight_repository {
                        if proposal.object_type.to_lowercase() == "flight" {
                            let flight = flight_repository
                                .find_by_id(&proposal.object_id)
                                .await
                                .map_err(|e| AiActionProposalError::internal(e.to_string()))?;

                            match flight {
                                Some(flight) => {
                                    if flight.version as i64 != expected_version {
                                        return Err(AiActionProposalError::conflict(format!(
                                            "Flight version mismatch: expected {}, got {}",
                                            expected_version, flight.version
                                        )));
                                    }
                                }
                                None => {
                                    return Err(AiActionProposalError::conflict(format!(
                                        "Flight {} not found",
                                        proposal.object_id
                                    )));
                                }
                            }
                        }
                    }
                }
            }
        }

        // 2. Idempotency Key Check
        if let Some(metadata_obj) = proposal.metadata.as_object() {
            if let Some(idempotency_val) = metadata_obj.get("idempotency_key") {
                if let Some(idempotency_key) = idempotency_val.as_str() {
                    if let Some(repo) = &self.repository {
                        let count = repo
                            .count(&ActionProposalQuery {
                                idempotency_key: Some(idempotency_key.to_string()),
                                status: Some(ActionProposalStatus::Executed),
                                ..Default::default()
                            })
                            .await?;

                        if count > 0 {
                            return Err(AiActionProposalError::conflict(format!(
                                "Proposal with idempotency key '{}' has already been executed",
                                idempotency_key
                            )));
                        }
                    }
                }
            }
        }

        // 3. Parameter Schema Check
        self.validate_arguments_schema(&proposal).await?;

        // 4. Constraint Recomputing
        self.recompute_constraints(&proposal).await?;

        proposal
            .transition_to(ActionProposalStatus::Executing)
            .map_err(AiActionProposalError::validation)?;
        self.persist_proposal(&proposal).await?;

        if let Some(recorder) = &self.audit_recorder {
            let _ = recorder
                .record_execution_event(&ProposalAuditEvent {
                    proposal_id: proposal.proposal_id.clone(),
                    job_id: proposal.job_id.clone(),
                    run_id: proposal.run_id.clone(),
                    event_type: "proposal.execution_started".to_string(),
                    payload: Some(serde_json::json!({
                        "proposal_id": proposal.proposal_id,
                        "executor_id": req.executor_id,
                    })),
                })
                .await;
        }

        // 通过 DomainActionExecutor 或 AiRuntimeService 执行
        let execution_result: Result<Value, AiActionProposalError> = if let Some(executor) =
            &self.domain_action_executor
        {
            let receipt = executor
                .execute_approved_action(
                    &proposal.object_type,
                    &proposal.object_id,
                    &proposal.action_name,
                    &proposal.arguments,
                    &req.executor_id,
                )
                .await
                .map_err(|e| AiActionProposalError::execution(&e.to_string()))?;

            let val = receipt.result.clone();
            proposal.mark_executed(&req.executor_id, val.clone());
            Ok(val)
        } else if let Some(runtime) = &self.ai_runtime_service {
            if !feature_enabled("FMS_AI_LEGACY_TOOL_FALLBACK_ENABLED") {
                proposal.mark_failed("domain_action_executor unavailable and legacy tool fallback disabled");
                Err(AiActionProposalError::execution(
                    "domain_action_executor unavailable and legacy tool fallback disabled",
                ))
            } else {
                tracing::warn!(
                        "DEPRECATED: FMS_AI_LEGACY_TOOL_FALLBACK_ENABLED is true — using legacy runtime fallback in execute_proposal. Scheduled retirement: 2026-09-30"
                    );
                let spec = AiToolExecutionSpec {
                    tool_name: self.map_action_to_tool(&proposal.object_type, &proposal.action_name),
                    category: proposal.object_type.clone(),
                    operation_level: if proposal.risk_level.requires_supervisor() {
                        "supervisor".to_string()
                    } else if proposal.risk_level.requires_approval() {
                        "operator".to_string()
                    } else {
                        "auto".to_string()
                    },
                    side_effect: true,
                    query_intent: None,
                    query_dataset: None,
                };

                let result = runtime
                    .execute_tool(spec, proposal.arguments.clone(), Some(req.executor_id.clone()), vec![])
                    .await;
                proposal.mark_executed(&req.executor_id, result.clone());
                Ok(result)
            }
        } else {
            // 没有执行器，标记为失败
            proposal.mark_failed("neither domain_action_executor nor ai_runtime_service available");
            Err(AiActionProposalError::execution(
                "neither domain_action_executor nor ai_runtime_service available",
            ))
        };

        self.persist_proposal(&proposal).await?;

        match &execution_result {
            Ok(_) => {
                let _ = counter!(AI_ACTION_PROPOSAL_EXECUTED_METRIC,
                    "object_type" => proposal.object_type.clone(),
                    "action_name" => proposal.action_name.clone()
                );
                if let Some(recorder) = &self.audit_recorder {
                    let _ = recorder
                        .record_execution_event(&ProposalAuditEvent {
                            proposal_id: proposal.proposal_id.clone(),
                            job_id: proposal.job_id.clone(),
                            run_id: proposal.run_id.clone(),
                            event_type: "proposal.execution_succeeded".to_string(),
                            payload: Some(serde_json::json!({
                                "proposal_id": proposal.proposal_id,
                            })),
                        })
                        .await;
                }
            }
            Err(e) => {
                let _ = counter!(AI_ACTION_PROPOSAL_FAILED_METRIC,
                    "object_type" => proposal.object_type.clone(),
                    "action_name" => proposal.action_name.clone()
                );
                if let Some(recorder) = &self.audit_recorder {
                    let _ = recorder
                        .record_execution_event(&ProposalAuditEvent {
                            proposal_id: proposal.proposal_id.clone(),
                            job_id: proposal.job_id.clone(),
                            run_id: proposal.run_id.clone(),
                            event_type: "proposal.execution_failed".to_string(),
                            payload: Some(serde_json::json!({
                                "proposal_id": proposal.proposal_id,
                                "error": e.to_string(),
                            })),
                        })
                        .await;
                }
            }
        }

        // 发送通知
        if let Some(notif_svc) = &self.notification_service {
            let notif = if execution_result.is_ok() {
                NotificationCreate {
                    user_id: req.executor_id.clone(),
                    title: format!("AI 动作已执行: {}.{}", proposal.object_type, proposal.action_name),
                    body: format!(
                        "对象 [{}] 的动作 [{}] 已由 {} 执行完成",
                        proposal.object_id, proposal.action_name, req.executor_id
                    ),
                    category: Some("ai_action".to_string()),
                    severity: Some("info".to_string()),
                    flight_id: None,
                    related_entity_type: Some(proposal.object_type.clone()),
                    related_entity_id: Some(proposal.object_id.clone()),
                    dispatch_order_id: None,
                    group_id: None,
                    sender_user_id: Some(req.executor_id.clone()),
                    sender_username_snapshot: None,
                    origin_type: Some("ai_proposal".to_string()),
                    receipt_required: false,
                    receipt_group_id: None,
                }
            } else {
                NotificationCreate {
                    user_id: req.executor_id.clone(),
                    title: format!("AI 动作执行失败: {}.{}", proposal.object_type, proposal.action_name),
                    body: format!(
                        "对象 [{}] 的动作 [{}] 执行失败: {}",
                        proposal.object_id,
                        proposal.action_name,
                        proposal.execution_error.as_deref().unwrap_or("unknown error")
                    ),
                    category: Some("ai_action".to_string()),
                    severity: Some("error".to_string()),
                    flight_id: None,
                    related_entity_type: Some(proposal.object_type.clone()),
                    related_entity_id: Some(proposal.object_id.clone()),
                    dispatch_order_id: None,
                    group_id: None,
                    sender_user_id: Some(req.executor_id.clone()),
                    sender_username_snapshot: None,
                    origin_type: Some("ai_proposal".to_string()),
                    receipt_required: false,
                    receipt_group_id: None,
                }
            };
            let _ = notif_svc.send_notification(notif).await;
        }

        execution_result.map(|_| proposal)
    }

    // -----------------------------------------------------------------------
    // 7. 查询与统计
    // -----------------------------------------------------------------------

    pub async fn get_proposal(&self, proposal_id: &str) -> Result<AiActionProposal, AiActionProposalError> {
        // 先查内存
        {
            let state = self.state.read().await;
            if let Some(p) = state.active_proposals.get(proposal_id) {
                return Ok(p.clone());
            }
        }
        // 再查持久化
        if let Some(repo) = &self.repository {
            match repo.find_by_id(proposal_id).await? {
                Some(p) => {
                    // 缓存到内存
                    let mut state = self.state.write().await;
                    state.active_proposals.insert(proposal_id.to_string(), p.clone());
                    Ok(p)
                }
                None => Err(AiActionProposalError::not_found(proposal_id)),
            }
        } else {
            Err(AiActionProposalError::not_found(proposal_id))
        }
    }

    pub async fn list_proposals(
        &self,
        query: &ActionProposalQuery,
    ) -> Result<Vec<AiActionProposal>, AiActionProposalError> {
        if let Some(repo) = &self.repository {
            repo.search(query).await.map_err(Into::into)
        } else {
            let state = self.state.read().await;
            let mut results: Vec<_> = state.active_proposals.values().cloned().collect();

            if let Some(job_id) = &query.job_id {
                results.retain(|p| &p.job_id == job_id);
            }
            if let Some(run_id) = &query.run_id {
                results.retain(|p| &p.run_id == run_id);
            }
            if let Some(obj_type) = &query.object_type {
                results.retain(|p| &p.object_type == obj_type);
            }
            if let Some(obj_id) = &query.object_id {
                results.retain(|p| &p.object_id == obj_id);
            }
            if let Some(action_name) = &query.action_name {
                results.retain(|p| &p.action_name == action_name);
            }
            if let Some(status) = query.status {
                results.retain(|p| p.status == status);
            }
            if let Some(risk) = query.risk_level {
                results.retain(|p| p.risk_level == risk);
            }

            if let Some(limit) = query.limit {
                results.truncate(limit);
            }

            Ok(results)
        }
    }

    pub async fn get_stats(&self) -> Result<ActionProposalStats, AiActionProposalError> {
        if let Some(repo) = &self.repository {
            repo.get_stats().await.map_err(Into::into)
        } else {
            let state = self.state.read().await;
            let proposals: Vec<_> = state.active_proposals.values().cloned().collect();
            Ok(self.compute_stats(&proposals))
        }
    }

    // -----------------------------------------------------------------------
    // 8. 与 PendingActionRecord 的映射/桥接
    // -----------------------------------------------------------------------

    pub async fn link_to_pending_action(
        &self,
        proposal_id: &str,
        pending_action_id: &str,
    ) -> Result<AiActionProposal, AiActionProposalError> {
        let mut proposal = self.get_proposal(proposal_id).await?;
        proposal.pending_action_id = Some(pending_action_id.to_string());
        self.persist_proposal(&proposal).await?;

        if let Some(repo) = &self.repository {
            repo.link_pending_action(proposal_id, pending_action_id).await?;
        }

        Ok(proposal)
    }

    pub async fn find_by_pending_action_id(
        &self,
        pending_action_id: &str,
    ) -> Result<Option<AiActionProposal>, AiActionProposalError> {
        if let Some(repo) = &self.repository {
            repo.find_by_pending_action_id(pending_action_id)
                .await
                .map_err(Into::into)
        } else {
            let state = self.state.read().await;
            Ok(state
                .active_proposals
                .values()
                .find(|p| p.pending_action_id.as_deref() == Some(pending_action_id))
                .cloned())
        }
    }

    // -----------------------------------------------------------------------
    // 9. 过期清理
    // -----------------------------------------------------------------------

    pub async fn expire_stale_proposals(&self) -> Result<usize, AiActionProposalError> {
        let mut expired_count = 0usize;
        let mut to_update = Vec::new();

        {
            let state = self.state.read().await;
            for (id, proposal) in state.active_proposals.iter() {
                if proposal.is_expired() && proposal.status == ActionProposalStatus::Pending {
                    to_update.push(id.clone());
                }
            }
        }

        for id in to_update {
            if let Ok(mut proposal) = self.get_proposal(&id).await {
                if let Ok(()) = proposal.transition_to(ActionProposalStatus::Expired) {
                    if self.persist_proposal(&proposal).await.is_ok() {
                        expired_count += 1;
                        let _ = counter!(AI_ACTION_PROPOSAL_EXPIRED_METRIC,
                            "object_type" => proposal.object_type.clone(),
                            "action_name" => proposal.action_name.clone()
                        );
                    }
                }
            }
        }

        Ok(expired_count)
    }

    // -----------------------------------------------------------------------
    // 内部辅助方法
    // -----------------------------------------------------------------------

    async fn persist_proposal(&self, proposal: &AiActionProposal) -> Result<(), AiActionProposalError> {
        // 更新内存缓存
        {
            let mut state = self.state.write().await;
            state
                .active_proposals
                .insert(proposal.proposal_id.clone(), proposal.clone());
        }
        // 持久化
        if let Some(repo) = &self.repository {
            repo.save(proposal).await?;
        }
        Ok(())
    }

    fn infer_risk_and_policy(
        &self,
        object_type: &str,
        action_name: &str,
        _arguments: &Value,
    ) -> (RiskLevel, ApprovalPolicy) {
        // 契约 §4.4：ontology schema 是风险/审批策略的单一事实来源，
        // schema 内定义的动作一律以 schema 为准，避免硬编码表漂移。
        if let Some(def) = Self::ontology_action_def(object_type, action_name) {
            let risk = match def.risk_level.as_str() {
                "critical" => RiskLevel::Critical,
                "high" => RiskLevel::High,
                "medium" => RiskLevel::Medium,
                _ => RiskLevel::Low,
            };
            let policy = match def.approval_policy.as_str() {
                "require_supervisor_approval" => ApprovalPolicy::RequireSupervisorApproval,
                "require_flowable_approval" => ApprovalPolicy::RequireFlowableApproval,
                "require_approval" => ApprovalPolicy::RequireApproval,
                _ => ApprovalPolicy::AutoExecute,
            };
            return (risk, policy);
        }
        let key = format!("{}.{}", object_type, action_name);
        match key.as_str() {
            // 高风险：影响航班状态、取消、重大变更
            "Flight.cancel" | "Flight.divert" | "Flight.delete" => {
                (RiskLevel::Critical, ApprovalPolicy::RequireSupervisorApproval)
            }
            "Flight.update_status" | "Flight.change_stand" | "Flight.update_stand" | "Flight.update_gate" => {
                (RiskLevel::High, ApprovalPolicy::RequireApproval)
            }
            // 中风险：派工变更
            "DispatchOrder.cancel" | "DispatchOrder.reassign" | "Team.reassign" => {
                (RiskLevel::High, ApprovalPolicy::RequireApproval)
            }
            "DispatchOrder.create" | "Equipment.reassign" => (RiskLevel::Medium, ApprovalPolicy::RequireApproval),
            // 低风险：信息更新、标记
            "Flight.add_note" | "Flight.update_estimated_time" => (RiskLevel::Low, ApprovalPolicy::AutoExecute),
            "Anomaly.acknowledge" | "Anomaly.resolve" | "Todo.complete" => {
                (RiskLevel::Low, ApprovalPolicy::AutoExecute)
            }
            "Notification.send" | "BusinessCase.create" => (RiskLevel::Medium, ApprovalPolicy::RequireApproval),
            // 默认
            _ => {
                if action_name.contains("delete") || action_name.contains("cancel") || action_name.contains("reject") {
                    (RiskLevel::High, ApprovalPolicy::RequireApproval)
                } else if action_name.contains("create") || action_name.contains("update") {
                    (RiskLevel::Medium, ApprovalPolicy::RequireApproval)
                } else {
                    (RiskLevel::Low, ApprovalPolicy::AutoExecute)
                }
            }
        }
    }

    fn map_action_to_tool(&self, object_type: &str, action_name: &str) -> String {
        format!("{}_{}", object_type.to_lowercase(), action_name.to_lowercase())
    }

    /// 从确定性构建的 flight-ops.v1 schema 中查找动作定义（同步、无 IO）。
    fn ontology_action_def(
        object_type: &str,
        action_name: &str,
    ) -> Option<fms_domain::models::ai_ontology::OntologyActionDef> {
        let schema = fms_domain::ontology::flight_ops_v1::build_flight_ops_v1_schema();
        schema.objects.get(object_type)?.actions.get(action_name).cloned()
    }

    fn infer_required_permissions(&self, object_type: &str, action_name: &str) -> Vec<String> {
        // 契约 §4.4：权限以 ontology schema 为单一事实来源，
        // 保证 AI proposal 无法绕过新写动作的资源权限。
        if let Some(def) = Self::ontology_action_def(object_type, action_name) {
            if !def.required_permissions.is_empty() {
                return def.required_permissions;
            }
        }
        match (object_type, action_name) {
            ("Flight", "get_context") => vec!["flight:read".to_string()],
            ("Flight", "change_stand")
            | ("Flight", "update_status")
            | ("Flight", "add_note")
            | ("Flight", "update_estimated_time") => vec!["flight:write".to_string()],
            ("DispatchOrder", "recommend_replan") | ("Stand", "reserve") => {
                vec!["dispatch:write".to_string()]
            }
            ("DispatchOrder", "reassign") => vec!["dispatch:admin".to_string()],
            ("DispatchOrder", "publish") => vec!["dispatch:publish".to_string()],
            ("Anomaly", "acknowledge") | ("Anomaly", "escalate") => {
                vec!["anomaly:write".to_string()]
            }
            ("Notification", "send") => vec!["notification:send".to_string()],
            ("Todo", "create") | ("Todo", "complete") => vec!["todo:write".to_string()],
            ("BusinessCase", "create") => vec!["business_case:create".to_string()],
            ("BusinessCase", "close_case") => vec!["business_case:update".to_string()],
            _ => vec![],
        }
    }

    fn ensure_not_expired(&self, proposal: &AiActionProposal) -> Result<(), AiActionProposalError> {
        if proposal.is_expired() {
            return Err(AiActionProposalError::conflict(format!(
                "proposal {} is expired",
                proposal.proposal_id
            )));
        }
        Ok(())
    }

    fn ensure_action_permissions(
        &self,
        phase: &str,
        proposal: &AiActionProposal,
        actor_id: &str,
        actor_permissions: &[String],
    ) -> Result<(), AiActionProposalError> {
        if AuthorizationService::has_ai_action_grants(actor_permissions, &proposal.required_permissions) {
            return Ok(());
        }

        Err(AiActionProposalError::forbidden(format!(
            "actor {} is not allowed to {} {}.{}; required permissions: {:?}",
            actor_id, phase, proposal.object_type, proposal.action_name, proposal.required_permissions
        )))
    }

    async fn ensure_object_policy_access(
        &self,
        phase: &str,
        proposal: &AiActionProposal,
        actor_id: &str,
        actor_permissions: &[String],
        actor_department_id: Option<&str>,
    ) -> Result<(), AiActionProposalError> {
        let Some(repository) = &self.object_policy_repository else {
            return Ok(());
        };

        for permission in &proposal.required_permissions {
            let decision = repository
                .evaluate_access(&AiObjectAccessRequest {
                    subject: object_policy_subject(actor_id, actor_permissions, actor_department_id),
                    object_type: proposal.object_type.clone(),
                    object_id: Some(proposal.object_id.clone()),
                    permission: permission.clone(),
                    object_snapshot: proposal
                        .before_snapshot
                        .clone()
                        .or_else(|| proposal.after_preview.clone()),
                })
                .await
                .map_err(|e| AiActionProposalError::repository(e.to_string()))?;

            if decision == AiObjectAccessDecision::Deny {
                return Err(AiActionProposalError::forbidden(format!(
                    "actor {} is denied by object policy while attempting to {} {}.{} on {}",
                    actor_id, phase, proposal.object_type, proposal.action_name, proposal.object_id
                )));
            }
        }

        Ok(())
    }

    fn check_approval_permission(
        &self,
        proposal: &AiActionProposal,
        approver_id: &str,
        approver_permissions: &[String],
    ) -> Result<(), AiActionProposalError> {
        if proposal.approval_policy == ApprovalPolicy::RequireSupervisorApproval {
            if !AuthorizationService::has_ai_supervisor_approval_grant(approver_permissions) {
                return Err(AiActionProposalError::forbidden(format!(
                    "actor {} is not allowed to supervisor-approve proposal {}",
                    approver_id, proposal.proposal_id
                )));
            }
        }

        if proposal.approval_policy == ApprovalPolicy::RequireFlowableApproval {
            return Err(AiActionProposalError::conflict(format!(
                "proposal {} requires Flowable approval before manual approval",
                proposal.proposal_id
            )));
        }
        Ok(())
    }

    fn compute_stats(&self, proposals: &[AiActionProposal]) -> ActionProposalStats {
        let total = proposals.len();
        if total == 0 {
            return ActionProposalStats::default();
        }

        let mut by_status = serde_json::Map::new();
        let mut by_risk = serde_json::Map::new();
        let mut by_object = serde_json::Map::new();

        let mut approved_count = 0usize;
        let mut rejected_count = 0usize;
        let mut executed_count = 0usize;
        let mut failed_count = 0usize;
        let mut total_confidence = 0.0f64;

        for p in proposals {
            let status_key = p.status.label().to_string();
            *by_status
                .entry(status_key)
                .and_modify(|v| {
                    if let Some(n) = v.as_u64() {
                        *v = serde_json::json!(n + 1);
                    }
                })
                .or_insert(serde_json::json!(1)) = by_status[&p.status.label().to_string()].clone();

            let risk_key = p.risk_level.label().to_string();
            *by_risk
                .entry(risk_key)
                .and_modify(|v| {
                    if let Some(n) = v.as_u64() {
                        *v = serde_json::json!(n + 1);
                    }
                })
                .or_insert(serde_json::json!(1)) = by_risk[&p.risk_level.label().to_string()].clone();

            let obj_key = p.object_type.clone();
            *by_object
                .entry(obj_key)
                .and_modify(|v| {
                    if let Some(n) = v.as_u64() {
                        *v = serde_json::json!(n + 1);
                    }
                })
                .or_insert(serde_json::json!(1)) = by_object[&p.object_type].clone();

            if p.status == ActionProposalStatus::Approved {
                approved_count += 1;
            }
            if p.status == ActionProposalStatus::Rejected {
                rejected_count += 1;
            }
            if p.status == ActionProposalStatus::Executed {
                executed_count += 1;
            }
            if p.status == ActionProposalStatus::Failed {
                failed_count += 1;
            }
            total_confidence += p.confidence;
        }

        let terminal_count = approved_count + rejected_count;
        let execution_count = executed_count + failed_count;

        ActionProposalStats {
            total,
            by_status: Value::Object(by_status),
            by_risk_level: Value::Object(by_risk),
            by_object_type: Value::Object(by_object),
            avg_confidence: total_confidence / total as f64,
            approval_rate: if terminal_count > 0 {
                approved_count as f64 / terminal_count as f64
            } else {
                0.0
            },
            rejection_rate: if terminal_count > 0 {
                rejected_count as f64 / terminal_count as f64
            } else {
                0.0
            },
            execution_success_rate: if execution_count > 0 {
                executed_count as f64 / execution_count as f64
            } else {
                0.0
            },
            avg_execution_time_ms: None,
        }
    }

    async fn validate_arguments_schema(&self, proposal: &AiActionProposal) -> Result<(), AiActionProposalError> {
        let schema = if let Some(repo) = &self.ontology_repository {
            match repo.load_active_schema().await {
                Ok(Some(s)) => s,
                _ => fms_domain::ontology::flight_ops_v1::build_flight_ops_v1_schema(),
            }
        } else {
            fms_domain::ontology::flight_ops_v1::build_flight_ops_v1_schema()
        };

        let object_def = schema.objects.get(&proposal.object_type).ok_or_else(|| {
            AiActionProposalError::validation(format!(
                "Object type '{}' not found in active ontology schema",
                proposal.object_type
            ))
        })?;

        let action_def = object_def.actions.get(&proposal.action_name).ok_or_else(|| {
            AiActionProposalError::validation(format!(
                "Action '{}' not found for object '{}' in active ontology schema",
                proposal.action_name, proposal.object_type
            ))
        })?;

        for (param_name, param_def) in &action_def.parameters {
            let val = proposal.arguments.get(param_name);
            if param_def.required {
                if val.is_none() || matches!(val, Some(v) if v.is_null()) {
                    return Err(AiActionProposalError::validation(format!(
                        "Required parameter '{}' is missing in proposal arguments",
                        param_name
                    )));
                }
            }

            if let Some(v) = val {
                if !v.is_null() {
                    let type_lower = param_def.param_type.to_lowercase();
                    match type_lower.as_str() {
                        "string" | "text" => {
                            if !v.is_string() {
                                return Err(AiActionProposalError::validation(format!(
                                    "Parameter '{}' expects string, got {:?}",
                                    param_name, v
                                )));
                            }
                        }
                        "boolean" | "bool" => {
                            if !v.is_boolean() {
                                return Err(AiActionProposalError::validation(format!(
                                    "Parameter '{}' expects boolean, got {:?}",
                                    param_name, v
                                )));
                            }
                        }
                        "integer" | "int" | "double" | "number" | "float" => {
                            if !v.is_number() {
                                return Err(AiActionProposalError::validation(format!(
                                    "Parameter '{}' expects number, got {:?}",
                                    param_name, v
                                )));
                            }
                        }
                        "array" => {
                            if !v.is_array() {
                                return Err(AiActionProposalError::validation(format!(
                                    "Parameter '{}' expects array, got {:?}",
                                    param_name, v
                                )));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(())
    }

    async fn recompute_constraints(&self, proposal: &AiActionProposal) -> Result<(), AiActionProposalError> {
        if proposal.object_type == "Flight" && proposal.action_name == "change_stand" {
            if let Some(new_stand_id) = proposal
                .arguments
                .get("new_stand_id")
                .and_then(Value::as_str)
                .or_else(|| proposal.arguments.get("stand_id").and_then(Value::as_str))
            {
                if let Some(stand_repo) = &self.stand_repository {
                    match stand_repo.is_active(new_stand_id).await {
                        Ok(true) => {}
                        Ok(false) => {
                            return Err(AiActionProposalError::conflict(format!(
                                "Constraint validation failed: Stand '{}' is not available",
                                new_stand_id
                            )));
                        }
                        Err(fms_domain::error::DomainError::NotFound { .. }) => {
                            return Err(AiActionProposalError::conflict(format!(
                                "Constraint validation failed: Stand '{}' does not exist",
                                new_stand_id
                            )));
                        }
                        Err(e) => {
                            return Err(AiActionProposalError::internal(e.to_string()));
                        }
                    }
                }
            }
        }

        if proposal.object_type == "Anomaly"
            && (proposal.action_name == "escalate" || proposal.action_name == "acknowledge")
        {
            if let Some(anomaly_repository) = &self.anomaly_repository {
                let anomaly = anomaly_repository
                    .find_by_id(&proposal.object_id)
                    .await
                    .map_err(|e| AiActionProposalError::internal(e.to_string()))?;

                match anomaly {
                    Some(anomaly) => {
                        if anomaly.status.as_ref() == "resolved" {
                            return Err(AiActionProposalError::conflict(format!(
                                "Constraint validation failed: Anomaly '{}' is already resolved",
                                proposal.object_id
                            )));
                        }
                    }
                    None => {
                        return Err(AiActionProposalError::conflict(format!(
                            "Constraint validation failed: Anomaly '{}' does not exist",
                            proposal.object_id
                        )));
                    }
                }
            }
        }

        Ok(())
    }
}
