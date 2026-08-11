#![allow(dead_code)]
//! 认证管理路由

use actix_web::{web, HttpMessage, HttpRequest, HttpResponse};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use fms_application::schemas::permission_template_schemas::{
    ApplyTemplateRequest, PermissionTemplateCreate, PermissionTemplateUpdate,
};
use fms_application::services::auth_admin_service::{AuthAdminCommandService, AuthAdminQueryService};

#[derive(Debug, Deserialize)]
pub struct ListPermissionTemplatesQuery {
    pub category: Option<String>,
    pub include_inactive: Option<bool>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

fn request_id(req: &HttpRequest) -> Option<String> {
    req.headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| req.extensions().get::<String>().cloned())
}

fn ok_resp(req: &HttpRequest, data: impl serde::Serialize) -> HttpResponse {
    HttpResponse::Ok().json(json!({
        "success": true,
        "data": data,
        "error": null,
        "request_id": request_id(req),
    }))
}

fn ensure_admin(claims: &JwtAuth) -> Result<(), ApiError> {
    if claims.0.is_admin.unwrap_or(false) {
        return Ok(());
    }
    Err(ApiError::Forbidden("需要管理员权限".into()))
}

async fn list_permission_templates(
    req: HttpRequest,
    svc: web::Data<Arc<AuthAdminQueryService>>,
    claims: JwtAuth,
    query: web::Query<ListPermissionTemplatesQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("auth:view")?;
    let page = query.page.unwrap_or(1);
    if page < 1 {
        return Err(ApiError::ValidationError(
            "page must be greater than or equal to 1".into(),
        ));
    }
    let page_size = query.page_size.unwrap_or(100);
    if !(1..=500).contains(&page_size) {
        return Err(ApiError::ValidationError("page_size must be between 1 and 500".into()));
    }
    let items = svc
        .list_permission_templates(
            query.category.as_deref(),
            query.include_inactive.unwrap_or(false),
            page,
            page_size,
        )
        .await?;
    Ok(ok_resp(&req, items))
}

pub async fn get_permission_template(
    req: HttpRequest,
    svc: web::Data<Arc<AuthAdminQueryService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("auth:view")?;
    let template_id = path.into_inner();
    let Some(item) = svc.get_permission_template(&template_id).await? else {
        return Err(ApiError::NotFound("模板不存在".into()));
    };
    Ok(ok_resp(&req, item))
}

pub async fn list_departments_in_use(
    req: HttpRequest,
    svc: web::Data<Arc<AuthAdminQueryService>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("auth:view")?;
    let items = svc.list_departments_in_use().await?;
    Ok(ok_resp(&req, items))
}

pub async fn create_permission_template(
    req: HttpRequest,
    svc: web::Data<Arc<AuthAdminCommandService>>,
    claims: JwtAuth,
    body: web::Json<PermissionTemplateCreate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("auth:manage")?;
    ensure_admin(&claims)?;
    let saved = svc.create_permission_template(body.into_inner()).await?;
    Ok(HttpResponse::Created().json(json!({
        "success": true,
        "data": saved,
        "error": null,
        "request_id": request_id(&req),
    })))
}

pub async fn update_permission_template(
    req: HttpRequest,
    svc: web::Data<Arc<AuthAdminCommandService>>,
    path: web::Path<String>,
    claims: JwtAuth,
    body: web::Json<PermissionTemplateUpdate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("auth:manage")?;
    ensure_admin(&claims)?;
    let saved = svc
        .update_permission_template(&path.into_inner(), body.into_inner())
        .await?;
    Ok(ok_resp(&req, saved))
}

pub async fn delete_permission_template(
    req: HttpRequest,
    svc: web::Data<Arc<AuthAdminCommandService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("auth:manage")?;
    ensure_admin(&claims)?;
    svc.delete_permission_template(&path.into_inner()).await?;
    Ok(ok_resp(&req, json!({ "success": true, "message": "模板已删除" })))
}

pub async fn apply_template(
    req: HttpRequest,
    svc: web::Data<Arc<AuthAdminCommandService>>,
    path: web::Path<String>,
    claims: JwtAuth,
    body: web::Json<ApplyTemplateRequest>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("auth:manage")?;
    ensure_admin(&claims)?;
    let role_id = path.into_inner();
    let payload = body.into_inner();
    let (template_name, role_name) = svc
        .apply_template_to_role(&role_id, &payload.template_id, &payload.mode)
        .await?;
    Ok(ok_resp(
        &req,
        json!({
            "success": true,
            "message": format!("已将模板 '{}' 应用到角色 '{}'", template_name, role_name)
        }),
    ))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/auth/admin")
            .route("/permission-templates", web::get().to(list_permission_templates))
            .route("/departments", web::get().to(list_departments_in_use))
            .route(
                "/permission-templates/{template_id}",
                web::get().to(get_permission_template),
            )
            .route("/permission-templates", web::post().to(create_permission_template))
            .route(
                "/permission-templates/{template_id}",
                web::put().to(update_permission_template),
            )
            .route(
                "/permission-templates/{template_id}",
                web::delete().to(delete_permission_template),
            )
            .route("/roles/{role_id}/apply-template", web::post().to(apply_template)),
    );
}
