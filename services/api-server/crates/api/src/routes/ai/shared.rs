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

pub use fms_application::services::ai_route_service::{
    ai_feature_enabled, AiEventPayload, AiRouteError, AiRouteService,
};
pub use fms_application::services::ai_runtime_service::AiRuntimeError;

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
        AiRouteError::Runtime(AiRuntimeError::NotFound(msg)) => ApiError::NotFound(msg),
        AiRouteError::Runtime(AiRuntimeError::Validation(msg)) => ApiError::BadRequest(msg),
        AiRouteError::Runtime(AiRuntimeError::Conflict { message, .. }) => ApiError::Conflict(message),
    }
}

pub fn ai_sidecar_base_url() -> String {
    ai_sidecar_url()
}
