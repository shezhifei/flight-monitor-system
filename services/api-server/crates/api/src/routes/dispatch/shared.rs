//! 派工路由
//!
//! 对应 Python dispatch_order_v2_routes.py (22 endpoints) — 全覆盖。

pub(crate) use crate::error::ApiError;
pub(crate) use crate::middleware::jwt::JwtAuth;
pub(crate) use crate::routes::dispatch_resources;
pub(crate) use actix_web::{web, HttpRequest, HttpResponse};
pub(crate) use fms_application::schemas::dispatch_schemas::*;
pub(crate) use fms_application::services::authorization_service::{AuthorizationService, PermissionCatalog};
pub(crate) use fms_application::services::dispatch_frontend_replan_service::DispatchFrontendReplanService;
pub(crate) use fms_application::services::dispatch_query_service::DispatchQueryService;
pub(crate) use fms_application::services::dispatch_service::DispatchService;
pub(crate) use serde_json::{json, Value};
pub(crate) use std::sync::Arc;
#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct DispatchOrderCancelQuery {
    pub(crate) reason: Option<String>,
    pub(crate) client_action_id: Option<String>,
}

// ===== 新增端点 =====

/// POST /{order_id}/eta-report
#[derive(serde::Deserialize)]
pub struct AutoDispatchQuery {
    pub flight_id: String,
    pub task_type: String,
    pub stand_id: String,
    pub planned_start_time: chrono::DateTime<chrono::Utc>,
    pub planned_end_time: Option<chrono::DateTime<chrono::Utc>>,
    pub terminal: Option<String>,
    pub department_id: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct BatchDispatchQuery {
    pub flight_id: String,
    pub stand_id: String,
    pub eta: chrono::DateTime<chrono::Utc>,
    pub etd: chrono::DateTime<chrono::Utc>,
    pub terminal: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct DispatchOrderBatchPublishRequest {
    pub order_ids: Option<Vec<String>>,
    pub at_time: Option<chrono::DateTime<chrono::Utc>>,
    pub event_code: Option<String>,
    pub flight_id: Option<String>,
    pub limit: Option<i64>,
    pub force: Option<bool>,
}

#[derive(serde::Deserialize)]
pub struct OptimalDispatchQuery {
    pub flight_id: Option<String>,
    pub stand_id: Option<String>,
    pub eta: Option<chrono::DateTime<chrono::Utc>>,
    pub etd: Option<chrono::DateTime<chrono::Utc>>,
    pub terminal: Option<String>,
    pub scope: Option<String>,
    pub window_start: Option<chrono::DateTime<chrono::Utc>>,
    pub window_end: Option<chrono::DateTime<chrono::Utc>>,
    pub freeze_order_ids: Option<String>,
    pub lock_policy: Option<String>,
    pub time_limit: Option<f64>,
}

#[derive(serde::Deserialize)]
pub struct GenerateDraftsQuery {
    pub flight_id: String,
    pub stand_id: String,
    pub eta: chrono::DateTime<chrono::Utc>,
    pub etd: chrono::DateTime<chrono::Utc>,
    pub terminal: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct BatchPublishDraftsRequest {
    pub assignments: Vec<serde_json::Value>,
}

pub(crate) fn merge_cancel_request(
    query: Option<web::Query<DispatchOrderCancelQuery>>,
    body: Option<web::Json<DispatchOrderCancelRequest>>,
) -> DispatchOrderCancelRequest {
    let mut dto = body.map(web::Json::into_inner).unwrap_or_default();
    let query = query.map(web::Query::into_inner).unwrap_or_default();

    if dto.reason.as_deref().map(str::trim).unwrap_or_default().is_empty() {
        dto.reason = query.reason;
    }
    if dto
        .client_action_id
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        dto.client_action_id = query.client_action_id;
    }

    dto
}

pub(crate) fn request_id(req: &HttpRequest) -> Option<String> {
    req.headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn ok_resp(req: &HttpRequest, data: impl serde::Serialize) -> HttpResponse {
    HttpResponse::Ok().json(json!({
        "success": true,
        "data": data,
        "error": null,
        "request_id": request_id(req),
    }))
}

pub(crate) async fn load_created_order_record(
    query_svc: &DispatchQueryService,
    order_id: &str,
) -> Result<Value, ApiError> {
    query_svc
        .get_order_record(order_id, true, None)
        .await?
        .ok_or_else(|| ApiError::Internal(format!("创建派工单后未能加载读模型: {order_id}")))
}

pub(crate) fn public_replan_snapshot_payload(payload: &DispatchReplanSnapshotResponse) -> Value {
    json!({
        "snapshot_id": &payload.snapshot_id,
        "model_version": &payload.model_version,
        "solver_version": &payload.solver_version,
        "generated_at": &payload.generated_at,
        "window_start": &payload.window_start,
        "window_end": &payload.window_end,
        "strategy": &payload.strategy,
        "max_suggestions": payload.max_suggestions,
        "travel_time_mode": &payload.travel_time_mode,
        "objective_config": &payload.objective_config,
        "unsupported_features": &payload.unsupported_features,
        "impact_summary": &payload.impact_summary,
        "changed_orders": &payload.changed_orders,
        "risk_level": &payload.risk_level,
        "requires_manual_confirmation": payload.requires_manual_confirmation,
        "optimizable_orders": &payload.optimizable_orders,
        "fixed_anchor_orders": &payload.fixed_anchor_orders,
        "orders": if payload.orders.is_empty() {
            json!(&payload.optimizable_orders)
        } else {
            json!(&payload.orders)
        },
        "fixed_orders": if payload.fixed_orders.is_empty() {
            json!(&payload.fixed_anchor_orders)
        } else {
            json!(&payload.fixed_orders)
        },
        "employee_anchor_states": &payload.employee_anchor_states,
        "equipment_anchor_states": &payload.equipment_anchor_states,
        "employee_free_windows": &payload.employee_free_windows,
        "equipment_free_windows": &payload.equipment_free_windows,
        "employee_unavailable_blocks": &payload.employee_unavailable_blocks,
        "equipment_unavailable_blocks": &payload.equipment_unavailable_blocks,
        "resource_travel_edges": if payload.resource_travel_edges.is_empty() {
            json!(&payload.travel_edges)
        } else {
            json!(&payload.resource_travel_edges)
        },
        "turnaround_pairs": &payload.turnaround_pairs,
    })
}

pub(crate) fn public_replan_apply_payload(payload: &DispatchReplanApplyResponse) -> Value {
    json!({
        "snapshot_id": &payload.snapshot_id,
        "applied": payload.applied,
        "order_results": &payload.order_results,
        "suggestions": if payload.suggestions.is_empty() {
            json!(&payload.order_results)
        } else {
            json!(&payload.suggestions)
        },
        "personnel_slot_assignments": &payload.personnel_slot_assignments,
        "equipment_slot_assignments": &payload.equipment_slot_assignments,
        "continuity_decisions": &payload.continuity_decisions,
        "objective_breakdown": &payload.objective_breakdown,
        "solver_run_metadata": if payload.solver_run_metadata.is_empty() {
            json!(&payload.solver_metadata)
        } else {
            json!(&payload.solver_run_metadata)
        },
        "solver_metadata": if payload.solver_metadata.is_empty() {
            json!(&payload.solver_run_metadata)
        } else {
            json!(&payload.solver_metadata)
        },
        "notification_summary": &payload.notification_summary,
        "message": &payload.message,
    })
}

/// 权限检查守卫
pub(crate) fn has_grant(claims: &JwtAuth, permission: &str) -> bool {
    AuthorizationService::has_grant(&claims.0, permission)
}

pub(crate) fn ensure_grant(claims: &JwtAuth, permission: &str) -> Result<(), ApiError> {
    if has_grant(claims, permission) {
        return Ok(());
    }
    Err(ApiError::Forbidden(format!("缺少权限: {permission}")))
}

pub(crate) fn ensure_all_grants(claims: &JwtAuth, permissions: &[&str]) -> Result<(), ApiError> {
    for permission in permissions {
        ensure_grant(claims, permission)?;
    }
    Ok(())
}

// === DTOs ===

pub(crate) fn order_status_label(status: fms_domain::models::dispatch::DispatchOrderStatus) -> &'static str {
    match status {
        fms_domain::models::dispatch::DispatchOrderStatus::Pending => "pending",
        fms_domain::models::dispatch::DispatchOrderStatus::Assigned => "assigned",
        fms_domain::models::dispatch::DispatchOrderStatus::InProgress => "in_progress",
        fms_domain::models::dispatch::DispatchOrderStatus::Completed => "completed",
        fms_domain::models::dispatch::DispatchOrderStatus::Cancelled => "cancelled",
    }
}
