//! AI 动作建议 (AiActionProposal) 领域模型
//!
//! 用对象-动作语义替代工具驱动写入。
//! 每个 AiActionProposal 代表 AI 对某个 Ontology 对象提出的一个结构化动作建议，
//! 包含完整的风险评估、约束校验、审批策略和执行生命周期。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// RiskLevel — 风险等级
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum RiskLevel {
    #[default]
    Low = 0,
    Medium = 1,
    High = 2,
    Critical = 3,
}

impl RiskLevel {
    pub fn code(self) -> i32 {
        self as i32
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }

    pub fn from_code(code: i32) -> Option<Self> {
        match code {
            0 => Some(Self::Low),
            1 => Some(Self::Medium),
            2 => Some(Self::High),
            3 => Some(Self::Critical),
            _ => None,
        }
    }

    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "low" | "低" | "0" => Some(Self::Low),
            "medium" | "中" | "1" => Some(Self::Medium),
            "high" | "高" | "2" => Some(Self::High),
            "critical" | "严重" | "3" => Some(Self::Critical),
            _ => None,
        }
    }

    pub fn requires_approval(self) -> bool {
        matches!(self, Self::High | Self::Critical)
    }

    pub fn requires_supervisor(self) -> bool {
        self == Self::Critical
    }
}

// ---------------------------------------------------------------------------
// ApprovalPolicy — 审批策略
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ApprovalPolicy {
    AutoExecute = 0,
    #[default]
    RequireApproval = 1,
    RequireSupervisorApproval = 2,
    RequireFlowableApproval = 3,
}

impl ApprovalPolicy {
    pub fn code(self) -> i32 {
        self as i32
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::AutoExecute => "auto_execute",
            Self::RequireApproval => "require_approval",
            Self::RequireSupervisorApproval => "require_supervisor_approval",
            Self::RequireFlowableApproval => "require_flowable_approval",
        }
    }

    pub fn from_code(code: i32) -> Option<Self> {
        match code {
            0 => Some(Self::AutoExecute),
            1 => Some(Self::RequireApproval),
            2 => Some(Self::RequireSupervisorApproval),
            3 => Some(Self::RequireFlowableApproval),
            _ => None,
        }
    }

    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "auto_execute" | "auto" | "自动执行" | "0" => Some(Self::AutoExecute),
            "require_approval" | "approval" | "需审批" | "1" => Some(Self::RequireApproval),
            "require_supervisor_approval" | "supervisor" | "需主管审批" | "2" => {
                Some(Self::RequireSupervisorApproval)
            }
            "require_flowable_approval" | "flowable" | "需流程审批" | "3" => Some(Self::RequireFlowableApproval),
            _ => None,
        }
    }

    pub fn is_auto(self) -> bool {
        self == Self::AutoExecute
    }
}

// ---------------------------------------------------------------------------
// ActionProposalStatus — 建议状态
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ActionProposalStatus {
    Draft = 0,
    Validating = 1,
    Pending = 2,
    Approved = 3,
    Rejected = 4,
    Executing = 5,
    Executed = 6,
    Failed = 7,
    Cancelled = 8,
    Expired = 9,
}

impl ActionProposalStatus {
    pub fn code(self) -> i32 {
        self as i32
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Validating => "validating",
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Executing => "executing",
            Self::Executed => "executed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
        }
    }

    pub fn from_code(code: i32) -> Option<Self> {
        match code {
            0 => Some(Self::Draft),
            1 => Some(Self::Validating),
            2 => Some(Self::Pending),
            3 => Some(Self::Approved),
            4 => Some(Self::Rejected),
            5 => Some(Self::Executing),
            6 => Some(Self::Executed),
            7 => Some(Self::Failed),
            8 => Some(Self::Cancelled),
            9 => Some(Self::Expired),
            _ => None,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Executed | Self::Failed | Self::Rejected | Self::Cancelled | Self::Expired
        )
    }

    pub fn can_approve(self) -> bool {
        self == Self::Pending
    }

    pub fn can_reject(self) -> bool {
        matches!(self, Self::Pending | Self::Draft | Self::Validating)
    }

    pub fn can_execute(self) -> bool {
        self == Self::Approved
    }
}

impl Default for ActionProposalStatus {
    fn default() -> Self {
        Self::Draft
    }
}

// ---------------------------------------------------------------------------
// ConstraintResult — 约束校验结果
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConstraintResult {
    pub constraint_name: String,
    pub constraint_type: String,
    pub passed: bool,
    pub message: Option<String>,
    pub severity: String,
}

impl ConstraintResult {
    pub fn new(constraint_name: impl Into<String>, constraint_type: impl Into<String>, passed: bool) -> Self {
        Self {
            constraint_name: constraint_name.into(),
            constraint_type: constraint_type.into(),
            passed,
            message: None,
            severity: if passed { "info" } else { "error" }.to_string(),
        }
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    pub fn with_severity(mut self, severity: impl Into<String>) -> Self {
        self.severity = severity.into();
        self
    }
}

// ---------------------------------------------------------------------------
// AiActionProposal — AI 动作建议主体
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiActionProposal {
    pub proposal_id: String,
    pub job_id: String,
    pub run_id: String,
    pub ontology_version: String,
    pub object_type: String,
    pub object_id: String,
    pub action_name: String,
    pub arguments: Value,
    pub risk_level: RiskLevel,
    pub required_permissions: Vec<String>,
    pub approval_policy: ApprovalPolicy,
    pub before_snapshot: Option<Value>,
    pub after_preview: Option<Value>,
    pub constraint_results: Vec<ConstraintResult>,
    pub confidence: f64,
    pub reasoning: String,
    pub status: ActionProposalStatus,
    pub pending_action_id: Option<String>,
    pub approved_by: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
    pub rejected_by: Option<String>,
    pub rejected_reason: Option<String>,
    pub rejected_at: Option<DateTime<Utc>>,
    pub executed_by: Option<String>,
    pub executed_at: Option<DateTime<Utc>>,
    pub execution_result: Option<Value>,
    pub execution_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub correlation_id: Option<String>,
    pub metadata: Value,
}

impl AiActionProposal {
    pub fn new(
        proposal_id: impl Into<String>,
        job_id: impl Into<String>,
        run_id: impl Into<String>,
        object_type: impl Into<String>,
        object_id: impl Into<String>,
        action_name: impl Into<String>,
        arguments: Value,
    ) -> Self {
        let now = Utc::now();
        Self {
            proposal_id: proposal_id.into(),
            job_id: job_id.into(),
            run_id: run_id.into(),
            ontology_version: String::from("v1"),
            object_type: object_type.into(),
            object_id: object_id.into(),
            action_name: action_name.into(),
            arguments,
            risk_level: RiskLevel::default(),
            required_permissions: Vec::new(),
            approval_policy: ApprovalPolicy::default(),
            before_snapshot: None,
            after_preview: None,
            constraint_results: Vec::new(),
            confidence: 0.0,
            reasoning: String::new(),
            status: ActionProposalStatus::Draft,
            pending_action_id: None,
            approved_by: None,
            approved_at: None,
            rejected_by: None,
            rejected_reason: None,
            rejected_at: None,
            executed_by: None,
            executed_at: None,
            execution_result: None,
            execution_error: None,
            created_at: now,
            updated_at: now,
            expires_at: None,
            correlation_id: None,
            metadata: Value::Null,
        }
    }

    pub fn with_ontology_version(mut self, version: impl Into<String>) -> Self {
        self.ontology_version = version.into();
        self
    }

    pub fn with_risk_level(mut self, risk_level: RiskLevel) -> Self {
        self.risk_level = risk_level;
        self
    }

    pub fn with_approval_policy(mut self, policy: ApprovalPolicy) -> Self {
        self.approval_policy = policy;
        self
    }

    pub fn with_required_permissions(mut self, permissions: Vec<String>) -> Self {
        self.required_permissions = permissions;
        self
    }

    pub fn with_before_snapshot(mut self, snapshot: Value) -> Self {
        self.before_snapshot = Some(snapshot);
        self
    }

    pub fn with_after_preview(mut self, preview: Value) -> Self {
        self.after_preview = Some(preview);
        self
    }

    pub fn with_constraint_results(mut self, results: Vec<ConstraintResult>) -> Self {
        self.constraint_results = results;
        self
    }

    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    pub fn with_reasoning(mut self, reasoning: impl Into<String>) -> Self {
        self.reasoning = reasoning.into();
        self
    }

    pub fn with_correlation_id(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = Some(correlation_id.into());
        self
    }

    pub fn with_expires_at(mut self, expires_at: DateTime<Utc>) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn all_constraints_passed(&self) -> bool {
        self.constraint_results.iter().all(|c| c.passed)
    }

    pub fn failed_constraints(&self) -> Vec<&ConstraintResult> {
        self.constraint_results.iter().filter(|c| !c.passed).collect()
    }

    pub fn transition_to(&mut self, new_status: ActionProposalStatus) -> Result<(), String> {
        let valid = match (self.status, new_status) {
            (ActionProposalStatus::Draft, ActionProposalStatus::Validating) => true,
            (ActionProposalStatus::Draft, ActionProposalStatus::Cancelled) => true,
            (ActionProposalStatus::Validating, ActionProposalStatus::Pending) => true,
            (ActionProposalStatus::Validating, ActionProposalStatus::Rejected) => true,
            (ActionProposalStatus::Validating, ActionProposalStatus::Cancelled) => true,
            (ActionProposalStatus::Pending, ActionProposalStatus::Approved) => true,
            (ActionProposalStatus::Pending, ActionProposalStatus::Rejected) => true,
            (ActionProposalStatus::Pending, ActionProposalStatus::Expired) => true,
            (ActionProposalStatus::Approved, ActionProposalStatus::Executing) => true,
            (ActionProposalStatus::Approved, ActionProposalStatus::Cancelled) => true,
            (ActionProposalStatus::Executing, ActionProposalStatus::Executed) => true,
            (ActionProposalStatus::Executing, ActionProposalStatus::Failed) => true,
            _ => false,
        };

        if valid {
            self.status = new_status;
            self.updated_at = Utc::now();
            Ok(())
        } else {
            Err(format!(
                "invalid status transition from {} to {}",
                self.status.label(),
                new_status.label()
            ))
        }
    }

    pub fn approve(&mut self, approver_id: impl Into<String>) -> Result<(), String> {
        if !self.status.can_approve() {
            return Err(format!("cannot approve proposal in status {}", self.status.label()));
        }
        self.status = ActionProposalStatus::Approved;
        self.approved_by = Some(approver_id.into());
        self.approved_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn reject(&mut self, rejecter_id: impl Into<String>, reason: impl Into<String>) -> Result<(), String> {
        if !self.status.can_reject() {
            return Err(format!("cannot reject proposal in status {}", self.status.label()));
        }
        self.status = ActionProposalStatus::Rejected;
        self.rejected_by = Some(rejecter_id.into());
        self.rejected_reason = Some(reason.into());
        self.rejected_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn mark_executed(&mut self, executor_id: impl Into<String>, result: Value) {
        self.status = ActionProposalStatus::Executed;
        self.executed_by = Some(executor_id.into());
        self.executed_at = Some(Utc::now());
        self.execution_result = Some(result);
        self.updated_at = Utc::now();
    }

    pub fn mark_failed(&mut self, error: impl Into<String>) {
        self.status = ActionProposalStatus::Failed;
        self.execution_error = Some(error.into());
        self.updated_at = Utc::now();
    }

    pub fn is_expired(&self) -> bool {
        match self.expires_at {
            Some(exp) => Utc::now() > exp,
            None => false,
        }
    }

    pub fn to_pending_action_summary(&self) -> Value {
        serde_json::json!({
            "proposal_id": self.proposal_id,
            "object_type": self.object_type,
            "object_id": self.object_id,
            "action_name": self.action_name,
            "arguments": self.arguments,
            "risk_level": self.risk_level.label(),
            "approval_policy": self.approval_policy.label(),
            "reasoning": self.reasoning,
            "confidence": self.confidence,
            "status": self.status.label(),
            "created_at": self.created_at,
            "expires_at": self.expires_at,
        })
    }
}

// ---------------------------------------------------------------------------
// ActionProposalQuery — 查询参数
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct ActionProposalQuery {
    pub job_id: Option<String>,
    pub run_id: Option<String>,
    pub object_type: Option<String>,
    pub object_id: Option<String>,
    pub action_name: Option<String>,
    pub status: Option<ActionProposalStatus>,
    pub risk_level: Option<RiskLevel>,
    pub approval_policy: Option<ApprovalPolicy>,
    pub requester_user_id: Option<String>,
    pub pending_action_id: Option<String>,
    /// Match proposals whose metadata JSON contains this idempotency key.
    pub idempotency_key: Option<String>,
    pub created_after: Option<DateTime<Utc>>,
    pub created_before: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

// ---------------------------------------------------------------------------
// ActionProposalStats — 统计聚合
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActionProposalStats {
    pub total: usize,
    pub by_status: Value,
    pub by_risk_level: Value,
    pub by_object_type: Value,
    pub avg_confidence: f64,
    pub approval_rate: f64,
    pub rejection_rate: f64,
    pub execution_success_rate: f64,
    pub avg_execution_time_ms: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_proposal() -> AiActionProposal {
        AiActionProposal {
            proposal_id: "p-1".into(),
            job_id: "j-1".into(),
            run_id: "r-1".into(),
            ontology_version: "v1".into(),
            object_type: "flight_leg".into(),
            object_id: "leg-100".into(),
            action_name: "update_status".into(),
            arguments: json!({"status": "delayed"}),
            risk_level: RiskLevel::Low,
            required_permissions: vec![],
            approval_policy: ApprovalPolicy::RequireApproval,
            before_snapshot: None,
            after_preview: None,
            constraint_results: vec![],
            confidence: 0.9,
            reasoning: "test".into(),
            status: ActionProposalStatus::Draft,
            pending_action_id: None,
            approved_by: None,
            approved_at: None,
            rejected_by: None,
            rejected_reason: None,
            rejected_at: None,
            executed_by: None,
            executed_at: None,
            execution_result: None,
            execution_error: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            expires_at: None,
            correlation_id: None,
            metadata: json!({}),
        }
    }

    // --- RiskLevel ---

    #[test]
    fn risk_level_code_roundtrip() {
        for level in [RiskLevel::Low, RiskLevel::Medium, RiskLevel::High, RiskLevel::Critical] {
            assert_eq!(RiskLevel::from_code(level.code()), Some(level));
        }
        assert_eq!(RiskLevel::from_code(99), None);
    }

    #[test]
    fn risk_level_str_loose() {
        assert_eq!(RiskLevel::from_str_loose("high"), Some(RiskLevel::High));
        assert_eq!(RiskLevel::from_str_loose("高"), Some(RiskLevel::High));
        assert_eq!(RiskLevel::from_str_loose("2"), Some(RiskLevel::High));
        assert_eq!(RiskLevel::from_str_loose("bogus"), None);
    }

    #[test]
    fn risk_level_requires_approval() {
        assert!(!RiskLevel::Low.requires_approval());
        assert!(!RiskLevel::Medium.requires_approval());
        assert!(RiskLevel::High.requires_approval());
        assert!(RiskLevel::Critical.requires_approval());
    }

    #[test]
    fn risk_level_requires_supervisor() {
        assert!(!RiskLevel::High.requires_supervisor());
        assert!(RiskLevel::Critical.requires_supervisor());
    }

    // --- ApprovalPolicy ---

    #[test]
    fn approval_policy_code_roundtrip() {
        for p in [
            ApprovalPolicy::AutoExecute,
            ApprovalPolicy::RequireApproval,
            ApprovalPolicy::RequireSupervisorApproval,
            ApprovalPolicy::RequireFlowableApproval,
        ] {
            assert_eq!(ApprovalPolicy::from_code(p.code()), Some(p));
        }
    }

    #[test]
    fn approval_policy_is_auto() {
        assert!(ApprovalPolicy::AutoExecute.is_auto());
        assert!(!ApprovalPolicy::RequireApproval.is_auto());
    }

    // --- ActionProposalStatus ---

    #[test]
    fn status_code_roundtrip() {
        for s in [
            ActionProposalStatus::Draft,
            ActionProposalStatus::Validating,
            ActionProposalStatus::Pending,
            ActionProposalStatus::Approved,
            ActionProposalStatus::Rejected,
            ActionProposalStatus::Executing,
            ActionProposalStatus::Executed,
            ActionProposalStatus::Failed,
            ActionProposalStatus::Cancelled,
            ActionProposalStatus::Expired,
        ] {
            assert_eq!(ActionProposalStatus::from_code(s.code()), Some(s));
        }
    }

    #[test]
    fn status_is_terminal() {
        assert!(ActionProposalStatus::Executed.is_terminal());
        assert!(ActionProposalStatus::Failed.is_terminal());
        assert!(ActionProposalStatus::Rejected.is_terminal());
        assert!(ActionProposalStatus::Cancelled.is_terminal());
        assert!(ActionProposalStatus::Expired.is_terminal());
        assert!(!ActionProposalStatus::Draft.is_terminal());
        assert!(!ActionProposalStatus::Pending.is_terminal());
    }

    #[test]
    fn status_can_approve_only_pending() {
        assert!(ActionProposalStatus::Pending.can_approve());
        assert!(!ActionProposalStatus::Draft.can_approve());
        assert!(!ActionProposalStatus::Approved.can_approve());
    }

    #[test]
    fn status_can_reject_draft_validating_pending() {
        assert!(ActionProposalStatus::Draft.can_reject());
        assert!(ActionProposalStatus::Validating.can_reject());
        assert!(ActionProposalStatus::Pending.can_reject());
        assert!(!ActionProposalStatus::Approved.can_reject());
    }

    // --- ConstraintResult ---

    #[test]
    fn constraint_result_severity_defaults() {
        let pass = ConstraintResult::new("c1", "type", true);
        assert_eq!(pass.severity, "info");
        let fail = ConstraintResult::new("c2", "type", false);
        assert_eq!(fail.severity, "error");
    }

    // --- AiActionProposal state machine ---

    #[test]
    fn valid_transitions_succeed() {
        let mut p = sample_proposal();
        p.transition_to(ActionProposalStatus::Validating).unwrap();
        assert_eq!(p.status, ActionProposalStatus::Validating);
        p.transition_to(ActionProposalStatus::Pending).unwrap();
        assert_eq!(p.status, ActionProposalStatus::Pending);
        p.transition_to(ActionProposalStatus::Approved).unwrap();
        assert_eq!(p.status, ActionProposalStatus::Approved);
        p.transition_to(ActionProposalStatus::Executing).unwrap();
        assert_eq!(p.status, ActionProposalStatus::Executing);
        p.transition_to(ActionProposalStatus::Executed).unwrap();
        assert_eq!(p.status, ActionProposalStatus::Executed);
    }

    #[test]
    fn invalid_transition_returns_error() {
        let mut p = sample_proposal();
        let err = p.transition_to(ActionProposalStatus::Executed).unwrap_err();
        assert!(err.contains("invalid status transition"));
        assert_eq!(p.status, ActionProposalStatus::Draft);
    }

    #[test]
    fn approve_from_pending_succeeds() {
        let mut p = sample_proposal();
        p.status = ActionProposalStatus::Pending;
        p.approve("mgr-1").unwrap();
        assert_eq!(p.status, ActionProposalStatus::Approved);
        assert_eq!(p.approved_by.as_deref(), Some("mgr-1"));
    }

    #[test]
    fn approve_from_non_pending_fails() {
        let mut p = sample_proposal();
        assert!(p.approve("mgr-1").is_err());
    }

    #[test]
    fn reject_from_pending_succeeds() {
        let mut p = sample_proposal();
        p.status = ActionProposalStatus::Pending;
        p.reject("mgr-2", "not safe").unwrap();
        assert_eq!(p.status, ActionProposalStatus::Rejected);
        assert_eq!(p.rejected_by.as_deref(), Some("mgr-2"));
        assert_eq!(p.rejected_reason.as_deref(), Some("not safe"));
    }

    #[test]
    fn reject_from_approved_fails() {
        let mut p = sample_proposal();
        p.status = ActionProposalStatus::Approved;
        assert!(p.reject("mgr-2", "no").is_err());
    }

    #[test]
    fn mark_executed_sets_fields() {
        let mut p = sample_proposal();
        p.status = ActionProposalStatus::Executing;
        p.mark_executed("bot", json!({"ok": true}));
        assert_eq!(p.status, ActionProposalStatus::Executed);
        assert_eq!(p.executed_by.as_deref(), Some("bot"));
        assert_eq!(p.execution_result, Some(json!({"ok": true})));
    }

    #[test]
    fn mark_failed_sets_error() {
        let mut p = sample_proposal();
        p.status = ActionProposalStatus::Executing;
        p.mark_failed("timeout");
        assert_eq!(p.status, ActionProposalStatus::Failed);
        assert_eq!(p.execution_error.as_deref(), Some("timeout"));
    }

    #[test]
    fn all_constraints_passed_and_failed() {
        let mut p = sample_proposal();
        p.constraint_results = vec![
            ConstraintResult::new("c1", "t", true),
            ConstraintResult::new("c2", "t", true),
        ];
        assert!(p.all_constraints_passed());
        assert!(p.failed_constraints().is_empty());

        p.constraint_results.push(ConstraintResult::new("c3", "t", false));
        assert!(!p.all_constraints_passed());
        assert_eq!(p.failed_constraints().len(), 1);
    }

    #[test]
    fn to_pending_action_summary_contains_fields() {
        let p = sample_proposal();
        let summary = p.to_pending_action_summary();
        assert_eq!(summary["proposal_id"], "p-1");
        assert_eq!(summary["action_name"], "update_status");
        assert_eq!(summary["risk_level"], "low");
    }
}
