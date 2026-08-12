//! Flowable 工作流路由
//!
//! 当前已对齐核心 Flowable REST 代理接口，流程草案 AI 助手接口仍在继续迁移。

pub(crate) use crate::error::ApiError;
pub(crate) use crate::middleware::jwt::JwtAuth;
pub(crate) use crate::services::runtime_error_monitor::record_service_unavailable_background;
pub(crate) use actix_multipart::Multipart;
pub(crate) use actix_web::{web, Error as ActixError, HttpRequest, HttpResponse};
pub(crate) use fms_application::schemas::flowable_draft_schemas::FlowableDraftAssistantChatRequest;
pub(crate) use fms_application::services::authorization_service::{
    AuthorizationService, PermissionCatalog, ScopeLevel,
};
pub(crate) use fms_application::services::business_case_workflow_service::BusinessCaseWorkflowService;
pub(crate) use fms_application::services::flowable_draft_service::{
    FlowableDraftAssistantStreamEvent, FlowableDraftService, FlowableDraftServiceError,
};
pub(crate) use fms_application::services::flowable_service::{FlowableService, FlowableServiceError};
pub(crate) use fms_runtime::spawn_tracked::spawn_tracked;
pub(crate) use futures_core::Stream;
pub(crate) use futures_util::{StreamExt, TryStreamExt};
pub(crate) use serde::Deserialize;
pub(crate) use serde_json::{json, Value};
pub(crate) use std::collections::VecDeque;
pub(crate) use std::pin::Pin;
pub(crate) use std::sync::Arc;
pub(crate) use std::task::{Context, Poll};
pub(crate) use tokio::sync::mpsc;
#[derive(Debug, Deserialize)]
pub(crate) struct ProcessDefinitionsQuery {
    pub(crate) key: Option<String>,
    pub(crate) tenant_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DeploymentsQuery {
    pub(crate) name: Option<String>,
    pub(crate) tenant_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DeleteDeploymentQuery {
    pub(crate) cascade: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateDeploymentRequest {
    pub(crate) bpmn_xml: String,
    pub(crate) deployment_name: Option<String>,
    pub(crate) category: Option<String>,
    pub(crate) tenant_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StartProcessInstanceRequest {
    pub(crate) process_key: String,
    pub(crate) business_key: Option<String>,
    pub(crate) variables: Option<serde_json::Map<String, serde_json::Value>>,
    pub(crate) tenant_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProcessInstancesQuery {
    pub(crate) process_key: Option<String>,
    pub(crate) business_key: Option<String>,
    pub(crate) tenant_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DeleteProcessInstanceQuery {
    pub(crate) delete_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TasksQuery {
    pub(crate) assignee: Option<String>,
    pub(crate) owner: Option<String>,
    pub(crate) process_instance_id: Option<String>,
    pub(crate) process_definition_key: Option<String>,
    pub(crate) tenant_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ClaimTaskRequest {
    pub(crate) user_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CompleteTaskRequest {
    pub(crate) variables: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StartProcessWithSubprocessRequest {
    pub(crate) process_key: String,
    pub(crate) business_key: Option<String>,
    pub(crate) variables: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SetProcessInstanceVariablesRequest {
    pub(crate) variables: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct HistoricProcessInstancesQuery {
    pub(crate) process_definition_key: Option<String>,
    pub(crate) business_key: Option<String>,
    pub(crate) start_time_before: Option<String>,
    pub(crate) start_time_after: Option<String>,
    pub(crate) end_time_before: Option<String>,
    pub(crate) end_time_after: Option<String>,
    pub(crate) started_by: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct HistoricTasksQuery {
    pub(crate) process_instance_id: Option<String>,
    pub(crate) assignee: Option<String>,
    pub(crate) owner: Option<String>,
    pub(crate) task_definition_key: Option<String>,
    pub(crate) start_time_before: Option<String>,
    pub(crate) start_time_after: Option<String>,
    pub(crate) end_time_before: Option<String>,
    pub(crate) end_time_after: Option<String>,
}

pub(crate) const COMMON_TENANT: &str = "COMMON";
pub(crate) fn collect_filters<'a>(items: &'a [(&'a str, Option<String>)]) -> Vec<(&'a str, String)> {
    items
        .iter()
        .filter_map(|(key, value)| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| (*key, value.to_string()))
        })
        .collect()
}

pub(crate) struct RouteSseStream {
    pub(crate) receiver: mpsc::Receiver<String>,
    pub(crate) initial_events: VecDeque<String>,
}

pub(crate) fn ok_resp(data: impl serde::Serialize, message: &str) -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "data": data,
        "message": message,
    }))
}

pub(crate) fn has_resource_wildcard(claims: &JwtAuth, permission: &str) -> bool {
    permission
        .split_once('.')
        .map(|(resource, _)| format!("{resource}.*"))
        .map(|wildcard| claims.0.permissions.iter().any(|value| value == &wildcard))
        .unwrap_or(false)
}

pub(crate) fn ensure_grant(claims: &JwtAuth, permission: &str) -> Result<(), ApiError> {
    if AuthorizationService::has_grant(&claims.0, permission) || has_resource_wildcard(claims, permission) {
        return Ok(());
    }
    Err(ApiError::Forbidden(format!("缺少权限: {permission}")))
}

pub(crate) fn ensure_scope_grant(claims: &JwtAuth, permission: &str, scope: ScopeLevel) -> Result<(), ApiError> {
    if AuthorizationService::scope_grant(&claims.0, permission, scope) || has_resource_wildcard(claims, permission) {
        return Ok(());
    }
    Err(ApiError::Forbidden(format!("缺少权限: {permission} @ {:?}", scope)))
}

pub(crate) fn scope_from_tenant_id(tenant_id: &str) -> ScopeLevel {
    if tenant_id == COMMON_TENANT {
        ScopeLevel::Common
    } else {
        ScopeLevel::Department
    }
}

pub(crate) fn ensure_authenticated(claims: &JwtAuth) -> Result<(), ApiError> {
    if AuthorizationService::is_authenticated(&claims.0) {
        return Ok(());
    }
    Err(ApiError::Unauthorized("未认证".into()))
}

pub(crate) fn normalize_optional_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(crate) fn decode_base64_url_segment(segment: &str) -> Option<Vec<u8>> {
    let mut output = Vec::with_capacity(segment.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0u8;

    for byte in segment.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            b'=' => continue,
            _ => return None,
        } as u32;

        buffer = (buffer << 6) | value;
        bits += 6;

        while bits >= 8 {
            bits -= 8;
            output.push(((buffer >> bits) & 0xff) as u8);
        }

        if bits > 0 {
            buffer &= (1u32 << bits) - 1;
        } else {
            buffer = 0;
        }
    }

    Some(output)
}

pub(crate) fn extract_claim_from_authorization(req: &HttpRequest, key: &str) -> Option<String> {
    let token = req
        .headers()
        .get("Authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer ").or_else(|| value.strip_prefix("bearer ")))?;
    let payload_segment = token.split('.').nth(1)?;
    let payload_bytes = decode_base64_url_segment(payload_segment)?;
    let payload: Value = serde_json::from_slice(&payload_bytes).ok()?;
    normalize_optional_string(payload.get(key).and_then(Value::as_str))
}

pub(crate) fn current_department_tenant(req: &HttpRequest, claims: &JwtAuth) -> Option<String> {
    extract_claim_from_authorization(req, "department_id")
        .or_else(|| extract_claim_from_authorization(req, "departmentId"))
        .or_else(|| normalize_optional_string(claims.0.department.as_deref()))
        .or_else(|| extract_claim_from_authorization(req, "department"))
}

pub(crate) fn resolve_requested_tenant(
    req: &HttpRequest,
    claims: &JwtAuth,
    requested_tenant: Option<&str>,
) -> Result<String, ApiError> {
    let requested_tenant = normalize_optional_string(requested_tenant);
    let current_tenant = current_department_tenant(req, claims);

    match requested_tenant.as_deref() {
        Some(COMMON_TENANT) => Ok(COMMON_TENANT.to_string()),
        Some(tenant) => {
            let Some(current_tenant) = current_tenant else {
                return Err(ApiError::Forbidden("当前用户未配置部门租户，仅允许访问 COMMON".into()));
            };

            if tenant == current_tenant {
                return Ok(current_tenant);
            }

            Err(ApiError::Forbidden(format!(
                "禁止访问租户 {tenant}，仅允许当前部门租户或 COMMON"
            )))
        }
        None => {
            current_tenant.ok_or_else(|| ApiError::BadRequest("当前用户缺少部门信息，请显式传 tenant_id=COMMON".into()))
        }
    }
}

pub(crate) fn map_service_error(error: FlowableServiceError) -> ApiError {
    match error {
        FlowableServiceError::Validation(message) => ApiError::BadRequest(message),
        FlowableServiceError::NotFound(message) => ApiError::NotFound(message),
        FlowableServiceError::Upstream(message) => ApiError::Internal(message),
    }
}

pub(crate) fn map_draft_error(error: FlowableDraftServiceError) -> HttpResponse {
    match error {
        FlowableDraftServiceError::Validation(message) | FlowableDraftServiceError::InvalidRequest(message) => {
            detail_response(
                actix_web::http::StatusCode::UNPROCESSABLE_ENTITY,
                "INVALID_REQUEST",
                &message,
            )
        }
        FlowableDraftServiceError::ProcessDocument {
            status_code,
            code,
            message,
        } => detail_response(
            actix_web::http::StatusCode::from_u16(status_code)
                .unwrap_or(actix_web::http::StatusCode::UNPROCESSABLE_ENTITY),
            &code,
            &message,
        ),
        FlowableDraftServiceError::AIUnavailable(message) => HttpResponse::ServiceUnavailable().json({
            record_service_unavailable_background(message.clone(), "flowable_draft_ai_unavailable", "infrastructure");
            json!({
                "detail": {
                    "code": "AI_UNAVAILABLE",
                    "message": message,
                }
            })
        }),
        FlowableDraftServiceError::BpmnDraftValidation { code, message } => {
            detail_response(actix_web::http::StatusCode::UNPROCESSABLE_ENTITY, &code, &message)
        }
    }
}

pub(crate) fn detail_response(status: actix_web::http::StatusCode, code: &str, message: &str) -> HttpResponse {
    HttpResponse::build(status).json(json!({
        "detail": {
            "code": code,
            "message": message,
        }
    }))
}

pub(crate) fn raw_detail_message(status: actix_web::http::StatusCode, message: &str) -> HttpResponse {
    HttpResponse::build(status).json(json!({
        "detail": message
    }))
}

pub(crate) fn missing_process_instance_response(message: &str) -> HttpResponse {
    HttpResponse::BadGateway().json(json!({
        "detail": message
    }))
}

pub(crate) fn flowable_health_error_response() -> HttpResponse {
    record_service_unavailable_background("Flowable REST API 调用失败", "flowable_health_check", "infrastructure");
    HttpResponse::ServiceUnavailable().json(serde_json::json!({
        "success": false,
        "data": {
            "status": "error",
            "message": "Flowable REST API 调用失败",
        },
        "message": "Flowable服务健康检查失败"
    }))
}

pub(crate) fn flowable_draft_service_unavailable() -> HttpResponse {
    record_service_unavailable_background("流程草案生成服务不可用", "flowable_draft_service", "infrastructure");
    HttpResponse::ServiceUnavailable().json(json!({
        "detail": "流程草案生成服务不可用"
    }))
}

pub(crate) fn flowable_client_unavailable() -> HttpResponse {
    record_service_unavailable_background("Flowable 客户端不可用", "flowable_client", "infrastructure");
    HttpResponse::ServiceUnavailable().json(json!({
        "detail": "Flowable 客户端不可用"
    }))
}

pub(crate) fn flowable_service_unavailable() -> HttpResponse {
    record_service_unavailable_background(
        "Flowable 应用服务不可用",
        "flowable_application_service",
        "infrastructure",
    );
    HttpResponse::ServiceUnavailable().json(json!({
        "detail": "Flowable 应用服务不可用"
    }))
}

pub(crate) fn build_sse_event_string(event: &str, payload: Value) -> String {
    format!(
        "event: {event}\ndata: {}\n\n",
        serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string())
    )
}

pub(crate) fn flowable_stream_event_to_sse(
    request_id: &str,
    user_id: &str,
    event: FlowableDraftAssistantStreamEvent,
) -> String {
    let scene = "flowable_assistant";
    let timestamp = chrono::Utc::now().to_rfc3339();
    match event {
        FlowableDraftAssistantStreamEvent::Progress { stage, message, mode } => build_sse_event_string(
            "progress",
            json!({
                "request_id": request_id,
                "scene": scene,
                "user_id": user_id,
                "timestamp": timestamp,
                "stage": stage,
                "message": message,
                "mode": mode,
            }),
        ),
        FlowableDraftAssistantStreamEvent::Error { mode, message } => build_sse_event_string(
            "error",
            json!({
                "request_id": request_id,
                "scene": scene,
                "user_id": user_id,
                "timestamp": timestamp,
                "stage": "stream",
                "mode": mode,
                "message": message,
            }),
        ),
        FlowableDraftAssistantStreamEvent::TextDelta {
            mode,
            delta,
            accumulated_chars,
        } => build_sse_event_string(
            "text_delta",
            json!({
                "request_id": request_id,
                "scene": scene,
                "user_id": user_id,
                "timestamp": timestamp,
                "mode": mode,
                "delta": delta,
                "accumulated_chars": accumulated_chars,
            }),
        ),
        FlowableDraftAssistantStreamEvent::Completed {
            mode,
            warning_count,
            model,
        } => build_sse_event_string(
            "done",
            json!({
                "request_id": request_id,
                "scene": scene,
                "user_id": user_id,
                "timestamp": timestamp,
                "mode": mode,
                "model": model,
                "warnings_count": warning_count,
            }),
        ),
    }
}

impl Stream for RouteSseStream {
    type Item = Result<actix_web::web::Bytes, ActixError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(initial) = self.initial_events.pop_front() {
            return Poll::Ready(Some(Ok(actix_web::web::Bytes::from(initial))));
        }

        // poll_recv registers the waker so the stream is re-woken when a
        // payload arrives; try_recv + Pending would hang forever.
        match self.receiver.poll_recv(cx) {
            Poll::Ready(Some(payload)) => Poll::Ready(Some(Ok(actix_web::web::Bytes::from(payload)))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}
