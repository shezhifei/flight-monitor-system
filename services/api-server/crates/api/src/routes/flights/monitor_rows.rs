use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::routes::ttl_bytes_cache::{json_bytes_response, TtlBytesCache};
use actix_web::{web, HttpRequest, HttpResponse};
use chrono::NaiveDate;
use fms_application::types::ConcreteFlightMonitorRowService;
use once_cell::sync::Lazy;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Deserialize)]
pub struct Query {
    pub workspace_date: Option<NaiveDate>,
    pub q: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

const MONITOR_ROWS_CACHE_TTL: Duration = Duration::from_secs(1);

static MONITOR_ROWS_CACHE: Lazy<TtlBytesCache> = Lazy::new(|| TtlBytesCache::new(MONITOR_ROWS_CACHE_TTL));

fn can_use_monitor_rows_cache(query: &Query, page: i64, size: i64) -> bool {
    page == 1
        && size == 20
        && query.workspace_date.is_none()
        && query
            .q
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
}

pub async fn list(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteFlightMonitorRowService>>,
    claims: JwtAuth,
    query: web::Query<Query>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_grant(fms_application::services::authorization_service::PermissionCatalog::FLIGHT_READ)?;
    let page = query.page.unwrap_or(1).max(1);
    let size = query.page_size.unwrap_or(100).clamp(1, 500);
    let use_cache = can_use_monitor_rows_cache(&query, page, size);
    if use_cache {
        if let Some(body) = MONITOR_ROWS_CACHE.get() {
            return Ok(json_bytes_response(body));
        }
    }
    let (items, total) = svc
        .list(query.workspace_date, query.q.as_deref(), size, (page - 1) * size)
        .await?;
    let payload = serde_json::json!({
        "success": true,
        "data": { "items": items, "total": total, "page": page, "size": size },
        "error": null,
        "request_id": req.headers().get("x-request-id").and_then(|value| value.to_str().ok()),
    });
    let body = web::Bytes::from(serde_json::to_vec(&payload).map_err(|error| ApiError::Internal(error.to_string()))?);
    if use_cache {
        MONITOR_ROWS_CACHE.store(body.clone());
    }
    Ok(json_bytes_response(body))
}
