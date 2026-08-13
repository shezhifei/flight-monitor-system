//! AI 动作建议路由
//!
//! 将 AiActionProposalService 和微模型执行能力暴露为 REST API。

pub(crate) use crate::error::ApiError;
pub(crate) use crate::middleware::jwt::JwtAuth;
pub(crate) use crate::middleware::permissions::PermissionCheck;
pub(crate) use actix_web::{web, HttpResponse};
pub(crate) use fms_application::services::ai_action_proposal_service::{
    AiActionProposalError, AiActionProposalService, ApproveProposalRequest, ExecuteProposalRequest,
    GenerateProposalRequest, RejectProposalRequest, ValidateProposalRequest,
};
pub(crate) use fms_domain::models::ai_proposal::{ActionProposalQuery, ConstraintResult};
pub(crate) use serde::Deserialize;
pub(crate) use serde_json::{json, Value};
pub(crate) use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub(crate) struct ProposalGenerateRequest {
    pub(crate) job_id: String,
    pub(crate) run_id: String,
    pub(crate) ontology_version: Option<String>,
    pub(crate) object_type: String,
    pub(crate) object_id: String,
    pub(crate) action_name: String,
    #[serde(default = "default_object")]
    pub(crate) arguments: Value,
    pub(crate) reasoning: Option<String>,
    pub(crate) confidence: Option<f64>,
    pub(crate) correlation_id: Option<String>,
    pub(crate) idempotency_key: Option<String>,
    pub(crate) expected_object_version: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProposalValidateRequest {
    pub(crate) proposal_id: String,
    #[serde(default = "default_object")]
    pub(crate) before_snapshot: Value,
    #[serde(default = "default_object")]
    pub(crate) after_preview: Value,
    pub(crate) constraint_results: Option<Vec<ConstraintResult>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProposalApproveRequest {
    #[serde(default = "default_object")]
    pub(crate) modified_arguments: Value,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProposalRejectRequest {
    pub(crate) reason: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProposalListQuery {
    pub(crate) job_id: Option<String>,
    pub(crate) run_id: Option<String>,
    pub(crate) object_type: Option<String>,
    pub(crate) object_id: Option<String>,
    pub(crate) action_name: Option<String>,
    pub(crate) status: Option<String>,
    #[serde(default = "default_limit")]
    pub(crate) limit: usize,
    #[serde(default)]
    pub(crate) offset: usize,
}

pub(crate) fn default_object() -> Value {
    json!({})
}

pub(crate) fn default_limit() -> usize {
    50
}

pub(crate) fn ok_resp(data: impl serde::Serialize) -> HttpResponse {
    HttpResponse::Ok().json(json!({ "success": true, "data": data }))
}

pub(crate) fn current_user_id(claims: &JwtAuth) -> String {
    claims
        .0
        .sub
        .clone()
        .or_else(|| claims.0.username.clone())
        .unwrap_or_else(|| "unknown_user".to_string())
}

pub(crate) fn current_permissions(claims: &JwtAuth) -> Vec<String> {
    let mut permissions = claims.0.permissions.clone();
    if claims.0.is_admin.unwrap_or(false) && !permissions.iter().any(|item| item == "*") {
        permissions.push("*".to_string());
    }
    permissions
}

pub(crate) fn parse_status_code(s: &str) -> i32 {
    match s.trim().to_lowercase().as_str() {
        "draft" => 0,
        "validating" => 1,
        "pending" => 2,
        "approved" => 3,
        "rejected" => 4,
        "executing" => 5,
        "executed" => 6,
        "failed" => 7,
        "cancelled" => 8,
        "expired" => 9,
        _ => -1,
    }
}

pub(crate) fn map_proposal_error(err: AiActionProposalError) -> ApiError {
    match err {
        AiActionProposalError::NotFound(id) => ApiError::NotFound(id),
        AiActionProposalError::Validation(msg) => ApiError::BadRequest(msg),
        AiActionProposalError::Conflict(msg) => ApiError::Conflict(msg),
        AiActionProposalError::Execution(msg) => ApiError::Internal(msg),
        AiActionProposalError::Repository(msg) => ApiError::Internal(msg),
        AiActionProposalError::Forbidden(msg) => ApiError::Forbidden(msg),
    }
}
