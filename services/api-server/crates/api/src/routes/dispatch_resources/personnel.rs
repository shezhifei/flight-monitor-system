use actix_web::{web, HttpRequest, HttpResponse};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::routes::dispatch_resources::ok_resp;
use fms_application::types::ConcreteDispatchResourceService;

#[derive(Debug, Deserialize)]
pub struct PersonnelAttributesUpdate {
    #[serde(default)]
    pub attributes: Value,
}

pub async fn get_runtime(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let runtime = svc.get_personnel_runtime(&path.into_inner()).await?;
    Ok(ok_resp(&req, runtime))
}

pub async fn update_attributes(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<PersonnelAttributesUpdate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let actor_id = claims.0.sub.as_deref().unwrap_or("unknown");
    let runtime = svc
        .update_personnel_attributes(&path.into_inner(), body.into_inner().attributes, actor_id)
        .await?;
    Ok(ok_resp(&req, runtime))
}
