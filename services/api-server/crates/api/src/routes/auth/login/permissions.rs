use super::*;

pub(crate) async fn list_permissions(
    svc: web::Data<Arc<AuthService>>,
    query: web::Query<PageQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_admin(&claims)?;
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(100).clamp(1, 500);
    let perms = svc.list_permissions_paginated(page, page_size).await?;
    Ok(ok_resp(perms))
}

pub(crate) async fn create_role(
    svc: web::Data<Arc<AuthService>>,
    claims: JwtAuth,
    body: web::Json<fms_application::schemas::auth_schemas::RoleCreate>,
) -> Result<HttpResponse, ApiError> {
    ensure_admin(&claims)?;
    let role = svc.create_role(body.into_inner()).await?;
    Ok(HttpResponse::Created().json(role))
}

pub(crate) async fn list_roles(
    svc: web::Data<Arc<AuthService>>,
    query: web::Query<PageQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_admin(&claims)?;
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(50).clamp(1, 200);
    let roles = svc.list_roles_paginated(page, page_size).await?;
    Ok(ok_resp(roles))
}
