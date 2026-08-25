use actix_web::{web, HttpRequest, HttpResponse};
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::routes::dispatch_resources::{created_resp, ok_resp};
use fms_application::schemas::dispatch_schemas::{
    ScheduleAvailabilityResponse, ScheduleExceptionCreate, ShiftInstanceCreate, ShiftInstanceResponse,
    ShiftTemplateCreate, ShiftTemplateResponse,
};
use fms_application::services::dispatch_resource_service::{
    to_shift_instance_response, to_shift_template_response, ScheduleAvailabilityQuery, ScheduleExceptionsQuery,
    ScheduleInstancesQuery, ScheduleTemplatesQuery,
};
use fms_application::types::ConcreteDispatchScheduleService;

pub async fn list_schedule_templates(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchScheduleService>>,
    claims: JwtAuth,
    query: web::Query<ScheduleTemplatesQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("schedule:view")?;
    let items = svc
        .list_templates(
            query.resource_type.as_deref(),
            query.resource_id.as_deref(),
            query.enabled,
            query.limit.unwrap_or(100),
        )
        .await?;
    let payload: Vec<ShiftTemplateResponse> = items.into_iter().map(to_shift_template_response).collect();
    Ok(ok_resp(&req, payload))
}

pub async fn create_schedule_template(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchScheduleService>>,
    claims: JwtAuth,
    body: web::Json<ShiftTemplateCreate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("schedule:manage")?;
    let saved = svc.create_template(body.into_inner()).await?;
    Ok(created_resp(&req, to_shift_template_response(saved)))
}

pub async fn list_schedule_instances(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchScheduleService>>,
    claims: JwtAuth,
    query: web::Query<ScheduleInstancesQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("schedule:view")?;
    let items = svc
        .list_instances(
            query.resource_type.as_deref(),
            query.resource_id.as_deref(),
            query.window_start,
            query.window_end,
            query.limit.unwrap_or(200),
        )
        .await?;
    let payload: Vec<ShiftInstanceResponse> = items.into_iter().map(to_shift_instance_response).collect();
    Ok(ok_resp(&req, payload))
}

pub async fn create_schedule_instance(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchScheduleService>>,
    claims: JwtAuth,
    body: web::Json<ShiftInstanceCreate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("schedule:manage")?;
    if body.end_time <= body.start_time {
        return Err(ApiError::BadRequest("end_time 必须晚于 start_time".into()));
    }
    let saved = svc.create_instance(body.into_inner()).await?;
    Ok(created_resp(&req, to_shift_instance_response(saved)))
}

pub async fn list_schedule_exceptions(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchScheduleService>>,
    claims: JwtAuth,
    query: web::Query<ScheduleExceptionsQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("schedule:view")?;
    let items = svc
        .list_exceptions(query.window_start, query.window_end, query.limit.unwrap_or(200))
        .await?;
    Ok(ok_resp(&req, items))
}

pub async fn create_schedule_exception(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchScheduleService>>,
    claims: JwtAuth,
    body: web::Json<ScheduleExceptionCreate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("schedule:manage")?;
    if body.end_time <= body.start_time {
        return Err(ApiError::BadRequest("end_time 必须晚于 start_time".into()));
    }
    let saved = svc.create_exception(body.into_inner()).await?;
    Ok(created_resp(&req, saved))
}

pub async fn get_schedule_availability(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchScheduleService>>,
    claims: JwtAuth,
    query: web::Query<ScheduleAvailabilityQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("schedule:view")?;
    if query.planned_end_time <= query.planned_start_time {
        return Err(ApiError::BadRequest(
            "planned_end_time 必须晚于 planned_start_time".into(),
        ));
    }
    let resource_ids = query
        .resource_ids
        .as_deref()
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let items = svc
        .get_availability(
            &query.resource_type,
            query.planned_start_time,
            query.planned_end_time,
            query.terminal.as_deref(),
            &resource_ids,
        )
        .await?;
    let payload: Vec<ScheduleAvailabilityResponse> = items;
    Ok(ok_resp(&req, payload))
}
