//! 航班列表响应缓存。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use actix_web::{http::header, web, HttpRequest, HttpResponse};
use arc_swap::ArcSwapOption;
use once_cell::sync::Lazy;
use serde_json::json;

use crate::error::ApiError;

pub(crate) const PROTOBUF_MEDIA_TYPE: &str = "application/x-protobuf";
const FLIGHT_LIST_RESPONSE_CACHE_MAX_AGE: Duration = Duration::from_secs(1);

static FLIGHT_LIST_TRACE_COUNTER: AtomicU64 = AtomicU64::new(0);
static FLIGHT_LIST_RESPONSE_CACHE: Lazy<ArcSwapOption<FlightListResponseCacheEntry>> = Lazy::new(ArcSwapOption::empty);

struct FlightListResponseCacheEntry {
    cached_at: Instant,
    body: web::Bytes,
}

fn perf_trace_enabled() -> bool {
    std::env::var("FMS_PERF_TRACE")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

pub fn should_emit_perf_trace(counter: &AtomicU64) -> bool {
    if !perf_trace_enabled() {
        return false;
    }
    let sample_rate = std::env::var("FMS_PERF_TRACE_SAMPLE_RATE")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(1000);
    counter.fetch_add(1, Ordering::Relaxed) % sample_rate == 0
}

pub fn request_wants_protobuf(req: &HttpRequest) -> bool {
    req.headers()
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_ascii_lowercase().contains(PROTOBUF_MEDIA_TYPE))
        .unwrap_or(false)
}

pub fn can_use_flight_list_response_cache(
    req: &HttpRequest,
    page: i64,
    size: i64,
    has_open_anomaly: Option<bool>,
) -> bool {
    page == 1 && size == 20 && has_open_anomaly.is_none() && !request_wants_protobuf(req)
}

pub async fn invalidate_flight_list_response_cache() {
    invalidate_flight_list_response_cache_now();
}

pub fn invalidate_flight_list_response_cache_now() {
    FLIGHT_LIST_RESPONSE_CACHE.store(None);
}

pub async fn invalidate_flight_list_response_and_publish(
    cache_invalidation: Option<&fms_application::services::cache_invalidation_service::CacheInvalidationService>,
    flight_id: Option<&str>,
    mut keys: Vec<fms_application::services::cache_invalidation_service::CacheInvalidationKey>,
) {
    use fms_application::services::cache_invalidation_service::CacheInvalidationKey;
    if !keys.contains(&CacheInvalidationKey::FlightListResponse) {
        keys.push(CacheInvalidationKey::FlightListResponse);
    }
    if let Some(cache_invalidation) = cache_invalidation {
        let event = match flight_id.map(str::trim).filter(|value| !value.is_empty()) {
            Some(flight_id) => cache_invalidation.flight_event(flight_id, keys),
            None => cache_invalidation.flight_list_event(keys),
        };
        cache_invalidation.invalidate_and_publish(event).await;
    } else {
        invalidate_flight_list_response_cache().await;
    }
}

pub fn cached_flight_list_response() -> Option<web::Bytes> {
    let entry = FLIGHT_LIST_RESPONSE_CACHE.load_full()?;
    if entry.cached_at.elapsed() <= FLIGHT_LIST_RESPONSE_CACHE_MAX_AGE {
        return Some(entry.body.clone());
    }
    None
}

pub fn store_flight_list_response_cache(body: web::Bytes) {
    FLIGHT_LIST_RESPONSE_CACHE.store(Some(Arc::new(FlightListResponseCacheEntry {
        cached_at: Instant::now(),
        body,
    })));
}

pub fn ok_resp_bytes(message: impl Into<String>, data: impl serde::Serialize) -> Result<web::Bytes, ApiError> {
    let payload = json!({
        "success": true,
        "data": data,
        "message": message.into(),
    });
    serde_json::to_vec(&payload)
        .map(web::Bytes::from)
        .map_err(|error| ApiError::Internal(error.to_string()))
}

pub fn json_body_response(body: web::Bytes) -> HttpResponse {
    HttpResponse::Ok()
        .insert_header((header::CONTENT_TYPE, "application/json"))
        .body(body)
}

pub fn trace_counter() -> &'static AtomicU64 {
    &FLIGHT_LIST_TRACE_COUNTER
}

pub fn should_emit_list_trace() -> bool {
    should_emit_perf_trace(&FLIGHT_LIST_TRACE_COUNTER)
}
