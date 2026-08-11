use actix_web::{web, HttpRequest, HttpResponse};
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::routes::dispatch_resources::ok_resp;
use fms_application::schemas::dispatch_schemas::{
    DispatchAnalyticsBreakdownItem, DispatchAnalyticsSummaryResponse, DispatchAnalyticsTrendItem,
    DispatchScenarioPreviewRequest,
};
use fms_application::services::dispatch_analytics_service::DispatchAnalyticsService;
use fms_application::services::dispatch_resource_service::{
    AnalyticsBreakdownQuery, AnalyticsTrendQuery, AnalyticsWindowQuery,
};
use fms_application::services::dispatch_scenario_service::DispatchScenarioService;

pub async fn get_dispatch_analytics_summary(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchAnalyticsService>>,
    claims: JwtAuth,
    query: web::Query<AnalyticsWindowQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    if let (Some(start), Some(end)) = (query.window_start, query.window_end) {
        if end <= start {
            return Err(ApiError::BadRequest("window_end 必须晚于 window_start".into()));
        }
    }
    let payload: DispatchAnalyticsSummaryResponse =
        svc.get_operations_summary(query.window_start, query.window_end).await?;
    Ok(ok_resp(&req, payload))
}

pub async fn get_dispatch_analytics_breakdown(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchAnalyticsService>>,
    claims: JwtAuth,
    query: web::Query<AnalyticsBreakdownQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    if let (Some(start), Some(end)) = (query.window_start, query.window_end) {
        if end <= start {
            return Err(ApiError::BadRequest("window_end 必须晚于 window_start".into()));
        }
    }
    let group_by = query.group_by.as_deref().unwrap_or("team");
    if !matches!(group_by, "team" | "terminal" | "step") {
        return Err(ApiError::BadRequest("group_by 仅支持 team|terminal|step".into()));
    }
    let payload: Vec<DispatchAnalyticsBreakdownItem> = svc
        .get_breakdown(query.window_start, query.window_end, group_by)
        .await?;
    Ok(ok_resp(&req, payload))
}

pub async fn get_dispatch_analytics_trend(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchAnalyticsService>>,
    claims: JwtAuth,
    query: web::Query<AnalyticsTrendQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    if let (Some(start), Some(end)) = (query.window_start, query.window_end) {
        if end <= start {
            return Err(ApiError::BadRequest("window_end 必须晚于 window_start".into()));
        }
    }
    let bucket = query.bucket.as_deref().unwrap_or("hour");
    let payload: Vec<DispatchAnalyticsTrendItem> = svc
        .get_performance_trend(query.window_start, query.window_end, bucket)
        .await?;
    Ok(ok_resp(&req, payload))
}

pub async fn preview_dispatch_scenario(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchScenarioService>>,
    claims: JwtAuth,
    body: web::Json<DispatchScenarioPreviewRequest>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    if body.window_end <= body.window_start {
        return Err(ApiError::BadRequest("window_end 必须晚于 window_start".into()));
    }
    let payload = svc.preview(&body).await?;
    Ok(ok_resp(&req, payload))
}
