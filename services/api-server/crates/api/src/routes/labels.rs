//! 标签管理路由。

use std::sync::Arc;

use actix_web::{web, HttpResponse};
use serde::Deserialize;
use serde_json::json;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use fms_application::schemas::label_schemas::{AttachLabelRequest, CreateLabelRequest, UpdateLabelRequest};
use fms_application::services::label_service::LabelService;
use fms_domain::error::DomainError;

#[derive(Debug, Deserialize)]
struct ListLabelsQuery {
    active_only: Option<bool>,
}

fn actor_id(claims: &JwtAuth) -> Option<String> {
    claims
        .0
        .username
        .as_deref()
        .or(claims.0.sub.as_deref())
        .map(str::to_string)
}

fn map_label_error(error: DomainError) -> ApiError {
    match error {
        DomainError::NotFound { entity_type, id } => match entity_type {
            "label" => ApiError::NotFound(format!("标签 '{id}' 不存在")),
            _ => ApiError::NotFound(format!("{entity_type} (id={id}) 未找到")),
        },
        DomainError::ValidationError(message) | DomainError::BusinessRuleViolation(message) => {
            ApiError::BadRequest(message)
        }
        DomainError::BusinessRuleViolationWithDetails { message, details } => {
            ApiError::BadRequestWithDetails { message, details }
        }
        DomainError::Conflict(message) | DomainError::ConcurrencyConflict(message) => ApiError::Conflict(message),
        DomainError::PermissionDenied(message) => ApiError::Forbidden(message),
        DomainError::Unauthorized(message) => ApiError::Unauthorized(message),
        DomainError::InvalidStateTransition { from, to } => {
            ApiError::BadRequest(format!("非法状态转换: {from} → {to}"))
        }
        DomainError::Internal(message) => ApiError::Internal(message),
    }
}

async fn list_labels(
    svc: web::Data<Arc<LabelService>>,
    query: web::Query<ListLabelsQuery>,
    _claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let items = svc
        .list_labels(query.active_only.unwrap_or(true))
        .await
        .map_err(map_label_error)?;
    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "data": items,
    })))
}

async fn create_label(
    svc: web::Data<Arc<LabelService>>,
    body: web::Json<CreateLabelRequest>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let label = svc
        .create_label(body.into_inner(), actor_id(&claims))
        .await
        .map_err(map_label_error)?;
    Ok(HttpResponse::Created().json(json!({
        "success": true,
        "data": label,
    })))
}

async fn update_label(
    svc: web::Data<Arc<LabelService>>,
    path: web::Path<String>,
    body: web::Json<UpdateLabelRequest>,
    _claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let updated = svc
        .update_label(&path.into_inner(), body.into_inner())
        .await
        .map_err(map_label_error)?;
    if !updated {
        return Err(ApiError::NotFound("标签不存在或无变更".into()));
    }

    Ok(HttpResponse::Ok().json(json!({
        "success": true,
    })))
}

async fn delete_label(
    svc: web::Data<Arc<LabelService>>,
    path: web::Path<String>,
    _claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let deleted = svc.delete_label(&path.into_inner()).await.map_err(map_label_error)?;
    if !deleted {
        return Err(ApiError::Forbidden("系统标签不可删除，或标签不存在".into()));
    }

    Ok(HttpResponse::Ok().json(json!({
        "success": true,
    })))
}

async fn attach_flight_label(
    svc: web::Data<Arc<LabelService>>,
    path: web::Path<String>,
    body: web::Json<AttachLabelRequest>,
    _claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    svc.attach_flight_label(&path.into_inner(), body.into_inner())
        .await
        .map_err(map_label_error)?;
    Ok(HttpResponse::Ok().json(json!({
        "success": true,
    })))
}

async fn detach_flight_label(
    svc: web::Data<Arc<LabelService>>,
    path: web::Path<(String, String)>,
    _claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let (flight_id, code) = path.into_inner();
    svc.detach_flight_label(&flight_id, &code)
        .await
        .map_err(map_label_error)?;
    Ok(HttpResponse::Ok().json(json!({
        "success": true,
    })))
}

async fn attach_leg_label(
    svc: web::Data<Arc<LabelService>>,
    path: web::Path<(String, String)>,
    body: web::Json<AttachLabelRequest>,
    _claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let (flight_id, leg_type) = path.into_inner();
    svc.attach_leg_label(&flight_id, &leg_type, body.into_inner())
        .await
        .map_err(map_label_error)?;
    Ok(HttpResponse::Ok().json(json!({
        "success": true,
    })))
}

async fn detach_leg_label(
    svc: web::Data<Arc<LabelService>>,
    path: web::Path<(String, String, String)>,
    _claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let (flight_id, leg_type, code) = path.into_inner();
    svc.detach_leg_label(&flight_id, &leg_type, &code)
        .await
        .map_err(map_label_error)?;
    Ok(HttpResponse::Ok().json(json!({
        "success": true,
    })))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/labels")
            .route("", web::get().to(list_labels))
            .route("", web::post().to(create_label))
            .route("/{label_id}", web::put().to(update_label))
            .route("/{label_id}", web::delete().to(delete_label))
            .route("/flights/{flight_id}", web::post().to(attach_flight_label))
            .route("/flights/{flight_id}/{code}", web::delete().to(detach_flight_label))
            .route("/flights/{flight_id}/legs/{leg_type}", web::post().to(attach_leg_label))
            .route(
                "/flights/{flight_id}/legs/{leg_type}/{code}",
                web::delete().to(detach_leg_label),
            ),
    );
}
