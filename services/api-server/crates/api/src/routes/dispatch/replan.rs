use super::*;

/// POST /replan
pub(crate) async fn replan(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchService>>,
    claims: JwtAuth,
    body: web::Json<ReplanRequest>,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::DISPATCH_ORDER_UPDATE)?;
    let request = body.into_inner().normalize().map_err(ApiError::ValidationError)?;
    if request.window_end <= request.window_start {
        return Err(ApiError::BadRequest("window_end 必须晚于 window_start".to_string()));
    }
    let result = svc.replan(request).await?;
    Ok(ok_resp(&req, result))
}

/// POST /auto
pub(crate) async fn auto_dispatch(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchService>>,
    claims: JwtAuth,
    query: web::Query<AutoDispatchQuery>,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::DISPATCH_ORDER_CREATE)?;
    let result = svc
        .auto_dispatch(
            &query.flight_id,
            &query.task_type,
            &query.stand_id,
            query.planned_start_time,
            query.planned_end_time,
            query.terminal.as_deref(),
            query.department_id.as_deref(),
        )
        .await?;
    let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
    if !success {
        return Ok(HttpResponse::BadRequest().json(json!({
            "success": false,
            "data": result,
            "error": result.get("message"),
            "request_id": request_id(&req),
        })));
    }
    Ok(ok_resp(&req, result))
}

/// POST /generate-drafts
pub(crate) async fn generate_drafts(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchService>>,
    claims: JwtAuth,
    query: web::Query<GenerateDraftsQuery>,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::DISPATCH_ORDER_CREATE)?;
    let drafted_orders = svc
        .generate_draft_orders(
            &query.flight_id,
            &query.stand_id,
            query.eta,
            query.etd,
            query.terminal.as_deref(),
        )
        .await?;
    let payload: Vec<Value> = drafted_orders
        .into_iter()
        .map(|order| {
            json!({
                "id": order.id,
                "flight_id": order.flight_id,
                "task_type": order.task_type,
                "stand_id": order.stand_id,
                "planned_start_time": order.planned_start_time,
                "planned_end_time": order.planned_end_time,
                "status": order_status_label(order.status),
                "publication_state": order.publication_state,
                "generation_rule_id": order.generation_rule_id,
                "department_id": order.department_id,
                "leg_scope": order.leg_scope,
                "crew_requirement_snapshot": order.crew_requirement_snapshot,
                "equipment_requirement_snapshot": order.equipment_requirement_snapshot,
            })
        })
        .collect();
    Ok(ok_resp(
        &req,
        json!({
            "success": true,
            "total": payload.len(),
            "orders": payload,
        }),
    ))
}

/// POST /batch-publish-drafts
pub(crate) async fn batch_publish_drafts(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchService>>,
    claims: JwtAuth,
    body: web::Json<BatchPublishDraftsRequest>,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::DISPATCH_ORDER_PUBLISH)?;
    let actor = claims.0.sub.as_deref().unwrap_or("dispatch_system");
    let published_orders = svc.batch_publish_draft_orders(&body.assignments, actor).await?;
    let payload: Vec<Value> = published_orders
        .into_iter()
        .map(|order| {
            json!({
                "id": order.id,
                "flight_id": order.flight_id,
                "task_type": order.task_type,
                "status": order_status_label(order.status),
                "publication_state": order.publication_state,
                "task_crew": order.task_crew,
                "equipment_assignment": order.equipment_assignment,
            })
        })
        .collect();
    Ok(ok_resp(
        &req,
        json!({
            "success": true,
            "total": payload.len(),
            "orders": payload,
        }),
    ))
}

/// POST /batch — [DEPRECATED] 请使用 /generate-drafts + /batch-publish-drafts
pub(crate) async fn batch_dispatch(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchService>>,
    claims: JwtAuth,
    query: web::Query<BatchDispatchQuery>,
) -> Result<HttpResponse, ApiError> {
    tracing::warn!("DEPRECATED: POST /api/v2/dispatch-orders/batch called — use /generate-drafts + /batch-publish-drafts instead. Scheduled for removal in Q3 2026");
    ensure_all_grants(
        &claims,
        &[
            PermissionCatalog::DISPATCH_ORDER_CREATE,
            PermissionCatalog::DISPATCH_ORDER_PUBLISH,
        ],
    )?;
    let result = svc
        .batch_dispatch_for_flight(
            &query.flight_id,
            &query.stand_id,
            query.eta,
            query.etd,
            query.terminal.as_deref(),
        )
        .await?;
    Ok(ok_resp(&req, result))
}

/// POST /optimal
pub(crate) async fn optimal_dispatch(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchService>>,
    claims: JwtAuth,
    query: web::Query<OptimalDispatchQuery>,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::DISPATCH_ORDER_CREATE)?;
    let query = query.into_inner();
    let scope = query.scope.as_deref().unwrap_or("flight").trim().to_lowercase();
    if scope == "flight" {
        if query
            .flight_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
            || query
                .stand_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none()
            || query.eta.is_none()
            || query.etd.is_none()
        {
            return Err(ApiError::BadRequest(
                "flight scope 需要 flight_id/stand_id/eta/etd".to_string(),
            ));
        }
    } else if scope == "window" {
        if query.window_start.is_none() || query.window_end.is_none() {
            return Err(ApiError::BadRequest(
                "window scope 需要 window_start/window_end".to_string(),
            ));
        }
    } else {
        return Err(ApiError::BadRequest("scope 必须为 flight 或 window".to_string()));
    }
    let freeze_order_ids = query
        .freeze_order_ids
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();

    let result = svc
        .optimal_batch_dispatch(
            query.flight_id.as_deref(),
            query.stand_id.as_deref(),
            query.eta,
            query.etd,
            query.terminal.as_deref(),
            query.time_limit.unwrap_or(5.0),
            &scope,
            query.window_start,
            query.window_end,
            &freeze_order_ids,
            query.lock_policy.as_deref(),
        )
        .await?;
    Ok(ok_resp(&req, result))
}
