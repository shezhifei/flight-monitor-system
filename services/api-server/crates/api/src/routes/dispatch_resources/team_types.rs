use actix_web::{web, HttpRequest, HttpResponse};
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::routes::dispatch_resources::ok_resp;
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

/// PR2：班组类型写路径已下线（计划 :688「去掉班组类型段（或降为只读历史）」）。
/// 读路径（list/get）保留；POST/PUT/DELETE 一律 410，服务层 create/update/delete
/// 保留但不再经路由暴露。
const TEAM_TYPE_WRITE_GONE: &str = "班组类型已下线为只读历史目录，不再接受创建/修改/删除";

pub async fn create_team_type(
    _req: HttpRequest,
    _svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    _body: web::Json<TeamTypeCreate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("team:manage")?;
    Err(ApiError::Gone(TEAM_TYPE_WRITE_GONE.into()))
}

pub async fn update_team_type(
    _req: HttpRequest,
    _svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    _path: web::Path<String>,
    _body: web::Json<TeamTypeUpdate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("team:manage")?;
    Err(ApiError::Gone(TEAM_TYPE_WRITE_GONE.into()))
}

pub async fn delete_team_type(
    _req: HttpRequest,
    _svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    _path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("team:manage")?;
    Err(ApiError::Gone(TEAM_TYPE_WRITE_GONE.into()))
}
