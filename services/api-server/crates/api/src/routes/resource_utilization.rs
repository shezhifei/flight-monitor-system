//! 资源利用率路由。
//!
//! 对齐 Python `resource_utilization_routes.py`。

use actix_web::{web, HttpResponse};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use fms_application::services::resource_utilization_service::ResourceUtilizationService;

#[derive(Debug, Deserialize)]
struct WindowQuery {
    window_start: Option<DateTime<Utc>>,
    window_end: Option<DateTime<Utc>>,
}

/// GET /api/v2/resources/utilization/summary
async fn summary(
    svc: web::Data<Arc<ResourceUtilizationService>>,
    query: web::Query<WindowQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    Ok(HttpResponse::Ok().json(svc.get_summary(query.window_start, query.window_end).await?))
}

/// GET /api/v2/resources/utilization/stands
async fn stands(
    svc: web::Data<Arc<ResourceUtilizationService>>,
    query: web::Query<WindowQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    Ok(HttpResponse::Ok().json(svc.get_stand_utilization(query.window_start, query.window_end).await?))
}

/// GET /api/v2/resources/utilization/teams
async fn teams(
    svc: web::Data<Arc<ResourceUtilizationService>>,
    query: web::Query<WindowQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    Ok(HttpResponse::Ok().json(svc.get_team_workload(query.window_start, query.window_end).await?))
}

/// GET /api/v2/resources/utilization/equipment
async fn equipment(
    svc: web::Data<Arc<ResourceUtilizationService>>,
    query: web::Query<WindowQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    Ok(HttpResponse::Ok().json(
        svc.get_equipment_utilization(query.window_start, query.window_end)
            .await?,
    ))
}

fn configure_utilization_routes(scope: actix_web::Scope) -> actix_web::Scope {
    scope
        .route("/summary", web::get().to(summary))
        .route("/stands", web::get().to(stands))
        .route("/teams", web::get().to(teams))
        .route("/equipment", web::get().to(equipment))
}

/// 注册资源利用率路由
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(configure_utilization_routes(web::scope(
        "/api/v2/dispatch/analytics/resource-utilization",
    )));
}
