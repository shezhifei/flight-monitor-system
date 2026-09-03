use actix_web::{web, HttpRequest, HttpResponse};
use serde::Deserialize;
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::routes::dispatch_resources::ok_resp;
use fms_application::services::metadata_catalog_service::{
    MetadataCatalogCreate, MetadataCatalogEntryCreate, MetadataCatalogEntryUpdate, MetadataCatalogUpdate,
};
use fms_application::types::ConcreteMetadataCatalogService;

#[derive(Debug, Deserialize)]
pub struct CatalogListQuery {
    pub include_inactive: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CatalogPath {
    pub catalog_code: String,
}

#[derive(Debug, Deserialize)]
pub struct CatalogEntryPath {
    pub catalog_code: String,
    pub entry_code: String,
}

pub async fn list_catalogs(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteMetadataCatalogService>>,
    claims: JwtAuth,
    query: web::Query<CatalogListQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let items = svc.list_catalogs(query.include_inactive.unwrap_or(false)).await?;
    Ok(ok_resp(&req, items))
}

pub async fn get_catalog(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteMetadataCatalogService>>,
    claims: JwtAuth,
    path: web::Path<CatalogPath>,
    query: web::Query<CatalogListQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let item = svc
        .get_catalog(&path.catalog_code, query.include_inactive.unwrap_or(true))
        .await?;
    Ok(ok_resp(&req, item))
}

pub async fn create_catalog(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteMetadataCatalogService>>,
    claims: JwtAuth,
    body: web::Json<MetadataCatalogCreate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc.create_catalog(body.into_inner()).await?;
    Ok(crate::routes::dispatch_resources::created_resp(&req, saved))
}

pub async fn update_catalog(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteMetadataCatalogService>>,
    claims: JwtAuth,
    path: web::Path<CatalogPath>,
    body: web::Json<MetadataCatalogUpdate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc.update_catalog(&path.catalog_code, body.into_inner()).await?;
    Ok(ok_resp(&req, saved))
}

pub async fn deactivate_catalog(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteMetadataCatalogService>>,
    claims: JwtAuth,
    path: web::Path<CatalogPath>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc.set_catalog_active(&path.catalog_code, false).await?;
    Ok(ok_resp(&req, saved))
}

pub async fn activate_catalog(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteMetadataCatalogService>>,
    claims: JwtAuth,
    path: web::Path<CatalogPath>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc.set_catalog_active(&path.catalog_code, true).await?;
    Ok(ok_resp(&req, saved))
}

pub async fn create_entry(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteMetadataCatalogService>>,
    claims: JwtAuth,
    path: web::Path<CatalogPath>,
    body: web::Json<MetadataCatalogEntryCreate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc.create_entry(&path.catalog_code, body.into_inner()).await?;
    Ok(crate::routes::dispatch_resources::created_resp(&req, saved))
}

pub async fn update_entry(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteMetadataCatalogService>>,
    claims: JwtAuth,
    path: web::Path<CatalogEntryPath>,
    body: web::Json<MetadataCatalogEntryUpdate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc
        .update_entry(&path.catalog_code, &path.entry_code, body.into_inner())
        .await?;
    Ok(ok_resp(&req, saved))
}

pub async fn deactivate_entry(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteMetadataCatalogService>>,
    claims: JwtAuth,
    path: web::Path<CatalogEntryPath>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc
        .set_entry_active(&path.catalog_code, &path.entry_code, false)
        .await?;
    Ok(ok_resp(&req, saved))
}

pub async fn activate_entry(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteMetadataCatalogService>>,
    claims: JwtAuth,
    path: web::Path<CatalogEntryPath>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc.set_entry_active(&path.catalog_code, &path.entry_code, true).await?;
    Ok(ok_resp(&req, saved))
}
