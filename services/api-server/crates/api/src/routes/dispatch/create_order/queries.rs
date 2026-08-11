use super::*;

/// POST /mobile/sync/actions
pub(crate) async fn mobile_sync_actions(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchService>>,
    claims: JwtAuth,
    body: web::Json<MobileSyncRequest>,
) -> Result<HttpResponse, ApiError> {
    let actor = claims.0.sub.as_deref().unwrap_or("unknown");
    let dto = body.into_inner();
    dto.validate().map_err(ApiError::ValidationError)?;
    let result = svc.sync_mobile_actions(dto, actor).await?;
    Ok(ok_resp(&req, result))
}

/// GET /followup-queue
pub(crate) async fn followup_queue(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchService>>,
    query: web::Query<DispatchFollowupQueueQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::DISPATCH_ORDER_READ)?;
    let query = query.into_inner();
    let assignee = query.assignee.as_deref().or(claims.0.sub.as_deref());
    let result = svc
        .get_followup_queue(assignee, query.source_type.as_deref(), query.limit)
        .await?;
    Ok(ok_resp(&req, result))
}

/// GET /burden-metrics
pub(crate) async fn burden_metrics(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchService>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::DISPATCH_ORDER_READ)?;
    Ok(ok_resp(&req, svc.get_burden_metrics().await?))
}
