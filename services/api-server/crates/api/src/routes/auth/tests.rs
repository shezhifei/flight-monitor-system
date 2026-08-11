use super::*;
use actix_web::test::TestRequest;
use fms_domain::error::DomainError;
use std::time::Duration;

#[test]
fn login_failure_limiter_blocks_after_consecutive_failed_attempts() {
    let limiter = LoginFailureRateLimiter::with_policy(3, Duration::from_secs(60));
    let key = "alice|198.51.100.10";

    assert_eq!(limiter.check(key), LoginRateLimitDecision::Allowed);

    limiter.record_login_error(key, &DomainError::Unauthorized("bad credentials".into()));
    limiter.record_login_error(key, &DomainError::Unauthorized("bad credentials".into()));
    assert_eq!(limiter.check(key), LoginRateLimitDecision::Allowed);

    limiter.record_login_error(key, &DomainError::Unauthorized("bad credentials".into()));
    assert!(matches!(
        limiter.check(key),
        LoginRateLimitDecision::Limited { retry_after_secs } if retry_after_secs > 0
    ));
}

#[test]
fn login_failure_limiter_does_not_count_success_or_non_auth_failures() {
    let limiter = LoginFailureRateLimiter::with_policy(2, Duration::from_secs(60));
    let key = "bob|198.51.100.20";

    limiter.record_login_success(key);
    limiter.record_login_error(key, &DomainError::Internal("database unavailable".into()));
    assert_eq!(limiter.check(key), LoginRateLimitDecision::Allowed);

    limiter.record_login_error(key, &DomainError::Unauthorized("bad credentials".into()));
    limiter.record_login_success(key);
    assert_eq!(limiter.check(key), LoginRateLimitDecision::Allowed);
}

#[test]
fn login_rate_limit_key_uses_normalized_username_and_peer_ip_when_proxy_untrusted() {
    // Without TRUSTED_PROXY_CIDRS matching the peer, X-Forwarded-For is ignored.
    let req = TestRequest::default()
        .peer_addr("10.0.0.8:54000".parse().expect("peer address"))
        .insert_header(("X-Forwarded-For", "203.0.113.200"))
        .to_http_request();

    assert_eq!(login_rate_limit_key("  Alice  ", &req), "alice|10.0.0.8");
}

#[test]
fn login_rate_limit_key_separates_users_on_same_ip() {
    let req = TestRequest::default()
        .peer_addr("198.51.100.30:443".parse().expect("peer address"))
        .to_http_request();

    let key_a = login_rate_limit_key("alice", &req);
    let key_b = login_rate_limit_key("bob", &req);
    assert_ne!(key_a, key_b);
    assert!(key_a.starts_with("alice|"));
    assert!(key_b.starts_with("bob|"));
}

#[test]
fn token_json_for_web_omits_refresh_and_session_secret() {
    let token = fms_application::schemas::auth_schemas::Token {
        access_token: "access".into(),
        token_type: "bearer".into(),
        expires_in: 3600,
        refresh_token: Some("refresh-secret".into()),
        sse_token: Some("sse".into()),
        sse_expires_in: Some(600),
        session_secret: Some("session-secret".into()),
    };
    let web = token_json_for_surface(&token, ClientSurface::Web);
    assert!(web.refresh_token.is_none());
    assert!(web.session_secret.is_none());
    assert_eq!(web.access_token, "access");
    assert_eq!(web.sse_token.as_deref(), Some("sse"));

    let native = token_json_for_surface(&token, ClientSurface::Native);
    assert_eq!(native.refresh_token.as_deref(), Some("refresh-secret"));
    assert_eq!(native.session_secret.as_deref(), Some("session-secret"));
}

#[test]
fn parse_client_surface_defaults_to_web() {
    let req = TestRequest::default().to_http_request();
    assert_eq!(parse_client_surface(&req), ClientSurface::Web);

    let native = TestRequest::default()
        .insert_header((CLIENT_SURFACE_HEADER, "native"))
        .to_http_request();
    assert_eq!(parse_client_surface(&native), ClientSurface::Native);
}

#[test]
fn resolve_refresh_token_ignores_query_only_source() {
    assert_eq!(resolve_refresh_token_sources(None, None), None);
    assert_eq!(
        resolve_refresh_token_sources(Some("  body-token  "), None).as_deref(),
        Some("body-token")
    );
    assert_eq!(
        resolve_refresh_token_sources(None, Some("cookie-token")).as_deref(),
        Some("cookie-token")
    );
    assert_eq!(
        resolve_refresh_token_sources(Some("body"), Some("cookie")).as_deref(),
        Some("body")
    );
}

#[test]
fn refresh_token_query_reject_only_binds_exact_key() {
    let rejected: RefreshTokenQueryReject =
        serde_urlencoded::from_str("refresh_token=secret").expect("parse exact key");
    assert_eq!(rejected.refresh_token.as_deref(), Some("secret"));

    let benign: RefreshTokenQueryReject =
        serde_urlencoded::from_str("not_refresh_token=secret").expect("parse unrelated key");
    assert!(benign.refresh_token.is_none());
}
