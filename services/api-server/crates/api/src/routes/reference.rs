use actix_web::{web, HttpRequest, HttpResponse};
use serde_json::json;
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use fms_application::schemas::dispatch_schemas::DepartmentResponse;
use fms_application::services::business_case_service::BUSINESS_CASE_STATUS_METADATA;
use fms_application::services::business_case_type_service::BusinessCaseTypeService;
use fms_application::types::ConcreteDispatchResourceService;
use fms_domain::models::dispatch::Department;

#[derive(Debug, serde::Deserialize)]
struct DepartmentListQuery {
    include_inactive: Option<bool>,
    page: Option<i64>,
    page_size: Option<i64>,
}

#[derive(Debug, serde::Deserialize)]
struct BusinessCaseTypeListQuery {
    active_only: Option<bool>,
}

fn ensure_dispatch_view(claims: &JwtAuth) -> Result<(), ApiError> {
    if claims.0.is_admin.unwrap_or(false)
        || claims
            .0
            .permissions
            .iter()
            .any(|item| item == "dispatch:view" || item == "*")
    {
        return Ok(());
    }

    Err(ApiError::Forbidden("缺少权限: dispatch:view".into()))
}

fn request_id(req: &HttpRequest) -> Option<String> {
    req.headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn ok_resp(req: &HttpRequest, data: impl serde::Serialize) -> HttpResponse {
    HttpResponse::Ok().json(json!({
        "success": true,
        "data": data,
        "error": null,
        "request_id": request_id(req),
    }))
}

fn to_department_response(department: Department) -> DepartmentResponse {
    DepartmentResponse {
        id: department.id,
        name: department.name,
        code: department.code,
        description: department.description,
        manager_id: department.manager_id,
        terminal: department.terminal,
        created_at: department.created_at,
        updated_at: department.updated_at,
        is_active: department.is_active,
        attributes: department.attributes,
    }
}

async fn list_departments(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    query: web::Query<DepartmentListQuery>,
) -> Result<HttpResponse, ApiError> {
    ensure_dispatch_view(&claims)?;
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

async fn get_department(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    ensure_dispatch_view(&claims)?;
    match svc.get_department(&path.into_inner()).await? {
        Some(item) => Ok(ok_resp(&req, to_department_response(item))),
        None => Err(ApiError::NotFound("科室不存在".into())),
    }
}

async fn list_business_case_types(
    req: HttpRequest,
    svc: web::Data<Arc<BusinessCaseTypeService>>,
    claims: JwtAuth,
    query: web::Query<BusinessCaseTypeListQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_authenticated()?;
    let items = svc
        .list_case_types(query.active_only.unwrap_or(true))
        .await
        .map_err(ApiError::from)?;
    Ok(ok_resp(&req, items))
}

async fn list_business_case_statuses(req: HttpRequest, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    claims.ensure_authenticated()?;
    Ok(ok_resp(&req, BUSINESS_CASE_STATUS_METADATA))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/reference")
            .route("/departments", web::get().to(list_departments))
            .route("/departments/{department_id}", web::get().to(get_department))
            .route("/business-case-types", web::get().to(list_business_case_types))
            .route("/business-case-statuses", web::get().to(list_business_case_statuses)),
    );
}
