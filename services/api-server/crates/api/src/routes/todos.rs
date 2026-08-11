//! 待办事项路由。
//!
//! 对齐 Python `todo_routes.py`。

use actix_web::{web, HttpResponse};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use fms_application::schemas::todo_schemas::*;
use fms_application::services::todo_service::TodoService;

fn current_user_id(claims: &JwtAuth) -> Result<&str, ApiError> {
    claims
        .0
        .sub
        .as_deref()
        .ok_or_else(|| ApiError::Unauthorized("未认证".into()))
}

/// POST /api/v2/todos
async fn create_todo(
    svc: web::Data<Arc<TodoService>>,
    body: web::Json<TodoCreate>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("todo:write")?;
    let result = svc
        .create_todo(body.into_inner().into(), current_user_id(&claims)?)
        .await?;
    Ok(HttpResponse::Created().json(result))
}

/// GET /api/v2/todos
async fn list_todos(
    svc: web::Data<Arc<TodoService>>,
    query: web::Query<TodoListQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("todo:read")?;
    let page = query.page.unwrap_or(1).max(1);
    let size = query.size.unwrap_or(20).clamp(1, 100);
    let result = svc
        .list_todos(
            query.status.as_deref(),
            query.priority.as_deref(),
            query.category.as_deref(),
            query.assignee.as_deref(),
            query.source_type.as_deref(),
            query.source_id.as_deref(),
            query.agent_status.as_deref(),
            query.agent_entity_id.as_deref(),
            query.agent_run_id.as_deref(),
            page,
            size,
        )
        .await?;
    Ok(HttpResponse::Ok().json(result))
}

/// GET /api/v2/todos/stats
async fn get_stats(svc: web::Data<Arc<TodoService>>, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("todo:read")?;
    Ok(HttpResponse::Ok().json(svc.get_stats().await?))
}

/// GET /api/v2/todos/agent-context/metrics
async fn agent_context_metrics(
    svc: web::Data<Arc<TodoService>>,
    query: web::Query<AgentContextMetricsQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("system:config")?;
    let metrics = svc.get_agent_context_query_metrics();

    let repo_get_calls = value_as_f64(metrics.get("repo_get_calls"));
    let repo_batch_get_calls = value_as_f64(metrics.get("repo_batch_get_calls"));
    let repo_find_calls = value_as_f64(metrics.get("repo_find_todo_ids_calls"));
    let total_samples = repo_get_calls + repo_batch_get_calls + repo_find_calls;

    let compat_fallback_ratio = value_as_f64(metrics.get("dedicated_query_compat_fallback_ratio"));

    let enough_samples = total_samples >= query.min_samples as f64;
    let compat_within_threshold = compat_fallback_ratio <= query.compat_fallback_ratio_threshold;

    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "data": {
            "metrics": metrics,
            "rollout_check": {
                "min_samples": query.min_samples,
                "total_samples": total_samples,
                "enough_samples": enough_samples,
                "compat_fallback_ratio_threshold": query.compat_fallback_ratio_threshold,
                "compat_fallback_ratio": compat_fallback_ratio,
                "compat_within_threshold": compat_within_threshold,
            }
        }
    })))
}

/// GET /api/v2/todos/{todo_id}
async fn get_todo(
    svc: web::Data<Arc<TodoService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("todo:read")?;
    let todo_id = path.into_inner();
    match svc.get_todo(&todo_id).await? {
        Some(todo) => Ok(HttpResponse::Ok().json(todo)),
        None => Err(ApiError::NotFound("Not found".into())),
    }
}

/// PUT /api/v2/todos/{todo_id}
async fn update_todo(
    svc: web::Data<Arc<TodoService>>,
    path: web::Path<String>,
    body: web::Json<TodoUpdate>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("todo:write")?;
    let todo_id = path.into_inner();
    let todo = svc
        .update_todo(&todo_id, body.into_inner(), current_user_id(&claims)?)
        .await?;
    Ok(HttpResponse::Ok().json(todo))
}

/// POST /api/v2/todos/{todo_id}/complete
async fn complete_todo(
    svc: web::Data<Arc<TodoService>>,
    path: web::Path<String>,
    body: web::Json<TodoComplete>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("todo:write")?;
    let todo_id = path.into_inner();
    let todo = svc
        .complete_todo(&todo_id, body.into_inner(), current_user_id(&claims)?)
        .await?;
    Ok(HttpResponse::Ok().json(todo))
}

/// POST /api/v2/todos/{todo_id}/cancel
async fn cancel_todo(
    svc: web::Data<Arc<TodoService>>,
    path: web::Path<String>,
    body: web::Json<TodoCancel>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("todo:write")?;
    let todo_id = path.into_inner();
    let todo = svc
        .cancel_todo(&todo_id, body.into_inner(), current_user_id(&claims)?)
        .await?;
    Ok(HttpResponse::Ok().json(todo))
}

/// POST /api/v2/todos/{todo_id}/assign
async fn assign_todo(
    svc: web::Data<Arc<TodoService>>,
    path: web::Path<String>,
    body: web::Json<TodoAssign>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("todo:write")?;
    let todo_id = path.into_inner();
    let todo = svc
        .assign_todo(&todo_id, body.into_inner(), current_user_id(&claims)?)
        .await?;
    Ok(HttpResponse::Ok().json(todo))
}

/// POST /api/v2/todos/{todo_id}/progress
async fn update_progress(
    svc: web::Data<Arc<TodoService>>,
    path: web::Path<String>,
    body: web::Json<TodoProgress>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("todo:write")?;
    let todo_id = path.into_inner();
    let todo = svc
        .update_progress(&todo_id, body.into_inner(), current_user_id(&claims)?)
        .await?;
    Ok(HttpResponse::Ok().json(todo))
}

#[derive(Deserialize)]
pub struct TodoListQuery {
    page: Option<i64>,
    size: Option<i64>,
    status: Option<String>,
    priority: Option<String>,
    category: Option<String>,
    assignee: Option<String>,
    source_type: Option<String>,
    source_id: Option<String>,
    agent_status: Option<String>,
    agent_entity_id: Option<String>,
    agent_run_id: Option<String>,
}

#[derive(Deserialize)]
pub struct AgentContextMetricsQuery {
    #[serde(default = "default_min_samples")]
    min_samples: i64,
    #[serde(default = "default_legacy_hit_ratio_threshold")]
    legacy_hit_ratio_threshold: f64,
    #[serde(default = "default_compat_fallback_ratio_threshold")]
    compat_fallback_ratio_threshold: f64,
}

fn default_min_samples() -> i64 {
    100
}

fn default_legacy_hit_ratio_threshold() -> f64 {
    0.01
}

fn default_compat_fallback_ratio_threshold() -> f64 {
    0.05
}

fn value_as_f64(value: Option<&serde_json::Value>) -> f64 {
    value.and_then(|item| item.as_f64()).unwrap_or(0.0)
}

/// 注册待办路由
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/todos")
            .route("", web::post().to(create_todo))
            .route("", web::get().to(list_todos))
            .route("/stats", web::get().to(get_stats))
            .route("/agent-context/metrics", web::get().to(agent_context_metrics))
            .route("/{todo_id}", web::get().to(get_todo))
            .route("/{todo_id}", web::put().to(update_todo))
            .route("/{todo_id}/complete", web::post().to(complete_todo))
            .route("/{todo_id}/cancel", web::post().to(cancel_todo))
            .route("/{todo_id}/assign", web::post().to(assign_todo))
            .route("/{todo_id}/progress", web::post().to(update_progress)),
    );
}
