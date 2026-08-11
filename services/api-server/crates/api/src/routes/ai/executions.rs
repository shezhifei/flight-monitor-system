use actix_web::{web, HttpResponse};
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::sse::hub::SseHub;
use fms_application::services::ai_route_service::AiRouteService;
use fms_domain::error::DomainError;

use super::shared::*;

pub async fn execute_todo(
    svc: web::Data<Arc<AiRouteService>>,
    sse_hub: web::Data<Arc<SseHub>>,
    path: web::Path<String>,
    claims: JwtAuth,
    body: Option<web::Json<TodoExecuteRequest>>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:execute")?;
    let todo_id = path.into_inner();
    let body = body.map(|item| item.into_inner()).unwrap_or(TodoExecuteRequest {
        entity_id: None,
        max_iterations: default_max_iterations(),
        system_prompt_override: None,
    });
    let (data, event) = svc
        .execute_todo(
            todo_id,
            body.entity_id,
            body.max_iterations,
            body.system_prompt_override,
            current_user_id(&claims),
            current_user_roles(&claims),
        )
        .await
        .map_err(map_route_error)?;
    broadcast_ai_event(&sse_hub, &event.event, event.payload).await;
    Ok(ok_resp(data))
}

pub async fn execute_todo_tree(
    svc: web::Data<Arc<AiRouteService>>,
    sse_hub: web::Data<Arc<SseHub>>,
    path: web::Path<String>,
    claims: JwtAuth,
    body: Option<web::Json<TodoTreeExecuteRequest>>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:execute")?;
    let todo_id = path.into_inner();
    let body = body.map(|item| item.into_inner()).unwrap_or(TodoTreeExecuteRequest {
        max_iterations_per_todo: default_max_iterations(),
        fail_fast: true,
    });
    let (data, event) = svc
        .execute_todo_tree(
            todo_id,
            body.max_iterations_per_todo,
            body.fail_fast,
            current_user_id(&claims),
            current_user_roles(&claims),
        )
        .await
        .map_err(map_route_error)?;
    broadcast_ai_event(&sse_hub, &event.event, event.payload).await;
    Ok(ok_resp(data))
}

pub async fn create_chain(
    svc: web::Data<Arc<AiRouteService>>,
    claims: JwtAuth,
    body: web::Json<TodoChainCreateFromTemplateRequest>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:execute")?;
    let payload = svc
        .create_chain(&body.template_id, body.context.clone())
        .await
        .map_err(map_route_error)?;
    Ok(ok_resp(payload))
}

pub async fn get_chain_status(
    svc: web::Data<Arc<AiRouteService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    let root_todo_id = path.into_inner();
    let payload = svc.get_chain_status(&root_todo_id).await.map_err(map_route_error)?;
    Ok(ok_resp(payload))
}

pub async fn list_chain_templates(
    svc: web::Data<Arc<AiRouteService>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    let payload = svc.list_chain_templates().await.map_err(map_route_error)?;
    Ok(ok_resp(payload))
}

pub async fn get_execution(
    svc: web::Data<Arc<AiRouteService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    let run_id = path.into_inner();
    match svc
        .get_execution(&run_id, &claims.0.permissions, &current_user_id(&claims))
        .await
    {
        Ok(Some(data)) => Ok(ok_resp(data)),
        Ok(None) => Err(ApiError::NotFound(format!("执行记录不存在: {run_id}"))),
        Err(AiRouteError::Domain(DomainError::PermissionDenied(_))) => {
            Err(ApiError::Forbidden("无权访问该执行记录".to_string()))
        }
        Err(error) => Err(map_route_error(error)),
    }
}

pub async fn list_executions(
    svc: web::Data<Arc<AiRouteService>>,
    query: web::Query<ExecutionListQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    let can_view_all = claims.has_permission("ai:monitor");
    let payload = svc
        .list_executions(
            query.todo_id.as_deref(),
            query.entity_id.as_deref(),
            query.status.as_deref(),
            query.limit,
            can_view_all,
            &current_user_id(&claims),
        )
        .await
        .map_err(map_route_error)?;
    Ok(ok_resp(payload))
}

pub async fn cancel_execution(
    svc: web::Data<Arc<AiRouteService>>,
    sse_hub: web::Data<Arc<SseHub>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:execute")?;
    let run_id = path.into_inner();
    match svc
        .cancel_execution(run_id.clone(), &claims.0.permissions, current_user_id(&claims))
        .await
    {
        Ok((success, Some(event))) => {
            broadcast_ai_event(&sse_hub, &event.event, event.payload).await;
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": success,
                "message": if success { "执行已取消" } else { "无法取消执行" }
            })))
        }
        Ok((success, None)) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "success": success,
            "message": if success { "执行已取消" } else { "无法取消执行" }
        }))),
        Err(AiRouteError::Domain(DomainError::PermissionDenied(_))) => {
            Err(ApiError::Forbidden("无权取消该执行".to_string()))
        }
        Err(error) => Err(map_route_error(error)),
    }
}
