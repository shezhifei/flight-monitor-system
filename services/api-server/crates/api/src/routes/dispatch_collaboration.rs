//! 派工协作路由。

use actix_web::{web, HttpResponse};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use super::dispatch_chat;
use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use fms_application::services::dispatch_collaboration_query_service::DispatchCollaborationQueryService;
use fms_domain::ports::dispatch_collaboration_repository::DispatchCollaborationRepository;

#[derive(Debug, Deserialize)]
struct OffsetPagination {
    limit: Option<i64>,
    offset: Option<i64>,
}

/// GET /api/v2/dispatch/collaboration/flights/{flight_id}
async fn get_flight_collab(
    svc: web::Data<Arc<DispatchCollaborationQueryService>>,
    path: web::Path<String>,
    query: web::Query<OffsetPagination>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let user_id = claims.0.sub.as_deref();
    let payload = svc
        .get_flight_view(
            &path.into_inner(),
            user_id,
            query.limit.unwrap_or(50),
            query.offset.unwrap_or(0),
        )
        .await?;
    Ok(HttpResponse::Ok().json(payload))
}

/// GET /api/v2/dispatch/collaboration/flights/{flight_id}/events
async fn get_flight_events(
    svc: web::Data<Arc<DispatchCollaborationQueryService>>,
    path: web::Path<String>,
    query: web::Query<OffsetPagination>,
    _claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let payload = svc
        .list_flight_events(
            &path.into_inner(),
            query.limit.unwrap_or(100),
            query.offset.unwrap_or(0),
        )
        .await?;
    Ok(HttpResponse::Ok().json(payload))
}

/// GET /api/v2/dispatch/collaboration/flights/{flight_id}/stakeholders
async fn get_flight_stakeholders(
    chat_repo: web::Data<Arc<dyn DispatchCollaborationRepository + Send + Sync>>,
    path: web::Path<String>,
    _claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let flight_id = path.into_inner();

    let group = chat_repo.get_group_by_flight(&flight_id).await?;
    let Some(group) = group else {
        return Ok(HttpResponse::Ok().json(json!({
            "items": [],
            "flight_id": flight_id,
        })));
    };

    let members = chat_repo.find_active_members(&group.group_id).await?;
    let items: Vec<serde_json::Value> = members
        .into_iter()
        .map(|m| {
            json!({
                "user_id": m.user_id.trim(),
                "username": m.username.as_deref().unwrap_or("").trim(),
                "is_assignee": m.is_assignee,
                "is_dispatcher": m.is_dispatcher,
            })
        })
        .collect();

    Ok(HttpResponse::Ok().json(json!({
        "items": items,
        "flight_id": flight_id,
    })))
}

/// GET /api/v2/dispatch/collaboration/orders/{order_id}
async fn get_order_collab(
    svc: web::Data<Arc<DispatchCollaborationQueryService>>,
    path: web::Path<String>,
    query: web::Query<OffsetPagination>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let order_id = path.into_inner();
    let payload = svc
        .get_order_view(
            &order_id,
            claims.0.sub.as_deref(),
            query.limit.unwrap_or(50),
            query.offset.unwrap_or(0),
        )
        .await?;

    match payload {
        Some(payload) => Ok(HttpResponse::Ok().json(payload)),
        None => Err(ApiError::NotFound("派工单不存在".into())),
    }
}

/// GET /api/v2/dispatch/collaboration/orders/{order_id}/events
async fn get_order_events(
    svc: web::Data<Arc<DispatchCollaborationQueryService>>,
    path: web::Path<String>,
    query: web::Query<OffsetPagination>,
    _claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let payload = svc
        .list_order_events(
            &path.into_inner(),
            query.limit.unwrap_or(100),
            query.offset.unwrap_or(0),
        )
        .await?;
    Ok(HttpResponse::Ok().json(payload))
}

/// 注册派工协作路由 (5 endpoints)
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/dispatch/collaboration")
            .configure(dispatch_chat::configure_under_collaboration_scope)
            .route("/flights/{flight_id}", web::get().to(get_flight_collab))
            .route("/flights/{flight_id}/events", web::get().to(get_flight_events))
            .route(
                "/flights/{flight_id}/stakeholders",
                web::get().to(get_flight_stakeholders),
            )
            .route("/orders/{order_id}", web::get().to(get_order_collab))
            .route("/orders/{order_id}/events", web::get().to(get_order_events)),
    );
}
