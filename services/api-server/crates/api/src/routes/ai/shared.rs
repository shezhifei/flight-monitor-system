use actix_web::HttpResponse;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::services::python_sidecar_proxy::ai_sidecar_url;
use crate::sse::hub::SseHub;
use fms_application::schemas::response::ApiErrorResponse;

pub use fms_application::services::ai_route_service::{AiRouteError, AiRouteService};
pub use fms_application::services::ai_runtime_service::{AiRuntimeError, AiRuntimeService};

pub fn ai_feature_enabled(flag_name: &str, default: bool) -> bool {
    std::env::var(flag_name)
        .ok()
        .map(|value| match value.trim().to_ascii_lowercase().as_str() {
            "0" | "false" | "off" | "no" => false,
            "1" | "true" | "on" | "yes" => true,
            _ => default,
        })
        .unwrap_or(default)
}

#[derive(Debug, Deserialize)]
pub struct ListToolsQuery {
    pub category: Option<String>,
    pub invocation_mode: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ToolExecuteRequest {
    pub tool_name: String,
    #[serde(default = "default_object")]
    pub tool_args: Value,
}

#[derive(Debug, Deserialize)]
pub struct PendingActionListQuery {
    pub status: Option<String>,
    pub tool_name: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

#[derive(Debug, Deserialize)]
pub struct PendingActionDecisionRequest {
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BatchActionRequest {
    pub action_ids: Vec<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PendingActionModifiedApprovalRequest {
    #[serde(default = "default_object")]
    pub modified_arguments: Value,
}

#[derive(Debug, Deserialize)]
pub struct ExecutionListQuery {
    pub todo_id: Option<String>,
    pub entity_id: Option<String>,
    pub status: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Debug, Deserialize)]
pub struct TodoExecuteRequest {
    pub entity_id: Option<String>,
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
    pub system_prompt_override: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TodoTreeExecuteRequest {
    #[serde(default = "default_max_iterations")]
    pub max_iterations_per_todo: usize,
    #[serde(default = "default_true")]
    pub fail_fast: bool,
}

#[derive(Debug, Deserialize)]
pub struct TodoChainCreateFromTemplateRequest {
    pub template_id: String,
    #[serde(default = "default_object")]
    pub context: Value,
}

#[derive(Debug, Deserialize, serde::Serialize)]
pub struct TaskPlanRequest {
    pub prompt: String,
    pub entity_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TodoGraphPilotQuery {
    pub entity_id: Option<String>,
    #[serde(default = "default_window_hours")]
    pub window_hours: i32,
    #[serde(default = "default_sample_limit")]
    pub sample_limit: i32,
    #[serde(default = "default_pending_stale")]
    pub pending_stale_after_minutes: i32,
}

pub fn default_limit() -> usize {
    50
}

pub fn default_object() -> Value {
    json!({})
}

pub fn default_max_iterations() -> usize {
    10
}

pub fn default_true() -> bool {
    true
}

pub fn default_window_hours() -> i32 {
    168
}

pub fn default_sample_limit() -> i32 {
    200
}

pub fn default_pending_stale() -> i32 {
    30
}

pub fn ok_resp(data: impl serde::Serialize) -> HttpResponse {
    HttpResponse::Ok().json(json!({ "success": true, "data": data }))
}

pub fn ok_resp_with_message(data: impl serde::Serialize, message: &str) -> HttpResponse {
    HttpResponse::Ok().json(json!({ "success": true, "data": data, "message": message }))
}

pub fn execution_owner_id(execution: &Value) -> Option<&str> {
    execution.get("user_id").and_then(Value::as_str)
}

pub fn can_access_execution(claims: &JwtAuth, execution: &Value) -> bool {
    if claims.has_permission("ai:monitor") {
        return true;
    }
    let current = current_user_id(claims);
    let owner = execution_owner_id(execution).unwrap_or("");
    !owner.is_empty() && owner == current
}

pub fn current_user_id(claims: &JwtAuth) -> String {
    claims
        .0
        .sub
        .clone()
        .or_else(|| claims.0.username.clone())
        .unwrap_or_else(|| "unknown_user".to_string())
}

pub fn current_user_roles(claims: &JwtAuth) -> Vec<String> {
    if claims.0.is_admin.unwrap_or(false) {
        vec!["admin".to_string()]
    } else {
        Vec::new()
    }
}

pub fn runtime_conflict_response(code: String, message: String, blocked_reason: Option<String>) -> HttpResponse {
    let mut response = HttpResponse::Conflict();
    response.insert_header(("X-Error-Code", code));
    if let Some(reason) = blocked_reason {
        response.insert_header(("X-Decision-Blocked-Reason", reason));
    }
    response.json(ApiErrorResponse::new(
        actix_web::http::StatusCode::CONFLICT.as_u16(),
        message,
    ))
}

pub fn raw_detail(status: actix_web::http::StatusCode, detail: impl serde::Serialize) -> HttpResponse {
    HttpResponse::build(status).json(json!({ "detail": detail }))
}

pub fn execution_result_response(data: &Value) -> Value {
    let execution_result = data.get("execution_result").unwrap_or(&Value::Null);
    json!({
        "success": execution_result.get("status").and_then(Value::as_str) == Some("success"),
        "status": execution_result.get("status").cloned().unwrap_or_else(|| json!("error")),
        "code": execution_result.get("code"),
        "message": execution_result.get("message"),
        "recoverable": execution_result.get("recoverable"),
        "retryable": execution_result.get("retryable"),
        "severity": execution_result.get("severity"),
        "approval_id": data.get("pending_action").and_then(|item| item.get("action_id")),
        "data": data,
        "meta": { "contract_version": "2.0" },
    })
}

pub fn rejection_response(data: &Value) -> Value {
    json!({
        "success": true,
        "status": "success",
        "code": "APPROVAL_REJECTED",
        "message": "approval request rejected by human reviewer",
        "recoverable": true,
        "retryable": false,
        "severity": "warning",
        "approval_id": data.get("pending_action").and_then(|item| item.get("action_id")),
        "data": data,
        "meta": { "contract_version": "2.0" },
    })
}

pub fn batch_error_result(action_id: &str, status: &str, code: &str, message: impl Into<String>) -> Value {
    json!({
        "action_id": action_id,
        "success": false,
        "status": status,
        "code": code,
        "message": message.into(),
    })
}

pub fn batch_approve_success_result(action_id: &str, data: &Value) -> Value {
    let pending_action = data.get("pending_action").unwrap_or(&Value::Null);
    let execution_result = data.get("execution_result").unwrap_or(&Value::Null);
    let pending_status = pending_action
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase();
    let execution_status = execution_result
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase();
    let status = if pending_status.is_empty() {
        if execution_status.is_empty() {
            "unknown".to_string()
        } else {
            execution_status.clone()
        }
    } else {
        pending_status
    };
    let success = matches!(status.as_str(), "approved" | "executed" | "success")
        && matches!(execution_status.as_str(), "" | "success" | "executed");
    json!({
        "action_id": action_id,
        "success": success,
        "status": status,
        "code": execution_result.get("code").or_else(|| pending_action.get("status_code")),
        "message": execution_result.get("message").or_else(|| pending_action.get("execution_error")),
        "data": data,
    })
}

pub fn batch_reject_success_result(action_id: &str, data: &Value) -> Value {
    let pending_action = data.get("pending_action").unwrap_or(&Value::Null);
    json!({
        "action_id": action_id,
        "success": true,
        "status": "rejected",
        "code": pending_action.get("status_code").cloned().unwrap_or_else(|| json!("APPROVAL_REJECTED")),
        "message": "approval request rejected by human reviewer",
        "data": data,
    })
}

pub fn map_runtime_error(error: AiRuntimeError) -> ApiError {
    match error {
        AiRuntimeError::NotFound(message) => ApiError::NotFound(message),
        AiRuntimeError::Validation(message) => ApiError::BadRequest(message),
        AiRuntimeError::Conflict { message, .. } => ApiError::Conflict(message),
    }
}

pub async fn broadcast_ai_event(hub: &Arc<SseHub>, event: &str, payload: Value) {
    let _ = hub.broadcast_event("ai_execution", Some(event), payload.clone()).await;
    let _ = hub
        .broadcast_event(
            "smart_monitor",
            Some(event),
            json!({
                "event": event,
                "data": payload,
            }),
        )
        .await;
}

pub fn map_route_error(error: AiRouteError) -> ApiError {
    match error {
        AiRouteError::Domain(domain_err) => ApiError::from(domain_err),
    }
}

pub fn ai_sidecar_base_url() -> String {
    ai_sidecar_url()
}
