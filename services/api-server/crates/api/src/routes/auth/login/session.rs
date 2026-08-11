use super::*;

pub(crate) async fn login(
    svc: web::Data<Arc<AuthService>>,
    login_failure_limiter: web::Data<Arc<LoginFailureRateLimiter>>,
    performance_metrics: Option<web::Data<Arc<PerformanceMetricsService>>>,
    req: HttpRequest,
    body: web::Json<UserLogin>,
) -> Result<HttpResponse, ApiError> {
    let login_body = body.into_inner();
    let rate_limit_key = login_rate_limit_key(&login_body.username, &req);
    if let LoginRateLimitDecision::Limited { retry_after_secs } = login_failure_limiter.check(&rate_limit_key) {
        if let Some(metrics) = performance_metrics {
            metrics.record_auth_login(false);
        }
        return Ok(login_rate_limited_response(retry_after_secs));
    }

    let client_ip = extract_client_ip(&req);
    let user_agent_hash = build_user_agent_hash(extract_user_agent(&req).as_deref());
    let ip_subnet_hash = build_ip_subnet_hash(client_ip.as_deref());
    let surface = parse_client_surface(&req);
    match svc
        .login(
            login_body,
            client_ip.as_deref(),
            Some(user_agent_hash.as_str()),
            Some(ip_subnet_hash.as_str()),
        )
        .await
    {
        Ok(token) => {
            login_failure_limiter.record_login_success(&rate_limit_key);
            if let Some(metrics) = performance_metrics {
                metrics.record_auth_login(true);
            }
            attach_token_response_cookies(&token, surface)
        }
        Err(error) => {
            login_failure_limiter.record_login_error(&rate_limit_key, &error);
            if let Some(metrics) = performance_metrics {
                metrics.record_auth_login(false);
            }
            Err(ApiError::from(error))
        }
    }
}

pub(crate) async fn register(
    svc: web::Data<Arc<AuthService>>,
    claims: JwtAuth,
    body: web::Bytes,
) -> Result<HttpResponse, ApiError> {
    ensure_admin(&claims)?;
    let payload = parse_register_payload(&body)?;
    let mut user_create = payload;
    user_create.is_admin = false;
    let user = svc.register(user_create).await?;
    Ok(HttpResponse::Created().json(json!({
        "success": true,
        "message": "用户注册成功",
        "data": {
            "user_id": user.id,
            "username": user.username,
            "email": user.email,
            "is_admin": user.is_admin,
        }
    })))
}

pub(crate) async fn refresh(
    svc: web::Data<Arc<AuthService>>,
    performance_metrics: Option<web::Data<Arc<PerformanceMetricsService>>>,
    req: HttpRequest,
    body: Option<web::Json<RefreshTokenRequest>>,
    query: web::Query<RefreshTokenQueryReject>,
) -> Result<HttpResponse, ApiError> {
    // Reject the exact query key refresh_token (serde-decoded).
    // Unrelated keys such as not_refresh_token do not match and are ignored.
    // Token sources: JSON body (native) or HttpOnly cookie (web) only.
    if query.refresh_token.is_some() {
        return Err(ApiError::ValidationError(
            "refresh_token must not be supplied as a query parameter".into(),
        ));
    }

    let cookie_refresh = req
        .cookie(REFRESH_TOKEN_COOKIE)
        .map(|cookie| cookie.value().to_string());
    let refresh_token = resolve_refresh_token_sources(
        body.as_ref().map(|value| value.refresh_token.as_str()),
        cookie_refresh.as_deref(),
    )
    .ok_or_else(|| ApiError::ValidationError("refresh_token is required".into()))?;
    let client_ip = extract_client_ip(&req);
    let user_agent_hash = build_user_agent_hash(extract_user_agent(&req).as_deref());
    let ip_subnet_hash = build_ip_subnet_hash(client_ip.as_deref());
    let surface = parse_client_surface(&req);
    match svc
        .refresh_token(
            &refresh_token,
            Some(user_agent_hash.as_str()),
            Some(ip_subnet_hash.as_str()),
        )
        .await
    {
        Ok(token) => {
            if let Some(metrics) = performance_metrics {
                metrics.record_auth_refresh(true, false);
            }
            attach_token_response_cookies(&token, surface)
        }
        Err(error) => {
            if let Some(metrics) = performance_metrics {
                metrics.record_auth_refresh(false, false);
            }
            Err(ApiError::from(error))
        }
    }
}

pub(crate) async fn sse_token(
    svc: web::Data<Arc<AuthService>>,
    req: HttpRequest,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let sub = claims
        .0
        .sub
        .as_deref()
        .ok_or_else(|| ApiError::Unauthorized("未认证".into()))?;
    let client_ip = extract_client_ip(&req);
    let user_agent_hash = build_user_agent_hash(extract_user_agent(&req).as_deref());
    let ip_subnet_hash = build_ip_subnet_hash(client_ip.as_deref());
    let (sse_token, expires_in) = svc
        .issue_sse_token(sub, Some(user_agent_hash.as_str()), Some(ip_subnet_hash.as_str()))
        .await?;
    Ok(HttpResponse::Ok().json(json!({ "sse_token": sse_token, "sse_expires_in": expires_in })))
}

pub(crate) async fn logout(
    svc: web::Data<Arc<AuthService>>,
    performance_metrics: Option<web::Data<Arc<PerformanceMetricsService>>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let user_id = claims
        .0
        .sub
        .as_deref()
        .ok_or_else(|| ApiError::Unauthorized("未认证".into()))?;
    svc.logout(user_id).await?;
    if let Some(metrics) = performance_metrics {
        metrics.record_auth_logout();
    }
    let mut response = auth_resp("已成功登出");
    let _ = response.add_cookie(&build_clear_cookie(ACCESS_TOKEN_COOKIE));
    let _ = response.add_cookie(&build_clear_cookie(REFRESH_TOKEN_COOKIE));
    let _ = response.add_cookie(&build_clear_cookie(SESSION_SECRET_COOKIE));
    Ok(response)
}

pub(crate) async fn heartbeat(
    svc: web::Data<Arc<AuthService>>,
    performance_metrics: Option<web::Data<Arc<PerformanceMetricsService>>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let user_id = claims
        .0
        .sub
        .as_deref()
        .ok_or_else(|| ApiError::Unauthorized("未认证".into()))?;
    svc.heartbeat(user_id).await?;
    if let Some(metrics) = performance_metrics {
        metrics.record_auth_heartbeat();
    }
    Ok(auth_resp("心跳已接收"))
}
