//! 调度器与流式运行时路由。
//!
//! 对齐 Python `scheduler_routes.py`。

use actix_web::{web, HttpResponse};
use chrono::Utc;
use serde_json::json;
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::services::scheduler_runtime_service::SchedulerRuntimeService;
use crate::services::task_status_types::TaskStatus;
use fms_application::services::authorization_service::PermissionCatalog;

fn ensure_scheduler_manual_trigger_permission(claims: &JwtAuth) -> Result<(), ApiError> {
    claims.ensure_permission(PermissionCatalog::SYSTEM_OPS_ADMIN)
}

async fn get_buffer_status_endpoint(
    scheduler_service: web::Data<Arc<SchedulerRuntimeService>>,
    query: web::Query<BufferStatusQuery>,
    _claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let status = scheduler_service.get_buffer_status(query.flight_no.clone(), true).await;
    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "data": status,
        "message": "成功获取缓冲区状态",
    })))
}

async fn get_sse_stats_endpoint(
    scheduler_service: web::Data<Arc<SchedulerRuntimeService>>,
    _claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "data": scheduler_service.get_sse_stats().await,
        "message": "成功获取SSE统计信息",
    })))
}

async fn get_scheduler_status(
    scheduler_service: web::Data<Arc<SchedulerRuntimeService>>,
    _claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let snapshot = scheduler_service.get_scheduler_status_snapshot().await;
    let task_status_map = snapshot
        .tasks
        .iter()
        .map(|task| (task.name.clone(), task.status))
        .collect::<std::collections::HashMap<_, _>>();

    let scheduler_running = snapshot.running;
    let mut overall_status = if scheduler_running { "running" } else { "stopped" }.to_string();
    if snapshot.tasks.iter().any(|task| task.status == TaskStatus::Error) {
        overall_status = "degraded".to_string();
    }

    let now_iso = Utc::now().to_rfc3339();
    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "data": {
            "active": scheduler_running,
            "status": overall_status,
            "tasks": {
                "boarding_deadline": task_status_map.get("boarding_deadline").cloned().unwrap_or(TaskStatus::Registered),
                "baggage_pull": task_status_map.get("baggage_pull").cloned().unwrap_or(TaskStatus::Registered),
                "base_scheduler": if scheduler_running { "active" } else { "stopped" },
            },
            "last_run": snapshot.last_run.clone().unwrap_or_else(|| snapshot.started_at.clone()),
            "next_run": snapshot.next_run.clone().unwrap_or(now_iso.clone()),
        },
        "message": format!("调度器状态获取成功（注册任务 {} 个）", snapshot.task_count),
    })))
}

async fn trigger_scheduler_check(
    scheduler_service: web::Data<Arc<SchedulerRuntimeService>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_scheduler_manual_trigger_permission(&claims)?;
    let trigger_result = scheduler_service.run_tasks_now().await;
    let result_map = trigger_result
        .results
        .iter()
        .map(|item| (item.name.clone(), item.status))
        .collect::<std::collections::HashMap<_, _>>();

    Ok(HttpResponse::Ok().json(json!({
        "success": trigger_result.triggered,
        "data": {
            "triggered": trigger_result.triggered,
            "timestamp": Utc::now().to_rfc3339(),
            "tasks_checked": trigger_result.task_names,
            "results": result_map,
            "details": trigger_result.results,
        },
        "message": if trigger_result.triggered { "调度器检查触发成功" } else { "当前无可执行任务" },
    })))
}

async fn bulk_update_flights(
    scheduler_service: web::Data<Arc<SchedulerRuntimeService>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("system:config")?;
    let result = scheduler_service.get_bulk_update_summary().await?;
    Ok(HttpResponse::Ok().json(json!({
        "success": result.success,
        "data": result.data,
        "message": result.message,
    })))
}

async fn get_flight_sync_status(
    scheduler_service: web::Data<Arc<SchedulerRuntimeService>>,
    _claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let status = scheduler_service.get_flight_sync_status().await?;
    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "data": status,
        "message": "成功获取本站航班同步状态",
    })))
}

async fn trigger_flight_sync(
    scheduler_service: web::Data<Arc<SchedulerRuntimeService>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("system:config")?;
    let result = scheduler_service.run_flight_sync_now().await?;
    Ok(HttpResponse::Ok().json(json!({
        "success": result.success,
        "data": result.data,
        "message": result.message,
    })))
}

#[derive(Debug, serde::Deserialize)]
struct BufferStatusQuery {
    flight_no: Option<String>,
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route(
        "/api/v2/system/runtime/streaming/buffer-status",
        web::get().to(get_buffer_status_endpoint),
    )
    .route(
        "/api/v2/system/runtime/streaming/sse-stats",
        web::get().to(get_sse_stats_endpoint),
    )
    .route("/api/v2/system/scheduler/status", web::get().to(get_scheduler_status))
    .route(
        "/api/v2/system/scheduler/trigger-check",
        web::post().to(trigger_scheduler_check),
    )
    .route(
        "/api/v2/system/scheduler/bulk-update",
        web::post().to(bulk_update_flights),
    )
    .route(
        "/api/v2/system/scheduler/flight-sync/status",
        web::get().to(get_flight_sync_status),
    )
    .route(
        "/api/v2/system/scheduler/flight-sync/trigger",
        web::post().to(trigger_flight_sync),
    );
}

#[cfg(test)]
mod tests {
    use super::ensure_scheduler_manual_trigger_permission;
    use crate::middleware::jwt::JwtAuth;
    use fms_application::schemas::auth_schemas::TokenData;
    use fms_application::services::authorization_service::PermissionCatalog;

    fn claims(permissions: &[&str]) -> JwtAuth {
        JwtAuth(TokenData {
            sub: Some("user-1".to_string()),
            email: None,
            username: Some("tester".to_string()),
            token_kind: Some("access".to_string()),
            is_admin: Some(false),
            permissions: permissions.iter().map(|item| item.to_string()).collect(),
            department: Some("ops".to_string()),
            department_id: Some("ops-1".to_string()),
            pv: Some(1),
            iat: None,
            exp: None,
            iss: None,
            aud: None,
            ua_hash: None,
            ip_subnet_hash: None,
        })
    }

    #[test]
    fn scheduler_manual_trigger_rejects_system_config_only() {
        let result = ensure_scheduler_manual_trigger_permission(&claims(&["system:config"]));

        assert!(
            result.is_err(),
            "legacy system:config must not be enough to manually trigger scheduler tasks"
        );
    }

    #[test]
    fn scheduler_manual_trigger_allows_ops_admin_permission() {
        let result = ensure_scheduler_manual_trigger_permission(&claims(&[PermissionCatalog::SYSTEM_OPS_ADMIN]));

        assert!(result.is_ok());
    }

    #[test]
    fn scheduler_manual_trigger_allows_legacy_system_admin_alias() {
        let result = ensure_scheduler_manual_trigger_permission(&claims(&["system:admin"]));

        assert!(result.is_ok());
    }
}
