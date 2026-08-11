use super::*;

/// POST /{order_id}/safety-checklist/items/{item_code}
pub(crate) async fn safety_checklist_check_item(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchService>>,
    path: web::Path<(String, String)>,
    claims: JwtAuth,
    body: web::Json<SafetyChecklistItemRequest>,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::DISPATCH_ORDER_UPDATE)?;
    let (order_id, item_code) = path.into_inner();
    let actor = claims.0.sub.as_deref().unwrap_or("unknown");
    let result = svc
        .submit_safety_checklist_item(&order_id, &item_code, body.into_inner(), actor)
        .await?;
    Ok(ok_resp(&req, result))
}

/// POST /{order_id}/safety-checklist/batch-submit
pub(crate) async fn safety_checklist_batch_submit(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchService>>,
    path: web::Path<String>,
    claims: JwtAuth,
    body: web::Json<DispatchSafetyChecklistBatchSubmitRequest>,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::DISPATCH_ORDER_UPDATE)?;
    let actor = claims.0.sub.as_deref().unwrap_or("unknown");
    let result = svc
        .submit_safety_checklist_batch(&path.into_inner(), body.into_inner(), actor)
        .await?;
    Ok(ok_resp(&req, result))
}

/// GET /safety-checklist/templates/{task_type}
pub(crate) async fn get_safety_template(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::DISPATCH_CATALOG_READ)?;
    let task_type = path.into_inner();
    match svc.get_safety_template(&task_type).await? {
        Some(result) => Ok(ok_resp(&req, result)),
        None => Err(ApiError::NotFound("未找到该作业类型的生效安全检查清单模板".to_string())),
    }
}

/// PUT /safety-checklist/templates/{task_type}
pub(crate) async fn update_safety_template(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchService>>,
    path: web::Path<String>,
    claims: JwtAuth,
    body: web::Json<SafetyTemplateUpsertRequest>,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::DISPATCH_CATALOG_EDIT)?;
    let actor = claims.0.sub.as_deref().unwrap_or("unknown");
    let result = svc
        .upsert_safety_template(&path.into_inner(), body.into_inner(), actor)
        .await?;
    Ok(ok_resp(&req, result))
}

/// GET /{order_id}/safety-checklist
pub(crate) async fn get_order_safety_checklist(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::DISPATCH_ORDER_READ)?;
    let result = svc.get_order_safety_checklist(&path.into_inner()).await?;
    Ok(ok_resp(&req, result))
}

/// POST /safety-checklist/progress
pub(crate) async fn safety_checklist_progress(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchService>>,
    claims: JwtAuth,
    body: web::Json<SafetyChecklistProgressRequest>,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::DISPATCH_ORDER_READ)?;
    let result = svc.evaluate_checklist_progress(body.into_inner().orders).await?;
    Ok(ok_resp(&req, result))
}
