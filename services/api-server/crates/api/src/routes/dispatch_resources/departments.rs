use actix_web::{web, HttpRequest, HttpResponse};
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::routes::dispatch_resources::{created_resp, ok_resp};
use fms_application::schemas::dispatch_schemas::{DepartmentCreate, DepartmentResponse, DepartmentUpdate};
use fms_application::types::ConcreteDispatchResourceService;
use fms_application::services::dispatch_resource_service::{
    to_department_response, PageQuery,
};

pub async fn list_departments(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    query: web::Query<PageQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let items = svc
        .list_departments(
            query.include_inactive.unwrap_or(false),
            query.page.unwrap_or(1),
            query.page_size.unwrap_or(50),
        )
        .await?;

    let payload: Vec<DepartmentResponse> = items.into_iter().map(to_department_response).collect();
    Ok(ok_resp(&req, payload))
}

pub async fn get_department(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let department = svc.get_department(&path.into_inner()).await?;
    match department {
        Some(item) => Ok(ok_resp(&req, to_department_response(item))),
        None => Err(ApiError::NotFound("科室不存在".into())),
    }
}

pub async fn create_department(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    body: web::Json<DepartmentCreate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("team:manage")?;
    let saved = svc.create_department(body.into_inner()).await?;
    Ok(created_resp(&req, to_department_response(saved)))
}

pub async fn update_department(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<DepartmentUpdate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("team:manage")?;
    let saved = svc.update_department(&path.into_inner(), body.into_inner()).await?;
    Ok(ok_resp(&req, to_department_response(saved)))
}
