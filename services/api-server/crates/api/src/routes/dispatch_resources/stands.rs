use actix_web::{web, HttpRequest, HttpResponse};
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::routes::dispatch_resources::{created_resp, ok_resp};
use fms_application::schemas::dispatch_schemas::{StandCreate, StandResponse};
use fms_application::types::ConcreteDispatchResourceService;
use fms_application::services::dispatch_resource_service::{
    to_stand_response, StandListQuery,
};

pub async fn list_stands(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    query: web::Query<StandListQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let items = svc
        .list_stands(
            query.terminal.as_deref(),
            query.include_inactive.unwrap_or(false),
            query.page.unwrap_or(1),
            query.page_size.unwrap_or(50),
        )
        .await?;
    let payload: Vec<StandResponse> = items.into_iter().map(to_stand_response).collect();
    Ok(ok_resp(&req, payload))
}

pub async fn get_stand(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let item = svc.get_stand(&path.into_inner()).await?;
    match item {
        Some(stand) => Ok(ok_resp(&req, to_stand_response(stand))),
        None => Err(ApiError::NotFound("机位不存在".into())),
    }
}

pub async fn create_stand(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    body: web::Json<StandCreate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc.create_stand(body.into_inner()).await?;
    Ok(created_resp(&req, to_stand_response(saved)))
}
