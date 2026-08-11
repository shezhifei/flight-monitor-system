use super::*;

pub(crate) async fn me(
    svc: web::Data<Arc<AuthService>>,
    operator_identity_svc: Option<web::Data<Arc<OperatorIdentityService>>>,
    req: HttpRequest,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let user_id = claims.0.sub.as_deref().unwrap_or("unknown");
    match svc.find_user_by_id(user_id).await? {
        Some(u) => {
            let enriched = maybe_enrich_user_response(u, operator_identity_svc, Some(&req)).await?;
            Ok(HttpResponse::Ok().json(enriched))
        }
        None => Ok(HttpResponse::Ok().json(claims.0)),
    }
}

pub(crate) async fn update_profile(
    svc: web::Data<Arc<AuthService>>,
    operator_identity_svc: Option<web::Data<Arc<OperatorIdentityService>>>,
    claims: JwtAuth,
    req: HttpRequest,
    body: web::Json<ProfileUpdate>,
) -> Result<HttpResponse, ApiError> {
    let user_id = claims.0.sub.as_deref().unwrap_or("unknown");
    let update = fms_application::schemas::auth_schemas::UserAdminUpdate {
        username: None,
        email: None,
        display_name: body.into_inner().display_name,
        is_active: None,
        is_admin: None,
        roles: None,
        department: None,
        job_level: None,
        job_title: None,
        password: None,
    };
    match svc.update_user(user_id, update).await? {
        Some(u) => {
            let enriched = maybe_enrich_user_response(u, operator_identity_svc, Some(&req)).await?;
            Ok(HttpResponse::Ok().json(enriched))
        }
        None => Err(ApiError::NotFound("用户未找到".into())),
    }
}

pub(crate) async fn update_operator_context(
    svc: web::Data<Arc<AuthService>>,
    operator_identity_svc: web::Data<Arc<OperatorIdentityService>>,
    claims: JwtAuth,
    req: HttpRequest,
    body: web::Json<OperatorContextUpdate>,
) -> Result<HttpResponse, ApiError> {
    let user_id = claims
        .0
        .sub
        .as_deref()
        .ok_or_else(|| ApiError::Unauthorized("未认证".into()))?;
    let user = svc
        .find_user_by_id(user_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("用户未找到".into()))?;
    let (context_type, context_id) = extract_operator_context(&req, &operator_identity_svc)?;
    let enriched = operator_identity_svc
        .update_operator_context(user, &context_type, &context_id, body.operator_name.as_deref())
        .await?;
    Ok(HttpResponse::Ok().json(enriched))
}

pub(crate) async fn change_password(
    svc: web::Data<Arc<AuthService>>,
    claims: JwtAuth,
    body: web::Json<fms_application::schemas::auth_schemas::ChangePassword>,
) -> Result<HttpResponse, ApiError> {
    let user_id = claims.0.sub.as_deref().unwrap_or("unknown");
    svc.change_password(user_id, body.into_inner()).await?;
    Ok(auth_resp("密码修改成功"))
}

pub(crate) async fn list_users(
    svc: web::Data<Arc<AuthService>>,
    operator_identity_svc: Option<web::Data<Arc<OperatorIdentityService>>>,
    req: HttpRequest,
    query: web::Query<PageQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_admin(&claims)?;
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(50).clamp(1, 200);
    let users = svc.list_users_paginated(page, page_size).await?;
    let mut enriched = Vec::with_capacity(users.len());
    for user in users {
        enriched.push(maybe_enrich_user_response(user, operator_identity_svc.clone(), Some(&req)).await?);
    }
    Ok(ok_resp(enriched))
}

pub(crate) async fn get_user(
    svc: web::Data<Arc<AuthService>>,
    operator_identity_svc: Option<web::Data<Arc<OperatorIdentityService>>>,
    req: HttpRequest,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_admin(&claims)?;
    let user_id = path.into_inner();
    if let Some(validation_response) = validate_user_id_path_response(&user_id) {
        return Ok(validation_response);
    }
    match svc.find_user_by_id(&user_id).await? {
        Some(u) => Ok(HttpResponse::Ok().json(maybe_enrich_user_response(u, operator_identity_svc, Some(&req)).await?)),
        None => Err(ApiError::NotFound("用户不存在".into())),
    }
}

pub(crate) async fn update_user(
    svc: web::Data<Arc<AuthService>>,
    operator_identity_svc: Option<web::Data<Arc<OperatorIdentityService>>>,
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<fms_application::schemas::auth_schemas::UserAdminUpdate>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_admin(&claims)?;
    let user_id = path.into_inner();
    if let Some(validation_response) = validate_user_id_path_response(&user_id) {
        return Ok(validation_response);
    }
    match svc.update_user(&user_id, body.into_inner()).await? {
        Some(u) => Ok(HttpResponse::Ok().json(maybe_enrich_user_response(u, operator_identity_svc, Some(&req)).await?)),
        None => Err(ApiError::NotFound("用户不存在".into())),
    }
}

pub(crate) async fn delete_user(
    svc: web::Data<Arc<AuthService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_admin(&claims)?;
    let user_id = path.into_inner();
    if let Some(validation_response) = validate_user_id_path_response(&user_id) {
        return Ok(validation_response);
    }
    let deleted = svc.delete_user(&user_id).await?;
    if deleted {
        Ok(HttpResponse::Ok().json(json!({
            "success": true,
            "message": "用户已删除",
            "data": null,
        })))
    } else {
        Err(ApiError::NotFound("用户未找到".into()))
    }
}
