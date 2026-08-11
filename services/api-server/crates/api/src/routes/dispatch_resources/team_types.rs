use actix_web::{web, HttpRequest, HttpResponse};
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::routes::dispatch_resources::{created_resp, ok_resp, MessageResponse};
use fms_application::schemas::dispatch_schemas::{TeamTypeCreate, TeamTypeResponse, TeamTypeUpdate};
use fms_application::services::dispatch_resource_service::{to_team_type_response, PageQuery};
use fms_application::types::ConcreteDispatchResourceService;

pub async fn list_team_types(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    query: web::Query<PageQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let items = svc
        .list_team_types(
            query.include_inactive.unwrap_or(false),
            query.page.unwrap_or(1),
            query.page_size.unwrap_or(50),
        )
        .await?;
    let payload: Vec<TeamTypeResponse> = items.into_iter().map(to_team_type_response).collect();
    Ok(ok_resp(&req, payload))
}

pub async fn get_team_type(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let item = svc.get_team_type(&path.into_inner()).await?;
    match item {
        Some(team_type) => Ok(ok_resp(&req, to_team_type_response(team_type))),
        None => Err(ApiError::NotFound("班组类型不存在".into())),
    }
}

pub async fn create_team_type(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    body: web::Json<TeamTypeCreate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("team:manage")?;
    let saved = svc.create_team_type(body.into_inner()).await?;
    Ok(created_resp(&req, to_team_type_response(saved)))
}

pub async fn update_team_type(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<TeamTypeUpdate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("team:manage")?;
    let saved = svc.update_team_type(&path.into_inner(), body.into_inner()).await?;
    Ok(ok_resp(&req, to_team_type_response(saved)))
}

pub async fn delete_team_type(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("team:manage")?;
    svc.delete_team_type(&path.into_inner()).await?;
    Ok(ok_resp(
        &req,
        MessageResponse {
            message: "班组类型已删除".into(),
        },
    ))
}
