use super::*;

/// GET /replan-snapshot
pub(crate) async fn replan_snapshot(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchFrontendReplanService>>,
    query: web::Query<DispatchReplanSnapshotQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::DISPATCH_ORDER_READ)?;
    let query = query.into_inner().normalize().map_err(ApiError::ValidationError)?;
    if query.window_end <= query.window_start {
        return Err(ApiError::BadRequest("window_end 必须晚于 window_start".to_string()));
    }
    let payload = svc
        .build_snapshot(
            query.window_start,
            query.window_end,
            query.strategy.clone(),
            query.max_suggestions,
        )
        .await?;
    Ok(ok_resp(&req, public_replan_snapshot_payload(&payload)))
}

/// POST /replan-apply
pub(crate) async fn replan_apply(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchFrontendReplanService>>,
    claims: JwtAuth,
    body: web::Json<DispatchReplanApplyRequest>,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::DISPATCH_ORDER_UPDATE)?;
    let mut request = body.into_inner().normalize().map_err(ApiError::ValidationError)?;
    if request.order_results.is_empty() && !request.suggestions.is_empty() {
        request.order_results = request
            .suggestions
            .iter()
            .cloned()
            .map(DispatchFrontendReplanService::suggestion_to_order_result)
            .collect();
    }
    let payload = svc
        .apply_snapshot(request, claims.0.sub.clone(), claims.0.username.clone())
        .await?;
    Ok(ok_resp(&req, public_replan_apply_payload(&payload)))
}
