use super::*;

/// POST /api/v2/dispatch-orders — 创建
pub(crate) async fn create_order(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchService>>,
    query_svc: web::Data<Arc<DispatchQueryService>>,
    body: web::Json<DispatchOrderCreate>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::DISPATCH_ORDER_CREATE)?;
    let actor = claims.0.sub.as_deref().unwrap_or("dispatch_system");
    let created = svc.create_order(body.into_inner(), actor).await?;
    let payload = load_created_order_record(query_svc.get_ref().as_ref(), &created.id).await?;
    Ok(ok_resp(&req, payload))
}

pub(crate) async fn get_order(
    req: HttpRequest,
    query_svc: web::Data<Arc<DispatchQueryService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::DISPATCH_ORDER_READ)?;
    let order_id = path.into_inner();
    let payload = query_svc
        .get_order_record(&order_id, true, None)
        .await?
        .ok_or_else(|| ApiError::NotFound("派工单不存在".to_string()))?;
    Ok(ok_resp(&req, payload))
}

pub(crate) async fn publish_orders(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchService>>,
    body: web::Json<DispatchOrderBatchPublishRequest>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::DISPATCH_ORDER_PUBLISH)?;
    let actor = claims.0.sub.as_deref().unwrap_or("dispatch_system");
    let mut order_ids = body
        .order_ids
        .clone()
        .unwrap_or_default()
        .iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    order_ids.sort();
    order_ids.dedup();
    if order_ids.len() > 200 {
        return Err(ApiError::ValidationError("order_ids 数量不能超过 200".to_string()));
    }
    let limit = body.limit.unwrap_or(200).clamp(1, 200) as usize;
    let result = svc
        .publish_orders(
            (!order_ids.is_empty()).then_some(order_ids.as_slice()),
            actor,
            body.at_time,
            body.event_code.as_deref(),
            body.flight_id.as_deref(),
            limit,
            true,
        )
        .await?;
    let published_count = result
        .get("published_count")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0) as i32;
    Ok(ok_resp(
        &req,
        json!({
            "published": published_count > 0,
            "published_count": published_count,
            "published_orders": result.get("published_orders").cloned().unwrap_or_else(|| json!([])),
            "skipped_orders": result.get("skipped_orders").cloned().unwrap_or_else(|| json!([])),
            "message": if published_count > 0 {
                format!("已发布 {published_count} 条派工单")
            } else {
                "没有可发布的预发布派工单".to_string()
            },
        }),
    ))
}

pub(crate) async fn publish_order(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::DISPATCH_ORDER_PUBLISH)?;
    let actor = claims.0.sub.as_deref().unwrap_or("dispatch_system");
    let result = svc.publish_order(&path.into_inner(), actor).await?;
    let published_count = result
        .get("published_count")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0) as i32;
    if published_count <= 0 {
        let detail = result
            .get("skipped_orders")
            .and_then(serde_json::Value::as_array)
            .and_then(|items| items.first())
            .and_then(|item| item.get("reason"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("没有可发布的预发布派工单");
        return Err(ApiError::BadRequest(detail.to_string()));
    }
    Ok(ok_resp(
        &req,
        json!({
            "published": true,
            "published_count": published_count,
            "published_orders": result.get("published_orders").cloned().unwrap_or_else(|| json!([])),
            "skipped_orders": result.get("skipped_orders").cloned().unwrap_or_else(|| json!([])),
            "message": "派工单已正式发布",
        }),
    ))
}

/// POST /{order_id}/accept
pub(crate) async fn accept_order(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchService>>,
    path: web::Path<String>,
    body: Option<web::Json<DispatchOrderAcceptRequest>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let id = path.into_inner();
    let actor = claims.0.sub.as_deref().unwrap_or("unknown");
    let dto = body.map(web::Json::into_inner).unwrap_or(DispatchOrderAcceptRequest {
        note: None,
        client_action_id: None,
    });
    dto.validate().map_err(ApiError::ValidationError)?;
    let result = svc.accept_order(&id, dto, actor).await?;
    Ok(ok_resp(&req, result))
}

/// POST /{order_id}/start
pub(crate) async fn start_order(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchService>>,
    path: web::Path<String>,
    body: web::Json<DispatchOrderStart>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let id = path.into_inner();
    let actor = claims.0.sub.as_deref().unwrap_or("unknown");
    Ok(ok_resp(&req, svc.start_order(&id, body.into_inner(), actor).await?))
}

/// POST /{order_id}/complete
pub(crate) async fn complete_order(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchService>>,
    path: web::Path<String>,
    body: web::Json<DispatchOrderCompleteReq>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let id = path.into_inner();
    let actor = claims.0.sub.as_deref().unwrap_or("unknown");
    Ok(ok_resp(&req, svc.complete_order(&id, body.into_inner(), actor).await?))
}

/// POST /{order_id}/cancel
pub(crate) async fn cancel_order(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchService>>,
    path: web::Path<String>,
    query: Option<web::Query<DispatchOrderCancelQuery>>,
    body: Option<web::Json<DispatchOrderCancelRequest>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let id = path.into_inner();
    let actor = claims.0.sub.as_deref().unwrap_or("unknown");
    let is_privileged = has_grant(&claims, PermissionCatalog::DISPATCH_ORDER_CANCEL);
    let dto = merge_cancel_request(query, body);
    dto.validate().map_err(ApiError::ValidationError)?;
    svc.cancel_order(&id, dto, actor, is_privileged).await?;
    Ok(ok_resp(&req, json!({ "message": "派工单已取消" })))
}

/// POST /{order_id}/checkin
pub(crate) async fn checkin_order(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchService>>,
    path: web::Path<String>,
    body: web::Json<DispatchOrderCheckInRequest>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let id = path.into_inner();
    let actor = claims.0.sub.as_deref().unwrap_or("unknown");
    let dto = body.into_inner();
    dto.validate().map_err(ApiError::ValidationError)?;
    let result = svc.checkin_order(&id, dto, actor).await?;
    Ok(ok_resp(&req, result))
}

/// POST /{order_id}/checkout
pub(crate) async fn checkout_order(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchService>>,
    path: web::Path<String>,
    body: web::Json<DispatchOrderCheckOutRequest>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let id = path.into_inner();
    let actor = claims.0.sub.as_deref().unwrap_or("unknown");
    let dto = body.into_inner();
    dto.validate().map_err(ApiError::ValidationError)?;
    let result = svc.checkout_order(&id, dto, actor).await?;
    Ok(ok_resp(&req, result))
}

pub(crate) async fn eta_report(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchService>>,
    path: web::Path<String>,
    claims: JwtAuth,
    body: web::Json<EtaReportRequest>,
) -> Result<HttpResponse, ApiError> {
    let id = path.into_inner();
    let actor = claims.0.sub.as_deref().unwrap_or("unknown");
    let dto = body.into_inner();
    dto.validate().map_err(ApiError::ValidationError)?;
    let result = svc.report_eta(&id, dto, actor).await?;
    Ok(ok_resp(&req, result))
}

/// POST /{order_id}/report-issue
pub(crate) async fn report_issue(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchService>>,
    path: web::Path<String>,
    claims: JwtAuth,
    body: web::Json<ReportIssueRequest>,
) -> Result<HttpResponse, ApiError> {
    let id = path.into_inner();
    let actor = claims.0.sub.as_deref().unwrap_or("unknown");
    let dto = body.into_inner();
    dto.validate().map_err(ApiError::ValidationError)?;
    let result = svc.report_issue(&id, dto, actor).await?;
    Ok(ok_resp(&req, result))
}

/// POST /validate
pub(crate) async fn validate_order(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchService>>,
    claims: JwtAuth,
    body: web::Json<ValidateOrderRequest>,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::DISPATCH_ORDER_UPDATE)?;
    let dto = body.into_inner();
    if dto.planned_end_time <= dto.planned_start_time {
        return Err(ApiError::BadRequest(
            "planned_end_time 必须晚于 planned_start_time".to_string(),
        ));
    }
    let result = svc.validate_order_conflicts_only(dto).await?;
    Ok(ok_resp(&req, result))
}
