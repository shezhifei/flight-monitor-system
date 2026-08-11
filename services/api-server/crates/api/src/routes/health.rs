//! 健康检查路由。
//!
//! 尽量对齐 Python `health_routes.py` 的公开契约。

use actix_web::{web, HttpResponse};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::services::scheduler_runtime_service::SchedulerRuntimeService;
use fms_application::schemas::response::ApiResponse;

#[derive(Deserialize)]
struct ErrorQuery {
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct ErrorReportQuery {
    hours: Option<i64>,
}

fn response_message(count: usize) -> String {
    format!("获取到 {count} 个错误")
}

/// GET /api/v2/health — 基本健康
async fn healthz(
    scheduler_service: web::Data<Arc<SchedulerRuntimeService>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("system:config")?;
    Ok(HttpResponse::Ok().json(
        scheduler_service
            .build_health_payload(None, false)
            .await
            .map_err(ApiError::Internal)?,
    ))
}

/// GET /api/v2/health/ping — Ping
async fn ping() -> Result<HttpResponse, ApiError> {
    Ok(HttpResponse::Ok().json(json!({
        "status": "ok",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    })))
}

/// GET /api/v2/health/errors — 错误日志
async fn get_errors(
    scheduler_service: web::Data<Arc<SchedulerRuntimeService>>,
    query: web::Query<ErrorQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("system:config")?;

    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let errors = scheduler_service.get_recent_errors(limit).await;

    Ok(HttpResponse::Ok().json(ApiResponse::ok_with_message(
        errors.clone(),
        response_message(errors.len()),
    )))
}

/// POST /api/v2/health/errors/clear — 清除错误
async fn clear_errors(
    scheduler_service: web::Data<Arc<SchedulerRuntimeService>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("system:config")?;
    scheduler_service.clear_error_state().await;
    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "message": "错误列表已清空",
    })))
}

/// GET /api/v2/health/error_report — 错误报告
async fn error_report(
    scheduler_service: web::Data<Arc<SchedulerRuntimeService>>,
    query: web::Query<ErrorReportQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("system:config")?;

    let hours = query.hours.unwrap_or(24).max(1);
    let payload = scheduler_service.get_error_report(hours).await;

    Ok(HttpResponse::Ok().json(ApiResponse::ok_with_message(
        payload,
        format!("获取到最近 {hours} 小时的错误报告"),
    )))
}

/// GET /api/v2/health/performance — 性能指标
async fn performance(
    scheduler_service: web::Data<Arc<SchedulerRuntimeService>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("system:config")?;
    let metrics = scheduler_service.get_performance_metrics().await;

    Ok(HttpResponse::Ok().json(ApiResponse::ok_with_message(metrics, "性能指标获取成功")))
}

fn configure_health_routes(scope: actix_web::Scope) -> actix_web::Scope {
    scope
        .route("", web::get().to(healthz))
        .route("/ping", web::get().to(ping))
        .route("/errors", web::get().to(get_errors))
        .route("/errors/clear", web::post().to(clear_errors))
        .route("/error_report", web::get().to(error_report))
        .route("/performance", web::get().to(performance))
}

/// 注册健康路由 — Python 兼容 (/api/v2/system/runtime/health) + 原路径 (/api/v2/health)
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(configure_health_routes(web::scope("/api/v2/system/runtime/health")));
    cfg.service(configure_health_routes(web::scope("/api/v2/health")));
}
