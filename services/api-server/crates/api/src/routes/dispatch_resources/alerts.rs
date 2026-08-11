//! 派工预排冲突告警 HTTP API。

use actix_web::{web, HttpRequest, HttpResponse};
use serde::Deserialize;
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::routes::dispatch_resources::ok_resp;
use fms_application::services::dispatch_service::dispatch_overrun_warning_service::{
    overrun_alert_to_json, DispatchOverrunWarningService,
};

#[derive(Debug, Deserialize)]
pub struct ListAlertsQuery {
    /// 仅返回未关闭告警;默认 true。
    pub unresolved: Option<bool>,
    pub flight_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ResolveAlertBody {
    pub notes: Option<String>,
}

pub async fn list_alerts(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchOverrunWarningService>>,
    claims: JwtAuth,
    query: web::Query<ListAlertsQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let unresolved = query.unresolved.unwrap_or(true);
    if !unresolved {
        return Err(ApiError::BadRequest(
            "当前仅支持 unresolved=true;完整历史查询尚未开放".into(),
        ));
    }
    let alerts = svc.list_unresolved(query.flight_id.as_deref()).await?;
    let payload: Vec<_> = alerts.iter().map(overrun_alert_to_json).collect();
    Ok(ok_resp(&req, payload))
}

pub async fn acknowledge_alert(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchOverrunWarningService>>,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let actor_id = claims
        .0
        .sub
        .clone()
        .ok_or_else(|| ApiError::Unauthorized("未登录".into()))?;
    let alert = svc.acknowledge(path.as_str(), &actor_id).await?;
    Ok(ok_resp(&req, overrun_alert_to_json(&alert)))
}

pub async fn resolve_alert(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchOverrunWarningService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: Option<web::Json<ResolveAlertBody>>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let actor_id = claims
        .0
        .sub
        .clone()
        .ok_or_else(|| ApiError::Unauthorized("未登录".into()))?;
    let notes = body.and_then(|value| value.into_inner().notes);
    let alert = svc.resolve(path.as_str(), &actor_id, notes.as_deref()).await?;
    Ok(ok_resp(&req, overrun_alert_to_json(&alert)))
}
