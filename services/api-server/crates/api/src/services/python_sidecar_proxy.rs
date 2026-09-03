use actix_web::{web, HttpRequest, HttpResponse};
use jsonwebtoken::{encode, EncodingKey, Header};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::OnceLock;
use std::time::Duration;
use tracing::warn;

use crate::middleware::jwt::JwtSecret;
use crate::request_context::{extract_client_ip, extract_user_agent};

const DEFAULT_AI_SIDECAR_URL: &str = "http://localhost:9000";
const DEFAULT_AI_SIDECAR_TIMEOUT_SECS: u64 = 30;
const DEFAULT_AI_SIDECAR_CONNECT_TIMEOUT_SECS: u64 = 10;
const DEFAULT_AI_SIDECAR_HEALTH_TIMEOUT_SECS: u64 = 5;
const DEFAULT_AI_SIDECAR_SSE_CONNECT_TIMEOUT_SECS: u64 = 10;
const DEFAULT_AI_SIDECAR_SSE_MAX_DURATION_SECS: u64 = 600;
const DEFAULT_AI_SIDECAR_POOL_MAX_IDLE_PER_HOST: usize = 20;
const DEFAULT_AI_SIDECAR_POOL_IDLE_TIMEOUT_SECS: u64 = 90;
const SERVICE_IDENTITY_HEADER: &str = "X-Service-Identity";
const SERVICE_ISSUER: &str = "fms-rust-api";
const SERVICE_SUBJECT: &str = "rust-api-gateway";
const SERVICE_IDENTITY_TTL_SECS: u64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SidecarHttpClientConfig {
    request_timeout: Duration,
    connect_timeout: Duration,
    pool_max_idle_per_host: usize,
    pool_idle_timeout: Duration,
}

impl SidecarHttpClientConfig {
    fn from_env() -> Self {
        Self {
            request_timeout: env_duration_secs(&["AI_SIDECAR_TIMEOUT_SECS"], DEFAULT_AI_SIDECAR_TIMEOUT_SECS),
            connect_timeout: env_duration_secs(
                &["AI_SIDECAR_CONNECT_TIMEOUT_SECS"],
                DEFAULT_AI_SIDECAR_CONNECT_TIMEOUT_SECS,
            ),
            pool_max_idle_per_host: env_usize(
                &["AI_SIDECAR_POOL_MAX_IDLE_PER_HOST", "REQWEST_POOL_MAX_IDLE_PER_HOST"],
                DEFAULT_AI_SIDECAR_POOL_MAX_IDLE_PER_HOST,
            ),
            pool_idle_timeout: env_duration_secs(
                &["AI_SIDECAR_POOL_IDLE_TIMEOUT_SECS", "REQWEST_POOL_IDLE_TIMEOUT_SECS"],
                DEFAULT_AI_SIDECAR_POOL_IDLE_TIMEOUT_SECS,
            ),
        }
    }
}

fn env_duration_secs(keys: &[&str], default_secs: u64) -> Duration {
    Duration::from_secs(
        keys.iter()
            .find_map(|key| std::env::var(key).ok().and_then(|value| value.parse::<u64>().ok()))
            .unwrap_or(default_secs),
    )
}

fn env_duration_millis_or_secs(millis_key: &str, secs_key: &str, default_secs: u64) -> Duration {
    if let Some(millis) = std::env::var(millis_key)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
    {
        return Duration::from_millis(millis);
    }

    env_duration_secs(&[secs_key], default_secs)
}

fn env_usize(keys: &[&str], default: usize) -> usize {
    keys.iter()
        .find_map(|key| std::env::var(key).ok().and_then(|value| value.parse::<usize>().ok()))
        .unwrap_or(default)
}

fn build_sidecar_reqwest_client(config: SidecarHttpClientConfig) -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(config.request_timeout)
        .connect_timeout(config.connect_timeout)
        .pool_max_idle_per_host(config.pool_max_idle_per_host)
        .pool_idle_timeout(config.pool_idle_timeout)
        .tcp_keepalive(Duration::from_secs(60))
        .build()
}

/// 全局共享的 `reqwest::Client`，内建连接池（Keep-Alive 复用），
/// 杜绝每次请求重建 Client 导致的连接池丢弃和 TIME_WAIT 堆积。
fn global_reqwest_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        build_sidecar_reqwest_client(SidecarHttpClientConfig::from_env()).unwrap_or_else(|error| {
            warn!(error = %error, "failed to build configured sidecar reqwest client; falling back to default client");
            reqwest::Client::new()
        })
    })
}

/// 全局 SSE 代理专用的 `reqwest::Client` 单例.
///
/// BUG FIX: 之前的 `reqwest_client_with_connect_timeout()` 每次调用都会
/// `Client::builder().build()` 创建新的 Client 实例。由于 reqwest/hyper 的连接池
/// 绑定到 Client 生命周期，这意味着每次 SSE 请求都会创建全新的 TCP 连接池，
/// 导致 TIME_WAIT 堆积和连接握手延迟.
///
/// 现在改用 OnceLock 单例，共享底层连接池，复用 TCP Keep-Alive 连接.
///
/// 可通过环境变量配置:
/// - `AI_SIDECAR_SSE_CONNECT_TIMEOUT_SECS`: 连接建立超时（默认 10 秒）
/// - `REQWEST_POOL_MAX_IDLE_PER_HOST`: 每个 host 最大空闲连接数（默认 20）
/// - `REQWEST_POOL_IDLE_TIMEOUT_SECS`: 空闲连接超时（默认 90 秒）
fn global_sse_reqwest_client() -> &'static reqwest::Client {
    static SSE_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    SSE_CLIENT.get_or_init(|| {
        let connect_timeout = Duration::from_secs(
            std::env::var("AI_SIDECAR_SSE_CONNECT_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(DEFAULT_AI_SIDECAR_SSE_CONNECT_TIMEOUT_SECS),
        );
        reqwest::Client::builder()
            .connect_timeout(connect_timeout)
            .pool_max_idle_per_host(
                std::env::var("REQWEST_POOL_MAX_IDLE_PER_HOST")
                    .ok()
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(20),
            )
            .pool_idle_timeout(Duration::from_secs(
                std::env::var("REQWEST_POOL_IDLE_TIMEOUT_SECS")
                    .ok()
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(90),
            ))
            .tcp_keepalive(Duration::from_secs(60))
            .build()
            .unwrap_or_else(|error| {
                warn!(error = %error, "failed to build configured sidecar SSE reqwest client; falling back to default client");
                reqwest::Client::new()
            })
    })
}

#[derive(Debug, Clone)]
pub enum SidecarAuth {
    None,
    ServiceIdentity { audience: &'static str, path: String },
    VerificationToken(String),
}

#[derive(Debug, Serialize, Deserialize)]
struct ServiceIdentityClaims {
    iss: String,
    sub: String,
    aud: String,
    iat: usize,
    exp: usize,
    path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PythonSidecarHealth {
    pub sidecar_url: String,
    pub reachable: bool,
    pub status: String,
    pub detail: String,
}

pub fn ai_sidecar_url() -> String {
    #[cfg(test)]
    {
        std::env::var("AI_SIDECAR_URL").unwrap_or_else(|_| DEFAULT_AI_SIDECAR_URL.to_string())
    }

    #[cfg(not(test))]
    {
        static AI_SIDECAR_URL: OnceLock<String> = OnceLock::new();
        AI_SIDECAR_URL
            .get_or_init(|| std::env::var("AI_SIDECAR_URL").unwrap_or_else(|_| DEFAULT_AI_SIDECAR_URL.to_string()))
            .clone()
    }
}

pub fn ai_sidecar_timeout() -> Duration {
    env_duration_secs(&["AI_SIDECAR_TIMEOUT_SECS"], DEFAULT_AI_SIDECAR_TIMEOUT_SECS)
}

pub fn ai_sidecar_health_timeout() -> Duration {
    env_duration_millis_or_secs(
        "AI_SIDECAR_HEALTH_TIMEOUT_MILLIS",
        "AI_SIDECAR_HEALTH_TIMEOUT_SECS",
        DEFAULT_AI_SIDECAR_HEALTH_TIMEOUT_SECS,
    )
}

/// SSE 连接阶段超时（建立 TCP 连接 + HTTP 首字节）
pub fn ai_sidecar_sse_connect_timeout() -> Duration {
    env_duration_secs(
        &["AI_SIDECAR_SSE_CONNECT_TIMEOUT_SECS"],
        DEFAULT_AI_SIDECAR_SSE_CONNECT_TIMEOUT_SECS,
    )
}

/// SSE 最大持续时长（超过此时间自动断开）
pub fn ai_sidecar_sse_max_duration() -> Duration {
    env_duration_secs(
        &["AI_SIDECAR_SSE_MAX_DURATION_SECS"],
        DEFAULT_AI_SIDECAR_SSE_MAX_DURATION_SECS,
    )
}

/// 将 public API 路径映射到 sidecar 内部路径
/// Python sidecar 不再暴露 /api/v2/* 公共路径，所有内部请求走 /internal/ai/v1/*
pub fn map_to_internal_path(public_path: &str) -> String {
    match public_path {
        "/api/v2/ai/nl-query" => "/internal/ai/v1/runs".to_string(),
        "/api/v2/ai/nl-query/stream" => "/internal/ai/v1/runs/stream".to_string(),
        "/api/v2/ai/nl-query/stream-with-tools" => "/internal/ai/v1/runs/stream-with-tools".to_string(),
        "/api/v2/ai/eval/jobs" => "/internal/ai/v1/eval/jobs".to_string(),
        "/api/v2/ai/generate_plan" => "/internal/ai/v1/runs".to_string(),
        "/api/v2/ai/events/stream" => "/internal/ai/v1/events/stream".to_string(),
        p => format!("/internal/ai/v1{}", p.strip_prefix("/api/v2/ai").unwrap_or(p)),
    }
}

pub fn ai_sidecar_target_url(req: &HttpRequest) -> String {
    let base = ai_sidecar_url();
    let internal_path = map_to_internal_path(req.path());
    if req.query_string().trim().is_empty() {
        format!("{base}{internal_path}")
    } else {
        format!("{base}{internal_path}?{}", req.query_string())
    }
}

pub fn ai_sidecar_auth_for_path(internal_path: &str) -> SidecarAuth {
    SidecarAuth::ServiceIdentity {
        audience: "python-ai-runtime",
        path: internal_path.to_string(),
    }
}

pub fn ai_sidecar_auth(req: &HttpRequest) -> SidecarAuth {
    let internal_path = map_to_internal_path(req.path());
    ai_sidecar_auth_for_path(&internal_path)
}

pub async fn probe_ai_sidecar_health() -> PythonSidecarHealth {
    let sidecar_url = ai_sidecar_url();
    let timeout = ai_sidecar_health_timeout();
    let reachable = tokio::time::timeout(
        timeout,
        global_reqwest_client()
            .get(format!("{}/internal/ai/v1/health", sidecar_url))
            .timeout(timeout)
            .send(),
    )
    .await
    .ok()
    .and_then(Result::ok)
    .map(|resp| resp.status().is_success())
    .unwrap_or(false);

    PythonSidecarHealth {
        sidecar_url,
        reachable,
        status: if reachable {
            "healthy".to_string()
        } else {
            "degraded".to_string()
        },
        detail: if reachable {
            "Python AI runtime reachable".to_string()
        } else {
            "Python AI runtime unreachable".to_string()
        },
    }
}

pub async fn forward_ai_sidecar_json(req: &HttpRequest, method: Method, body: &Value) -> HttpResponse {
    forward_json_request(
        req,
        method,
        &ai_sidecar_target_url(req),
        body,
        ai_sidecar_auth(req),
        ai_sidecar_timeout(),
    )
    .await
}

pub async fn forward_ai_sidecar_request(req: &HttpRequest, method: Method) -> HttpResponse {
    forward_request(
        req,
        method,
        &ai_sidecar_target_url(req),
        ai_sidecar_auth(req),
        ai_sidecar_timeout(),
    )
    .await
}

pub async fn forward_ai_sidecar_sse(req: &HttpRequest) -> HttpResponse {
    forward_sse_request(
        req,
        &ai_sidecar_target_url(req),
        ai_sidecar_auth(req),
        ai_sidecar_sse_connect_timeout(),
    )
    .await
}

pub async fn forward_ai_sidecar_sse_json(req: &HttpRequest, method: Method, body: &Value) -> HttpResponse {
    forward_sse_json_request(
        req,
        method,
        &ai_sidecar_target_url(req),
        body,
        ai_sidecar_auth(req),
        ai_sidecar_sse_connect_timeout(),
    )
    .await
}

/// Deprecated public routes still proxy to Python, but must not surface sidecar 404 as route-not-found.
/// Scheduled for removal in Q3 2026.
pub fn coerce_deprecated_sidecar_proxy_response(resp: HttpResponse) -> HttpResponse {
    if resp.status() == actix_web::http::StatusCode::NOT_FOUND {
        return degraded_response("该 nl-query 接口为 deprecated fallback，Python AI Runtime 暂未提供对应内部端点");
    }
    resp
}

pub async fn forward_ai_sidecar_json_deprecated(req: &HttpRequest, method: Method, body: &Value) -> HttpResponse {
    warn!("DEPRECATED: forward_ai_sidecar_json_deprecated called — sidecar json proxy fallback scheduled for removal in Q3 2026");
    coerce_deprecated_sidecar_proxy_response(forward_ai_sidecar_json(req, method, body).await)
}

pub async fn forward_ai_sidecar_request_deprecated(req: &HttpRequest, method: Method) -> HttpResponse {
    warn!("DEPRECATED: forward_ai_sidecar_request_deprecated called — sidecar request proxy fallback scheduled for removal in Q3 2026");
    coerce_deprecated_sidecar_proxy_response(forward_ai_sidecar_request(req, method).await)
}

pub async fn forward_json_request(
    req: &HttpRequest,
    method: Method,
    url: &str,
    body: &Value,
    auth: SidecarAuth,
    timeout: Duration,
) -> HttpResponse {
    let mut outbound = global_reqwest_client().request(method, url).timeout(timeout).json(body);
    outbound = apply_common_headers(outbound, req);

    match apply_auth_headers(outbound, req, auth) {
        Ok(outbound) => match outbound.send().await {
            Ok(resp) => build_passthrough_response(resp).await,
            Err(error) => {
                if error.is_timeout() {
                    degraded_response("下游服务请求超时")
                } else if error.is_connect() {
                    degraded_response("无法连接下游 Python 服务")
                } else {
                    log_sidecar_proxy_error("sidecar JSON proxy request failed", error);
                    degraded_response("下游代理请求失败")
                }
            }
        },
        Err(response) => response,
    }
}

pub async fn forward_request(
    req: &HttpRequest,
    method: Method,
    url: &str,
    auth: SidecarAuth,
    timeout: Duration,
) -> HttpResponse {
    let mut outbound = global_reqwest_client().request(method, url).timeout(timeout);
    outbound = apply_common_headers(outbound, req);

    match apply_auth_headers(outbound, req, auth) {
        Ok(outbound) => match outbound.send().await {
            Ok(resp) => build_passthrough_response(resp).await,
            Err(error) => {
                if error.is_timeout() {
                    degraded_response("下游服务请求超时")
                } else if error.is_connect() {
                    degraded_response("无法连接下游 Python 服务")
                } else {
                    log_sidecar_proxy_error("sidecar proxy request failed", error);
                    degraded_response("下游代理请求失败")
                }
            }
        },
        Err(response) => response,
    }
}

pub async fn forward_sse_request(
    req: &HttpRequest,
    url: &str,
    auth: SidecarAuth,
    _connect_timeout: Duration,
) -> HttpResponse {
    // FIX: Use global SSE client singleton for connection pool reuse.
    // Previously reqwest_client_with_connect_timeout() created a new Client on every call,
    // causing TIME_WAIT socket exhaustion. Now we share the global singleton.
    let client = global_sse_reqwest_client();
    let mut outbound = client.get(url);
    outbound = apply_common_headers(outbound, req);

    forward_sse_builder(req, outbound, auth).await
}

pub async fn forward_sse_json_request(
    req: &HttpRequest,
    method: Method,
    url: &str,
    body: &Value,
    auth: SidecarAuth,
    _connect_timeout: Duration,
) -> HttpResponse {
    // FIX: Use global SSE client singleton for connection pool reuse.
    let client = global_sse_reqwest_client();
    let mut outbound = client.request(method, url).json(body);
    outbound = apply_common_headers(outbound, req);

    forward_sse_builder(req, outbound, auth).await
}

pub async fn forward_sse_json_request_raw(
    req: &HttpRequest,
    method: Method,
    url: &str,
    body: &Value,
    auth: SidecarAuth,
    _connect_timeout: Duration,
) -> Result<reqwest::Response, HttpResponse> {
    // FIX: Use global SSE client singleton for connection pool reuse.
    let client = global_sse_reqwest_client();
    let mut outbound = client.request(method, url).json(body);
    outbound = apply_common_headers(outbound, req);

    forward_sse_builder_raw(req, outbound, auth).await
}

async fn forward_sse_builder_raw(
    req: &HttpRequest,
    outbound: reqwest::RequestBuilder,
    auth: SidecarAuth,
) -> Result<reqwest::Response, HttpResponse> {
    match apply_auth_headers(outbound, req, auth) {
        Ok(outbound) => match outbound.send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    return Err(degraded_response(&format!("下游 SSE 服务返回 {}", resp.status())));
                }
                Ok(resp)
            }
            Err(error) => {
                if error.is_timeout() {
                    Err(degraded_response("下游 SSE 服务请求超时"))
                } else if error.is_connect() {
                    Err(degraded_response("无法连接下游 Python SSE 服务"))
                } else {
                    log_sidecar_proxy_error("sidecar SSE proxy request failed", error);
                    Err(degraded_response("下游 SSE 代理失败"))
                }
            }
        },
        Err(response) => Err(response),
    }
}

async fn forward_sse_builder(req: &HttpRequest, outbound: reqwest::RequestBuilder, auth: SidecarAuth) -> HttpResponse {
    match apply_auth_headers(outbound, req, auth) {
        Ok(outbound) => match outbound.send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    return degraded_response(&format!("下游 SSE 服务返回 {}", resp.status()));
                }

                HttpResponse::Ok()
                    .content_type("text/event-stream")
                    .insert_header(("Cache-Control", "no-cache"))
                    .insert_header(("Connection", "keep-alive"))
                    .insert_header(("X-Accel-Buffering", "no"))
                    .streaming(resp.bytes_stream())
            }
            Err(error) => {
                if error.is_timeout() {
                    degraded_response("下游 SSE 服务请求超时")
                } else if error.is_connect() {
                    degraded_response("无法连接下游 Python SSE 服务")
                } else {
                    log_sidecar_proxy_error("sidecar SSE proxy request failed", error);
                    degraded_response("下游 SSE 代理失败")
                }
            }
        },
        Err(response) => response,
    }
}

pub fn degraded_response(message: &str) -> HttpResponse {
    HttpResponse::ServiceUnavailable().json(json!({
        "success": false,
        "data": null,
        "message": message,
        "degraded": true,
    }))
}

fn log_sidecar_proxy_error(message: &'static str, error: reqwest::Error) {
    warn!(error = %error.without_url(), "{message}");
}

fn apply_common_headers(builder: reqwest::RequestBuilder, req: &HttpRequest) -> reqwest::RequestBuilder {
    let mut builder = builder;

    if let Some(value) = req
        .headers()
        .get("x-request-id")
        .and_then(|header| header.to_str().ok())
    {
        builder = builder.header("x-request-id", value);
    }

    if let Some(client_ip) = extract_client_ip(req) {
        builder = builder
            .header("X-Forwarded-For", client_ip.clone())
            .header("X-Real-IP", client_ip);
    }

    if let Some(user_agent) = extract_user_agent(req) {
        builder = builder.header("User-Agent", user_agent);
    }

    builder
}

fn apply_auth_headers(
    builder: reqwest::RequestBuilder,
    req: &HttpRequest,
    auth: SidecarAuth,
) -> Result<reqwest::RequestBuilder, HttpResponse> {
    match auth {
        SidecarAuth::None => Ok(builder),
        SidecarAuth::VerificationToken(token) => Ok(builder.header("X-Verification-Token", token)),
        SidecarAuth::ServiceIdentity { audience, path } => {
            let jwt_secret = req
                .app_data::<web::Data<JwtSecret>>()
                .ok_or_else(|| degraded_response("服务身份密钥未配置"))?;
            let token = build_service_identity(jwt_secret.get_ref(), audience, &path)?;
            Ok(builder.header(SERVICE_IDENTITY_HEADER, token))
        }
    }
}

fn build_service_identity(jwt_secret: &JwtSecret, audience: &str, path: &str) -> Result<String, HttpResponse> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as usize;

    let claims = ServiceIdentityClaims {
        iss: SERVICE_ISSUER.to_string(),
        sub: SERVICE_SUBJECT.to_string(),
        aud: audience.to_string(),
        iat: now,
        exp: now + SERVICE_IDENTITY_TTL_SECS as usize,
        path: path.to_string(),
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret.0.as_bytes()),
    )
    .map_err(|_| degraded_response("服务身份令牌签发失败"))
}

async fn build_passthrough_response(resp: reqwest::Response) -> HttpResponse {
    let status = resp.status();
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .unwrap_or_else(|| "application/json".to_string());

    // 流式透传响应体，避免将大 JSON 完全缓存到内存中
    HttpResponse::build(
        actix_web::http::StatusCode::from_u16(status.as_u16())
            .unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR),
    )
    .content_type(content_type)
    .streaming(resp.bytes_stream())
}

#[cfg(test)]
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    #[test]
    fn sidecar_http_client_config_has_bounded_pool_and_timeouts() {
        let _env_lock = lock_env();
        let _timeout = EnvVarGuard::set("AI_SIDECAR_TIMEOUT_SECS", "7");
        let _connect_timeout = EnvVarGuard::set("AI_SIDECAR_CONNECT_TIMEOUT_SECS", "3");
        let _pool_max_idle = EnvVarGuard::set("AI_SIDECAR_POOL_MAX_IDLE_PER_HOST", "11");
        let _pool_idle_timeout = EnvVarGuard::set("AI_SIDECAR_POOL_IDLE_TIMEOUT_SECS", "17");

        let config = SidecarHttpClientConfig::from_env();

        assert_eq!(config.request_timeout, Duration::from_secs(7));
        assert_eq!(config.connect_timeout, Duration::from_secs(3));
        assert_eq!(config.pool_max_idle_per_host, 11);
        assert_eq!(config.pool_idle_timeout, Duration::from_secs(17));
    }

    #[test]
    fn common_sidecar_headers_do_not_forward_client_authorization() {
        let req = TestRequest::get()
            .uri("/api/v2/ai/nl-query")
            .insert_header(("Authorization", "Bearer user-token"))
            .insert_header(("x-request-id", "req-1"))
            .to_http_request();

        let request = apply_common_headers(reqwest::Client::new().get("http://127.0.0.1/internal/ai/v1/runs"), &req)
            .build()
            .expect("request should build");

        assert!(request.headers().get(reqwest::header::AUTHORIZATION).is_none());
        assert_eq!(request.headers().get("x-request-id").unwrap(), "req-1");
    }

    #[tokio::test]
    async fn sidecar_health_probe_honors_configured_total_timeout() {
        let _env_lock = lock_env();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind hanging health probe listener");
        let sidecar_url = format!("http://{}", listener.local_addr().expect("listener addr"));
        let server = tokio::spawn(async move {
            if let Ok((_socket, _peer)) = listener.accept().await {
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
        let _url = EnvVarGuard::set("AI_SIDECAR_URL", &sidecar_url);
        let _timeout = EnvVarGuard::set("AI_SIDECAR_HEALTH_TIMEOUT_MILLIS", "50");

        let started = std::time::Instant::now();
        let result = tokio::time::timeout(Duration::from_millis(500), probe_ai_sidecar_health()).await;

        server.abort();
        let health = result.expect("health probe should be cancelled by its own timeout");
        assert!(!health.reachable);
        assert!(
            started.elapsed() < Duration::from_millis(300),
            "health probe should not wait for an unresponsive sidecar socket"
        );
    }

    #[test]
    fn map_nl_query_to_internal_runs() {
        assert_eq!(map_to_internal_path("/api/v2/ai/nl-query"), "/internal/ai/v1/runs");
    }

    #[test]
    fn map_nl_query_stream_to_internal_runs_stream() {
        assert_eq!(
            map_to_internal_path("/api/v2/ai/nl-query/stream"),
            "/internal/ai/v1/runs/stream"
        );
    }

    #[test]
    fn map_nl_query_stream_with_tools_to_internal_runs_stream_with_tools() {
        assert_eq!(
            map_to_internal_path("/api/v2/ai/nl-query/stream-with-tools"),
            "/internal/ai/v1/runs/stream-with-tools"
        );
    }

    #[test]
    fn map_generate_plan_to_internal_runs() {
        assert_eq!(map_to_internal_path("/api/v2/ai/generate_plan"), "/internal/ai/v1/runs");
    }

    #[test]
    fn map_eval_jobs_to_internal_eval_jobs() {
        assert_eq!(
            map_to_internal_path("/api/v2/ai/eval/jobs"),
            "/internal/ai/v1/eval/jobs"
        );
    }

    #[test]
    fn ai_sidecar_auth_uses_internal_path() {
        use actix_web::test::TestRequest;

        let req = TestRequest::with_uri("/api/v2/ai/nl-query").to_http_request();
        let auth = ai_sidecar_auth(&req);

        match auth {
            SidecarAuth::ServiceIdentity { audience, path } => {
                assert_eq!(audience, "python-ai-runtime");
                assert_eq!(path, "/internal/ai/v1/runs");
            }
            _ => panic!("Expected ServiceIdentity auth"),
        }
    }

    #[test]
    fn ai_sidecar_auth_for_stream_uses_internal_stream_path() {
        use actix_web::test::TestRequest;

        let req = TestRequest::with_uri("/api/v2/ai/nl-query/stream").to_http_request();
        let auth = ai_sidecar_auth(&req);

        match auth {
            SidecarAuth::ServiceIdentity { audience, path } => {
                assert_eq!(audience, "python-ai-runtime");
                assert_eq!(path, "/internal/ai/v1/runs/stream");
            }
            _ => panic!("Expected ServiceIdentity auth"),
        }
    }

    #[test]
    fn ai_sidecar_auth_for_path_directly() {
        let auth = ai_sidecar_auth_for_path("/internal/ai/v1/ontology/schema");

        match auth {
            SidecarAuth::ServiceIdentity { audience, path } => {
                assert_eq!(audience, "python-ai-runtime");
                assert_eq!(path, "/internal/ai/v1/ontology/schema");
            }
            _ => panic!("Expected ServiceIdentity auth"),
        }
    }

    #[test]
    fn coerce_deprecated_sidecar_proxy_response_maps_404_to_degraded() {
        use actix_web::http::StatusCode;

        let not_found = HttpResponse::NotFound().body("{}");
        let coerced = coerce_deprecated_sidecar_proxy_response(not_found);
        assert_eq!(coerced.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn coerce_deprecated_sidecar_proxy_response_passes_through_non_404() {
        use actix_web::http::StatusCode;

        let unavailable = HttpResponse::ServiceUnavailable().body("{}");
        let coerced = coerce_deprecated_sidecar_proxy_response(unavailable);
        assert_eq!(coerced.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn stream_jwt_path_claim_is_internal_runs_stream() {
        let secret = "test-stream-path-secret";
        let jwt_secret = JwtSecret(secret.to_string());
        let token = build_service_identity(&jwt_secret, "python-ai-runtime", "/internal/ai/v1/runs/stream")
            .expect("token build");

        let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
        validation.set_audience(&["python-ai-runtime"]);
        validation.set_issuer(&[SERVICE_ISSUER]);

        let decoded = jsonwebtoken::decode::<ServiceIdentityClaims>(
            &token,
            &jsonwebtoken::DecodingKey::from_secret(secret.as_bytes()),
            &validation,
        )
        .expect("token decode");

        assert_eq!(decoded.claims.path, "/internal/ai/v1/runs/stream");
        assert_eq!(decoded.claims.aud, "python-ai-runtime");
        assert_eq!(decoded.claims.iss, SERVICE_ISSUER);
        assert_eq!(decoded.claims.sub, SERVICE_SUBJECT);
    }

    #[test]
    fn non_stream_jwt_path_claim_is_internal_runs() {
        let secret = "test-non-stream-path-secret";
        let jwt_secret = JwtSecret(secret.to_string());
        let token =
            build_service_identity(&jwt_secret, "python-ai-runtime", "/internal/ai/v1/runs").expect("token build");

        let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
        validation.set_audience(&["python-ai-runtime"]);
        validation.set_issuer(&[SERVICE_ISSUER]);

        let decoded = jsonwebtoken::decode::<ServiceIdentityClaims>(
            &token,
            &jsonwebtoken::DecodingKey::from_secret(secret.as_bytes()),
            &validation,
        )
        .expect("token decode");

        assert_eq!(decoded.claims.path, "/internal/ai/v1/runs");
    }

    #[test]
    fn deprecated_sidecar_proxy_has_warnings() {
        let source = include_str!("python_sidecar_proxy.rs");
        let test_marker = "#[cfg(test)]";
        let main_code = &source[..source.find(test_marker).unwrap_or(source.len())];
        assert!(
            main_code.contains("tracing::warn") || main_code.contains("warn!"),
            "Deprecated proxy functions should log deprecation warnings"
        );
    }

    fn env_lock() -> &'static Mutex<()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    fn lock_env() -> MutexGuard<'static, ()> {
        match env_lock().lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(value) = &self.previous {
                std::env::set_var(self.key, value);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }
}
