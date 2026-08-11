//! 航班归档路由。
//!
//! 对齐 Python `archive_routes.py`。

use actix_web::{web, HttpResponse};
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use fms_application::services::flight_archive_service::FlightArchiveService;

fn ok_resp(data: impl serde::Serialize, message: &str) -> HttpResponse {
    HttpResponse::Ok().json(json!({
        "success": true,
        "data": data,
        "message": message,
    }))
}

/// GET /api/v2/archived/flights
async fn list_archived_flights(
    svc: web::Data<Arc<FlightArchiveService>>,
    query: web::Query<ArchiveListQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_authenticated()?;
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let offset = query.offset.unwrap_or(0).max(0);
    let flights = svc.find_archived_flights(limit, offset).await?;
    Ok(ok_resp(
        &flights,
        &format!("Retrieved {} archived flights", flights.len()),
    ))
}

/// GET /api/v2/archived/flights/{flight_id}
async fn get_archived_flight(
    svc: web::Data<Arc<FlightArchiveService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_authenticated()?;
    let flight_id = path.into_inner();
    let Some(flight) = svc.find_archived_flight_by_id(&flight_id).await? else {
        return Err(ApiError::NotFound("Archived flight not found".into()));
    };
    Ok(ok_resp(flight, "Retrieved archived flight detail"))
}

/// GET /api/v2/archived/stats
async fn get_archive_stats(
    svc: web::Data<Arc<FlightArchiveService>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_authenticated()?;
    Ok(ok_resp(svc.get_archive_stats().await?, "归档统计获取成功"))
}

/// POST /api/v2/archived/trigger
async fn trigger_archive(
    svc: web::Data<Arc<FlightArchiveService>>,
    query: web::Query<ArchiveTriggerQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("system:admin")?;
    let cutoff_date = normalize_iso_date(query.cutoff_date.as_deref(), "cutoff_date")?;
    let target_date = normalize_iso_date(query.target_date.as_deref(), "target_date")?;
    if cutoff_date.is_none() && target_date.is_none() {
        return Err(ApiError::ValidationError(
            "Must provide cutoff_date or target_date".into(),
        ));
    }
    let result = svc
        .trigger_archive(cutoff_date.as_deref(), target_date.as_deref())
        .await?;
    Ok(ok_resp(result, "Archive process executed"))
}

#[derive(Debug, Deserialize)]
struct ArchiveListQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ArchiveTriggerQuery {
    cutoff_date: Option<String>,
    target_date: Option<String>,
}

fn normalize_iso_date(value: Option<&str>, field_name: &str) -> Result<Option<String>, ApiError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let normalized = value.trim();
    if normalized.is_empty() {
        return Ok(None);
    }
    NaiveDate::parse_from_str(normalized, "%Y-%m-%d")
        .map(|parsed| Some(parsed.format("%Y-%m-%d").to_string()))
        .map_err(|_| ApiError::ValidationError(format!("{field_name} must be YYYY-MM-DD")))
}

fn configure_archive_routes(scope: actix_web::Scope) -> actix_web::Scope {
    scope
        .route("/flights", web::get().to(list_archived_flights))
        .route("/flights/{flight_id}", web::get().to(get_archived_flight))
        .route("/stats", web::get().to(get_archive_stats))
        .route("/trigger", web::post().to(trigger_archive))
}

/// 注册归档路由
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(configure_archive_routes(web::scope("/api/v2/archive")));
}
