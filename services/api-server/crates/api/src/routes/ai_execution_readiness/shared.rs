pub(crate) use crate::error::ApiError;
pub(crate) use crate::middleware::jwt::JwtAuth;
pub(crate) use crate::middleware::permissions::PermissionCheck;
pub(crate) use actix_web::{web, HttpResponse};
pub(crate) use fms_application::services::ai_execution_metrics_service::AiExecutionMetricsService;
pub(crate) use fms_application::services::ai_execution_readiness_service::AiExecutionReadinessService;
pub(crate) use fms_application::services::ai_rollout_status_service::AiRolloutStatusService;
pub(crate) use serde::Deserialize;
pub(crate) use std::sync::Arc;
#[derive(Debug, Deserialize)]
pub(crate) struct CleanupQuery {
    pub(crate) older_than_hours: Option<i64>,
    pub(crate) dry_run: Option<bool>,
    pub(crate) confirm: Option<bool>,
}

pub(crate) async fn get_execution_readiness(
    service: web::Data<Arc<AiExecutionReadinessService>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims
        .ensure_permission("system.config_read")
        .or_else(|_| claims.ensure_permission("ai.execution.readiness"))?;

    let report = service.evaluate().await;

    Ok(HttpResponse::Ok().json(report))
}

pub(crate) async fn get_execution_readiness_metrics(
    service: web::Data<Arc<AiExecutionMetricsService>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims
        .ensure_permission("system.config_read")
        .or_else(|_| claims.ensure_permission("ai.execution.readiness"))?;

    let snapshot = service.snapshot().await.map_err(ApiError::Internal)?;

    Ok(HttpResponse::Ok().json(snapshot))
}

pub(crate) async fn get_rollout_status(
    service: web::Data<Arc<AiRolloutStatusService>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims
        .ensure_permission("system.config_read")
        .or_else(|_| claims.ensure_permission("ai.execution.readiness"))?;

    let status = service.evaluate().await.map_err(ApiError::Internal)?;
    Ok(HttpResponse::Ok().json(status))
}

pub(crate) async fn post_cleanup_smoke(
    service: web::Data<Arc<AiRolloutStatusService>>,
    claims: JwtAuth,
    query: web::Query<CleanupQuery>,
) -> Result<HttpResponse, ApiError> {
    claims
        .ensure_permission("system.ops_admin")
        .or_else(|_| claims.ensure_permission("system.config_write"))?;

    let older_than_hours = query.older_than_hours.unwrap_or(24);
    let dry_run = query.dry_run.unwrap_or(true);
    let confirm = query.confirm.unwrap_or(false);

    let result = service
        .cleanup_smoke_data(older_than_hours, dry_run, confirm)
        .await
        .map_err(ApiError::BadRequest)?;

    Ok(HttpResponse::Ok().json(result))
}

/// Register execution-readiness routes RELATIVE to a parent `/api/v2/ai` scope.
pub fn register_scoped_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/execution-readiness", web::get().to(get_execution_readiness))
        .route(
            "/execution-readiness/metrics",
            web::get().to(get_execution_readiness_metrics),
        )
        .route("/execution-readiness/rollout-status", web::get().to(get_rollout_status))
        .route("/execution-readiness/cleanup-smoke", web::post().to(post_cleanup_smoke));
}
