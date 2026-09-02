use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::routes::dispatch_resources::ok_resp;
use actix_web::{web, HttpRequest, HttpResponse};
use fms_application::services::field_overlay_service::FieldOverlayWrite;
use fms_application::types::ConcreteFieldOverlayService;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub object_name: Option<String>,
    pub include_inactive: Option<bool>,
}
#[derive(Debug, Deserialize)]
pub struct Path {
    pub object_name: String,
    pub field_name: String,
}

pub async fn list(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteFieldOverlayService>>,
    claims: JwtAuth,
    q: web::Query<ListQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    Ok(ok_resp(
        &req,
        svc.list(q.object_name.as_deref(), q.include_inactive.unwrap_or(false))
            .await?,
    ))
}
pub async fn save(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteFieldOverlayService>>,
    claims: JwtAuth,
    body: web::Json<FieldOverlayWrite>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    Ok(ok_resp(&req, svc.save(body.into_inner()).await?))
}
pub async fn deactivate(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteFieldOverlayService>>,
    claims: JwtAuth,
    path: web::Path<Path>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    Ok(ok_resp(
        &req,
        svc.set_active(&path.object_name, &path.field_name, false).await?,
    ))
}
pub async fn activate(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteFieldOverlayService>>,
    claims: JwtAuth,
    path: web::Path<Path>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    Ok(ok_resp(
        &req,
        svc.set_active(&path.object_name, &path.field_name, true).await?,
    ))
}
