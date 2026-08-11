//! 航班调度时间线路由。

use std::sync::Arc;

use actix_web::web;
use serde_json::json;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;

use super::shared::{actor_id, ok_resp};
use fms_application::schemas::flight_schemas::{DispatchTimelineEventCreate, DispatchTimelineListResponse};
use fms_application::services::flight_runtime_service::FlightRuntimeService;

/// GET /api/v2/flights/{flight_id}/dispatch-timeline
pub async fn get_dispatch_timeline(
    runtime: web::Data<Arc<FlightRuntimeService>>,
    path: web::Path<String>,
    _claims: JwtAuth,
) -> Result<actix_web::HttpResponse, ApiError> {
    let flight_id = path.into_inner();
    let items = runtime.list_dispatch_timeline(&flight_id).await?;
    Ok(ok_resp(
        format!("获取到 {} 条时间线事件", items.len()),
        DispatchTimelineListResponse { items },
    ))
}

/// POST /api/v2/flights/{flight_id}/dispatch-timeline/events
///
/// 写后 SSE / 缓存由 domain_event outbox → subscriber 消费（ADR-0002），路由不广播。
pub async fn create_dispatch_timeline_event(
    runtime: web::Data<Arc<FlightRuntimeService>>,
    path: web::Path<String>,
    body: web::Json<DispatchTimelineEventCreate>,
    claims: JwtAuth,
) -> Result<actix_web::HttpResponse, ApiError> {
    claims.ensure_permission("flight:manage")?;
    let flight_id = path.into_inner();
    let mut payload = body.into_inner();
    if payload.recorded_by.is_none() {
        payload.recorded_by = Some(actor_id(&claims).to_string());
    }
    let write_result = runtime.create_dispatch_timeline_event(&flight_id, payload).await?;
    Ok(ok_resp("时间线事件写入成功", write_result.event))
}

/// DELETE /api/v2/flights/{flight_id}/dispatch-timeline/events/{timeline_id}
///
/// 写后 SSE / 缓存由 domain_event outbox → subscriber 消费（ADR-0002），路由不广播。
pub async fn delete_dispatch_timeline_event(
    runtime: web::Data<Arc<FlightRuntimeService>>,
    path: web::Path<(String, String)>,
    claims: JwtAuth,
) -> Result<actix_web::HttpResponse, ApiError> {
    claims.ensure_permission("flight:manage")?;
    let (flight_id, timeline_id) = path.into_inner();
    if !runtime.delete_dispatch_timeline_event(&flight_id, &timeline_id).await? {
        return Err(ApiError::NotFound("时间线事件未找到".into()));
    }
    Ok(ok_resp("时间线事件撤销成功", json!({ "timeline_id": timeline_id })))
}
