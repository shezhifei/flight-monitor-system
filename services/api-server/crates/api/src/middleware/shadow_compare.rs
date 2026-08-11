//! Shadow comparison middleware for Rust-primary cutover verification.
//!
//! Mirrors Python `shadow_compare.py` behavior:
//! - activates only for requests carrying `X-Shadow-Compare`
//! - skips SSE/static/frontend paths
//! - compares Python JSON response with the current Rust response
//! - persists mismatches to `runtime_diagnostic_events`

use actix_web::{
    body::{to_bytes, BoxBody, MessageBody},
    dev::{forward_ready, Payload, Service, ServiceRequest, ServiceResponse, Transform},
    web, Error, HttpMessage, HttpResponse,
};
use fms_domain::ports::runtime_diagnostic_sink::RuntimeDiagnosticSink;
use fms_runtime::spawn_tracked::spawn_tracked;
use futures_util::future::{ready, LocalBoxFuture, Ready};
use futures_util::StreamExt;
use reqwest::header::{
    HeaderMap as ReqwestHeaderMap, HeaderName as ReqwestHeaderName, HeaderValue as ReqwestHeaderValue,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SHADOW_DIFF_TOPIC: &str = "shadow_compare";
const DEFAULT_SHADOW_TARGET_URL: &str = "http://localhost:8000";
const DEFAULT_TIMEOUT_SECS: u64 = 5;
const VOLATILE_KEYS: &[&str] = &[
    "timestamp",
    "created_at",
    "updated_at",
    "finished_at",
    "cancelled_at",
    "started_at",
    "deleted_at",
    "request_id",
    "trace_id",
    "x_request_id",
    "server",
    "version",
];
const SKIP_PATH_PREFIXES: &[&str] = &[
    "/api/v2/sse/",
    "/api/v2/flights/stream",
    "/api/v2/flights/ws",
    "/api/v2/health/stream",
    "/static/",
    "/frontend/",
    "/css/",
    "/pics/",
    "/favicon",
];
const FORWARDED_METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE"];

#[derive(Debug, Clone)]
pub struct ShadowCompareConfig {
    pub enabled: bool,
    pub target_base_url: String,
    pub timeout: Duration,
}

impl Default for ShadowCompareConfig {
    fn default() -> Self {
        let target_base_url = std::env::var("SHADOW_COMPARE_TARGET_URL")
            .ok()
            .map(|value| value.trim().trim_end_matches('/').to_string())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                std::env::var("PYTHON_API_URL")
                    .ok()
                    .map(|value| value.trim().trim_end_matches('/').to_string())
                    .filter(|value| !value.is_empty())
            })
            .unwrap_or_else(|| DEFAULT_SHADOW_TARGET_URL.to_string());
        let timeout = std::env::var("SHADOW_COMPARE_TIMEOUT_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_TIMEOUT_SECS);

        Self {
            enabled: env_flag("SHADOW_COMPARE"),
            target_base_url,
            timeout: Duration::from_secs(timeout),
        }
    }
}

pub struct ShadowCompare {
    state: Arc<ShadowCompareState>,
}

impl ShadowCompare {
    pub fn new() -> Self {
        Self::with_config(ShadowCompareConfig::default())
    }

    pub fn with_config(config: ShadowCompareConfig) -> Self {
        let client = fms_application::http_client::shared_http_client();

        Self {
            state: Arc::new(ShadowCompareState { config, client }),
        }
    }
}

impl<S, B> Transform<S, ServiceRequest> for ShadowCompare
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: MessageBody + 'static,
    B::Error: Into<Error>,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type InitError = ();
    type Transform = ShadowCompareMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(ShadowCompareMiddleware {
            service: Rc::new(service),
            state: self.state.clone(),
        }))
    }
}

pub struct ShadowCompareMiddleware<S> {
    service: Rc<S>,
    state: Arc<ShadowCompareState>,
}

impl<S, B> Service<ServiceRequest> for ShadowCompareMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: MessageBody + 'static,
    B::Error: Into<Error>,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let srv = self.service.clone();
        let state = self.state.clone();

        Box::pin(async move {
            let mut req = req;
            let path = req.path().to_string();
            let method = req.method().as_str().to_uppercase();
            let should_compare = state.config.enabled
                && is_shadow_compare_header(req.headers())
                && should_shadow_path(&path)
                && is_forwarded_method(&method);

            let compare_request = if should_compare {
                let request_headers = build_forward_headers(req.headers());
                let query = req.uri().query().map(str::to_string);
                let diagnostics = req
                    .app_data::<web::Data<Arc<dyn RuntimeDiagnosticSink>>>()
                    .map(|value| value.get_ref().clone());
                let request_body = if method_supports_body(&method) {
                    let mut payload_stream = req.take_payload();
                    let mut body_bytes = web::BytesMut::new();
                    while let Some(chunk_result) = payload_stream.next().await {
                        let chunk = chunk_result?;
                        body_bytes.extend_from_slice(chunk.as_ref());
                    }
                    let body = body_bytes.freeze();
                    req.set_payload(Payload::from(body.clone()));
                    Some(body.to_vec())
                } else {
                    None
                };

                Some(ShadowCompareRequest {
                    path,
                    method,
                    query,
                    request_headers,
                    request_body,
                    diagnostics,
                })
            } else {
                None
            };

            let response = srv.call(req).await?;
            if compare_request.is_none() {
                return Ok(response.map_into_boxed_body());
            }

            let compare_request = compare_request.expect("checked above");
            let response = response.map_into_boxed_body();
            let (request, http_response) = response.into_parts();
            let status = http_response.status();
            let headers = http_response.headers().clone();
            let body = to_bytes(http_response.into_body()).await?;

            let rust_body = serde_json::from_slice::<Value>(&body).ok();
            if let Some(rust_body) = rust_body {
                let state = state.clone();
                spawn_tracked("shadow_compare:publish", async move {
                    state
                        .compare_and_publish(compare_request, status.as_u16(), rust_body)
                        .await;
                });
            }

            Ok(ServiceResponse::new(request, rebuild_response(status, &headers, body)))
        })
    }
}

#[derive(Clone)]
struct ShadowCompareRequest {
    path: String,
    method: String,
    query: Option<String>,
    request_headers: ReqwestHeaderMap,
    request_body: Option<Vec<u8>>,
    diagnostics: Option<Arc<dyn RuntimeDiagnosticSink>>,
}

#[derive(Clone)]
struct ShadowCompareState {
    config: ShadowCompareConfig,
    client: reqwest::Client,
}

impl ShadowCompareState {
    async fn compare_and_publish(&self, request: ShadowCompareRequest, rust_status: u16, rust_body: Value) {
        let python_response = self.call_python(&request).await;

        let mut diff = match python_response {
            None => json!({
                "match": false,
                "path": request.path,
                "method": request.method,
                "python_status": Value::Null,
                "rust_status": rust_status,
                "error": "python_unavailable",
                "missing_in_rust": [],
                "extra_in_rust": [],
                "value_diffs": {},
            }),
            Some(response) => {
                let python_status = response.status().as_u16();
                match response.json::<Value>().await {
                    Ok(python_body) => {
                        let mut diff = compare_json_responses(&python_body, &rust_body, &request.path);
                        if let Some(object) = diff.as_object_mut() {
                            object.insert("method".to_string(), json!(request.method));
                            object.insert("python_status".to_string(), json!(python_status));
                            object.insert("rust_status".to_string(), json!(rust_status));
                        }
                        diff
                    }
                    Err(error) => json!({
                        "match": false,
                        "path": request.path,
                        "method": request.method,
                        "python_status": python_status,
                        "rust_status": rust_status,
                        "error": format!("python_non_json:{error}"),
                        "missing_in_rust": [],
                        "extra_in_rust": [],
                        "value_diffs": {},
                    }),
                }
            }
        };

        if let Some(object) = diff.as_object_mut() {
            object.insert("ts".to_string(), json!(unix_timestamp_seconds()));
        }

        if diff.get("match").and_then(Value::as_bool).unwrap_or(false) {
            tracing::debug!(
                path = %diff
                    .get("path")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default(),
                method = %diff
                    .get("method")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default(),
                "shadow compare matched"
            );
            return;
        }

        tracing::info!(
            path = %diff
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or_default(),
            method = %diff
                .get("method")
                .and_then(|value| value.as_str())
                .unwrap_or_default(),
            python_status = %diff.get("python_status").map(value_to_log).unwrap_or_else(|| "null".to_string()),
            rust_status = %diff.get("rust_status").map(value_to_log).unwrap_or_else(|| "null".to_string()),
            missing_in_rust = ?diff.get("missing_in_rust").and_then(|value| value.as_array()),
            extra_in_rust = ?diff.get("extra_in_rust").and_then(|value| value.as_array()),
            value_diff_keys = ?diff
                .get("value_diffs")
                .and_then(|value| value.as_object())
                .map(|value| value.keys().cloned().collect::<Vec<String>>())
                .unwrap_or_default(),
            "shadow compare mismatch"
        );

        if let Some(diagnostics) = request.diagnostics {
            diagnostics.insert(SHADOW_DIFF_TOPIC, "shadow.diff", diff, None).await;
        }
    }

    async fn call_python(&self, request: &ShadowCompareRequest) -> Option<reqwest::Response> {
        let mut url = format!("{}{}", self.config.target_base_url, request.path);
        if let Some(query) = request.query.as_deref().filter(|value| !value.is_empty()) {
            url.push('?');
            url.push_str(query);
        }

        let method = reqwest::Method::from_bytes(request.method.as_bytes()).ok()?;
        let mut builder = self
            .client
            .request(method, url)
            .headers(request.request_headers.clone());

        if let Some(body) = request.request_body.clone() {
            builder = builder.body(body);
        }

        match builder.send().await {
            Ok(response) => Some(response),
            Err(error) => {
                tracing::warn!(
                    path = %request.path,
                    method = %request.method,
                    error = %error,
                    "shadow compare target request failed"
                );
                None
            }
        }
    }
}

fn env_flag(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .map(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

fn is_shadow_compare_header(headers: &actix_web::http::header::HeaderMap) -> bool {
    headers
        .get("x-shadow-compare")
        .and_then(|value| value.to_str().ok())
        .map(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

fn should_shadow_path(path: &str) -> bool {
    path.starts_with("/api/") && !SKIP_PATH_PREFIXES.iter().any(|prefix| path.starts_with(prefix))
}

fn is_forwarded_method(method: &str) -> bool {
    FORWARDED_METHODS.iter().any(|value| *value == method)
}

fn method_supports_body(method: &str) -> bool {
    matches!(method, "POST" | "PUT" | "PATCH")
}

/// 影子转发请求头白名单（纵深防御）。仅转发对响应语义必要、且不含敏感信息的请求头；
/// 任何未列出的请求头——包括所有凭证类头（authorization/cookie/x-api-key/…）以及未知的
/// 自定义内部头——一律丢弃，避免新增头在不知情时被泄露给影子目标。
/// 头名经 http crate 规范化为小写，故此处全部小写比较。
const SHADOW_FORWARD_HEADER_ALLOWLIST: &[&str] = &[
    // 内容协商 / 请求体解析
    "content-type",
    "accept",
    "accept-language",
    "accept-charset",
    // 链路追踪 / 请求关联
    "x-request-id",
    "x-correlation-id",
    "x-trace-id",
    "traceparent",
    "tracestate",
    // 客户端标识（无敏感信息）
    "user-agent",
];

fn build_forward_headers(headers: &actix_web::http::header::HeaderMap) -> ReqwestHeaderMap {
    let mut forwarded = ReqwestHeaderMap::new();
    for (name, value) in headers.iter() {
        let header_name = name.as_str();
        // 白名单纵深防御：仅放行显式允许的请求头。
        if !SHADOW_FORWARD_HEADER_ALLOWLIST.contains(&header_name) {
            continue;
        }

        let Ok(reqwest_name) = ReqwestHeaderName::from_bytes(header_name.as_bytes()) else {
            continue;
        };
        let Ok(reqwest_value) = ReqwestHeaderValue::from_bytes(value.as_bytes()) else {
            continue;
        };
        forwarded.append(reqwest_name, reqwest_value);
    }
    forwarded
}

fn rebuild_response(
    status: actix_web::http::StatusCode,
    headers: &actix_web::http::header::HeaderMap,
    body: web::Bytes,
) -> HttpResponse<BoxBody> {
    let mut builder = HttpResponse::build(status);
    for (name, value) in headers.iter() {
        if name == actix_web::http::header::CONTENT_LENGTH {
            continue;
        }
        builder.append_header((name.clone(), value.clone()));
    }
    builder.body(body)
}

#[derive(Debug, Default)]
struct ShadowDiffDetails {
    missing_in_rust: Vec<String>,
    extra_in_rust: Vec<String>,
    value_diffs: Map<String, Value>,
}

fn compare_json_responses(python_body: &Value, rust_body: &Value, path: &str) -> Value {
    let python = strip_volatile(python_body, 0);
    let rust = strip_volatile(rust_body, 0);
    let diff = deep_diff(&python, &rust, "");

    json!({
        "match": python == rust,
        "path": path,
        "missing_in_rust": diff.missing_in_rust,
        "extra_in_rust": diff.extra_in_rust,
        "value_diffs": diff.value_diffs,
    })
}

fn strip_volatile(value: &Value, depth: usize) -> Value {
    if depth > 10 {
        return value.clone();
    }

    match value {
        Value::Object(object) => {
            let filtered = object
                .iter()
                .filter(|(key, _)| !VOLATILE_KEYS.iter().any(|volatile| volatile == key))
                .map(|(key, value)| (key.clone(), strip_volatile(value, depth + 1)))
                .collect::<Map<String, Value>>();
            Value::Object(filtered)
        }
        Value::Array(items) => Value::Array(items.iter().map(|item| strip_volatile(item, depth + 1)).collect()),
        _ => value.clone(),
    }
}

fn deep_diff(python: &Value, rust: &Value, prefix: &str) -> ShadowDiffDetails {
    if python == rust {
        return ShadowDiffDetails::default();
    }

    match (python, rust) {
        (Value::Object(py_map), Value::Object(rust_map)) => {
            let py_keys = py_map.keys().cloned().collect::<BTreeSet<_>>();
            let rust_keys = rust_map.keys().cloned().collect::<BTreeSet<_>>();

            let mut details = ShadowDiffDetails {
                missing_in_rust: py_keys
                    .difference(&rust_keys)
                    .map(|key| prefixed_key(prefix, key))
                    .collect(),
                extra_in_rust: rust_keys
                    .difference(&py_keys)
                    .map(|key| prefixed_key(prefix, key))
                    .collect(),
                value_diffs: Map::new(),
            };

            for key in py_keys.intersection(&rust_keys) {
                let child_prefix = prefixed_key(prefix, key);
                let py_value = py_map.get(key).unwrap_or(&Value::Null);
                let rust_value = rust_map.get(key).unwrap_or(&Value::Null);
                if py_value == rust_value {
                    continue;
                }

                if py_value.is_object() && rust_value.is_object() {
                    let child = deep_diff(py_value, rust_value, &child_prefix);
                    details.missing_in_rust.extend(child.missing_in_rust);
                    details.extra_in_rust.extend(child.extra_in_rust);
                    details.value_diffs.extend(child.value_diffs);
                    continue;
                }

                if py_value.is_array() && rust_value.is_array() {
                    details.value_diffs.insert(
                        child_prefix,
                        json!({
                            "python": py_value,
                            "rust": rust_value,
                        }),
                    );
                    continue;
                }

                details.value_diffs.insert(
                    child_prefix,
                    json!({
                        "python": py_value,
                        "rust": rust_value,
                    }),
                );
            }

            details
        }
        _ => {
            let mut details = ShadowDiffDetails::default();
            details.value_diffs.insert(
                if prefix.is_empty() {
                    "root".to_string()
                } else {
                    prefix.to_string()
                },
                json!({
                    "python": python,
                    "rust": rust,
                }),
            );
            details
        }
    }
}

fn prefixed_key(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_string()
    } else {
        format!("{prefix}.{key}")
    }
}

fn unix_timestamp_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs_f64())
        .unwrap_or_default()
}

fn value_to_log(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::String(text) => text.clone(),
        _ => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_forward_headers, compare_json_responses, is_shadow_compare_header, should_shadow_path, strip_volatile,
    };
    use actix_web::http::header::{HeaderMap, HeaderName, HeaderValue};
    use serde_json::json;

    #[test]
    fn shadow_header_recognizes_truthy_values() {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("x-shadow-compare"),
            HeaderValue::from_static("true"),
        );
        assert!(is_shadow_compare_header(&headers));

        headers.insert(
            HeaderName::from_static("x-shadow-compare"),
            HeaderValue::from_static("0"),
        );
        assert!(!is_shadow_compare_header(&headers));
    }

    #[test]
    fn should_shadow_matches_python_skip_rules() {
        assert!(should_shadow_path("/api/v2/flights/search"));
        assert!(!should_shadow_path("/api/v2/sse/stream"));
        assert!(!should_shadow_path("/frontend/html/login.html"));
        assert!(!should_shadow_path("/favicon.ico"));
    }

    #[test]
    fn strip_volatile_removes_runtime_keys_recursively() {
        let payload = json!({
            "timestamp": "2026-01-01T00:00:00Z",
            "data": {
                "request_id": "req-1",
                "value": 1,
            }
        });

        assert_eq!(
            strip_volatile(&payload, 0),
            json!({
                "data": {
                    "value": 1,
                }
            })
        );
    }

    #[test]
    fn compare_json_responses_uses_python_as_baseline() {
        let python = json!({
            "status": "ok",
            "data": {
                "missing_field": 1,
                "same": true,
                "nested": {
                    "value": "python"
                }
            }
        });
        let rust = json!({
            "status": "ok",
            "data": {
                "same": true,
                "extra_field": 2,
                "nested": {
                    "value": "rust"
                }
            }
        });

        let diff = compare_json_responses(&python, &rust, "/api/v2/test");
        assert_eq!(diff["match"], false);
        assert_eq!(diff["path"], "/api/v2/test");
        assert_eq!(diff["missing_in_rust"], json!(["data.missing_field"]));
        assert_eq!(diff["extra_in_rust"], json!(["data.extra_field"]));
        assert_eq!(
            diff["value_diffs"]["data.nested.value"],
            json!({
                "python": "python",
                "rust": "rust"
            })
        );
    }

    #[test]
    fn shadow_forward_headers_drop_sensitive_client_credentials() {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("authorization"),
            HeaderValue::from_static("Bearer user-token"),
        );
        headers.insert(
            HeaderName::from_static("cookie"),
            HeaderValue::from_static("sid=secret"),
        );
        headers.insert(
            HeaderName::from_static("x-api-key"),
            HeaderValue::from_static("api-secret"),
        );
        headers.insert(
            HeaderName::from_static("x-request-id"),
            HeaderValue::from_static("req-1"),
        );

        let forwarded = build_forward_headers(&headers);

        assert!(forwarded.get(reqwest::header::AUTHORIZATION).is_none());
        assert!(forwarded.get(reqwest::header::COOKIE).is_none());
        assert!(forwarded.get("x-api-key").is_none());
        assert_eq!(forwarded.get("x-request-id").unwrap(), "req-1");
    }

    #[test]
    fn shadow_forward_headers_allowlist_drops_unknown_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("application/json"),
        );
        headers.insert(
            HeaderName::from_static("x-request-id"),
            HeaderValue::from_static("req-9"),
        );
        // 未列入白名单的自定义内部头：纵深防御下必须被丢弃。
        headers.insert(
            HeaderName::from_static("x-internal-feature-flag"),
            HeaderValue::from_static("on"),
        );
        headers.insert(
            HeaderName::from_static("authorization"),
            HeaderValue::from_static("Bearer t"),
        );

        let forwarded = build_forward_headers(&headers);

        // 白名单内的内容协商 / 关联头保留
        assert_eq!(
            forwarded.get(reqwest::header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
        assert_eq!(forwarded.get("x-request-id").unwrap(), "req-9");
        // 白名单外的未知头与凭证头一律丢弃
        assert!(forwarded.get("x-internal-feature-flag").is_none());
        assert!(forwarded.get(reqwest::header::AUTHORIZATION).is_none());
    }
}
