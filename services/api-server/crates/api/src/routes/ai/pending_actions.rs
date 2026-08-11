use actix_web::{web, HttpResponse};
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::sse::hub::SseHub;
use fms_application::services::ai_route_service::AiRouteService;
use fms_application::services::ai_runtime_service::AiRuntimeError;

use super::shared::*;

pub async fn list_pending_actions(
    svc: web::Data<Arc<AiRouteService>>,
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
        .await
        .map_err(map_route_error)?;
    Ok(ok_resp(data))
}

pub async fn get_action_diff(
    svc: web::Data<Arc<AiRouteService>>,
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
    match svc.get_action_diff(&action_id).await {
        Ok(data) => Ok(ok_resp(data)),
        Err(AiRouteError::Domain(fms_domain::error::DomainError::NotFound { .. })) => Ok(raw_detail(
            actix_web::http::StatusCode::NOT_FOUND,
            format!("待审批动作不存在: {action_id}"),
        )),
        Err(error) => Err(map_route_error(error)),
    }
}

pub async fn get_action_result(
    svc: web::Data<Arc<AiRouteService>>,
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
    match svc.get_action_result(&action_id).await {
        Ok(data) => Ok(ok_resp(data)),
        Err(AiRouteError::Domain(fms_domain::error::DomainError::NotFound { .. })) => Ok(raw_detail(
            actix_web::http::StatusCode::NOT_FOUND,
            format!("待审批动作不存在: {action_id}"),
        )),
        Err(error) => Err(map_route_error(error)),
    }
}

pub async fn approve_action(
    svc: web::Data<Arc<AiRouteService>>,
    sse_hub: web::Data<Arc<SseHub>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:execute")?;
    let action_id = path.into_inner();
    match svc.approve_action(action_id.clone(), current_user_id(&claims)).await {
        Ok((response, Some(event))) => {
            broadcast_ai_event(&sse_hub, &event.event, event.payload).await;
            Ok(HttpResponse::Ok().json(response))
        }
        Ok((response, None)) => Ok(HttpResponse::Ok().json(response)),
        Err(AiRouteError::Runtime(AiRuntimeError::Conflict {
            code,
            message,
            blocked_reason,
        })) => Ok(runtime_conflict_response(code, message, blocked_reason)),
        Err(error) => Err(map_route_error(error)),
    }
}

pub async fn reject_action(
    svc: web::Data<Arc<AiRouteService>>,
    sse_hub: web::Data<Arc<SseHub>>,
    path: web::Path<String>,
    claims: JwtAuth,
    body: Option<web::Json<PendingActionDecisionRequest>>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:execute")?;
    let action_id = path.into_inner();
    let reason = body.and_then(|item| item.reason.clone());
    match svc
        .reject_action(action_id.clone(), current_user_id(&claims), reason)
        .await
    {
        Ok((response, Some(event))) => {
            broadcast_ai_event(&sse_hub, &event.event, event.payload).await;
            Ok(HttpResponse::Ok().json(response))
        }
        Ok((response, None)) => Ok(HttpResponse::Ok().json(response)),
        Err(AiRouteError::Runtime(AiRuntimeError::Conflict {
            code,
            message,
            blocked_reason,
        })) => Ok(runtime_conflict_response(code, message, blocked_reason)),
        Err(error) => Err(map_route_error(error)),
    }
}

pub async fn batch_approve(
    svc: web::Data<Arc<AiRouteService>>,
    sse_hub: web::Data<Arc<SseHub>>,
    claims: JwtAuth,
    body: web::Json<BatchActionRequest>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:execute")?;
    let (response, event) = svc
        .batch_approve(body.action_ids.clone(), current_user_id(&claims))
        .await
        .map_err(map_route_error)?;
    if let Some(event) = event {
        broadcast_ai_event(&sse_hub, &event.event, event.payload).await;
    }
    Ok(HttpResponse::Ok().json(response))
}

pub async fn batch_reject(
    svc: web::Data<Arc<AiRouteService>>,
    sse_hub: web::Data<Arc<SseHub>>,
    claims: JwtAuth,
    body: web::Json<BatchActionRequest>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:execute")?;
    let (response, event) = svc
        .batch_reject(body.action_ids.clone(), body.reason.clone(), current_user_id(&claims))
        .await
        .map_err(map_route_error)?;
    if let Some(event) = event {
        broadcast_ai_event(&sse_hub, &event.event, event.payload).await;
    }
    Ok(HttpResponse::Ok().json(response))
}

pub async fn approve_modified(
    svc: web::Data<Arc<AiRouteService>>,
    sse_hub: web::Data<Arc<SseHub>>,
    path: web::Path<String>,
    claims: JwtAuth,
    body: web::Json<PendingActionModifiedApprovalRequest>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:execute")?;
    let action_id = path.into_inner();
    match svc
        .approve_modified(
            action_id.clone(),
            current_user_id(&claims),
            body.modified_arguments.clone(),
        )
        .await
    {
        Ok((response, Some(event))) => {
            broadcast_ai_event(&sse_hub, &event.event, event.payload).await;
            Ok(HttpResponse::Ok().json(response))
        }
        Ok((response, None)) => Ok(HttpResponse::Ok().json(response)),
        Err(AiRouteError::Runtime(AiRuntimeError::Conflict {
            code,
            message,
            blocked_reason,
        })) => Ok(runtime_conflict_response(code, message, blocked_reason)),
        Err(error) => Err(map_route_error(error)),
    }
}
