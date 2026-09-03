use crate::error::ApiError;
use crate::services::performance_metrics::PerformanceMetricsService;
use actix_web::http::header;
use actix_web::{
    dev::{forward_ready, Payload, Service, ServiceRequest, ServiceResponse, Transform},
    web, Error, HttpMessage,
};
use dashmap::DashMap;
use fms_domain::ports::nonce_replay_store::{NonceReplayDecision, NonceReplayStore, NonceReplayStoreError};
use futures_util::future::{ready, LocalBoxFuture, Ready};
use futures_util::StreamExt;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::warn;

const EMPTY_BODY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

static ANTI_REPLAY_TRACE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn perf_trace_enabled() -> bool {
    std::env::var("FMS_PERF_TRACE")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn should_emit_perf_trace(counter: &AtomicU64) -> bool {
    if !perf_trace_enabled() {
        return false;
    }
    let sample_rate = std::env::var("FMS_PERF_TRACE_SAMPLE_RATE")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(1000);
    counter.fetch_add(1, Ordering::Relaxed).is_multiple_of(sample_rate)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AntiReplayDecision {
    Required,
    Skip { reason: &'static str },
}

#[derive(Debug, Clone)]
pub struct AntiReplayPolicy;

impl AntiReplayPolicy {
    pub fn from_env() -> Self {
        Self
    }

    pub fn decide(&self, method: &str, path: &str) -> AntiReplayDecision {
        if is_public_path(path) {
            return AntiReplayDecision::Skip { reason: "public_path" };
        }
        if is_stream_path(path) {
            return AntiReplayDecision::Skip { reason: "stream_path" };
        }
        match method {
            "GET" | "HEAD" | "OPTIONS" => AntiReplayDecision::Required,
            "POST" | "PUT" | "PATCH" | "DELETE" => AntiReplayDecision::Required,
            _ => AntiReplayDecision::Required,
        }
    }
}

fn is_public_path(path: &str) -> bool {
    let exact_skips = [
        "/api/auth/login",
        "/api/auth/register",
        "/api/auth/refresh",
        "/api/v2/auth/login",
        "/api/v2/auth/register",
        "/api/v2/auth/refresh",
        "/api/ping",
        "/api/v2/ping",
        "/api/v2/health/ping",
        "/api/v2/system/runtime/health/ping",
        "/api/v2/system/health",
        "/",
        "/favicon.ico",
    ];
    if exact_skips.contains(&path) {
        return true;
    }
    let prefixes = [
        "/docs",
        "/openapi.json",
        "/redoc",
        "/frontend",
        "/static",
        "/pics",
        "/css",
        "/js",
    ];
    prefixes.iter().any(|prefix| path.starts_with(prefix))
}

fn is_stream_path(path: &str) -> bool {
    let prefixes = [
        "/api/v2/sse/stream",
        "/api/v2/flights/stream",
        "/api/v2/ai/events/stream",
        "/api/v2/notifications/stream",
        "/api/v2/dispatch/collaboration/stream",
        "/api/v2/kpi/stream",
        "/api/v2/anomalies/stream",
    ];
    prefixes.iter().any(|prefix| path.starts_with(prefix))
}

fn extract_token_for_replay(headers: &header::HeaderMap) -> Option<String> {
    if let Some(value) = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) {
        if value.len() > 7 && value[..7].eq_ignore_ascii_case("bearer ") {
            return Some(value[7..].to_string());
        }
    }

    if let Some(cookie_header) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) {
        for cookie_pair in cookie_header.split(';') {
            let trimmed = cookie_pair.trim();
            if let Some(value) = trimmed.strip_prefix("access_token=") {
                let value = value.trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }

    None
}

struct SessionSecretCacheEntry {
    session_secret_hex: String,
    expires_at: Instant,
}

struct SessionSecretCache {
    cache: DashMap<String, SessionSecretCacheEntry>,
    max_entries: usize,
    default_ttl_secs: u64,
}

impl SessionSecretCache {
    fn new(max_entries: usize, default_ttl_secs: u64) -> Self {
        Self {
            cache: DashMap::new(),
            max_entries,
            default_ttl_secs,
        }
    }

    fn get(&self, cache_key: &str) -> Option<String> {
        let entry = self.cache.get(cache_key)?;
        if Instant::now() < entry.expires_at {
            return Some(entry.session_secret_hex.clone());
        }
        drop(entry);
        self.cache.remove(cache_key);
        None
    }

    fn set(&self, cache_key: &str, session_secret_hex: String, ttl_secs: u64) {
        if self.max_entries == 0 {
            return;
        }
        let now = Instant::now();
        if self.cache.len() >= self.max_entries && !self.cache.contains_key(cache_key) {
            self.cache.retain(|_, entry| now < entry.expires_at);
            if self.cache.len() >= self.max_entries {
                let evict_key = {
                    let mut found = None;
                    for item in self.cache.iter() {
                        if item.key() != cache_key {
                            found = Some(item.key().clone());
                            break;
                        }
                    }
                    found
                };
                if let Some(evict_key) = evict_key {
                    self.cache.remove(&evict_key);
                }
            }
        }
        let ttl = if ttl_secs > 0 { ttl_secs } else { self.default_ttl_secs };
        self.cache.insert(
            cache_key.to_string(),
            SessionSecretCacheEntry {
                session_secret_hex,
                expires_at: now + Duration::from_secs(ttl),
            },
        );
    }
}

static SESSION_SECRET_CACHE: once_cell::sync::Lazy<SessionSecretCache> = once_cell::sync::Lazy::new(|| {
    let max_entries: usize = std::env::var("ANTI_REPLAY_SECRET_CACHE_MAX")
        .unwrap_or_else(|_| "10000".to_string())
        .parse()
        .unwrap_or(10000);
    let ttl_secs: u64 = std::env::var("ANTI_REPLAY_SECRET_CACHE_TTL_SECS")
        .unwrap_or_else(|_| "300".to_string())
        .parse()
        .unwrap_or(300);
    SessionSecretCache::new(max_entries, ttl_secs)
});

pub struct AntiReplay {
    max_timestamp_skew_secs: i64,
    policy: AntiReplayPolicy,
}

impl Default for AntiReplay {
    fn default() -> Self {
        Self::new()
    }
}

impl AntiReplay {
    pub fn new() -> Self {
        Self {
            max_timestamp_skew_secs: 60,
            policy: AntiReplayPolicy::from_env(),
        }
    }
}

impl<S, B> Transform<S, ServiceRequest> for AntiReplay
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = AntiReplayMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(AntiReplayMiddleware {
            service: Rc::new(service),
            max_timestamp_skew_secs: self.max_timestamp_skew_secs,
            policy: self.policy.clone(),
        }))
    }
}

pub struct AntiReplayMiddleware<S> {
    service: Rc<S>,
    max_timestamp_skew_secs: i64,
    policy: AntiReplayPolicy,
}

impl<S, B> Service<ServiceRequest> for AntiReplayMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let method = req.method().as_str();
        let path = req.path();

        match self.policy.decide(method, path) {
            AntiReplayDecision::Skip { reason: _ } => {
                if let Some(metrics) = req.app_data::<web::Data<Arc<PerformanceMetricsService>>>() {
                    metrics.increment_counter("anti_replay.skipped");
                }
                let srv = self.service.clone();
                return Box::pin(async move { srv.call(req).await });
            }
            AntiReplayDecision::Required => {}
        }

        let max_timestamp_skew_secs = self.max_timestamp_skew_secs;
        let srv = self.service.clone();

        Box::pin(async move {
            let mut req = req;
            let trace = should_emit_perf_trace(&ANTI_REPLAY_TRACE_COUNTER);
            let total_start = Instant::now();
            let headers = req.headers().clone();

            let access_token = match extract_token_for_replay(&headers) {
                Some(token) => token,
                None => {
                    return srv.call(req).await;
                }
            };

            let timestamp_str = match headers.get("X-Request-Timestamp").and_then(|v| v.to_str().ok()) {
                Some(v) => v.to_string(),
                None => {
                    return Err(ApiError::BadRequest("Missing Anti-Replay headers".into()).into());
                }
            };

            let nonce = match headers.get("X-Request-Nonce").and_then(|v| v.to_str().ok()) {
                Some(v) => v.to_string(),
                None => {
                    return Err(ApiError::BadRequest("Missing Anti-Replay headers".into()).into());
                }
            };

            let signature = match headers.get("X-Request-Signature").and_then(|v| v.to_str().ok()) {
                Some(v) => v.to_string(),
                None => {
                    return Err(ApiError::BadRequest("Missing Anti-Replay headers".into()).into());
                }
            };

            let body_hash = match headers.get("X-Request-Body-SHA256").and_then(|v| v.to_str().ok()) {
                Some(v) => v.to_string(),
                None => {
                    return Err(ApiError::BadRequest("Missing Anti-Replay headers".into()).into());
                }
            };

            let timestamp: i64 = match timestamp_str.parse() {
                Ok(t) => t,
                Err(_) => {
                    return Err(ApiError::BadRequest("Invalid timestamp format".into()).into());
                }
            };
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock before UNIX epoch")
                .as_secs() as i64;
            let skew = (now - timestamp).abs();
            if skew > max_timestamp_skew_secs {
                return Err(ApiError::BadRequest(format!(
                    "Request expired (timestamp skew {}s > {}s)",
                    skew, max_timestamp_skew_secs
                ))
                .into());
            }

            let Some(jwt_secret_data) = req.app_data::<web::Data<crate::middleware::jwt::JwtSecret>>() else {
                return Err(ApiError::Internal("Missing JwtSecret configuration".into()).into());
            };
            let jwt_secret = &jwt_secret_data.0;

            let token_cache_key = {
                let mut hasher = Sha256::new();
                hasher.update(access_token.as_bytes());
                hex::encode(hasher.finalize())
            };

            let secret_start = Instant::now();
            let mut secret_cache_hit = true;
            let session_secret_hex = match SESSION_SECRET_CACHE.get(&token_cache_key) {
                Some(cached) => cached,
                None => {
                    secret_cache_hit = false;
                    let mut mac_secret: Hmac<Sha256> = match Hmac::<Sha256>::new_from_slice(jwt_secret.as_bytes()) {
                        Ok(m) => m,
                        Err(_) => {
                            return Err(ApiError::Internal("HMAC initialization failed".into()).into());
                        }
                    };
                    mac_secret.update(access_token.as_bytes());
                    let derived = hex::encode(mac_secret.finalize().into_bytes());
                    SESSION_SECRET_CACHE.set(&token_cache_key, derived.clone(), 300);
                    derived
                }
            };
            let secret_ms = secret_start.elapsed().as_secs_f64() * 1000.0;

            let is_get_head =
                req.method() == actix_web::http::Method::GET || req.method() == actix_web::http::Method::HEAD;
            let method_str = req.method().as_str().to_string();

            let actual_body_hash = if is_get_head {
                if req
                    .headers()
                    .get(header::CONTENT_LENGTH)
                    .is_some_and(|v| v.to_str().ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0) > 0)
                {
                    return Err(ApiError::BadRequest(
                        "GET request body is not allowed for anti-replay protected requests".into(),
                    )
                    .into());
                }
                EMPTY_BODY_SHA256.to_string()
            } else {
                let hash_start = Instant::now();
                let mut payload_stream = req.take_payload();
                let mut body_bytes = web::BytesMut::new();
                while let Some(chunk_result) = payload_stream.next().await {
                    let chunk = chunk_result?;
                    body_bytes.extend_from_slice(chunk.as_ref());
                }
                let hash = {
                    let mut hasher = Sha256::new();
                    hasher.update(&body_bytes);
                    hex::encode(hasher.finalize())
                };
                req.set_payload(Payload::from(body_bytes.freeze()));
                if let Some(metrics) = req.app_data::<web::Data<Arc<PerformanceMetricsService>>>() {
                    metrics.record_latency(
                        "anti_replay.body_hash.duration_ms",
                        hash_start.elapsed().as_secs_f64() * 1000.0,
                    );
                }
                hash
            };

            if body_hash != actual_body_hash {
                return Err(ApiError::Unauthorized("Request body hash mismatch".into()).into());
            }

            let _ = req
                .app_data::<web::Data<Arc<PerformanceMetricsService>>>()
                .map(|m| m.increment_counter("anti_replay.checked"));

            let query = req.query_string();
            let uri = if query.is_empty() {
                req.path().to_string()
            } else {
                format!("{}?{}", req.path(), query)
            };
            let payload = format!("{}:{}:{}:{}:{}", method_str, uri, timestamp_str, nonce, body_hash);

            let signature_start = Instant::now();
            let mut mac_sig: Hmac<Sha256> = match Hmac::<Sha256>::new_from_slice(session_secret_hex.as_bytes()) {
                Ok(m) => m,
                Err(_) => {
                    return Err(ApiError::Internal("HMAC signature initialization failed".into()).into());
                }
            };
            mac_sig.update(payload.as_bytes());
            let expected_signature = hex::encode(mac_sig.finalize().into_bytes());
            let signature_ms = signature_start.elapsed().as_secs_f64() * 1000.0;

            if signature != expected_signature {
                return Err(ApiError::Unauthorized("Invalid request signature".into()).into());
            }

            let mut nonce_store_ms = 0.0;
            if let Some(store_data) = req.app_data::<web::Data<Arc<dyn NonceReplayStore>>>() {
                let redis_start = Instant::now();
                let redis_metrics = req.app_data::<web::Data<Arc<PerformanceMetricsService>>>();
                match store_data
                    .check_and_record(&session_secret_hex, timestamp, &nonce)
                    .await
                {
                    Ok(NonceReplayDecision::FirstSeen) => {
                        nonce_store_ms = redis_start.elapsed().as_secs_f64() * 1000.0;
                        if let Some(m) = &redis_metrics {
                            m.record_latency("anti_replay.redis_set.duration_ms", nonce_store_ms);
                        }
                    }
                    Ok(NonceReplayDecision::Replay) => {
                        nonce_store_ms = redis_start.elapsed().as_secs_f64() * 1000.0;
                        if let Some(m) = &redis_metrics {
                            m.record_latency("anti_replay.redis_set.duration_ms", nonce_store_ms);
                        }
                        return Err(ApiError::BadRequest("Replay attack detected (nonce already used)".into()).into());
                    }
                    Err(NonceReplayStoreError::Timeout) => {
                        warn!("anti-replay store timeout");
                        if let Some(m) = &redis_metrics {
                            m.increment_counter("anti_replay.redis_set.timeout");
                        }
                        return Err(ApiError::ServiceUnavailable("Anti-replay store timeout".into()).into());
                    }
                    Err(e) => {
                        warn!(error = %e, "anti-replay store error");
                        if let Some(m) = &redis_metrics {
                            m.increment_counter("anti_replay.redis_set.error");
                        }
                        return Err(ApiError::ServiceUnavailable("Anti-replay store error".into()).into());
                    }
                }
            }
            if trace {
                tracing::info!(
                    target: "fms_perf",
                    event = "anti_replay",
                    method = method_str.as_str(),
                    path = req.path(),
                    secret_cache_hit,
                    secret_ms,
                    signature_ms,
                    nonce_store_ms,
                    total_ms = total_start.elapsed().as_secs_f64() * 1000.0,
                );
            }

            srv.call(req).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::middleware::jwt::JwtSecret;
    use actix_web::http::StatusCode;
    use actix_web::{test as actix_test, App, HttpResponse};
    use async_trait::async_trait;

    fn default_policy() -> AntiReplayPolicy {
        AntiReplayPolicy::from_env()
    }

    #[derive(Clone, Copy)]
    enum StoreMode {
        FirstSeen,
        Replay,
        Timeout,
    }

    struct FakeNonceStore {
        mode: StoreMode,
    }

    #[async_trait]
    impl NonceReplayStore for FakeNonceStore {
        async fn check_and_record(
            &self,
            _session_hash: &str,
            _timestamp: i64,
            _nonce: &str,
        ) -> Result<NonceReplayDecision, NonceReplayStoreError> {
            match self.mode {
                StoreMode::FirstSeen => Ok(NonceReplayDecision::FirstSeen),
                StoreMode::Replay => Ok(NonceReplayDecision::Replay),
                StoreMode::Timeout => Err(NonceReplayStoreError::Timeout),
            }
        }
    }

    fn signed_headers(
        jwt_secret: &str,
        access_token: &str,
        method: &str,
        uri: &str,
        nonce: &str,
        body_hash: &str,
    ) -> Vec<(&'static str, String)> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_secs()
            .to_string();

        let mut mac_secret = Hmac::<Sha256>::new_from_slice(jwt_secret.as_bytes()).expect("jwt hmac");
        mac_secret.update(access_token.as_bytes());
        let session_secret_hex = hex::encode(mac_secret.finalize().into_bytes());

        let payload = format!("{method}:{uri}:{timestamp}:{nonce}:{body_hash}");
        let mut mac_sig = Hmac::<Sha256>::new_from_slice(session_secret_hex.as_bytes()).expect("request hmac");
        mac_sig.update(payload.as_bytes());
        let signature = hex::encode(mac_sig.finalize().into_bytes());

        vec![
            ("Authorization", format!("Bearer {access_token}")),
            ("X-Request-Timestamp", timestamp),
            ("X-Request-Nonce", nonce.to_string()),
            ("X-Request-Body-SHA256", body_hash.to_string()),
            ("X-Request-Signature", signature),
        ]
    }

    fn cookie_signed_headers(
        jwt_secret: &str,
        access_token: &str,
        method: &str,
        uri: &str,
        nonce: &str,
        body_hash: &str,
    ) -> Vec<(&'static str, String)> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_secs()
            .to_string();

        let mut mac_secret = Hmac::<Sha256>::new_from_slice(jwt_secret.as_bytes()).expect("jwt hmac");
        mac_secret.update(access_token.as_bytes());
        let session_secret_hex = hex::encode(mac_secret.finalize().into_bytes());

        let payload = format!("{method}:{uri}:{timestamp}:{nonce}:{body_hash}");
        let mut mac_sig = Hmac::<Sha256>::new_from_slice(session_secret_hex.as_bytes()).expect("request hmac");
        mac_sig.update(payload.as_bytes());
        let signature = hex::encode(mac_sig.finalize().into_bytes());

        vec![
            ("Cookie", format!("access_token={access_token}")),
            ("X-Request-Timestamp", timestamp),
            ("X-Request-Nonce", nonce.to_string()),
            ("X-Request-Body-SHA256", body_hash.to_string()),
            ("X-Request-Signature", signature),
        ]
    }

    #[test]
    fn default_policy_requires_auth_me_get() {
        let policy = default_policy();
        assert_eq!(policy.decide("GET", "/api/v2/auth/me"), AntiReplayDecision::Required);
    }

    #[test]
    fn default_policy_requires_flights_get() {
        let policy = default_policy();
        assert_eq!(
            policy.decide("GET", "/api/v2/flights?page=1&page_size=20"),
            AntiReplayDecision::Required
        );
    }

    #[test]
    fn default_policy_requires_authenticated_get() {
        let policy = default_policy();
        assert_eq!(
            policy.decide("GET", "/api/v2/todos?page=1&size=20"),
            AntiReplayDecision::Required
        );
    }

    #[test]
    fn public_health_path_is_skipped() {
        let policy = default_policy();
        assert_eq!(
            policy.decide("GET", "/api/v2/health/ping"),
            AntiReplayDecision::Skip { reason: "public_path" }
        );
    }

    #[test]
    fn public_login_path_is_skipped() {
        let policy = default_policy();
        assert_eq!(
            policy.decide("POST", "/api/v2/auth/login"),
            AntiReplayDecision::Skip { reason: "public_path" }
        );
    }

    #[test]
    fn stream_path_is_skipped() {
        let policy = default_policy();
        assert_eq!(
            policy.decide("GET", "/api/v2/sse/stream"),
            AntiReplayDecision::Skip { reason: "stream_path" }
        );
    }

    #[test]
    fn post_is_always_required() {
        let policy = default_policy();
        assert_eq!(policy.decide("POST", "/api/v2/flights"), AntiReplayDecision::Required);
    }

    #[test]
    fn put_is_always_required() {
        let policy = default_policy();
        assert_eq!(
            policy.decide("PUT", "/api/v2/flights/123"),
            AntiReplayDecision::Required
        );
    }

    #[test]
    fn patch_is_always_required() {
        let policy = default_policy();
        assert_eq!(
            policy.decide("PATCH", "/api/v2/flights/123/status"),
            AntiReplayDecision::Required
        );
    }

    #[test]
    fn delete_is_always_required() {
        let policy = default_policy();
        assert_eq!(
            policy.decide("DELETE", "/api/v2/todos/123"),
            AntiReplayDecision::Required
        );
    }

    #[test]
    fn head_is_required_by_default() {
        let policy = default_policy();
        assert_eq!(policy.decide("HEAD", "/api/v2/auth/me"), AntiReplayDecision::Required);
    }

    #[test]
    fn options_is_required_by_default() {
        let policy = default_policy();
        // OPTIONS to a non-public path should be required
        assert_eq!(
            policy.decide("OPTIONS", "/api/v2/flights"),
            AntiReplayDecision::Required
        );
    }

    #[test]
    fn empty_body_sha256_constant_is_correct() {
        // sha256("") in hex
        let expected = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert_eq!(EMPTY_BODY_SHA256, expected);
    }

    #[test]
    fn session_secret_cache_evicts_an_entry_when_full() {
        let cache = SessionSecretCache::new(2, 300);
        cache.set("token-a", "secret-a".to_string(), 300);
        cache.set("token-b", "secret-b".to_string(), 300);
        cache.set("token-c", "secret-c".to_string(), 300);

        let present = [
            cache.get("token-a").is_some(),
            cache.get("token-b").is_some(),
            cache.get("token-c").is_some(),
        ]
        .into_iter()
        .filter(|present| *present)
        .count();
        assert_eq!(present, 2);
        assert_eq!(cache.get("token-c"), Some("secret-c".to_string()));
    }

    #[test]
    fn session_secret_cache_max_entries_zero_disables_storage() {
        let cache = SessionSecretCache::new(0, 300);
        cache.set("token-a", "secret-a".to_string(), 300);

        assert_eq!(cache.get("token-a"), None);
    }

    #[test]
    fn read_policy_env_cannot_disable_authenticated_get_nonce() {
        std::env::set_var("ANTI_REPLAY_READ_POLICY", "skip_nonce");
        let policy = AntiReplayPolicy::from_env();
        assert_eq!(policy.decide("GET", "/api/v2/auth/me"), AntiReplayDecision::Required);
        std::env::remove_var("ANTI_REPLAY_READ_POLICY");
    }

    #[actix_web::test]
    async fn middleware_allows_first_seen_nonce() {
        let jwt_secret = "test-jwt-secret";
        let access_token = "test-access-token";
        let store: Arc<dyn NonceReplayStore> = Arc::new(FakeNonceStore {
            mode: StoreMode::FirstSeen,
        });
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(JwtSecret(jwt_secret.to_string())))
                .app_data(web::Data::new(store))
                .wrap(AntiReplay::new())
                .route(
                    "/api/v2/auth/me",
                    web::get().to(|| async { HttpResponse::Ok().finish() }),
                ),
        )
        .await;
        let mut req = actix_test::TestRequest::get().uri("/api/v2/auth/me");
        for (name, value) in signed_headers(
            jwt_secret,
            access_token,
            "GET",
            "/api/v2/auth/me",
            "nonce-first-seen",
            EMPTY_BODY_SHA256,
        ) {
            req = req.insert_header((name, value));
        }

        let response = actix_test::call_service(&app, req.to_request()).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[actix_web::test]
    async fn middleware_rejects_replayed_nonce() {
        let jwt_secret = "test-jwt-secret";
        let access_token = "test-access-token";
        let store: Arc<dyn NonceReplayStore> = Arc::new(FakeNonceStore {
            mode: StoreMode::Replay,
        });
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(JwtSecret(jwt_secret.to_string())))
                .app_data(web::Data::new(store))
                .wrap(AntiReplay::new())
                .route(
                    "/api/v2/auth/me",
                    web::get().to(|| async { HttpResponse::Ok().finish() }),
                ),
        )
        .await;
        let mut req = actix_test::TestRequest::get().uri("/api/v2/auth/me");
        for (name, value) in signed_headers(
            jwt_secret,
            access_token,
            "GET",
            "/api/v2/auth/me",
            "nonce-replay",
            EMPTY_BODY_SHA256,
        ) {
            req = req.insert_header((name, value));
        }

        let error = actix_test::try_call_service(&app, req.to_request())
            .await
            .expect_err("replayed nonce should be rejected");
        assert_eq!(
            error.as_response_error().error_response().status(),
            StatusCode::BAD_REQUEST
        );
    }

    #[actix_web::test]
    async fn middleware_rejects_nonce_store_timeout() {
        let jwt_secret = "test-jwt-secret";
        let access_token = "test-access-token";
        let store: Arc<dyn NonceReplayStore> = Arc::new(FakeNonceStore {
            mode: StoreMode::Timeout,
        });
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(JwtSecret(jwt_secret.to_string())))
                .app_data(web::Data::new(store))
                .wrap(AntiReplay::new())
                .route(
                    "/api/v2/auth/me",
                    web::get().to(|| async { HttpResponse::Ok().finish() }),
                ),
        )
        .await;
        let mut req = actix_test::TestRequest::get().uri("/api/v2/auth/me");
        for (name, value) in signed_headers(
            jwt_secret,
            access_token,
            "GET",
            "/api/v2/auth/me",
            "nonce-timeout",
            EMPTY_BODY_SHA256,
        ) {
            req = req.insert_header((name, value));
        }

        let error = actix_test::try_call_service(&app, req.to_request())
            .await
            .expect_err("nonce store timeout should be rejected");
        assert_eq!(
            error.as_response_error().error_response().status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[actix_web::test]
    async fn middleware_rejects_get_request_with_body() {
        let jwt_secret = "test-jwt-secret";
        let access_token = "test-access-token";
        let store: Arc<dyn NonceReplayStore> = Arc::new(FakeNonceStore {
            mode: StoreMode::FirstSeen,
        });
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(JwtSecret(jwt_secret.to_string())))
                .app_data(web::Data::new(store))
                .wrap(AntiReplay::new())
                .route(
                    "/api/v2/auth/me",
                    web::get().to(|| async { HttpResponse::Ok().finish() }),
                ),
        )
        .await;
        let mut req = actix_test::TestRequest::get().uri("/api/v2/auth/me");
        for (name, value) in signed_headers(
            jwt_secret,
            access_token,
            "GET",
            "/api/v2/auth/me",
            "nonce-get-body",
            EMPTY_BODY_SHA256,
        ) {
            req = req.insert_header((name, value));
        }
        let req = req.insert_header((header::CONTENT_LENGTH, "1"));

        let error = actix_test::try_call_service(&app, req.set_payload("x").to_request())
            .await
            .expect_err("GET body should be rejected");
        assert_eq!(
            error.as_response_error().error_response().status(),
            StatusCode::BAD_REQUEST
        );
    }

    #[actix_web::test]
    async fn middleware_enforces_anti_replay_for_cookie_authenticated_get() {
        let jwt_secret = "test-jwt-secret";
        let access_token = "cookie-access-token";
        let store: Arc<dyn NonceReplayStore> = Arc::new(FakeNonceStore {
            mode: StoreMode::FirstSeen,
        });
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(JwtSecret(jwt_secret.to_string())))
                .app_data(web::Data::new(store))
                .wrap(AntiReplay::new())
                .route(
                    "/api/v2/auth/me",
                    web::get().to(|| async { HttpResponse::Ok().finish() }),
                ),
        )
        .await;
        let mut req = actix_test::TestRequest::get().uri("/api/v2/auth/me");
        for (name, value) in cookie_signed_headers(
            jwt_secret,
            access_token,
            "GET",
            "/api/v2/auth/me",
            "nonce-cookie-first",
            EMPTY_BODY_SHA256,
        ) {
            req = req.insert_header((name, value));
        }

        let response = actix_test::call_service(&app, req.to_request()).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[actix_web::test]
    async fn middleware_rejects_cookie_auth_without_anti_replay_headers() {
        let jwt_secret = "test-jwt-secret";
        let store: Arc<dyn NonceReplayStore> = Arc::new(FakeNonceStore {
            mode: StoreMode::FirstSeen,
        });
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(JwtSecret(jwt_secret.to_string())))
                .app_data(web::Data::new(store))
                .wrap(AntiReplay::new())
                .route(
                    "/api/v2/auth/me",
                    web::get().to(|| async { HttpResponse::Ok().finish() }),
                ),
        )
        .await;
        let req = actix_test::TestRequest::get()
            .uri("/api/v2/auth/me")
            .insert_header(("Cookie", "access_token=some-cookie-token"));

        let error = actix_test::try_call_service(&app, req.to_request())
            .await
            .expect_err("cookie auth without anti-replay headers should be rejected");
        assert_eq!(
            error.as_response_error().error_response().status(),
            StatusCode::BAD_REQUEST
        );
    }

    #[actix_web::test]
    async fn middleware_rejects_cookie_auth_post_without_anti_replay_headers() {
        let jwt_secret = "test-jwt-secret";
        let store: Arc<dyn NonceReplayStore> = Arc::new(FakeNonceStore {
            mode: StoreMode::FirstSeen,
        });
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(JwtSecret(jwt_secret.to_string())))
                .app_data(web::Data::new(store))
                .wrap(AntiReplay::new())
                .route(
                    "/api/v2/flights",
                    web::post().to(|| async { HttpResponse::Ok().finish() }),
                ),
        )
        .await;
        let req = actix_test::TestRequest::post()
            .uri("/api/v2/flights")
            .insert_header(("Cookie", "access_token=some-cookie-token"));

        let error = actix_test::try_call_service(&app, req.to_request())
            .await
            .expect_err("cookie auth POST without anti-replay headers should be rejected");
        assert_eq!(
            error.as_response_error().error_response().status(),
            StatusCode::BAD_REQUEST
        );
    }
}
