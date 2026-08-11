//! 认证路由
//!
//! 对应 Python auth_routes.py (30 endpoints) — 全覆盖。

pub(crate) use actix_web::{
    cookie::{time::Duration as CookieDuration, Cookie, SameSite},
    http::header,
    http::StatusCode,
    web, HttpRequest, HttpResponse,
};
pub(crate) use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
pub(crate) use dashmap::DashMap;
pub(crate) use fms_application::schemas::response::ApiErrorResponse;
pub(crate) use fms_domain::error::DomainError;
pub(crate) use serde_json::json;
pub(crate) use std::sync::Arc;
pub(crate) use std::time::{Duration, Instant};

pub(crate) use crate::error::ApiError;
pub(crate) use crate::middleware::jwt::JwtAuth;
pub(crate) use crate::request_context::{
    build_ip_subnet_hash, build_user_agent_hash, extract_client_ip, extract_user_agent,
};
pub(crate) use crate::services::performance_metrics::PerformanceMetricsService;
pub(crate) use fms_application::schemas::auth_schemas::{RefreshTokenRequest, RoleUpdate, UserCreate, UserLogin};
pub(crate) use fms_application::services::auth_service::AuthService;
pub(crate) use fms_application::services::online_status_service::OnlineStatusService;
pub(crate) use fms_application::services::operator_identity_service::OperatorIdentityService;
pub(crate) const DEFAULT_LOGIN_FAILURE_LIMIT: u32 = 5;
pub(crate) const DEFAULT_LOGIN_FAILURE_WINDOW_SECS: u64 = 15 * 60;
pub(crate) const ACCESS_TOKEN_COOKIE: &str = "access_token";
pub(crate) const REFRESH_TOKEN_COOKIE: &str = "refresh_token";
pub(crate) const DEFAULT_REFRESH_COOKIE_DAYS: i64 = 7;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginRateLimitDecision {
    Allowed,
    Limited { retry_after_secs: u64 },
}

#[derive(Debug, Clone)]
pub(crate) struct LoginFailureBucket {
    pub(crate) first_failure_at: Instant,
    pub(crate) failures: u32,
}

#[derive(Debug)]
pub struct LoginFailureRateLimiter {
    pub(crate) buckets: DashMap<String, LoginFailureBucket>,
    pub(crate) max_failures: u32,
    pub(crate) window: Duration,
}

/// Client surface protocol for **token delivery shape only**.
///
/// This header selects response field omission; it is **not** an authentication
/// or authorization boundary and must not be treated as proof of client identity.
/// A malicious browser can send `X-Client-Surface: native`. Default remains Web
/// so ordinary browser sessions still omit long-lived secrets from JSON.
///
/// - `Web` (default): refresh_token is HttpOnly cookie only; JSON omits
///   `refresh_token` and `session_secret`. SPA anti-replay may read
///   `session_secret` from a short-lived non-HttpOnly cookie.
/// - `Native` (`X-Client-Surface: native`): JSON includes refresh/sse/session secrets
///   for encrypted mobile storage; cookies are still set for compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClientSurface {
    Web,
    Native,
}

pub(crate) const CLIENT_SURFACE_HEADER: &str = "X-Client-Surface";
pub(crate) const SESSION_SECRET_COOKIE: &str = "session_secret";

/// Present only to detect and reject the exact query key `refresh_token`.
#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct RefreshTokenQueryReject {
    pub(crate) refresh_token: Option<String>,
}

/// Resolve refresh token from body or cookie only — never from query.
pub(crate) fn resolve_refresh_token_sources(body_token: Option<&str>, cookie_token: Option<&str>) -> Option<String> {
    body_token
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            cookie_token
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
}

#[derive(serde::Deserialize)]
pub(crate) struct ProfileUpdate {
    pub(crate) display_name: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct OperatorContextUpdate {
    pub(crate) operator_name: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct OnlineHistoryQuery {
    pub(crate) user_id: Option<String>,
    pub(crate) start_date: Option<String>,
    pub(crate) end_date: Option<String>,
    pub(crate) page: Option<i64>,
    pub(crate) page_size: Option<i64>,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct PageQuery {
    pub(crate) page: Option<i64>,
    pub(crate) page_size: Option<i64>,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct OnlineUsersQuery {
    pub(crate) include_status: Option<bool>,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct PermissionAssignQuery {
    pub(crate) permission: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct PermissionAssignBody {
    pub(crate) permission: Option<String>,
}

impl Default for LoginFailureRateLimiter {
    fn default() -> Self {
        Self::with_policy(
            DEFAULT_LOGIN_FAILURE_LIMIT,
            Duration::from_secs(DEFAULT_LOGIN_FAILURE_WINDOW_SECS),
        )
    }
}

impl LoginFailureRateLimiter {
    pub fn with_policy(max_failures: u32, window: Duration) -> Self {
        Self {
            buckets: DashMap::new(),
            max_failures: max_failures.max(1),
            window: window.max(Duration::from_secs(1)),
        }
    }

    pub fn check(&self, key: &str) -> LoginRateLimitDecision {
        let Some(bucket) = self.buckets.get(key) else {
            return LoginRateLimitDecision::Allowed;
        };

        let elapsed = bucket.first_failure_at.elapsed();
        if elapsed >= self.window {
            drop(bucket);
            self.buckets.remove(key);
            return LoginRateLimitDecision::Allowed;
        }

        if bucket.failures < self.max_failures {
            return LoginRateLimitDecision::Allowed;
        }

        LoginRateLimitDecision::Limited {
            retry_after_secs: self.retry_after_secs(elapsed),
        }
    }

    pub fn record_login_error(&self, key: &str, error: &DomainError) {
        if !matches!(error, DomainError::Unauthorized(_)) {
            return;
        }

        let now = Instant::now();
        self.buckets
            .entry(key.to_string())
            .and_modify(|bucket| {
                if bucket.first_failure_at.elapsed() >= self.window {
                    bucket.first_failure_at = now;
                    bucket.failures = 1;
                } else {
                    bucket.failures = bucket.failures.saturating_add(1);
                }
            })
            .or_insert(LoginFailureBucket {
                first_failure_at: now,
                failures: 1,
            });
    }

    pub fn record_login_success(&self, key: &str) {
        self.buckets.remove(key);
    }

    fn retry_after_secs(&self, elapsed: Duration) -> u64 {
        self.window.saturating_sub(elapsed).as_secs().max(1)
    }
}

pub(crate) fn ok_resp(data: impl serde::Serialize) -> HttpResponse {
    HttpResponse::Ok().json(json!({ "success": true, "data": data }))
}

pub(crate) fn auth_resp(message: &str) -> HttpResponse {
    HttpResponse::Ok().json(json!({ "success": true, "message": message, "data": null }))
}

pub(crate) fn build_auth_cookie(name: &str, value: &str, max_age_secs: i64) -> Cookie<'static> {
    Cookie::build(name.to_string(), value.to_string())
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .max_age(CookieDuration::seconds(max_age_secs.max(0)))
        .path("/")
        .finish()
}

pub(crate) fn build_clear_cookie(name: &str) -> Cookie<'static> {
    Cookie::build(name.to_string(), "")
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .max_age(CookieDuration::seconds(0))
        .path("/")
        .finish()
}

pub(crate) fn parse_client_surface(req: &HttpRequest) -> ClientSurface {
    match req
        .headers()
        .get(CLIENT_SURFACE_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
    {
        Some(value) if value.eq_ignore_ascii_case("native") => ClientSurface::Native,
        _ => ClientSurface::Web,
    }
}

pub(crate) fn normalize_login_username(username: &str) -> String {
    username.trim().to_ascii_lowercase()
}

/// Rate-limit key: normalized username + real client IP (trusted-proxy aware).
pub(crate) fn login_rate_limit_key(username: &str, req: &HttpRequest) -> String {
    let user = normalize_login_username(username);
    let user = if user.is_empty() { "unknown".to_string() } else { user };
    let ip = extract_client_ip(req).unwrap_or_else(|| "unknown".to_string());
    format!("{user}|{ip}")
}

fn build_session_secret_cookie(value: &str, max_age_secs: i64) -> Cookie<'static> {
    // Intentionally NOT HttpOnly: browser SPA must read it for anti-replay HMAC.
    // It is short-lived (access token TTL), Secure, and SameSite=Lax.
    Cookie::build(SESSION_SECRET_COOKIE.to_string(), value.to_string())
        .http_only(false)
        .secure(true)
        .same_site(SameSite::Lax)
        .max_age(CookieDuration::seconds(max_age_secs.max(0)))
        .path("/")
        .finish()
}

pub(crate) fn attach_auth_cookies(
    response: &mut HttpResponse,
    token: &fms_application::schemas::auth_schemas::Token,
    surface: ClientSurface,
) -> Result<(), ApiError> {
    response
        .add_cookie(&build_auth_cookie(
            ACCESS_TOKEN_COOKIE,
            &token.access_token,
            token.expires_in,
        ))
        .map_err(|_| ApiError::Internal("设置认证 cookie 失败".into()))?;

    if let Some(refresh_token) = token.refresh_token.as_deref() {
        response
            .add_cookie(&build_auth_cookie(
                REFRESH_TOKEN_COOKIE,
                refresh_token,
                DEFAULT_REFRESH_COOKIE_DAYS * 24 * 60 * 60,
            ))
            .map_err(|_| ApiError::Internal("设置刷新 cookie 失败".into()))?;
    }

    // Web clients receive session_secret via a non-HttpOnly cookie (not JSON).
    if surface == ClientSurface::Web {
        if let Some(session_secret) = token.session_secret.as_deref() {
            response
                .add_cookie(&build_session_secret_cookie(session_secret, token.expires_in))
                .map_err(|_| ApiError::Internal("设置会话签名 cookie 失败".into()))?;
        }
    }

    Ok(())
}

/// JSON body policy by client surface: web omits long-lived secrets.
pub(crate) fn token_json_for_surface(
    token: &fms_application::schemas::auth_schemas::Token,
    surface: ClientSurface,
) -> fms_application::schemas::auth_schemas::Token {
    match surface {
        ClientSurface::Native => token.clone(),
        ClientSurface::Web => fms_application::schemas::auth_schemas::Token {
            access_token: token.access_token.clone(),
            token_type: token.token_type.clone(),
            expires_in: token.expires_in,
            refresh_token: None,
            sse_token: token.sse_token.clone(),
            sse_expires_in: token.sse_expires_in,
            session_secret: None,
        },
    }
}

pub(crate) fn attach_token_response_cookies(
    token: &fms_application::schemas::auth_schemas::Token,
    surface: ClientSurface,
) -> Result<HttpResponse, ApiError> {
    let body = token_json_for_surface(token, surface);
    let mut response = HttpResponse::Ok().json(body);
    attach_auth_cookies(&mut response, token, surface)?;
    Ok(response)
}

pub(crate) fn login_rate_limited_response(retry_after_secs: u64) -> HttpResponse {
    HttpResponse::build(StatusCode::TOO_MANY_REQUESTS)
        .insert_header((header::RETRY_AFTER, retry_after_secs.to_string()))
        .json(ApiErrorResponse::with_details(
            StatusCode::TOO_MANY_REQUESTS.as_u16(),
            "Too many failed login attempts. Please retry later.",
            json!({
                "retry_after_secs": retry_after_secs,
            }),
        ))
}

pub(crate) fn parse_register_payload(body: &[u8]) -> Result<UserCreate, ApiError> {
    let raw_value: serde_json::Value =
        serde_json::from_slice(body).map_err(|_error| ApiError::ValidationErrorWithDetails {
            message: "输入验证失败".to_string(),
            details: json!([
                {
                    "type": "json_invalid",
                    "loc": ["body"],
                    "msg": "JSON decode error",
                    "input": serde_json::Value::Null,
                }
            ]),
        })?;

    let object = raw_value
        .as_object()
        .ok_or_else(|| ApiError::ValidationErrorWithDetails {
            message: "输入验证失败".to_string(),
            details: json!([
                {
                    "type": "model_attributes_type",
                    "loc": ["body"],
                    "msg": "Input should be a valid dictionary",
                    "input": raw_value.clone(),
                }
            ]),
        })?;

    let missing_fields = ["username", "password"]
        .into_iter()
        .filter(|field| !object.contains_key(*field))
        .collect::<Vec<_>>();
    if !missing_fields.is_empty() {
        let details = missing_fields
            .into_iter()
            .map(|field| {
                json!({
                    "type": "missing",
                    "loc": ["body", field],
                    "msg": "Field required",
                    "input": raw_value.clone(),
                })
            })
            .collect::<Vec<_>>();
        return Err(ApiError::ValidationErrorWithDetails {
            message: "输入验证失败".to_string(),
            details: serde_json::Value::Array(details),
        });
    }

    serde_json::from_value(raw_value).map_err(|error| ApiError::ValidationErrorWithDetails {
        message: "输入验证失败".to_string(),
        details: json!([
            {
                "type": "json_invalid",
                "loc": ["body"],
                "msg": error.to_string(),
                "input": serde_json::Value::Null,
            }
        ]),
    })
}

pub(crate) async fn maybe_enrich_user_response(
    user: fms_application::schemas::auth_schemas::UserResponse,
    operator_identity_svc: Option<web::Data<Arc<OperatorIdentityService>>>,
    req: Option<&HttpRequest>,
) -> Result<fms_application::schemas::auth_schemas::UserResponse, ApiError> {
    let Some(operator_identity_svc) = operator_identity_svc else {
        return Ok(user);
    };

    let (context_type, context_id) = if let Some(req) = req {
        extract_optional_operator_context(req, operator_identity_svc.get_ref().as_ref())?
    } else {
        (None, None)
    };

    operator_identity_svc
        .enrich_user_response(user, context_type.as_deref(), context_id.as_deref())
        .await
        .map_err(ApiError::from)
}

pub(crate) fn extract_operator_context(
    req: &HttpRequest,
    svc: &OperatorIdentityService,
) -> Result<(String, String), ApiError> {
    let (context_type, context_id) = extract_optional_operator_context(req, svc)?;
    match (context_type, context_id) {
        (Some(context_type), Some(context_id)) => Ok((context_type, context_id)),
        _ => Err(ApiError::BadRequest("operator context headers are required".into())),
    }
}

pub(crate) fn extract_optional_operator_context(
    req: &HttpRequest,
    svc: &OperatorIdentityService,
) -> Result<(Option<String>, Option<String>), ApiError> {
    let context_type = req
        .headers()
        .get("X-Operator-Context-Type")
        .and_then(|value| value.to_str().ok());
    let context_id = req
        .headers()
        .get("X-Operator-Context-Id")
        .and_then(|value| value.to_str().ok());
    svc.normalize_context(context_type, context_id).map_err(ApiError::from)
}

pub(crate) fn ensure_admin(claims: &JwtAuth) -> Result<(), ApiError> {
    if claims.0.is_admin.unwrap_or(false) {
        return Ok(());
    }
    Err(ApiError::Forbidden("需要管理员权限".into()))
}

pub(crate) fn parse_online_history_date(
    value: Option<&str>,
    field_name: &str,
) -> Result<Option<DateTime<Utc>>, ApiError> {
    let normalized = value.unwrap_or("").trim();
    if normalized.is_empty() {
        return Ok(None);
    }

    if let Ok(parsed) = DateTime::parse_from_rfc3339(normalized) {
        return Ok(Some(parsed.with_timezone(&Utc)));
    }
    if let Ok(parsed) = NaiveDateTime::parse_from_str(normalized, "%Y-%m-%dT%H:%M:%S") {
        return Ok(Some(DateTime::<Utc>::from_naive_utc_and_offset(parsed, Utc)));
    }
    if let Ok(parsed) = NaiveDateTime::parse_from_str(normalized, "%Y-%m-%d %H:%M:%S") {
        return Ok(Some(DateTime::<Utc>::from_naive_utc_and_offset(parsed, Utc)));
    }
    if let Ok(parsed) = NaiveDate::parse_from_str(normalized, "%Y-%m-%d") {
        let datetime = parsed.and_hms_opt(0, 0, 0).ok_or_else(|| {
            ApiError::BadRequest(format!(
                "Invalid {field_name} format. Use ISO format (YYYY-MM-DDTHH:MM:SS)"
            ))
        })?;
        return Ok(Some(DateTime::<Utc>::from_naive_utc_and_offset(datetime, Utc)));
    }

    Err(ApiError::BadRequest(format!(
        "Invalid {field_name} format. Use ISO format (YYYY-MM-DDTHH:MM:SS)"
    )))
}

pub(crate) fn validate_user_id_path_response(user_id: &str) -> Option<HttpResponse> {
    let length = user_id.chars().count();
    if length != 26 {
        let message = format!("UserID 验证失败: 用户ID长度必须为26字符，当前长度: {length}");
        return Some(user_id_validation_error_response(&message));
    }

    let valid_ulid = user_id.chars().all(is_valid_ulid_char);
    if !valid_ulid {
        return Some(user_id_validation_error_response(
            "UserID 验证失败: 用户ID格式错误，必须为有效的ULID格式（26字符Crockford Base32）",
        ));
    }

    None
}

pub(crate) fn user_id_validation_error_response(message: &str) -> HttpResponse {
    HttpResponse::UnprocessableEntity().json(json!({
        "success": false,
        "error": {
            "code": "VALUE_OBJECT_VALIDATION_ERROR",
            "message": message,
            "details": {
                "field": "value",
                "value": serde_json::Value::Null,
                "field_errors": {
                    "value": [message]
                }
            },
            "timestamp": Utc::now().to_rfc3339(),
            "type": "domain_validation_error"
        }
    }))
}

pub(crate) fn is_valid_ulid_char(ch: char) -> bool {
    matches!(ch, '0'..='9' | 'A'..='H' | 'J'..='K' | 'M'..='N' | 'P'..='T' | 'V'..='Z')
}
