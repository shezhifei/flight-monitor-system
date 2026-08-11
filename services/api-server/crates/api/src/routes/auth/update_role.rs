use super::*;

pub(crate) async fn update_role(
    svc: web::Data<Arc<AuthService>>,
    path: web::Path<String>,
    claims: JwtAuth,
    body: web::Json<RoleUpdate>,
) -> Result<HttpResponse, ApiError> {
    ensure_admin(&claims)?;
    let role_id = path.into_inner();
    match svc.update_role(&role_id, body.into_inner()).await? {
        Some(r) => Ok(HttpResponse::Ok().json(r)),
        None => Err(ApiError::NotFound(format!("角色 {role_id} 未找到"))),
    }
}

pub(crate) async fn delete_role(
    svc: web::Data<Arc<AuthService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_admin(&claims)?;
    let result = svc.delete_role(&path.into_inner()).await?;
    if !result.found {
        return Err(ApiError::NotFound("角色不存在".into()));
    }
    if result.is_system {
        return Err(ApiError::BadRequest("系统角色不可删除".into()));
    }
    if !result.deleted {
        return Err(ApiError::Internal("角色删除失败".into()));
    }
    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "message": "角色删除成功",
        "data": {
            "affected_users": result.affected_users,
        },
    })))
}

pub(crate) async fn assign_role(
    svc: web::Data<Arc<AuthService>>,
    claims: JwtAuth,
    body: web::Json<fms_application::schemas::auth_schemas::UserRoleAssign>,
) -> Result<HttpResponse, ApiError> {
    ensure_admin(&claims)?;
    svc.assign_role(body.into_inner()).await?;
    Ok(auth_resp("角色分配成功"))
}

pub(crate) async fn add_permission(
    svc: web::Data<Arc<AuthService>>,
    path: web::Path<String>,
    claims: JwtAuth,
    query: web::Query<PermissionAssignQuery>,
    body: Option<web::Json<PermissionAssignBody>>,
) -> Result<HttpResponse, ApiError> {
    ensure_admin(&claims)?;
    let role_id = path.into_inner();
    let perm_name = query
        .permission
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            body.as_ref()
                .and_then(|payload| payload.permission.as_deref())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
        .ok_or_else(|| ApiError::ValidationError("permission is required".into()))?;
    svc.add_permission_to_role(&role_id, &perm_name).await?;
    Ok(auth_resp("权限添加成功"))
}

pub(crate) async fn remove_permission(
    svc: web::Data<Arc<AuthService>>,
    path: web::Path<(String, String)>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_admin(&claims)?;
    let (role_id, perm_name) = path.into_inner();
    svc.remove_permission_from_role(&role_id, &perm_name).await?;
    Ok(auth_resp("权限移除成功"))
}

pub(crate) async fn protected(claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    Ok(HttpResponse::Ok().json(json!({
        "message": "访问受保护路由成功",
        "user": {
            "username": claims.0.username,
            "email": claims.0.email,
        }
    })))
}

pub(crate) async fn admin_only(claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    ensure_admin(&claims)?;
    Ok(HttpResponse::Ok().json(json!({
        "message": "访问管理员路由成功"
    })))
}

pub(crate) async fn online_status(
    svc: web::Data<Arc<OnlineStatusService>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_admin(&claims)?;
    let summary = svc.get_online_summary().await?;
    Ok(ok_resp(json!({ "summary": summary })))
}

pub(crate) async fn online_history(
    claims: JwtAuth,
    query: web::Query<OnlineHistoryQuery>,
) -> Result<HttpResponse, ApiError> {
    ensure_admin(&claims)?;
    let page = query.page.unwrap_or(1);
    if page < 1 {
        return Err(ApiError::ValidationError(
            "page must be greater than or equal to 1".into(),
        ));
    }

    let page_size = query.page_size.unwrap_or(20);
    if !(1..=100).contains(&page_size) {
        return Err(ApiError::ValidationError("page_size must be between 1 and 100".into()));
    }

    let _ = parse_online_history_date(query.start_date.as_deref(), "start_date")?;
    let _ = parse_online_history_date(query.end_date.as_deref(), "end_date")?;
    let _ = query.user_id.as_deref();

    // Python is the migration truth source. In the distributed stack, the
    // current public contract is an empty pagination shell rather than the
    // richer repository-backed Rust history payload.
    Ok(ok_resp(json!({
        "items": [],
        "pagination": {
            "page": page,
            "page_size": page_size,
            "total": 0,
            "total_pages": 0,
        },
    })))
}

pub(crate) async fn force_offline(
    svc: web::Data<Arc<OnlineStatusService>>,
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
        return Err(ApiError::BadRequest("Cannot force offline yourself".into()));
    }

    if !svc.force_user_offline(&user_id).await? {
        return Err(ApiError::NotFound("User is not online or does not exist".into()));
    }

    Ok(auth_resp(&format!("User {user_id} has been forced offline")))
}
