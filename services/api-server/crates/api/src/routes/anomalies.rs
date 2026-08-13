//! 异常告警路由
//!
//! 对齐 Python anomaly_v2_routes.py 公共 v2 路由。

use actix_web::{web, HttpRequest, HttpResponse};
use fms_application::schemas::todo_schemas::TodoComplete;
use fms_application::services::todo_service::TodoService;
use serde_json::json;
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::sse::hub::SseHub;
use fms_application::services::anomaly_service::{AnomalyRuleCreate, AnomalyRuleUpdate, AnomalyService};

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

fn current_user_id(claims: &JwtAuth) -> &str {
    claims
        .0
        .username
        .as_deref()
        .or(claims.0.sub.as_deref())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("AnomalyResolver")
}

async fn broadcast_anomaly_event(hub: &Arc<SseHub>, event: &str, payload: serde_json::Value) {
    let _ = hub.broadcast_event("anomaly_alerts", Some(event), payload).await;
}

async fn removed_public_route() -> HttpResponse {
    HttpResponse::NotFound().finish()
}

/// GET /api/v2/anomalies
async fn list_anomalies(
    req: HttpRequest,
    svc: web::Data<Arc<AnomalyService>>,
    query: web::Query<AnomalyListQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("flight:read")?;
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let offset = query.offset.unwrap_or(0).max(0);
    let items = svc
        .list_anomalies(
            query.status.as_deref(),
            query.anomaly_type.as_deref(),
            query.start_date,
            query.end_date,
            limit,
            offset,
        )
        .await?;
    Ok(ok_resp(&req, json!({ "items": items, "total": items.len() })))
}

/// GET /api/v2/anomalies/stats
async fn get_stats(
    req: HttpRequest,
    svc: web::Data<Arc<AnomalyService>>,
    query: web::Query<AnomalyStatsQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("flight:read")?;
    let stats = svc.get_stats(query.start_date, query.end_date).await?;
    Ok(ok_resp(&req, stats))
}

/// GET /api/v2/anomalies/rules — 异常规则列表
async fn list_rules(
    req: HttpRequest,
    svc: web::Data<Arc<AnomalyService>>,
    query: web::Query<AnomalyRulesQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("system:config")?;
    let rules = svc.list_rules(query.enabled_only.unwrap_or(false)).await?;
    Ok(ok_resp(&req, rules))
}

/// POST /api/v2/anomalies/rules — 创建规则
async fn create_rule(
    req: HttpRequest,
    svc: web::Data<Arc<AnomalyService>>,
    claims: JwtAuth,
    body: web::Json<AnomalyRuleCreate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("system:config")?;
    let created = svc.create_rule(body.into_inner()).await?;
    Ok(ok_resp(&req, created))
}

/// PUT /api/v2/anomalies/rules/{rule_id} — 更新规则
async fn update_rule(
    req: HttpRequest,
    svc: web::Data<Arc<AnomalyService>>,
    path: web::Path<String>,
    claims: JwtAuth,
    body: web::Json<AnomalyRuleUpdate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("system:config")?;
    let rule_id = path.into_inner();
    match svc.update_rule(&rule_id, body.into_inner()).await? {
        Some(updated) => Ok(ok_resp(&req, updated)),
        None => Err(ApiError::NotFound("Rule not found".into())),
    }
}

/// GET /api/v2/anomalies/{anomaly_id}
async fn get_anomaly(
    req: HttpRequest,
    svc: web::Data<Arc<AnomalyService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("flight:read")?;
    let id = path.into_inner();
    match svc.get_anomaly(&id).await? {
        Some(a) => Ok(ok_resp(&req, a)),
        None => Err(ApiError::NotFound(format!("异常 {id} 未找到"))),
    }
}

/// POST /api/v2/anomalies/{anomaly_id}/acknowledge
async fn acknowledge(
    req: HttpRequest,
    svc: web::Data<Arc<AnomalyService>>,
    hub: web::Data<Arc<SseHub>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("anomaly:write")?;
    let id = path.into_inner();
    let ok = svc.acknowledge(&id).await?;
    if ok {
        broadcast_anomaly_event(
            &hub,
            "anomaly_acknowledged",
            json!({
                "type": "anomaly_acknowledged",
                "anomaly_id": id,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }),
        )
        .await;
        Ok(ok_resp(&req, json!({ "anomaly_id": id })))
    } else {
        Err(ApiError::NotFound("异常未找到或已解决".into()))
    }
}

/// POST /api/v2/anomalies/{anomaly_id}/resolve
async fn resolve_anomaly(
    req: HttpRequest,
    svc: web::Data<Arc<AnomalyService>>,
    todo_svc: web::Data<Arc<TodoService>>,
    hub: web::Data<Arc<SseHub>>,
    path: web::Path<String>,
    body: web::Json<AnomalyResolveRequest>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("anomaly:write")?;
    let id = path.into_inner();
    let payload = body.into_inner();
    let anomaly = svc
        .get_anomaly(&id)
        .await?
        .ok_or_else(|| ApiError::NotFound("异常未找到或已解决".into()))?;
    let ok = svc.resolve(&id).await?;
    if ok {
        let mut todo_resolved = false;
        let mut todo_resolution_failed = false;
        if payload.resolve_todo {
            if let Some(todo_id) = anomaly.linked_todo_id.clone() {
                match todo_svc
                    .complete_todo(
                        &todo_id,
                        TodoComplete {
                            actual_duration: None,
                            completed_by: Some(current_user_id(&claims).to_string()),
                        },
                        current_user_id(&claims),
                    )
                    .await
                {
                    Ok(Some(_)) => {
                        todo_resolved = true;
                    }
                    Ok(None) | Err(_) => {
                        todo_resolution_failed = true;
                    }
                }
            }
        }
        broadcast_anomaly_event(
            &hub,
            "anomaly_resolved",
            json!({
                "type": "anomaly_resolved",
                "anomaly_id": id,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }),
        )
        .await;
        Ok(ok_resp(
            &req,
            json!({
                "anomaly_id": id,
                "todo_resolved": todo_resolved,
                "todo_resolution_failed": todo_resolution_failed,
            }),
        ))
    } else {
        Err(ApiError::NotFound("异常未找到或已解决".into()))
    }
}

#[derive(serde::Deserialize)]
pub struct AnomalyListQuery {
    status: Option<String>,
    anomaly_type: Option<String>,
    start_date: Option<chrono::DateTime<chrono::Utc>>,
    end_date: Option<chrono::DateTime<chrono::Utc>>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(serde::Deserialize)]
pub struct AnomalyStatsQuery {
    start_date: Option<chrono::DateTime<chrono::Utc>>,
    end_date: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(serde::Deserialize)]
pub struct AnomalyResolveRequest {
    #[serde(default)]
    _note: Option<String>,
    #[serde(default = "default_true")]
    resolve_todo: bool,
}

#[derive(serde::Deserialize)]
pub struct AnomalyRulesQuery {
    enabled_only: Option<bool>,
}

fn default_true() -> bool {
    true
}

/// 注册异常路由（对齐 Python v2 公共契约）
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/anomalies")
            .route("", web::get().to(list_anomalies))
            .route("/stats", web::get().to(get_stats))
            .route("/rules", web::get().to(list_rules))
            .route("/rules", web::post().to(create_rule))
            .route("/rules/{rule_id}", web::put().to(update_rule))
            .route("/stream", web::get().to(removed_public_route))
            .route("/ws", web::get().to(removed_public_route))
            .route("/{anomaly_id}", web::get().to(get_anomaly))
            .route("/{anomaly_id}/acknowledge", web::post().to(acknowledge))
            .route("/{anomaly_id}/resolve", web::post().to(resolve_anomaly)),
    );
}
