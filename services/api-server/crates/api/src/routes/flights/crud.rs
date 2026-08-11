//! 航班 CRUD 路由（获取、创建、更新、历史、报表、事件经过）。

use std::sync::Arc;

use actix_web::{web, HttpResponse};
use serde_json::json;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;

use super::shared::{
    actor_id, map_flight_write_error, ok_resp, update_changed_fields, viewer_department_id, viewer_department_name,
    FlightHistoryQuery, FlightInsightQuery,
};
use fms_application::schemas::flight_schemas::{FlightCreate, FlightUpdate};
use fms_application::services::flight_commands::{FlightCreateCommand, FlightUpdateCommand};
use fms_application::services::flight_runtime_service::FlightRuntimeService;
use fms_application::services::flight_service::FlightService;

/// GET /api/v2/flights/{flight_id}
pub async fn get_flight(
    svc: web::Data<Arc<FlightService>>,
    runtime: web::Data<Arc<FlightRuntimeService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let id = path.into_inner();
    match svc.get_flight(&id).await? {
        Some(f) => Ok(ok_resp(
            "成功获取航班信息",
            runtime
                .enrich_flight_for_viewer(f, viewer_department_id(&claims), viewer_department_name(&claims))
                .await?,
        )),
        None => Err(ApiError::NotFound(format!("航班 {id} 未找到"))),
    }
}

/// GET /api/v2/flights/{flight_id}/history
pub async fn get_flight_history(
    runtime: web::Data<Arc<FlightRuntimeService>>,
    path: web::Path<String>,
    query: web::Query<FlightHistoryQuery>,
    _claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let flight_id = path.into_inner();
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(50).clamp(1, 200);
    let history = runtime.get_flight_update_history(&flight_id, page, page_size).await?;
    Ok(ok_resp(format!("获取到 {} 条历史记录", history.len()), history))
}

/// GET /api/v2/flights/{flight_id}/history-report
pub async fn get_flight_history_report(
    runtime: web::Data<Arc<FlightRuntimeService>>,
    path: web::Path<String>,
    query: web::Query<FlightInsightQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("flight:read")?;
    claims.ensure_permission("ai:execute")?;
    let id = path.into_inner();
    let report = runtime
        .generate_history_report(&id, query.hours.unwrap_or(24), query.incident_type.as_deref())
        .await?;
    Ok(ok_resp("航班动态报表生成成功", report))
}

/// GET /api/v2/flights/{flight_id}/event-journey
pub async fn get_event_journey(
    runtime: web::Data<Arc<FlightRuntimeService>>,
    path: web::Path<String>,
    query: web::Query<FlightInsightQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("flight:read")?;
    claims.ensure_permission("ai:execute")?;
    let id = path.into_inner();
    let journey = runtime.generate_event_journey(&id, query.hours.unwrap_or(24)).await?;
    Ok(ok_resp("航班事件经过生成成功", journey))
}

/// POST /api/v2/flights
///
/// 写后 SSE / 列表缓存失效由 domain_event outbox → subscriber 消费（ADR-0002），路由不广播。
pub async fn create_flight(
    svc: web::Data<Arc<FlightService>>,
    runtime: web::Data<Arc<FlightRuntimeService>>,
    body: web::Json<FlightCreate>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("flight:manage")?;
    let actor = actor_id(&claims).to_string();
    // Build the create command explicitly from the request body + actor, then
    // enforce the command boundary's invariants before touching the service.
    let mut command = FlightCreateCommand::new(body.into_inner(), Some(actor.clone()));
    command
        .validate()
        .map_err(|e| ApiError::ValidationError(e.to_string()))?;
    let flight = runtime
        .enrich_flight(svc.execute_create(command).await.map_err(map_flight_write_error)?)
        .await?;
    let _audit = runtime.record_created(&actor, &flight).await;
    Ok(HttpResponse::Created().json(json!({
        "success": true,
        "data": flight,
        "message": "航班创建成功",
    })))
}

/// PUT /api/v2/flights/{flight_id}
///
/// 写后 SSE / 列表缓存失效由 domain_event outbox → subscriber 消费（ADR-0002），路由不广播。
pub async fn update_flight(
    svc: web::Data<Arc<FlightService>>,
    runtime: web::Data<Arc<FlightRuntimeService>>,
    path: web::Path<String>,
    body: web::Json<FlightUpdate>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("flight:manage")?;
    let id = path.into_inner();
    let dto = body.into_inner();
    // Build the update command explicitly from the path id + body + actor. This
    // enforces the command boundary (non-empty flight_id, at least one touched
    // field) before any service work.
    let command = FlightUpdateCommand::build(id.clone(), dto, Some(actor_id(&claims).to_string()))
        .map_err(|e| ApiError::ValidationError(e.to_string()))?;
    let denied_fields =
        svc.denied_update_fields(&command.dto, claims.0.is_admin.unwrap_or(false), &claims.0.permissions);
    if !denied_fields.is_empty() {
        return Err(ApiError::Forbidden(format!(
            "权限不足：非管理员禁止修改外部同步受控字段: [{}]",
            denied_fields.join(", ")
        )));
    }
    let before = svc.get_flight(&id).await?;
    let changed_fields = update_changed_fields(&command.dto);
    if changed_fields.is_empty() {
        return Err(ApiError::ValidationError("未提供任何更新字段".into()));
    }
    match svc.execute_update(command).await.map_err(map_flight_write_error)? {
        Some(flight) => {
            let flight = runtime.enrich_flight(flight).await?;
            let audit_changed_fields: Vec<String> = changed_fields.iter().map(|field| (*field).to_owned()).collect();
            let _audit = runtime
                .record_updated(actor_id(&claims), before.as_ref(), &flight, &audit_changed_fields)
                .await;
            Ok(ok_resp("航班更新成功", flight))
        }
        None => Err(ApiError::NotFound(format!("航班 {id} 未找到"))),
    }
}

/// PATCH /api/v2/flights/{flight_id}
pub async fn patch_flight(
    svc: web::Data<Arc<FlightService>>,
    runtime: web::Data<Arc<FlightRuntimeService>>,
    path: web::Path<String>,
    body: web::Json<FlightUpdate>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    update_flight(svc, runtime, path, body, claims).await
}

/// POST /api/v2/flights/{flight_id}/confirm-draft
///
/// 批确认 draft 航班（ONTOLOGY_V1.md §3.3，不变量 5）：
/// 仅 passenger 种类且 is_draft=true 的航班可确认；确认后 is_draft=false，
/// 方允许被正式 StandOccupation 引用。乐观锁由版本号保证。
pub async fn confirm_draft_flight(
    svc: web::Data<Arc<FlightService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("flight:manage")?;
    let id = path.into_inner();
    let flight = svc
        .confirm_draft_flight(&id, Some(actor_id(&claims).to_string()))
        .await
        .map_err(map_flight_write_error)?;
    match flight {
        Some(flight) => Ok(ok_resp("航班批确认成功（is_draft=false）", flight)),
        None => Err(ApiError::NotFound(format!("航班 {id} 未找到"))),
    }
}
