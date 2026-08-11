use super::*;

// ==================== Adjustment Rules ====================

pub(crate) async fn list_adjustment_rules(
    req: HttpRequest,
    query: web::Query<ListAdjustmentRulesQuery>,
    svc: web::Data<Arc<ConcreteEventRuleAdminService>>,
    _auth: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let params = ListAdjustmentRulesParams {
        page: query.page,
        page_size: query.page_size,
        is_enabled: query.is_enabled,
        department_id: query.department_id.clone(),
    };

    let (records, total) = svc
        .list_adjustment_rules(params)
        .await
        .map_err(map_event_rule_admin_error)?;

    let items: Vec<_> = records.into_iter().map(record_to_adjustment_response).collect();

    Ok(ok_resp(&req, DispatchOrderAdjustmentRuleListResponse { items, total }))
}

pub(crate) async fn get_adjustment_rule(
    req: HttpRequest,
    path: web::Path<String>,
    svc: web::Data<Arc<ConcreteEventRuleAdminService>>,
    _auth: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let id = path.into_inner();

    let record = svc.get_adjustment_rule(&id).await.map_err(map_event_rule_admin_error)?;

    Ok(ok_resp(&req, record_to_adjustment_response(record)))
}

pub(crate) async fn create_adjustment_rule(
    req: HttpRequest,
    body: web::Json<DispatchOrderAdjustmentRuleCreate>,
    svc: web::Data<Arc<ConcreteEventRuleAdminService>>,
    _auth: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    if body.event_patterns.is_empty() {
        return Ok(err_resp(&req, "event_patterns cannot be empty"));
    }

    if body.name.trim().is_empty() {
        return Ok(err_resp(&req, "name cannot be empty"));
    }

    let created_by = req
        .headers()
        .get("x-user-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let record = svc
        .create_adjustment_rule(body.into_inner(), created_by.as_deref())
        .await
        .map_err(map_event_rule_admin_error)?;

    Ok(ok_resp(&req, record_to_adjustment_response(record)))
}

pub(crate) async fn update_adjustment_rule(
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<DispatchOrderAdjustmentRuleUpdate>,
    svc: web::Data<Arc<ConcreteEventRuleAdminService>>,
    _auth: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let id = path.into_inner();

    if body.event_patterns.as_ref().map(|v| v.is_empty()).unwrap_or(false) {
        return Ok(err_resp(&req, "event_patterns cannot be empty"));
    }

    let record = svc
        .update_adjustment_rule(&id, body.into_inner())
        .await
        .map_err(map_event_rule_admin_error)?;

    Ok(ok_resp(&req, record_to_adjustment_response(record)))
}

pub(crate) async fn delete_adjustment_rule(
    req: HttpRequest,
    path: web::Path<String>,
    svc: web::Data<Arc<ConcreteEventRuleAdminService>>,
    _auth: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let id = path.into_inner();

    svc.delete_adjustment_rule(&id)
        .await
        .map_err(map_event_rule_admin_error)?;

    Ok(ok_empty(&req))
}

pub(crate) async fn enable_adjustment_rule(
    req: HttpRequest,
    path: web::Path<String>,
    svc: web::Data<Arc<ConcreteEventRuleAdminService>>,
    _auth: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let id = path.into_inner();

    let record = svc
        .enable_adjustment_rule(&id)
        .await
        .map_err(map_event_rule_admin_error)?;

    Ok(ok_resp(&req, record_to_adjustment_response(record)))
}

pub(crate) async fn disable_adjustment_rule(
    req: HttpRequest,
    path: web::Path<String>,
    svc: web::Data<Arc<ConcreteEventRuleAdminService>>,
    _auth: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let id = path.into_inner();

    let record = svc
        .disable_adjustment_rule(&id)
        .await
        .map_err(map_event_rule_admin_error)?;

    Ok(ok_resp(&req, record_to_adjustment_response(record)))
}

// ==================== Generation Rules ====================

pub(crate) async fn list_generation_rules(
    req: HttpRequest,
    query: web::Query<ListGenerationRulesQuery>,
    svc: web::Data<Arc<ConcreteEventRuleAdminService>>,
    _auth: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let params = ListGenerationRulesParams {
        page: query.page,
        page_size: query.page_size,
        is_enabled: query.is_enabled,
        department_id: query.department_id.clone(),
    };

    let (records, total) = svc
        .list_generation_rules(params)
        .await
        .map_err(map_event_rule_admin_error)?;

    let items: Vec<_> = records.into_iter().map(record_to_generation_response).collect();

    Ok(ok_resp(&req, EventDrivenGenerationRuleListResponse { items, total }))
}

pub(crate) async fn get_generation_rule(
    req: HttpRequest,
    path: web::Path<String>,
    svc: web::Data<Arc<ConcreteEventRuleAdminService>>,
    _auth: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let id = path.into_inner();

    let record = svc.get_generation_rule(&id).await.map_err(map_event_rule_admin_error)?;

    Ok(ok_resp(&req, record_to_generation_response(record)))
}

pub(crate) async fn create_generation_rule(
    req: HttpRequest,
    body: web::Json<EventDrivenGenerationRuleCreate>,
    svc: web::Data<Arc<ConcreteEventRuleAdminService>>,
    _auth: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    if body.event_patterns.is_empty() {
        return Ok(err_resp(&req, "event_patterns cannot be empty"));
    }

    if body.name.trim().is_empty() {
        return Ok(err_resp(&req, "name cannot be empty"));
    }

    if body.config.task_type.trim().is_empty() {
        return Ok(err_resp(&req, "task_type cannot be empty"));
    }

    let created_by = req
        .headers()
        .get("x-user-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let record = svc
        .create_generation_rule(body.into_inner(), created_by.as_deref())
        .await
        .map_err(map_event_rule_admin_error)?;

    Ok(ok_resp(&req, record_to_generation_response(record)))
}

pub(crate) async fn update_generation_rule(
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<EventDrivenGenerationRuleUpdate>,
    svc: web::Data<Arc<ConcreteEventRuleAdminService>>,
    _auth: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let id = path.into_inner();

    if body.event_patterns.as_ref().map(|v| v.is_empty()).unwrap_or(false) {
        return Ok(err_resp(&req, "event_patterns cannot be empty"));
    }

    let record = svc
        .update_generation_rule(&id, body.into_inner())
        .await
        .map_err(map_event_rule_admin_error)?;

    Ok(ok_resp(&req, record_to_generation_response(record)))
}

pub(crate) async fn delete_generation_rule(
    req: HttpRequest,
    path: web::Path<String>,
    svc: web::Data<Arc<ConcreteEventRuleAdminService>>,
    _auth: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let id = path.into_inner();

    svc.delete_generation_rule(&id)
        .await
        .map_err(map_event_rule_admin_error)?;

    Ok(ok_empty(&req))
}

pub(crate) async fn enable_generation_rule(
    req: HttpRequest,
    path: web::Path<String>,
    svc: web::Data<Arc<ConcreteEventRuleAdminService>>,
    _auth: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let id = path.into_inner();

    let record = svc
        .enable_generation_rule(&id)
        .await
        .map_err(map_event_rule_admin_error)?;

    Ok(ok_resp(&req, record_to_generation_response(record)))
}

pub(crate) async fn disable_generation_rule(
    req: HttpRequest,
    path: web::Path<String>,
    svc: web::Data<Arc<ConcreteEventRuleAdminService>>,
    _auth: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let id = path.into_inner();

    let record = svc
        .disable_generation_rule(&id)
        .await
        .map_err(map_event_rule_admin_error)?;

    Ok(ok_resp(&req, record_to_generation_response(record)))
}

// ==================== Rule Preview ====================

pub(crate) async fn preview_rules(
    req: HttpRequest,
    body: web::Json<RulePreviewRequest>,
    svc: web::Data<Arc<ConcreteEventRuleAdminService>>,
    _auth: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let payload = body.into_inner();
    let preview_flight_id = resolve_preview_flight_id(&payload);
    let preview_inputs = svc
        .preview_inputs(&preview_flight_id)
        .await
        .map_err(map_event_rule_admin_error)?;

    let mut matched_adjustments = Vec::new();
    for rule in preview_inputs.adjustment_rules {
        if rule_matches_preview(
            &payload.event_type,
            &rule.event_patterns,
            &rule.conditions,
            &payload.payload,
        ) {
            let action_description = get_action_description(&rule.adjuster_type, &rule.config);
            let affected_orders =
                DispatchOrderAdjusterHandler::preview_affected_orders(&rule, &preview_inputs.pending_orders)?
                    .into_iter()
                    .map(|affected_order| RulePreviewAffectedOrder {
                        order_id: affected_order.order_id,
                        task_type: affected_order.task_type,
                        modified_fields: affected_order.modified_fields,
                        reason: affected_order.reason,
                    })
                    .collect();
            matched_adjustments.push(RulePreviewMatchedAdjustment {
                rule_id: rule.id,
                rule_name: rule.name,
                action_type: rule.adjuster_type,
                action_description,
                affected_orders,
            });
        }
    }

    let mut matched_generations = Vec::new();
    for rule in preview_inputs.generation_rules {
        if rule_matches_preview(
            &payload.event_type,
            &rule.event_patterns,
            &rule.conditions,
            &payload.payload,
        ) {
            let generated_order_preview = build_generation_order_preview(&rule, &payload)?;
            matched_generations.push(RulePreviewMatchedGeneration {
                rule_id: rule.id,
                rule_name: rule.name,
                would_generate: true,
                generated_order_preview,
            });
        }
    }

    let response = RulePreviewResponse {
        matched_adjustment_rules: matched_adjustments,
        matched_generation_rules: matched_generations,
        timestamp: chrono::Utc::now(),
    };

    Ok(ok_resp(&req, response))
}
