use actix_web::{web, HttpRequest, HttpResponse};
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::routes::dispatch_resources::{created_resp, ok_resp, MessageResponse};
use fms_application::schemas::dispatch_schemas::{
    EquipmentCreate, EquipmentResponse, EquipmentStatusUpdate, EquipmentTypeCreate, EquipmentTypeResponse,
    EquipmentTypeUpdate, EquipmentUpdate, PositionUpdate,
};
use fms_application::services::dispatch_resource_service::{
    to_equipment_response, to_equipment_type_response, EquipmentListQuery, EquipmentStatusQuery, PageQuery,
};
use fms_application::types::ConcreteDispatchResourceService;

pub async fn list_equipment_types(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    query: web::Query<PageQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("equipment:view")?;
    let items = svc
        .list_equipment_types(
            query.include_inactive.unwrap_or(false),
            query.page.unwrap_or(1),
            query.page_size.unwrap_or(50),
        )
        .await?;
    let payload: Vec<EquipmentTypeResponse> = items.into_iter().map(to_equipment_type_response).collect();
    Ok(ok_resp(&req, payload))
}

pub async fn create_equipment_type(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    body: web::Json<EquipmentTypeCreate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("equipment:manage")?;
    let saved = svc.create_equipment_type(body.into_inner()).await?;
    Ok(created_resp(&req, to_equipment_type_response(saved)))
}

pub async fn update_equipment_type(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<EquipmentTypeUpdate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("equipment:manage")?;
    let saved = svc.update_equipment_type(&path.into_inner(), body.into_inner()).await?;
    Ok(ok_resp(&req, to_equipment_type_response(saved)))
}

pub async fn delete_equipment_type(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("equipment:manage")?;
    svc.delete_equipment_type(&path.into_inner()).await?;
    Ok(ok_resp(
        &req,
        MessageResponse {
            message: "设备类型已删除".into(),
        },
    ))
}

pub async fn list_equipment(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    query: web::Query<EquipmentListQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("equipment:view")?;
    let items = svc
        .list_equipment(
            query.include_inactive.unwrap_or(false),
            query.equipment_type_id.as_deref(),
            query.terminal.as_deref(),
            query.status.as_deref(),
            query.page.unwrap_or(1),
            query.page_size.unwrap_or(50),
        )
        .await?;
    let payload: Vec<EquipmentResponse> = items.into_iter().map(to_equipment_response).collect();
    Ok(ok_resp(&req, payload))
}

pub async fn get_equipment(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("equipment:view")?;
    let item = svc.get_equipment(&path.into_inner()).await?;
    match item {
        Some(equipment) => Ok(ok_resp(&req, to_equipment_response(equipment))),
        None => Err(ApiError::NotFound("设备不存在".into())),
    }
}

pub async fn create_equipment(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    body: web::Json<EquipmentCreate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("equipment:manage")?;
    let saved = svc.create_equipment(body.into_inner()).await?;
    Ok(created_resp(&req, to_equipment_response(saved)))
}

pub async fn update_equipment(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<EquipmentUpdate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("equipment:manage")?;
    let saved = svc.update_equipment(&path.into_inner(), body.into_inner()).await?;
    Ok(ok_resp(&req, to_equipment_response(saved)))
}

pub async fn update_equipment_position(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<PositionUpdate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("equipment:manage")?;
    svc.update_equipment_position(&path.into_inner(), body.into_inner())
        .await?;
    Ok(ok_resp(
        &req,
        MessageResponse {
            message: "位置已更新".into(),
        },
    ))
}

pub async fn update_equipment_status(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    query: web::Query<EquipmentStatusQuery>,
    body: Option<web::Json<EquipmentStatusUpdate>>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("equipment:manage")?;
    let resolved_status = query
        .status
        .clone()
        .or_else(|| body.as_ref().map(|item| item.status.clone()))
        .ok_or_else(|| ApiError::ValidationError("缺少状态参数 status".into()))?;

    svc.update_equipment_status(&path.into_inner(), &resolved_status)
        .await?;
    Ok(ok_resp(
        &req,
        MessageResponse {
            message: "状态已更新".into(),
        },
    ))
}
