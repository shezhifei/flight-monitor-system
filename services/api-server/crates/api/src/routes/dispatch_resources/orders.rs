use actix_web::{http::header, web, HttpRequest, HttpResponse};
use serde_json::json;
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::routes::dispatch_resources::{department_scope, ok_resp, request_id, request_wants_protobuf};
use crate::routes::ttl_bytes_cache::{json_bytes_response, TtlBytesCache};
use fms_application::services::auth_service::AuthService;
use fms_application::services::dispatch_collaboration_query_service::DispatchCollaborationQueryService;
use fms_application::services::dispatch_query_service::DispatchQueryService;
use fms_application::services::dispatch_resource_service::{
    build_dispatch_timeline_envelope_bytes, normalize_status_filters, CascadePreviewQuery, ConflictQuery,
    MyOrdersQuery, OrderListQuery, OrderTimelineQuery, TimelineQuery,
};

const PROTOBUF_MEDIA_TYPE: &str = "application/x-protobuf";
const DISPATCH_ORDER_LIST_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(1);

static DISPATCH_ORDER_LIST_CACHE: once_cell::sync::Lazy<TtlBytesCache> =
    once_cell::sync::Lazy::new(|| TtlBytesCache::new(DISPATCH_ORDER_LIST_CACHE_TTL));

fn blank_filter(value: Option<&str>) -> bool {
    value.map(str::trim).filter(|item| !item.is_empty()).is_none()
}

fn can_use_dispatch_order_list_cache(
    req: &HttpRequest,
    department: Option<&str>,
    query: &OrderListQuery,
    page: i64,
    page_size: i64,
) -> bool {
    department.is_none()
        && page == 1
        && page_size == 20
        && blank_filter(query.flight_id.as_deref())
        && blank_filter(query.status.as_deref())
        && blank_filter(query.source.as_deref())
        && !request_wants_protobuf(req)
}

pub async fn list_orders(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchQueryService>>,
    auth_svc: web::Data<Arc<AuthService>>,
    claims: JwtAuth,
    query: web::Query<OrderListQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let department = department_scope(auth_svc.get_ref(), &claims).await?;
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).clamp(1, 100);
    let use_cache = can_use_dispatch_order_list_cache(&req, department.as_deref(), &query, page, page_size);
    if use_cache {
        if let Some(body) = DISPATCH_ORDER_LIST_CACHE.get() {
            return Ok(json_bytes_response(body));
        }
    }
    let orders = svc
        .list_order_records(
            query.flight_id.as_deref(),
            query.status.as_deref(),
            query.source.as_deref(),
            department.as_deref(),
            page,
            page_size,
        )
        .await?;
    if !use_cache {
        return Ok(ok_resp(&req, orders));
    }
    let payload = json!({
        "success": true,
        "data": orders,
        "error": null,
        "request_id": request_id(&req),
    });
    let body = web::Bytes::from(
        serde_json::to_vec(&payload).map_err(|error| ApiError::Internal(error.to_string()))?,
    );
    DISPATCH_ORDER_LIST_CACHE.store(body.clone());
    Ok(json_bytes_response(body))
}

pub async fn list_my_orders(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchQueryService>>,
    claims: JwtAuth,
    query: web::Query<MyOrdersQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let user_id = claims
        .0
        .sub
        .clone()
        .ok_or_else(|| ApiError::Unauthorized("未登录".into()))?;
    let orders = svc.list_my_order_records(&user_id, query.status.as_deref()).await?;
    Ok(ok_resp(&req, orders))
}

pub async fn get_timeline(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchQueryService>>,
    auth_svc: web::Data<Arc<AuthService>>,
    claims: JwtAuth,
    query: web::Query<TimelineQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let department = department_scope(auth_svc.get_ref(), &claims).await?;
    let statuses = normalize_status_filters(query.statuses.as_deref());
    let payload = svc
        .get_timeline(
            query.view_mode.as_deref().unwrap_or("flight"),
            query.window_start,
            query.window_end,
            query.terminal.as_deref(),
            &statuses,
            query.source.as_deref(),
            department.as_deref(),
            query.include_cancelled.unwrap_or(false),
            claims.0.is_admin.unwrap_or(false),
        )
        .await?;
    if request_wants_protobuf(&req) {
        return Ok(HttpResponse::Ok()
            .insert_header((header::VARY, "Accept"))
            .content_type(PROTOBUF_MEDIA_TYPE)
            .body(build_dispatch_timeline_envelope_bytes(&payload)));
    }

    Ok(ok_resp(&req, payload))
}

pub async fn list_conflicts(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchQueryService>>,
    claims: JwtAuth,
    query: web::Query<ConflictQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let now = chrono::Utc::now();
    let start = query.window_start.unwrap_or_else(|| now - chrono::Duration::hours(2));
    let end = query.window_end.unwrap_or_else(|| now + chrono::Duration::hours(4));
    if end <= start {
        return Err(ApiError::BadRequest("window_end 必须晚于 window_start".into()));
    }
    let conflicts = svc.list_conflicts(start, end, query.limit.unwrap_or(200)).await?;
    Ok(ok_resp(
        &req,
        json!({
            "has_conflicts": !conflicts.is_empty(),
            "conflict_count": conflicts.len(),
            "conflicts": conflicts,
        }),
    ))
}

pub async fn cascade_preview(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchQueryService>>,
    claims: JwtAuth,
    query: web::Query<CascadePreviewQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let payload = svc
        .cascade_delay_preview(
            &query.flight_id,
            &query.task_type,
            query.delay_minutes,
            query.scheduled_departure,
        )
        .await?;
    Ok(ok_resp(&req, payload))
}

pub async fn get_order_timeline(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchCollaborationQueryService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    query: web::Query<OrderTimelineQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let payload = svc
        .get_order_timeline(&path.into_inner(), query.limit.unwrap_or(200))
        .await?;
    match payload {
        Some(payload) => Ok(ok_resp(&req, payload)),
        None => Err(ApiError::NotFound("派工单不存在".into())),
    }
}
