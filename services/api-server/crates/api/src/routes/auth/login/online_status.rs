use super::*;

pub(crate) async fn online_users(
    svc: web::Data<Arc<AuthService>>,
    claims: JwtAuth,
    query: web::Query<OnlineUsersQuery>,
) -> Result<HttpResponse, ApiError> {
    ensure_admin(&claims)?;
    let runtime_status = svc.get_session_runtime_status().await?;
    if query.include_status.unwrap_or(false) {
        let users = svc.get_all_online_users_status().await?;
        return Ok(HttpResponse::Ok().json(json!({
            "success": true,
            "data": {
                "online_count": users.len(),
                "users": users,
                "session_backend": runtime_status,
            },
            "message": if runtime_status.mode == "fallback" {
                serde_json::Value::String("Redis unavailable, using fallback memory mode".to_string())
            } else {
                serde_json::Value::Null
            },
        })));
    }

    let users = svc.get_online_users().await?;
    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "data": {
            "online_count": users.len(),
            "users": users,
            "session_backend": runtime_status,
        },
        "message": if runtime_status.mode == "fallback" {
            serde_json::Value::String("Redis unavailable, using fallback memory mode".to_string())
        } else {
            serde_json::Value::Null
        },
    })))
}

pub(crate) async fn user_status(
    svc: web::Data<Arc<AuthService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_admin(&claims)?;
    let user_id = path.into_inner();
    let status = svc.get_user_online_status(&user_id).await?;
    Ok(ok_resp(status))
}

pub(crate) async fn kick_user(
    svc: web::Data<Arc<AuthService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_admin(&claims)?;
    let user_id = path.into_inner();
    let current_user_id = claims
        .0
        .sub
        .as_deref()
        .ok_or_else(|| ApiError::Unauthorized("未认证".into()))?;
    if current_user_id == user_id {
        return Err(ApiError::BadRequest("Cannot kick yourself".into()));
    }

    if !svc.kick_user_session(&user_id, "admin_kick").await? {
        return Err(ApiError::BadRequest("Failed to kick user".into()));
    }
    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "message": format!("User {user_id} has been kicked")
    })))
}
