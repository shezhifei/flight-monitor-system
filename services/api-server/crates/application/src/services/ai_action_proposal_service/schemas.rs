use fms_domain::models::ai_proposal::{ApprovalPolicy, ConstraintResult, RiskLevel};
use serde_json::Value;
#[derive(Debug, Clone)]
pub struct GenerateProposalRequest {
    pub job_id: String,
    pub run_id: String,
    pub ontology_version: Option<String>,
    pub object_type: String,
    pub object_id: String,
    pub action_name: String,
    pub arguments: Value,
    pub reasoning: Option<String>,
    pub confidence: Option<f64>,
    pub requester_user_id: Option<String>,
    pub requester_user_roles: Vec<String>,
    pub requester_department_id: Option<String>,
    pub correlation_id: Option<String>,
    pub idempotency_key: Option<String>,
    pub expected_object_version: Option<i64>,
    pub risk_level: Option<RiskLevel>,
    pub approval_policy: Option<ApprovalPolicy>,
    pub required_permissions: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct ValidateProposalRequest {
    pub proposal_id: String,
    pub before_snapshot: Option<Value>,
    pub after_preview: Option<Value>,
    pub constraint_results: Option<Vec<ConstraintResult>>,
}

#[derive(Debug, Clone)]
pub struct SubmitProposalRequest {
    pub proposal_id: String,
}

#[derive(Debug, Clone)]
pub struct ApproveProposalRequest {
    pub proposal_id: String,
    pub approver_id: String,
    pub approver_permissions: Vec<String>,
    pub approver_department_id: Option<String>,
    pub modified_arguments: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct RejectProposalRequest {
    pub proposal_id: String,
    pub rejecter_id: String,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct ExecuteProposalRequest {
    pub proposal_id: String,
    pub executor_id: String,
    pub executor_permissions: Vec<String>,
    pub executor_department_id: Option<String>,
}
