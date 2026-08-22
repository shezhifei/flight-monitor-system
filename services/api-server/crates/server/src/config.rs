//! 配置加载与安全辅助模块
//!
//! 负责从环境变量与 Vault 文件中解析与验证各项服务参数。

use actix_web::http::header::{HeaderMap, HeaderName, HeaderValue};
use actix_web::HttpResponse;
use fms_application::schemas::response::ApiErrorResponse;
use rustls::crypto::ring::default_provider as default_ring_crypto_provider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::ServerConfig as RustlsServerConfig;
use std::fs::File;
use std::io::{self, BufReader};
use std::path::Path;
use std::str::FromStr;
use url::Url;

pub const DEFAULT_CORS_ALLOWED_ORIGINS: &[&str] = &[
    "https://localhost:3000",
    "https://127.0.0.1:3000",
    "https://localhost:8080",
    "https://127.0.0.1:8080",
    "https://localhost:8000",
    "https://127.0.0.1:8000",
    "https://localhost:5000",
    "https://127.0.0.1:5000",
    "https://localhost",
    "https://127.0.0.1",
    "http://localhost:3000",
    "http://127.0.0.1:3000",
    "http://localhost:8080",
    "http://127.0.0.1:8080",
    "http://localhost:8000",
    "http://127.0.0.1:8000",
    "http://localhost:5000",
    "http://127.0.0.1:5000",
    "http://localhost",
    "http://127.0.0.1",
    "http://0.0.0.0:3000",
    "http://0.0.0.0:8000",
];

pub const STRICT_TRANSPORT_SECURITY_VALUE: &str = "max-age=31536000; includeSubDomains";
pub const X_CONTENT_TYPE_OPTIONS_VALUE: &str = "nosniff";
pub const X_FRAME_OPTIONS_VALUE: &str = "SAMEORIGIN";
pub const REFERRER_POLICY_VALUE: &str = "strict-origin-when-cross-origin";
pub const PERMISSIONS_POLICY_VALUE: &str = "camera=(), geolocation=(), microphone=(self)";
pub const CONTENT_SECURITY_POLICY_VALUE: &str = "default-src 'self'; base-uri 'self'; form-action 'self'; frame-ancestors 'self'; object-src 'none'; img-src 'self' data: blob: https:; font-src 'self' data: https:; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline' 'unsafe-eval'; connect-src 'self' https: wss:";
pub const CONTENT_SECURITY_POLICY_PRODUCTION_VALUE: &str = "default-src 'self'; base-uri 'self'; form-action 'self'; frame-ancestors 'self'; object-src 'none'; img-src 'self' data: blob: https:; font-src 'self' data: https:; style-src 'self'; script-src 'self'; connect-src 'self' https: wss:";

pub fn env_flag(key: &str, default: bool) -> bool {
    std::env::var(key)
        .map(|value| value.eq_ignore_ascii_case("true") || value == "1")
        .unwrap_or(default)
}

pub fn parse_bool_like(value: Option<&str>, default: bool) -> bool {
    value
        .map(|raw| matches!(raw.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(default)
}

pub fn normalize_cors_origin(origin: &str) -> Option<String> {
    let parsed = Url::parse(origin.trim()).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    if parsed.host_str().is_none() || parsed.query().is_some() || parsed.fragment().is_some() {
        return None;
    }
    if parsed.path() != "/" && !parsed.path().is_empty() {
        return None;
    }

    let host = parsed.host_str()?.to_ascii_lowercase();
    let normalized = match parsed.port() {
        Some(port) => format!("{}://{}:{}", parsed.scheme(), host, port),
        None => format!("{}://{}", parsed.scheme(), host),
    };
    Some(normalized)
}

pub fn load_cors_allowed_origins_for_environment(
    raw_value: Option<&str>,
    environment: Option<&str>,
) -> io::Result<Vec<String>> {
    let raw_value = raw_value.unwrap_or("").trim();
    if raw_value.is_empty() && is_production_environment(environment) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "CORS_ALLOWED_ORIGINS must be set explicitly in production",
        ));
    }

    let raw_origins: Vec<&str> = if raw_value.is_empty() {
        DEFAULT_CORS_ALLOWED_ORIGINS.to_vec()
    } else {
        raw_value.split(',').collect()
    };

    let mut normalized_origins: Vec<String> = Vec::new();
    for raw_origin in raw_origins {
        let trimmed = raw_origin.trim();
        if trimmed.is_empty() {
            continue;
        }

        let Some(normalized) = normalize_cors_origin(trimmed) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Invalid CORS origin: {trimmed}"),
            ));
        };

        if !normalized_origins.iter().any(|existing| existing == &normalized) {
            normalized_origins.push(normalized);
        }
    }

    if normalized_origins.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "At least one CORS allowed origin must be configured",
        ));
    }

    Ok(normalized_origins)
}

pub fn load_cors_allowed_origins_from_env(raw_value: Option<&str>) -> io::Result<Vec<String>> {
    load_cors_allowed_origins_for_environment(raw_value, None)
}

/// Runtime environment classification for security configuration.
///
/// Fail-closed design: unknown or missing environment values default to
/// `Production` to ensure security hardening (JWT audience, CORS, CSP, etc.)
/// cannot be silently disabled by a missing or typo'd env var.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeEnvironment {
    /// Development/test environment with relaxed security defaults.
    Development,
    /// Production environment with strict security requirements.
    Production,
}

impl RuntimeEnvironment {
    /// Parse environment string into enum. Unknown values map to Production.
    pub fn from_env_value(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            None | Some("") => RuntimeEnvironment::Production,
            Some(v)
                if v.eq_ignore_ascii_case("development")
                    || v.eq_ignore_ascii_case("dev")
                    || v.eq_ignore_ascii_case("test")
                    || v.eq_ignore_ascii_case("testing")
                    || v.eq_ignore_ascii_case("local")
                    || v.eq_ignore_ascii_case("localhost") =>
            {
                RuntimeEnvironment::Development
            }
            Some(_) => RuntimeEnvironment::Production,
        }
    }

    /// Returns true if this is a production environment.
    pub fn is_production(&self) -> bool {
        matches!(self, RuntimeEnvironment::Production)
    }

    /// Returns the environment name for logging.
    pub fn as_str(&self) -> &'static str {
        match self {
            RuntimeEnvironment::Development => "development",
            RuntimeEnvironment::Production => "production",
        }
    }
}

pub fn runtime_environment() -> Option<String> {
    std::env::var("APP_ENVIRONMENT")
        .or_else(|_| std::env::var("APP_ENV"))
        .or_else(|_| std::env::var("ENVIRONMENT"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn is_production_environment(environment: Option<&str>) -> bool {
    // Fail-closed: only explicit development/test/local values are treated as
    // non-production. Unknown, empty, or misspelled environment names default
    // to production so security hardening (JWT audience, CORS, CSP, SSE query
    // token) cannot be silently disabled by a missing or typo'd env var.
    RuntimeEnvironment::from_env_value(environment).is_production()
}

pub fn load_cors_allowed_origins() -> io::Result<Vec<String>> {
    let environment = runtime_environment();
    load_cors_allowed_origins_for_environment(
        std::env::var("CORS_ALLOWED_ORIGINS").ok().as_deref(),
        environment.as_deref(),
    )
}

pub fn is_cors_origin_allowed(origin: &str, allowed_origins: &[String]) -> bool {
    let Some(normalized) = normalize_cors_origin(origin) else {
        return false;
    };
    allowed_origins.iter().any(|allowed| allowed == &normalized)
}

pub fn resolve_redis_url_from_env(raw_value: Option<&str>, redis_required: bool) -> io::Result<String> {
    let redis_url = raw_value.unwrap_or("").trim();
    if redis_url.is_empty() {
        let message = if redis_required {
            "REDIS_URL must be set when Redis is required"
        } else {
            "REDIS_URL must be set before starting the runtime"
        };
        return Err(io::Error::new(io::ErrorKind::InvalidInput, message));
    }

    Ok(redis_url.to_string())
}

pub fn resolve_redis_url(redis_required: bool) -> io::Result<String> {
    resolve_redis_url_from_env(std::env::var("REDIS_URL").ok().as_deref(), redis_required)
}

pub fn redact_url_credentials(raw_url: &str) -> String {
    let Ok(mut parsed) = Url::parse(raw_url) else {
        return "<redacted-url>".to_string();
    };

    if !parsed.username().is_empty() {
        let _ = parsed.set_username("redacted");
    }
    if parsed.password().is_some() {
        let _ = parsed.set_password(Some("redacted"));
    }
    parsed.to_string()
}

pub fn request_uses_https(req: &actix_web::dev::ServiceRequest) -> bool {
    if req.connection_info().scheme().eq_ignore_ascii_case("https") {
        return true;
    }

    req.headers()
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.eq_ignore_ascii_case("https"))
        .unwrap_or(false)
}

pub fn insert_standard_security_headers(headers: &mut HeaderMap, is_secure_request: bool, is_production: bool) {
    headers.insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static(X_CONTENT_TYPE_OPTIONS_VALUE),
    );
    headers.insert(
        HeaderName::from_static("x-frame-options"),
        HeaderValue::from_static(X_FRAME_OPTIONS_VALUE),
    );
    headers.insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static(REFERRER_POLICY_VALUE),
    );
    headers.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static(PERMISSIONS_POLICY_VALUE),
    );
    let csp_value = if is_production {
        CONTENT_SECURITY_POLICY_PRODUCTION_VALUE
    } else {
        CONTENT_SECURITY_POLICY_VALUE
    };
    headers.insert(
        HeaderName::from_static("content-security-policy"),
        HeaderValue::from_str(csp_value).expect("CSP value must be a valid header value"),
    );

    let strict_transport_security = HeaderName::from_static("strict-transport-security");
    if is_secure_request {
        headers.insert(
            strict_transport_security,
            HeaderValue::from_static(STRICT_TRANSPORT_SECURITY_VALUE),
        );
    } else {
        headers.remove(strict_transport_security);
    }
}

pub fn env_i64(key: &str, default: i64) -> i64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(default)
}

pub fn env_string(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

pub fn env_optional_string(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// 环境变量：是否启用 TLS session tickets（用于会话复用）
const API_TLS_ENABLE_SESSION_TICKETS: &str = "API_TLS_ENABLE_SESSION_TICKETS";

/// 环境变量：TLS session timeout（秒）
const API_TLS_SESSION_TIMEOUT: &str = "API_TLS_SESSION_TIMEOUT";

pub const DEFAULT_API_TLS_CERT_FILE: &str = "certs/server.crt";
pub const DEFAULT_API_TLS_KEY_FILE: &str = "certs/server.key";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpTlsBindingConfig {
    pub cert_file: String,
    pub key_file: String,
}

#[derive(Debug, Clone)]
pub struct HttpTlsPerformanceConfig {
    /// 是否启用 TLS session tickets（用于会话复用）
    pub enable_session_tickets: bool,
    /// TLS session timeout（秒），默认 1 小时
    pub session_timeout: u32,
    /// HTTP/2 initial stream window size（字节），默认 1MB
    pub http2_initial_stream_window_size: u32,
    /// HTTP/2 initial connection window size（字节），默认 1MB
    pub http2_initial_connection_window_size: u32,
}

impl Default for HttpTlsPerformanceConfig {
    fn default() -> Self {
        Self {
            enable_session_tickets: true,
            session_timeout: 3600, // 1 hour in seconds
            http2_initial_stream_window_size: 1 << 20, // 1MB
            http2_initial_connection_window_size: 1 << 20, // 1MB
        }
    }
}

pub fn resolve_http_tls_binding_config(
    enabled: Option<&str>,
    cert_file: Option<&str>,
    key_file: Option<&str>,
) -> Option<HttpTlsBindingConfig> {
    if !parse_bool_like(enabled, false) {
        return None;
    }

    let cert_file = cert_file
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_API_TLS_CERT_FILE)
        .to_string();
    let key_file = key_file
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_API_TLS_KEY_FILE)
        .to_string();

    Some(HttpTlsBindingConfig { cert_file, key_file })
}

/// 解析 TLS 性能优化配置
pub fn resolve_http_tls_performance_config() -> HttpTlsPerformanceConfig {
    HttpTlsPerformanceConfig {
        enable_session_tickets: parse_bool_like(
            env_optional_string(API_TLS_ENABLE_SESSION_TICKETS).as_deref(),
            true,
        ),
        session_timeout: env_optional_string(API_TLS_SESSION_TIMEOUT)
            .and_then(|v| v.parse().ok())
            .unwrap_or(3600), // Default: 1 hour
        http2_initial_stream_window_size: env_optional_string("HTTP2_INITIAL_STREAM_WINDOW_SIZE")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1 << 20), // Default: 1MB
        http2_initial_connection_window_size: env_optional_string("HTTP2_INITIAL_CONNECTION_WINDOW_SIZE")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1 << 20), // Default: 1MB
    }
}

pub fn read_cert_chain(cert_file: &Path) -> io::Result<Vec<CertificateDer<'static>>> {
    let mut reader = BufReader::new(File::open(cert_file)?);
    rustls_pemfile::certs(&mut reader).collect()
}

pub fn read_private_key(key_file: &Path) -> io::Result<PrivateKeyDer<'static>> {
    let mut reader = BufReader::new(File::open(key_file)?);
    let pkcs8_keys = rustls_pemfile::pkcs8_private_keys(&mut reader)
        .map(|result| result.map(Into::into))
        .collect::<io::Result<Vec<PrivateKeyDer<'static>>>>()?;
    if let Some(key) = pkcs8_keys.into_iter().next() {
        return Ok(key);
    }

    let mut reader = BufReader::new(File::open(key_file)?);
    let rsa_keys = rustls_pemfile::rsa_private_keys(&mut reader)
        .map(|result| result.map(Into::into))
        .collect::<io::Result<Vec<PrivateKeyDer<'static>>>>()?;
    if let Some(key) = rsa_keys.into_iter().next() {
        return Ok(key);
    }

    let mut reader = BufReader::new(File::open(key_file)?);
    let sec1_keys = rustls_pemfile::ec_private_keys(&mut reader)
        .map(|result| result.map(Into::into))
        .collect::<io::Result<Vec<PrivateKeyDer<'static>>>>()?;
    if let Some(key) = sec1_keys.into_iter().next() {
        return Ok(key);
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("No supported private key found in {}", key_file.display()),
    ))
}

pub fn load_rustls_server_config(
    tls_binding_config: &HttpTlsBindingConfig,
    performance_config: &HttpTlsPerformanceConfig,
) -> io::Result<RustlsServerConfig> {
    let cert_chain = read_cert_chain(Path::new(&tls_binding_config.cert_file))?;
    if cert_chain.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("No certificates found in {}", tls_binding_config.cert_file),
        ));
    }

    let private_key = read_private_key(Path::new(&tls_binding_config.key_file))?;
    
    let mut config = RustlsServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, private_key)
        .map_err(io_other)?;

    if performance_config.enable_session_tickets {
        config.session_storage = rustls::server::ServerSessionMemoryCache::new(
            performance_config.session_timeout.max(32) as usize,
        );
    }

    Ok(config)
}

pub fn install_rustls_crypto_provider() -> io::Result<()> {
    default_ring_crypto_provider()
        .install_default()
        .map_err(|_| io_other("failed to install rustls ring crypto provider"))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DatabaseUrlDefaults {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub user: Option<String>,
    pub password: Option<String>,
}

impl DatabaseUrlDefaults {
    pub fn from_database_url(database_url: &str) -> Self {
        let parsed_options = sqlx::postgres::PgConnectOptions::from_str(database_url).ok();
        let parsed_url = Url::parse(database_url).ok();

        Self {
            host: parsed_options
                .as_ref()
                .map(|options| options.get_host().trim().to_string())
                .filter(|value| !value.is_empty()),
            port: parsed_options.as_ref().map(|options| options.get_port()),
            database: parsed_options
                .as_ref()
                .and_then(|options| options.get_database())
                .map(str::trim)
                .map(str::to_string)
                .filter(|value| !value.is_empty()),
            user: parsed_options
                .as_ref()
                .map(|options| options.get_username().trim().to_string())
                .filter(|value| !value.is_empty()),
            password: parsed_url
                .and_then(|url| url.password().map(str::to_string))
                .filter(|value| !value.is_empty()),
        }
    }
}

pub fn env_or_value(primary_key: &str, secondary_key: Option<&str>, fallback: Option<&str>, default: &str) -> String {
    env_optional_string(primary_key)
        .or_else(|| secondary_key.and_then(env_optional_string))
        .or_else(|| fallback.map(str::to_string).filter(|value| !value.is_empty()))
        .unwrap_or_else(|| default.to_string())
}

pub fn io_other(message: impl ToString) -> std::io::Error {
    std::io::Error::other(message.to_string())
}

pub fn normalize_runtime_role(raw_role: &str) -> String {
    let mut raw_role = raw_role.trim().to_ascii_lowercase();

    if matches!(raw_role.as_str(), "combined" | "standalone") {
        raw_role = "all".to_string();
    }

    match raw_role.as_str() {
        "all" | "api" | "worker" => raw_role,
        _ => "all".to_string(),
    }
}

pub fn runtime_role() -> String {
    normalize_runtime_role(&std::env::var("APP_RUNTIME_ROLE").unwrap_or_else(|_| "all".to_string()))
}

pub fn should_start_background_jobs_for_role(role: &str) -> bool {
    matches!(role, "all" | "worker")
}

pub fn should_start_http_server_for_role(role: &str) -> bool {
    matches!(role, "all" | "api")
}

pub fn max_request_size_bytes() -> usize {
    env_i64("API_MAX_REQUEST_SIZE", 10 * 1024 * 1024).max(1) as usize
}

pub fn is_distributed_mode_from(role: &str, app_distributed_mode: Option<&str>) -> bool {
    if app_distributed_mode.is_some() {
        return parse_bool_like(app_distributed_mode, false);
    }

    matches!(role, "api" | "worker")
}

pub fn is_redis_required_from(role: &str, app_distributed_mode: Option<&str>, redis_required: Option<&str>) -> bool {
    if redis_required.is_some() {
        return parse_bool_like(redis_required, false);
    }

    is_distributed_mode_from(role, app_distributed_mode)
}

pub fn is_redis_required_for_role(role: &str) -> bool {
    let app_distributed_mode = std::env::var("APP_DISTRIBUTED_MODE").ok();
    let redis_required = std::env::var("REDIS_REQUIRED").ok();
    is_redis_required_from(role, app_distributed_mode.as_deref(), redis_required.as_deref())
}

pub fn build_request_size_error_response(
    response_request_id: &str,
    max_size: usize,
    actual_size: usize,
) -> HttpResponse {
    let message = format!("请求体过大。最大允许大小：{max_size}字节，当前大小：{actual_size}字节");
    tracing::warn!(
        request_id = response_request_id,
        max_size = max_size,
        actual_size = actual_size,
        "Request size exceeds configured limit"
    );

    let mut response = HttpResponse::PayloadTooLarge().json(ApiErrorResponse::new(413, message));
    if let Ok(header_value) = HeaderValue::from_str(response_request_id) {
        response
            .headers_mut()
            .insert(HeaderName::from_static("x-request-id"), header_value);
    }
    response
}

pub fn load_required_vault_rendered_env(required_keys: &[&str]) -> io::Result<String> {
    let rendered_env_file = env_optional_string("VAULT_RENDERED_ENV_FILE").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "VAULT_RENDERED_ENV_FILE must be set before starting the runtime",
        )
    })?;

    dotenvy::from_path_override(&rendered_env_file).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Failed to load Vault rendered env file {}: {error}", rendered_env_file),
        )
    })?;

    let missing: Vec<&str> = required_keys
        .iter()
        .copied()
        .filter(|key| env_optional_string(key).is_none())
        .collect();
    if !missing.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Vault rendered env file is missing required keys: {}",
                missing.join(", ")
            ),
        ));
    }

    Ok(rendered_env_file)
}

pub fn resolve_jwt_secret() -> io::Result<String> {
    for key in ["JWT_SECRET", "JWT_SECRET_KEY"] {
        if let Ok(v) = std::env::var(key) {
            let trimmed = v.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "JWT_SECRET or JWT_SECRET_KEY must be set via Vault rendered env",
    ))
}

pub fn resolve_workflow_internal_token() -> io::Result<Option<String>> {
    let environment = runtime_environment();
    resolve_workflow_internal_token_for_environment(
        std::env::var("WORKFLOW_INTERNAL_TOKEN").ok().as_deref(),
        environment.as_deref(),
    )
}

pub fn resolve_workflow_internal_token_for_environment(
    raw_value: Option<&str>,
    environment: Option<&str>,
) -> io::Result<Option<String>> {
    let token = raw_value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    if token.is_none() && is_production_environment(environment) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "WORKFLOW_INTERNAL_TOKEN must be set explicitly in production",
        ));
    }

    Ok(token)
}

pub fn resolve_jwt_audiences_from_env(raw_value: Option<&str>) -> Vec<String> {
    raw_value
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn resolve_jwt_audiences_for_environment(
    raw_value: Option<&str>,
    environment: Option<&str>,
) -> io::Result<Vec<String>> {
    let audiences = resolve_jwt_audiences_from_env(raw_value);
    if audiences.is_empty() && is_production_environment(environment) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "JWT_AUDIENCE must be set explicitly in production",
        ));
    }
    Ok(audiences)
}

pub fn resolve_jwt_audiences() -> io::Result<Vec<String>> {
    let environment = runtime_environment();
    resolve_jwt_audiences_for_environment(std::env::var("JWT_AUDIENCE").ok().as_deref(), environment.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{
        body::to_bytes,
        http::{header::HeaderMap, StatusCode},
    };

    #[actix_web::test]
    async fn runtime_role_normalizes_aliases_and_invalid_values() {
        assert_eq!(normalize_runtime_role("combined"), "all");
        assert_eq!(normalize_runtime_role("worker"), "worker");
        assert_eq!(normalize_runtime_role("invalid"), "all");
        assert_eq!(normalize_runtime_role(""), "all");
    }

    #[actix_web::test]
    async fn runtime_role_ownership_matches_python_contract() {
        assert!(should_start_background_jobs_for_role("all"));
        assert!(should_start_background_jobs_for_role("worker"));
        assert!(!should_start_background_jobs_for_role("api"));

        assert!(should_start_http_server_for_role("all"));
        assert!(should_start_http_server_for_role("api"));
        assert!(!should_start_http_server_for_role("worker"));

        assert!(parse_bool_like(Some("true"), false));
        assert!(parse_bool_like(Some("YES"), false));
        assert!(!parse_bool_like(Some("false"), true));
        assert!(!parse_bool_like(None, false));

        assert!(is_distributed_mode_from("api", None));
        assert!(is_distributed_mode_from("worker", None));
        assert!(!is_distributed_mode_from("all", None));
        assert!(!is_distributed_mode_from("api", Some("false")));

        assert!(is_redis_required_from("api", None, None));
        assert!(is_redis_required_from("worker", None, None));
        assert!(!is_redis_required_from("all", None, None));
        assert!(!is_redis_required_from("worker", Some("true"), Some("false")));

        assert_eq!(max_request_size_bytes(), 10 * 1024 * 1024);
    }

    #[test]
    fn http2_windows_default_to_one_megabyte_and_are_env_overridable() {
        let defaults = HttpTlsPerformanceConfig::default();
        assert_eq!(defaults.http2_initial_stream_window_size, 1 << 20);
        assert_eq!(defaults.http2_initial_connection_window_size, 1 << 20);

        std::env::set_var("HTTP2_INITIAL_STREAM_WINDOW_SIZE", "65535");
        std::env::set_var("HTTP2_INITIAL_CONNECTION_WINDOW_SIZE", "131072");
        let overridden = resolve_http_tls_performance_config();
        std::env::remove_var("HTTP2_INITIAL_STREAM_WINDOW_SIZE");
        std::env::remove_var("HTTP2_INITIAL_CONNECTION_WINDOW_SIZE");
        assert_eq!(overridden.http2_initial_stream_window_size, 65535);
        assert_eq!(overridden.http2_initial_connection_window_size, 131072);
    }

    #[actix_web::test]
    async fn request_size_error_response_matches_python_413_envelope_shape() {
        let response = build_request_size_error_response("req-test", 10, 20);

        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(response.headers().get("x-request-id").unwrap(), "req-test");

        let body = to_bytes(response.into_body()).await.expect("read body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json body");
        assert_eq!(payload["success"], false);
        assert_eq!(payload["error"]["code"], "HTTP_413");
        assert_eq!(payload["error"]["type"], "http_error");
        assert_eq!(
            payload["error"]["message"],
            "请求体过大。最大允许大小：10字节，当前大小：20字节"
        );
    }

    #[actix_web::test]
    async fn database_url_defaults_extract_replication_connection_fields() {
        let defaults =
            DatabaseUrlDefaults::from_database_url("postgres://fm_replicator:secret@localhost:5433/flight_monitor_dev");

        assert_eq!(defaults.host.as_deref(), Some("localhost"));
        assert_eq!(defaults.port, Some(5433));
        assert_eq!(defaults.database.as_deref(), Some("flight_monitor_dev"));
        assert_eq!(defaults.user.as_deref(), Some("fm_replicator"));
        assert_eq!(defaults.password.as_deref(), Some("secret"));
    }

    #[test]
    fn normalize_cors_origin_rejects_non_origin_shapes() {
        assert_eq!(
            normalize_cors_origin("https://LOCALHOST:3000/"),
            Some("https://localhost:3000".to_string())
        );
        assert_eq!(
            normalize_cors_origin("http://127.0.0.1"),
            Some("http://127.0.0.1".to_string())
        );
        assert_eq!(normalize_cors_origin("ftp://localhost:3000"), None);
        assert_eq!(normalize_cors_origin("https://localhost:3000/path"), None);
        assert_eq!(normalize_cors_origin("https://localhost:3000?x=1"), None);
    }

    #[test]
    fn cors_origin_rules_require_explicit_allowed_origins() {
        let allowed_origins =
            load_cors_allowed_origins_from_env(Some("https://ops.example.com, http://127.0.0.1:3000"))
                .expect("cors origins should parse");

        assert!(is_cors_origin_allowed("https://ops.example.com", &allowed_origins));
        assert!(is_cors_origin_allowed("http://127.0.0.1:3000", &allowed_origins));
        assert!(!is_cors_origin_allowed("http://192.168.1.10:3000", &allowed_origins));
        assert!(!is_cors_origin_allowed(
            "https://ops.example.com/path",
            &allowed_origins
        ));
    }

    #[test]
    fn cors_origin_loader_rejects_invalid_entries() {
        let error = load_cors_allowed_origins_from_env(Some("https://ops.example.com,https://bad.example.com/path"))
            .expect_err("invalid path entry should be rejected");
        assert!(error
            .to_string()
            .contains("Invalid CORS origin: https://bad.example.com/path"));
    }

    #[test]
    fn cors_origin_loader_requires_explicit_origins_in_production() {
        let error = load_cors_allowed_origins_for_environment(None, Some("production"))
            .expect_err("production should not fall back to development CORS origins");

        assert!(error
            .to_string()
            .contains("CORS_ALLOWED_ORIGINS must be set explicitly in production"));
    }

    #[test]
    fn cors_origin_loader_keeps_development_default_for_local_runtime() {
        let allowed_origins = load_cors_allowed_origins_for_environment(None, Some("development"))
            .expect("development default CORS origins should load");

        assert!(allowed_origins.iter().any(|origin| origin == "http://localhost:3000"));
    }

    #[test]
    fn standard_security_headers_include_core_browser_protections() {
        let mut headers = HeaderMap::new();

        insert_standard_security_headers(&mut headers, false, false);

        assert_eq!(
            headers.get("x-content-type-options").unwrap(),
            X_CONTENT_TYPE_OPTIONS_VALUE
        );
        assert_eq!(headers.get("x-frame-options").unwrap(), X_FRAME_OPTIONS_VALUE);
        assert_eq!(headers.get("referrer-policy").unwrap(), REFERRER_POLICY_VALUE);
        assert_eq!(headers.get("permissions-policy").unwrap(), PERMISSIONS_POLICY_VALUE);
        assert_eq!(
            headers.get("content-security-policy").unwrap(),
            CONTENT_SECURITY_POLICY_VALUE
        );
        assert!(headers.get("strict-transport-security").is_none());
    }

    #[test]
    fn standard_security_headers_add_hsts_only_for_secure_requests() {
        let mut headers = HeaderMap::new();

        insert_standard_security_headers(&mut headers, true, false);

        assert_eq!(
            headers.get("strict-transport-security").unwrap(),
            STRICT_TRANSPORT_SECURITY_VALUE
        );
    }

    #[test]
    fn production_csp_removes_unsafe_inline_and_unsafe_eval() {
        let mut headers = HeaderMap::new();

        insert_standard_security_headers(&mut headers, true, true);

        let csp = headers.get("content-security-policy").unwrap().to_str().unwrap();
        assert!(!csp.contains("unsafe-inline"));
        assert!(!csp.contains("unsafe-eval"));
        assert!(csp.contains("script-src 'self'"));
        assert!(csp.contains("style-src 'self'"));
    }

    #[test]
    fn development_csp_keeps_unsafe_inline_and_unsafe_eval() {
        let mut headers = HeaderMap::new();

        insert_standard_security_headers(&mut headers, false, false);

        let csp = headers.get("content-security-policy").unwrap().to_str().unwrap();
        assert!(csp.contains("'unsafe-inline'"));
        assert!(csp.contains("'unsafe-eval'"));
    }

    #[test]
    fn redis_url_resolution_requires_explicit_url_when_redis_is_required() {
        let error = resolve_redis_url_from_env(None, true).expect_err("missing REDIS_URL should fail");
        assert!(error
            .to_string()
            .contains("REDIS_URL must be set when Redis is required"));
    }

    #[test]
    fn redis_url_resolution_requires_explicit_url_even_when_redis_is_optional() {
        let error = resolve_redis_url_from_env(None, false).expect_err("missing REDIS_URL should fail");
        assert!(error
            .to_string()
            .contains("REDIS_URL must be set before starting the runtime"));
        assert_eq!(
            resolve_redis_url_from_env(Some(" redis://:secret@redis:6379/0 "), true).expect("explicit redis url"),
            "redis://:secret@redis:6379/0"
        );
    }

    #[test]
    fn redact_url_credentials_removes_userinfo_secrets() {
        assert_eq!(
            redact_url_credentials("redis://:secret@redis:6379/0"),
            "redis://:redacted@redis:6379/0"
        );
        assert_eq!(
            redact_url_credentials("postgres://user:secret@db.example.com/app"),
            "postgres://redacted:redacted@db.example.com/app"
        );
        assert_eq!(redact_url_credentials("not a url"), "<redacted-url>");
    }

    #[test]
    fn jwt_audience_resolution_ignores_empty_entries() {
        assert_eq!(
            resolve_jwt_audiences_from_env(Some(" flight-monitor-api, mobile-api ,, ")),
            vec!["flight-monitor-api".to_string(), "mobile-api".to_string()]
        );
        assert!(resolve_jwt_audiences_from_env(None).is_empty());
    }

    #[test]
    fn jwt_audience_resolution_requires_explicit_audience_in_production() {
        let error = resolve_jwt_audiences_for_environment(None, Some("production"))
            .expect_err("production should not disable JWT audience validation");

        assert!(error
            .to_string()
            .contains("JWT_AUDIENCE must be set explicitly in production"));
    }

    #[test]
    fn jwt_audience_resolution_keeps_development_compatibility() {
        let audiences = resolve_jwt_audiences_for_environment(None, Some("development"))
            .expect("development compatibility should allow empty audience");

        assert!(audiences.is_empty());
    }

    #[test]
    fn http_tls_binding_config_defaults_and_overrides_match_host_runtime_contract() {
        assert_eq!(resolve_http_tls_binding_config(None, None, None), None);
        assert_eq!(resolve_http_tls_binding_config(Some("false"), None, None), None);

        let default_tls = resolve_http_tls_binding_config(Some("true"), None, None).expect("tls config when enabled");
        assert_eq!(default_tls.cert_file, "certs/server.crt");
        assert_eq!(default_tls.key_file, "certs/server.key");

        let override_tls =
            resolve_http_tls_binding_config(Some("1"), Some("custom/server.crt"), Some("custom/server.key"))
                .expect("override tls config when enabled");
        assert_eq!(override_tls.cert_file, "custom/server.crt");
        assert_eq!(override_tls.key_file, "custom/server.key");
    }

    #[test]
    fn workflow_internal_token_required_in_production() {
        let error = resolve_workflow_internal_token_for_environment(None, Some("production"))
            .expect_err("production should require WORKFLOW_INTERNAL_TOKEN");
        assert!(error
            .to_string()
            .contains("WORKFLOW_INTERNAL_TOKEN must be set explicitly in production"));

        let error = resolve_workflow_internal_token_for_environment(Some(""), Some("production"))
            .expect_err("empty token should also fail in production");
        assert!(error
            .to_string()
            .contains("WORKFLOW_INTERNAL_TOKEN must be set explicitly in production"));

        let error = resolve_workflow_internal_token_for_environment(Some("  "), Some("production"))
            .expect_err("whitespace-only token should also fail in production");
        assert!(error
            .to_string()
            .contains("WORKFLOW_INTERNAL_TOKEN must be set explicitly in production"));
    }

    #[test]
    fn workflow_internal_token_allows_none_in_development() {
        let token = resolve_workflow_internal_token_for_environment(None, Some("development"))
            .expect("dev should allow missing token");
        assert!(token.is_none());

        let token = resolve_workflow_internal_token_for_environment(None, Some("test"))
            .expect("test should allow missing token");
        assert!(token.is_none());
    }

    #[test]
    fn workflow_internal_token_returns_trimmed_value_when_present() {
        let token = resolve_workflow_internal_token_for_environment(Some("  my-secret-token  "), Some("production"))
            .expect("valid token should parse")
            .expect("token should be present");
        assert_eq!(token, "my-secret-token");
    }

    #[test]
    fn runtime_environment_enum_parses_known_development_values() {
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("development")),
            RuntimeEnvironment::Development
        );
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("dev")),
            RuntimeEnvironment::Development
        );
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("test")),
            RuntimeEnvironment::Development
        );
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("testing")),
            RuntimeEnvironment::Development
        );
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("local")),
            RuntimeEnvironment::Development
        );
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("localhost")),
            RuntimeEnvironment::Development
        );
    }

    #[test]
    fn runtime_environment_enum_defaults_to_production_for_unknown_values() {
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("production")),
            RuntimeEnvironment::Production
        );
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("prod")),
            RuntimeEnvironment::Production
        );
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("staging")),
            RuntimeEnvironment::Production
        );
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("unknown_typo")),
            RuntimeEnvironment::Production
        );
        assert_eq!(RuntimeEnvironment::from_env_value(None), RuntimeEnvironment::Production);
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("")),
            RuntimeEnvironment::Production
        );
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("  ")),
            RuntimeEnvironment::Production
        );
    }

    #[test]
    fn runtime_environment_enum_is_case_insensitive() {
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("DEVELOPMENT")),
            RuntimeEnvironment::Development
        );
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("Development")),
            RuntimeEnvironment::Development
        );
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("PRODUCTION")),
            RuntimeEnvironment::Production
        );
    }

    #[test]
    fn runtime_environment_enum_as_str_returns_canonical_name() {
        assert_eq!(RuntimeEnvironment::Development.as_str(), "development");
        assert_eq!(RuntimeEnvironment::Production.as_str(), "production");
    }

    #[test]
    fn runtime_environment_enum_is_production_matches_from_env_value() {
        assert!(RuntimeEnvironment::from_env_value(None).is_production());
        assert!(RuntimeEnvironment::from_env_value(Some("production")).is_production());
        assert!(RuntimeEnvironment::from_env_value(Some("unknown")).is_production());
        assert!(!RuntimeEnvironment::from_env_value(Some("development")).is_production());
        assert!(!RuntimeEnvironment::from_env_value(Some("test")).is_production());
    }
}

// ============================================================
// Tokio Runtime Configuration Helpers
// ============================================================

/// 解析自定义 Tokio runtime 配置
/// 默认值：CPU 核心数 * 2（适用于 IO 密集型场景）
#[allow(dead_code)]
pub fn get_tokio_worker_threads() -> usize {
    std::env::var("TOKIO_WORKER_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map(|v: usize| v.max(4))
        .unwrap_or_else(|| {
            let cpu_cores = num_cpus::get();
            cpu_cores.saturating_mul(2) // 默认为 CPU 核心数 * 2
        })
}

/// 解析最大 blocking 线程数
#[allow(dead_code)]
pub fn get_max_blocking_threads() -> usize {
    std::env::var("TOKIO_MAX_BLOCKING_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| num_cpus::get() * 8) // 默认 CPU 核心数 * 8
}
