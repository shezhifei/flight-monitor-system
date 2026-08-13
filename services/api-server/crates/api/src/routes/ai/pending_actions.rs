use actix_web::{web, HttpResponse};
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::sse::hub::SseHub;
use fms_application::services::ai_runtime_service::{AiRuntimeError, AiRuntimeService};

use super::shared::*;

pub async fn list_pending_actions(
    svc: web::Data<Arc<AiRuntimeService>>,
    query: web::Query<PendingActionListQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    let data = svc
        .list_pending_actions(
            query.status.as_deref(),
            query.tool_name.as_deref(),
            query.limit,
            query.offset,
        )
        .await;
    Ok(ok_resp(data))
}

pub async fn get_action_diff(
    svc: web::Data<Arc<AiRuntimeService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    if !ai_feature_enabled("AI_APPROVAL_DIFF_V1", true) {
        return Ok(raw_detail(
            actix_web::http::StatusCode::NOT_FOUND,
            "approval diff endpoint is disabled",
        ));
    }
    let action_id = path.into_inner();
    match svc.get_pending_action_diff(&action_id).await {
        Ok(data) => Ok(ok_resp(data)),
        Err(AiRuntimeError::NotFound(_)) => Ok(raw_detail(
            actix_web::http::StatusCode::NOT_FOUND,
            format!("待审批动作不存在: {action_id}"),
        )),
        Err(error) => Err(map_runtime_error(error)),
    }
}

pub async fn get_action_result(
    svc: web::Data<Arc<AiRuntimeService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    if !ai_feature_enabled("AI_APPROVAL_DIFF_V1", true) {
        return Ok(raw_detail(
            actix_web::http::StatusCode::NOT_FOUND,
            "approval result endpoint is disabled",
        ));
    }
    let action_id = path.into_inner();
    match svc.get_pending_action_result(&action_id).await {
        Ok(data) => Ok(ok_resp(data)),
        Err(AiRuntimeError::NotFound(_)) => Ok(raw_detail(
            actix_web::http::StatusCode::NOT_FOUND,
            format!("待审批动作不存在: {action_id}"),
        )),
        Err(error) => Err(map_runtime_error(error)),
    }
}

pub async fn approve_action(
    svc: web::Data<Arc<AiRuntimeService>>,
    sse_hub: web::Data<Arc<SseHub>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:execute")?;
    let action_id = path.into_inner();
    let approver_id = current_user_id(&claims);
    match svc.approve_pending_action(&action_id, &approver_id, None).await {
        Ok(data) => {
            broadcast_ai_event(
                &sse_hub,
                "action_approved",
                serde_json::json!({
                    "event": "approval_result", "status": "success", "action_id": action_id,
                    "approver_id": approver_id, "pending_action": data.get("pending_action"),
                    "execution_result": data.get("execution_result"),
                }),
            )
            .await;
            Ok(HttpResponse::Ok().json(execution_result_response(&data)))
        }
        Err(AiRuntimeError::Conflict {
            code,
            message,
            blocked_reason,
        }) => Ok(runtime_conflict_response(code, message, blocked_reason)),
        Err(error) => Err(map_runtime_error(error)),
    }
}

pub async fn reject_action(
    svc: web::Data<Arc<AiRuntimeService>>,
    sse_hub: web::Data<Arc<SseHub>>,
    path: web::Path<String>,
    claims: JwtAuth,
    body: Option<web::Json<PendingActionDecisionRequest>>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:execute")?;
    let action_id = path.into_inner();
    let reason = body.and_then(|item| item.reason.clone());
    let approver_id = current_user_id(&claims);
    match svc
        .reject_pending_action(&action_id, &approver_id, reason.as_deref())
        .await
    {
        Ok(data) => {
            broadcast_ai_event(
                &sse_hub,
                "action_rejected",
                serde_json::json!({
                    "event": "approval_result", "status": "error", "code": "APPROVAL_REJECTED",
                    "message": "approval request rejected by human reviewer", "action_id": action_id,
                    "approver_id": approver_id, "reason": reason,
                    "pending_action": data.get("pending_action"),
                }),
            )
            .await;
            Ok(HttpResponse::Ok().json(rejection_response(&data)))
        }
        Err(AiRuntimeError::Conflict {
            code,
            message,
            blocked_reason,
        }) => Ok(runtime_conflict_response(code, message, blocked_reason)),
        Err(error) => Err(map_runtime_error(error)),
    }
}

pub async fn batch_approve(
    svc: web::Data<Arc<AiRuntimeService>>,
    sse_hub: web::Data<Arc<SseHub>>,
    claims: JwtAuth,
    body: web::Json<BatchActionRequest>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:execute")?;
    validate_batch_action_ids(&body.action_ids)?;
    let approver_id = current_user_id(&claims);
    let mut results = Vec::new();
    let mut succeeded = 0usize;
    for action_id in &body.action_ids {
        match svc.approve_pending_action(action_id, &approver_id, None).await {
            Ok(data) => {
                let item = batch_approve_success_result(action_id, &data);
                if item["success"] == true {
                    succeeded += 1;
                }
                results.push(item);
            }
            Err(error) => results.push(batch_runtime_error(action_id, error)),
        }
    }
    let payload = serde_json::json!({ "total": body.action_ids.len(), "succeeded": succeeded, "failed": body.action_ids.len() - succeeded, "results": results });
    broadcast_ai_event(
        &sse_hub,
        "batch_approved",
        serde_json::json!({ "event": "batch_approval_result", "approver_id": approver_id, "data": payload }),
    )
    .await;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "success": body.action_ids.len() == succeeded, "data": payload })))
}

pub async fn batch_reject(
    svc: web::Data<Arc<AiRuntimeService>>,
    sse_hub: web::Data<Arc<SseHub>>,
    claims: JwtAuth,
    body: web::Json<BatchActionRequest>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:execute")?;
    validate_batch_action_ids(&body.action_ids)?;
    let approver_id = current_user_id(&claims);
    let mut results = Vec::new();
    let mut succeeded = 0usize;
    for action_id in &body.action_ids {
        match svc
            .reject_pending_action(action_id, &approver_id, body.reason.as_deref())
            .await
        {
            Ok(data) => {
                succeeded += 1;
                results.push(batch_reject_success_result(action_id, &data));
            }
            Err(error) => results.push(batch_runtime_error(action_id, error)),
        }
    }
    let payload = serde_json::json!({ "total": body.action_ids.len(), "succeeded": succeeded, "failed": body.action_ids.len() - succeeded, "reason": body.reason, "results": results });
    broadcast_ai_event(
        &sse_hub,
        "batch_rejected",
        serde_json::json!({ "event": "batch_rejection_result", "approver_id": approver_id, "data": payload }),
    )
    .await;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "success": body.action_ids.len() == succeeded, "data": payload })))
}

pub async fn approve_modified(
    svc: web::Data<Arc<AiRuntimeService>>,
    sse_hub: web::Data<Arc<SseHub>>,
    path: web::Path<String>,
    claims: JwtAuth,
    body: web::Json<PendingActionModifiedApprovalRequest>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:execute")?;
    let action_id = path.into_inner();
    let approver_id = current_user_id(&claims);
    match svc
        .approve_pending_action(&action_id, &approver_id, Some(body.modified_arguments.clone()))
        .await
    {
        Ok(data) => {
            broadcast_ai_event(
                &sse_hub,
                "action_approved",
                serde_json::json!({
                    "event": "approval_result", "status": "success", "action_id": action_id,
                    "approver_id": approver_id, "pending_action": data.get("pending_action"),
                    "execution_result": data.get("execution_result"), "modification": data.get("modification"),
                }),
            )
            .await;
            Ok(HttpResponse::Ok().json(execution_result_response(&data)))
        }
        Err(AiRuntimeError::Conflict {
            code,
            message,
            blocked_reason,
        }) => Ok(runtime_conflict_response(code, message, blocked_reason)),
        Err(error) => Err(map_runtime_error(error)),
    }
}

fn validate_batch_action_ids(action_ids: &[String]) -> Result<(), ApiError> {
    if action_ids.is_empty() {
        return Err(ApiError::BadRequest("action_ids 不可为空".to_string()));
    }
    if action_ids.len() > 50 {
        return Err(ApiError::BadRequest("单次批量操作上限为 50 条".to_string()));
    }
    Ok(())
}

fn batch_runtime_error(action_id: &str, error: AiRuntimeError) -> serde_json::Value {
    match error {
        AiRuntimeError::NotFound(_) => {
            batch_error_result(action_id, "error", "PENDING_ACTION_NOT_FOUND", format!("'{action_id}'"))
        }
        AiRuntimeError::Validation(message) => {
            batch_error_result(action_id, "error", "PENDING_ACTION_BATCH_ERROR", message)
        }
        AiRuntimeError::Conflict { code, message, .. } => {
            let status = if code == "PENDING_ACTION_EXPIRED" {
                "expired"
            } else {
                "conflict"
            };
            batch_error_result(action_id, status, &code, message)
        }
    }
}
