//! JWT 认证中间件
//!
//! 提供 `JwtAuth` extractor 用于保护需要认证的路由。
//! 使用方式：在 handler 参数中加入 `claims: JwtAuth` 即可自动验证。

use actix_web::dev::Payload;
use actix_web::{web, FromRequest, HttpRequest};
use futures_util::future::LocalBoxFuture;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Instant;
use tracing::warn;

use crate::error::ApiError;
use crate::request_context::{build_ip_subnet_hash, build_user_agent_hash, extract_client_ip, extract_user_agent};
use crate::services::performance_metrics::PerformanceMetricsService;
use fms_application::schemas::auth_schemas::TokenData;
use fms_application::services::auth_service::AuthService;
use fms_application::services::auth_validation_cache::AuthValidationCache;

const QUERY_TOKEN_ALLOWED_PREFIXES: &[&str] = &[
    "/api/v2/sse/stream",
    "/api/v2/flights/stream",
    "/api/v2/ai/events/stream",
    "/api/v2/system/runtime/health/stream/status",
    // "/api/v2/health/stream/status",  // Rust 独有，已移除以与 Python 一致
    "/api/v2/notifications/stream",
    "/api/v2/dispatch/collaboration/stream",
    // "/api/v2/dispatch-chat/stream",  // Rust 独有，已移除以与 Python 一致
    "/api/v2/kpi/stream",
    "/api/v2/anomalies/stream",
    "/api/v2/ai/realtime/audio",
];
const SSE_QUERY_TOKEN_AUTH_ENV: &str = "SSE_QUERY_TOKEN_AUTH_ENABLED";
const ACCESS_TOKEN_COOKIE: &str = "access_token";

/// JWT 密钥共享状态 (通过 `web::Data` 注入)
#[derive(Clone)]
pub struct JwtSecret(pub String);

/// 可选 JWT audience 校验配置。
///
/// 未注入时保持兼容：不校验 aud，以兼容历史 Rust token。
#[derive(Clone, Default)]
pub struct JwtAudience(pub Vec<String>);

/// 开关：允许 EventSource / WebSocket 客户端通过 URL query 传递 `sse_token`。
///
/// 这类客户端无法设置 Authorization 头，只能带 query 参数；默认关闭，仅对
/// `QUERY_TOKEN_ALLOWED_PREFIXES` 内的 stream/audio 路径生效。
#[derive(Clone, Copy, Default)]
pub struct SseQueryTokenAuth(pub bool);

/// Shared secret for internal workflow trigger endpoint authentication.
///
/// In production this is required; in dev/test it may be None to allow
/// JWT-only access. Inject via `web::Data`.
#[derive(Clone, Default)]
pub struct WorkflowInternalToken(pub Option<String>);

/// 从请求中提取并验证 JWT claims 的 extractor。
///
/// # 使用示例
/// ```text
/// async fn me(claims: JwtAuth) -> HttpResponse {
///     HttpResponse::Ok().json(claims.0)
/// }
/// ```
pub struct JwtAuth(pub TokenData);

impl FromRequest for JwtAuth {
    type Error = ApiError;
    type Future = LocalBoxFuture<'static, Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let req = req.clone();
        Box::pin(async move { extract_jwt(&req).await })
    }
}

pub async fn extract_jwt(req: &HttpRequest) -> Result<JwtAuth, ApiError> {
    let metrics = req.app_data::<web::Data<Arc<PerformanceMetricsService>>>();

    let decode_start = Instant::now();
    let token = extract_request_token(req)?;

    let audience = req.app_data::<web::Data<JwtAudience>>().map(|data| data.get_ref());
    let config = load_jwt_verifier_config()?;
    let validation = build_jwt_validation_full(audience, config.algorithm, config.issuer.as_deref());
    let decoding_key = build_decoding_key(req, config)?;

    let decoded = decode::<TokenData>(&token.value, &decoding_key, &validation).map_err(|e| {
        warn!(error = %e, "JWT validation failed");
        ApiError::Unauthorized("认证令牌无效".into())
    })?;

    if let Some(m) = &metrics {
        m.record_latency(
            "auth.jwt.decode.duration_ms",
            decode_start.elapsed().as_secs_f64() * 1000.0,
        );
    }

    validate_token_kind(&decoded.claims, token.kind)?;
    validate_client_context(req, &decoded.claims)?;

    if let Some(auth_svc) = req.app_data::<web::Data<Arc<AuthService>>>() {
        let token_user_id = decoded.claims.sub.as_deref().unwrap_or("").to_string();
        let token_permission_version = decoded.claims.pv.unwrap_or(1);
        let freshness_session_key = token_value_hash(&token.value);

        let cache = req.app_data::<web::Data<Arc<AuthValidationCache>>>();

        let freshness_start = Instant::now();
        let freshness_valid = if let Some(cache_data) = &cache {
            if let Some(cached) = cache_data
                .get_cached_freshness(&token_user_id, &freshness_session_key)
                .await
            {
                cached
            } else {
                let valid = auth_svc.validate_access_claims_freshness(&decoded.claims).await.is_ok();
                cache_data
                    .set_cached_freshness(&token_user_id, &freshness_session_key, valid)
                    .await;
                valid
            }
        } else {
            auth_svc
                .validate_access_claims_freshness(&decoded.claims)
                .await
                .map_err(ApiError::from)?;
            true
        };
        if let Some(m) = &metrics {
            m.record_latency(
                "auth.freshness.check.duration_ms",
                freshness_start.elapsed().as_secs_f64() * 1000.0,
            );
        }

        if !freshness_valid {
            return Err(ApiError::Unauthorized("令牌已失效，请重新登录".into()));
        }

        let perm_start = Instant::now();
        let permission_valid = if let Some(cache_data) = &cache {
            if let Some(cached) = cache_data
                .get_cached_permission(&token_user_id, token_permission_version)
                .await
            {
                cached
            } else {
                let valid = auth_svc
                    .is_permission_version_current_async(&token_user_id, token_permission_version)
                    .await;
                cache_data
                    .set_cached_permission(&token_user_id, token_permission_version, valid)
                    .await;
                valid
            }
        } else {
            auth_svc
                .is_permission_version_current_async(&token_user_id, token_permission_version)
                .await
        };
        if let Some(m) = &metrics {
            m.record_latency(
                "auth.permission_version.check.duration_ms",
                perm_start.elapsed().as_secs_f64() * 1000.0,
            );
        }

        if !permission_valid {
            return Err(ApiError::Unauthorized("权限已变更，请重新登录".into()));
        }
    }

    Ok(JwtAuth(decoded.claims))
}

fn validate_client_context(req: &HttpRequest, claims: &TokenData) -> Result<(), ApiError> {
    if let Some(expected_ua_hash) = claims.ua_hash.as_deref() {
        let actual_ua_hash = build_user_agent_hash(extract_user_agent(req).as_deref());
        if expected_ua_hash != actual_ua_hash {
            return Err(ApiError::Unauthorized("客户端环境已变化，请重新登录".into()));
        }
    }

    if let Some(expected_ip_subnet_hash) = claims.ip_subnet_hash.as_deref() {
        let actual_ip_subnet_hash = build_ip_subnet_hash(extract_client_ip(req).as_deref());
        if expected_ip_subnet_hash != actual_ip_subnet_hash {
            return Err(ApiError::Unauthorized("客户端网络环境已变化，请重新登录".into()));
        }
    }

    Ok(())
}

fn extract_request_token(req: &HttpRequest) -> Result<RequestToken, ApiError> {
    if let Some(token) = extract_bearer_token(req)? {
        return Ok(token);
    }

    if let Some(token) = extract_cookie_token(req) {
        return Ok(token);
    }

    if is_query_token_auth_enabled(req) && is_query_token_allowed(req.path()) {
        if let Some(token) = extract_query_token(req) {
            return Ok(token);
        }
    }

    Err(ApiError::Unauthorized("缺少认证令牌".into()))
}

fn extract_cookie_token(req: &HttpRequest) -> Option<RequestToken> {
    req.cookie(ACCESS_TOKEN_COOKIE)
        .map(|cookie| cookie.value().trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|token| RequestToken {
            value: token,
            kind: TokenKind::Access,
        })
}

fn extract_bearer_token(req: &HttpRequest) -> Result<Option<RequestToken>, ApiError> {
    let Some(auth_header) = req.headers().get("Authorization").and_then(|v| v.to_str().ok()) else {
        return Ok(None);
    };

    let token = auth_header
        .strip_prefix("Bearer ")
        .or_else(|| auth_header.strip_prefix("bearer "))
        .ok_or_else(|| ApiError::Unauthorized("Authorization 格式错误，需要 Bearer <token>".into()))?;

    Ok(Some(RequestToken {
        value: token.to_string(),
        kind: TokenKind::Access,
    }))
}

fn extract_query_token(req: &HttpRequest) -> Option<RequestToken> {
    let params = web::Query::<QueryTokenParams>::from_query(req.query_string()).ok()?;
    params
        .sse_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|token| RequestToken {
            value: token.to_string(),
            kind: TokenKind::Sse,
        })
}

fn validate_token_kind(claims: &TokenData, expected: TokenKind) -> Result<(), ApiError> {
    let actual = claims.token_kind.as_deref().unwrap_or("access");
    match expected {
        TokenKind::Access => {
            if actual == "sse" {
                return Err(ApiError::Unauthorized("SSE 令牌不能用于该接口".into()));
            }
        }
        TokenKind::Sse => {
            // Python allows access tokens on SSE paths — it just decodes them
            // differently. Only reject if the token is truly invalid, not if it's
            // an access token used on an SSE endpoint.
            // Accept both 'sse' and 'access' token kinds for SSE endpoints
            if actual != "sse" && actual != "access" {
                return Err(ApiError::Unauthorized(
                    "无效的令牌类型，SSE 端点只接受 sse 或 access 令牌".into(),
                ));
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum TokenKind {
    Access,
    Sse,
}

struct RequestToken {
    value: String,
    kind: TokenKind,
}

fn is_query_token_allowed(path: &str) -> bool {
    QUERY_TOKEN_ALLOWED_PREFIXES
        .iter()
        .any(|prefix| path.starts_with(prefix))
}

fn is_query_token_auth_enabled(req: &HttpRequest) -> bool {
    req.app_data::<web::Data<SseQueryTokenAuth>>()
        .map(|flag| flag.0)
        .unwrap_or_else(|| {
            std::env::var(SSE_QUERY_TOKEN_AUTH_ENV)
                .map(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
                .unwrap_or(false)
        })
}

fn build_jwt_validation_full(audience: Option<&JwtAudience>, algorithm: Algorithm, issuer: Option<&str>) -> Validation {
    let mut validation = Validation::new(algorithm);
    validation.validate_exp = true;
    validation.leeway = 30; // 30s 容差

    let audiences: Vec<&str> = audience
        .map(|aud| {
            aud.0
                .iter()
                .map(String::as_str)
                .filter(|value| !value.trim().is_empty())
                .collect()
        })
        .unwrap_or_default();

    if audiences.is_empty() {
        // Compatibility default: Python FastAPI tokens include aud, but older
        // Rust tokens may omit it. Production can inject JwtAudience to enforce.
        validation.validate_aud = false;
    } else {
        validation.set_audience(&audiences);
    }

    if let Some(iss) = issuer {
        validation.set_issuer(&[iss]);
    }

    validation
}

/// Backwards-compatible audience-only builder used by existing tests (HS256, no issuer).
fn build_jwt_validation(audience: Option<&JwtAudience>) -> Validation {
    build_jwt_validation_full(audience, Algorithm::HS256, None)
}

/// Lazy, process-wide JWT verification configuration.
///
/// Reads asymmetric-signing env vars (`JWT_ALGORITHM`, `JWT_PUBLIC_KEY`,
/// `JWT_ISSUER`) once on first use so the middleware can support RS256/ES256
/// verification without requiring wiring changes in `main.rs`.
struct JwtVerifierConfig {
    algorithm: Algorithm,
    /// `Some` when asymmetric verification is configured (RS256/ES256);
    /// `None` means HS256, where the key comes from the injected `JwtSecret`.
    decoding_key: Option<DecodingKey>,
    issuer: Option<String>,
}

fn build_verifier_config() -> Result<JwtVerifierConfig, String> {
    let env_algorithm = std::env::var("JWT_ALGORITHM").map(|value| value.trim().to_ascii_uppercase());

    let algorithm = match env_algorithm {
        Ok(algorithm) if algorithm == "RS256" => Algorithm::RS256,
        Ok(algorithm) if algorithm == "ES256" => Algorithm::ES256,
        Ok(algorithm) if algorithm.is_empty() || algorithm == "HS256" => Algorithm::HS256,
        Ok(algorithm) => return Err(format!("不支持的 JWT 算法: {algorithm}")),
        Err(_) => Algorithm::HS256,
    };

    let decoding_key = match algorithm {
        Algorithm::RS256 => {
            let pem = std::env::var("JWT_PUBLIC_KEY")
                .map_err(|_| "RS256 已配置但缺少 JWT_PUBLIC_KEY 环境变量".to_string())?;
            Some(
                DecodingKey::from_rsa_pem(pem.as_bytes())
                    .map_err(|e| format!("JWT_PUBLIC_KEY 解析失败 (RS256): {e}"))?,
            )
        }
        Algorithm::ES256 => {
            let pem = std::env::var("JWT_PUBLIC_KEY")
                .map_err(|_| "ES256 已配置但缺少 JWT_PUBLIC_KEY 环境变量".to_string())?;
            Some(
                DecodingKey::from_ec_pem(pem.as_bytes())
                    .map_err(|e| format!("JWT_PUBLIC_KEY 解析失败 (ES256): {e}"))?,
            )
        }
        Algorithm::HS256 => None,
        other => return Err(format!("不支持的 JWT 算法: {other:?}")),
    };

    let issuer = std::env::var("JWT_ISSUER")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    Ok(JwtVerifierConfig {
        algorithm,
        decoding_key,
        issuer,
    })
}

fn load_jwt_verifier_config() -> Result<&'static JwtVerifierConfig, ApiError> {
    static CONFIG: OnceLock<Result<JwtVerifierConfig, String>> = OnceLock::new();
    match CONFIG.get_or_init(build_verifier_config) {
        Ok(config) => Ok(config),
        Err(message) => Err(ApiError::Internal(format!("JWT 验证配置错误: {message}"))),
    }
}

/// Resolve the decoding key for the configured algorithm.
///
/// For HS256 the symmetric secret is taken from the injected `JwtSecret`. For
/// RS256/ES256 the pre-loaded public key is reused.
fn build_decoding_key(req: &HttpRequest, config: &JwtVerifierConfig) -> Result<DecodingKey, ApiError> {
    match config.algorithm {
        Algorithm::HS256 => {
            let secret = req
                .app_data::<web::Data<JwtSecret>>()
                .ok_or_else(|| ApiError::Internal("JWT secret 未配置".into()))?;
            Ok(DecodingKey::from_secret(secret.0.as_bytes()))
        }
        Algorithm::RS256 | Algorithm::ES256 => config
            .decoding_key
            .clone()
            .ok_or_else(|| ApiError::Internal("JWT 公钥未配置".into())),
        other => Err(ApiError::Internal(format!("不支持的 JWT 算法: {other:?}"))),
    }
}

fn token_value_hash(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

#[derive(serde::Deserialize)]
struct QueryTokenParams {
    sse_token: Option<String>,
}

/// 可选 JWT 提取（用于公共 + 认证双模路由）
pub struct OptionalJwtAuth(pub Option<TokenData>);

impl FromRequest for OptionalJwtAuth {
    type Error = ApiError;
    type Future = LocalBoxFuture<'static, Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let req = req.clone();
        Box::pin(async move {
            let result = match extract_jwt(&req).await {
                Ok(JwtAuth(claims)) => Ok(OptionalJwtAuth(Some(claims))),
                Err(_) => Ok(OptionalJwtAuth(None)),
            };
            result
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_jwt_validation, build_jwt_validation_full, extract_jwt, extract_query_token, extract_request_token,
        JwtAudience, JwtSecret, SseQueryTokenAuth, TokenKind,
    };
    use actix_web::test::TestRequest;
    use actix_web::web;
    use jsonwebtoken::Algorithm;

    #[test]
    fn query_token_accepts_sse_token_parameter() {
        let request = TestRequest::with_uri("/api/v2/flights/stream?sse_token=good").to_http_request();
        let token = extract_query_token(&request).expect("sse query token should be extracted");

        assert_eq!(token.value, "good");
        assert!(matches!(token.kind, TokenKind::Sse));
    }

    #[test]
    fn query_token_rejects_access_token_parameter() {
        let request = TestRequest::with_uri("/api/v2/flights/stream?access_token=good-access").to_http_request();

        assert!(extract_query_token(&request).is_none());
        assert!(extract_request_token(&request).is_err());
    }

    #[test]
    fn request_token_rejects_sse_query_tokens_by_default() {
        for uri in [
            "/api/v2/flights/stream?sse_token=good",
        ] {
            let request = TestRequest::with_uri(uri).to_http_request();

            assert!(
                extract_request_token(&request).is_err(),
                "{uri} should not authenticate from query parameters by default"
            );
        }
    }

    #[test]
    fn request_token_allows_sse_query_tokens_when_switch_is_enabled() {
        let request = TestRequest::with_uri("/api/v2/flights/stream?sse_token=good")
            .app_data(web::Data::new(SseQueryTokenAuth(true)))
            .to_http_request();
        let token = extract_request_token(&request).expect("sse_token query parameter should be accepted for stream paths");

        assert_eq!(token.value, "good");
        assert!(matches!(token.kind, TokenKind::Sse));
    }

    #[test]
    fn jwt_validation_keeps_audience_disabled_without_config_for_compatibility() {
        let validation = build_jwt_validation(None);

        assert!(!validation.validate_aud);
    }

    #[test]
    fn jwt_validation_enables_audience_when_configured() {
        let audience = JwtAudience(vec!["flight-monitor-api".to_string()]);
        let validation = build_jwt_validation(Some(&audience));

        assert!(validation.validate_aud);
    }

    #[test]
    fn jwt_validation_defaults_to_hs256_without_issuer() {
        let validation = build_jwt_validation(None);

        assert_eq!(validation.algorithms, vec![Algorithm::HS256]);
        assert!(validation.iss.is_none());
    }

    #[test]
    fn jwt_validation_full_enables_issuer_and_asymmetric_algorithm() {
        let audience = JwtAudience(vec!["flight-monitor-api".to_string()]);
        let validation = build_jwt_validation_full(Some(&audience), Algorithm::RS256, Some("flight-monitor"));

        assert_eq!(validation.algorithms, vec![Algorithm::RS256]);
        assert!(validation.validate_aud);
        assert!(validation.iss.is_some());
    }

    #[actix_web::test]
    async fn invalid_jwt_error_is_generic_for_clients() {
        let request = TestRequest::with_uri("/api/v2/protected")
            .insert_header(("Authorization", "Bearer not-a-jwt"))
            .app_data(web::Data::new(JwtSecret("secret-with-enough-length".to_string())))
            .to_http_request();

        let error = match extract_jwt(&request).await {
            Ok(_) => panic!("invalid token should fail"),
            Err(error) => error,
        };

        assert_eq!(error.to_string(), "Unauthorized: 认证令牌无效");
        assert!(!error.to_string().contains("InvalidToken"));
        assert!(!error.to_string().contains("JWT 验证失败"));
    }
}
