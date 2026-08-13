//! 航班列表、搜索和最近更新路由。

use std::sync::Arc;
use std::time::Instant;

use actix_web::{http::header, web, HttpRequest, HttpResponse};
use tracing::info;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;

use super::cache::{
    cached_flight_list_response, can_use_flight_list_response_cache, json_body_response, ok_resp_bytes,
    request_wants_protobuf, should_emit_list_trace, store_flight_list_response_cache, PROTOBUF_MEDIA_TYPE,
};
use super::proto::build_flights_list_envelope_bytes;
use super::shared::{
    ok_resp, viewer_department_id, viewer_department_name, FlightListQuery, FlightSearchQuery, RecentUpdatesQuery,
};
use fms_application::services::authorization_service::PermissionCatalog;
use fms_application::services::flight_runtime_service::FlightRuntimeService;
use fms_application::services::flight_service::FlightService;

/// GET /api/v2/flights
pub async fn list_flights(
    req: HttpRequest,
    svc: web::Data<Arc<FlightService>>,
    runtime: web::Data<Arc<FlightRuntimeService>>,
    query: web::Query<FlightListQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_grant(PermissionCatalog::FLIGHT_READ)?;
    let trace = should_emit_list_trace();
    let total_start = Instant::now();
    let page = query.page.unwrap_or(1).max(1);
    let size = query.page_size.unwrap_or(100).clamp(1, 500);
    let use_response_cache = can_use_flight_list_response_cache(&req, page, size, query.has_open_anomaly);
    if use_response_cache {
        if let Some(body) = cached_flight_list_response() {
            if trace {
                info!(
                    target: "fms_perf",
                    event = "flights_list_route",
                    format = "json",
                    page,
                    page_size = size,
                    service_items = 20,
                    items = 20,
                    service_ms = 0.0,
                    enrich_ms = 0.0,
                    response_ms = 0.0,
                    total_ms = total_start.elapsed().as_secs_f64() * 1000.0,
                    response_bytes = body.len(),
                    response_cache_hit = true,
                );
            }
            return Ok(json_body_response(body));
        }
    }
    let service_start = Instant::now();
    let result = svc.list_flights(page, size, query.has_open_anomaly).await?;
    let service_ms = service_start.elapsed().as_secs_f64() * 1000.0;
    let service_items = result.items.len();
    let enrich_start = Instant::now();
    let items = runtime
        .enrich_flights_for_viewer(
            result.items,
            viewer_department_id(&claims),
            viewer_department_name(&claims),
        )
        .await?;
    let enrich_ms = enrich_start.elapsed().as_secs_f64() * 1000.0;
    let message = format!("成功获取 {} 个航班", items.len());
    if request_wants_protobuf(&req) {
        let response_start = Instant::now();
        let payload = build_flights_list_envelope_bytes(&items, &message, page, size);
        let response_ms = response_start.elapsed().as_secs_f64() * 1000.0;
        if trace {
            info!(
                target: "fms_perf",
                event = "flights_list_route",
                format = "protobuf",
                page,
                page_size = size,
                service_items,
                items = items.len(),
                service_ms,
                enrich_ms,
                response_ms,
                total_ms = total_start.elapsed().as_secs_f64() * 1000.0,
                response_bytes = payload.len(),
            );
        }
        return Ok(HttpResponse::Ok()
            .insert_header((header::CONTENT_TYPE, PROTOBUF_MEDIA_TYPE))
            .insert_header((header::VARY, "Accept"))
            .body(payload));
    }
    let response_start = Instant::now();
    let body = ok_resp_bytes(message, &items)?;
    let response_ms = response_start.elapsed().as_secs_f64() * 1000.0;
    if use_response_cache {
        store_flight_list_response_cache(body.clone());
    }
    if trace {
        info!(
            target: "fms_perf",
            event = "flights_list_route",
            format = "json",
            page,
            page_size = size,
            service_items,
            items = items.len(),
            service_ms,
            enrich_ms,
            response_ms,
            total_ms = total_start.elapsed().as_secs_f64() * 1000.0,
            response_bytes = body.len(),
            response_cache_hit = false,
        );
    }
    Ok(json_body_response(body))
}

/// GET /api/v2/flights/search
pub async fn search_flights(
    req: HttpRequest,
    svc: web::Data<Arc<FlightService>>,
    runtime: web::Data<Arc<FlightRuntimeService>>,
    query: web::Query<FlightSearchQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_grant(PermissionCatalog::FLIGHT_READ)?;
    let page = query.page.unwrap_or(1).max(1);
    let size = query.page_size.unwrap_or(100).clamp(1, 500);
    let result = svc
        .search_flights(
            query.flight_no.as_deref(),
            query.status.as_deref(),
            query.origin.as_deref(),
            query.destination.as_deref(),
            query.has_open_anomaly,
            page,
            size,
        )
        .await?;
    let items = runtime
        .enrich_flights_for_viewer(result, viewer_department_id(&claims), viewer_department_name(&claims))
        .await?;
    let message = format!("找到 {} 个匹配航班", items.len());
    if request_wants_protobuf(&req) {
        let payload = build_flights_list_envelope_bytes(&items, &message, page, size);
        return Ok(HttpResponse::Ok()
            .insert_header((header::CONTENT_TYPE, PROTOBUF_MEDIA_TYPE))
            .insert_header((header::VARY, "Accept"))
            .body(payload));
    }
    Ok(ok_resp(message, items))
}

/// GET /api/v2/flights/updates/recent
pub async fn recent_updates(
    runtime: web::Data<Arc<FlightRuntimeService>>,
    query: web::Query<RecentUpdatesQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_grant(PermissionCatalog::FLIGHT_READ)?;
    let minutes = query.minutes.unwrap_or(60).clamp(1, 1440);
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let updates = runtime.get_recent_flight_updates(minutes, limit).await?;
    Ok(ok_resp(format!("获取到 {} 条最近更新", updates.len()), updates))
}
