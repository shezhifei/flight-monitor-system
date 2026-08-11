use actix_web::{web, HttpRequest, HttpResponse};
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::services::python_sidecar_proxy::{forward_ai_sidecar_sse, forward_ai_sidecar_sse_json};
use crate::sse::hub::SseHub;
use fms_application::services::ai_route_service::AiRouteService;

use super::shared::*;

pub async fn rate_limit_status(svc: web::Data<Arc<AiRouteService>>, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    Ok(ok_resp(svc.rate_limit_status().await.map_err(map_route_error)?))
}

pub async fn query_routing_metrics(
    svc: web::Data<Arc<AiRouteService>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    Ok(ok_resp(svc.query_routing_metrics().await.map_err(map_route_error)?))
}

pub async fn report_schema_metrics(
    svc: web::Data<Arc<AiRouteService>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    Ok(ok_resp(svc.report_schema_metrics().await.map_err(map_route_error)?))
}

pub async fn execution_visibility_metrics(
    svc: web::Data<Arc<AiRouteService>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    Ok(ok_resp(
        svc.execution_visibility_metrics().await.map_err(map_route_error)?,
    ))
}

pub async fn todo_graph_pilot_metrics(
    svc: web::Data<Arc<AiRouteService>>,
    claims: JwtAuth,
    query: web::Query<TodoGraphPilotQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    let data = svc
        .todo_graph_pilot_metrics(
            query.entity_id.clone(),
            query.window_hours,
            query.sample_limit,
            query.pending_stale_after_minutes,
        )
        .await
        .map_err(map_route_error)?;
    Ok(ok_resp(data))
}

pub async fn generate_plan(
    req: HttpRequest,
    claims: JwtAuth,
    body: web::Json<TaskPlanRequest>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:execute")?;
    Ok(forward_ai_sidecar_sse_json(
        &req,
        reqwest::Method::POST,
        &serde_json::to_value(body.into_inner()).unwrap_or_else(|_| serde_json::json!({})),
    )
    .await)
}

pub async fn events_stream(
    req: HttpRequest,
    _hub: web::Data<Arc<SseHub>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    Ok(forward_ai_sidecar_sse(&req).await)
}
