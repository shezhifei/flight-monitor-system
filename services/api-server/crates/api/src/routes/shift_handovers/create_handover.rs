use super::*;

pub(crate) async fn create_handover(
    svc: web::Data<Arc<ShiftHandoverService>>,
    auth_svc: web::Data<Arc<AuthService>>,
    operator_identity_svc: Option<web::Data<Arc<OperatorIdentityService>>>,
    req: HttpRequest,
    claims: JwtAuth,
    body: web::Json<ShiftHandoverCreateRequest>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission(PermissionCatalog::SHIFT_HANDOVER_CREATE)?;
    body.validate().map_err(invalid_shift_handover_request)?;
    let actor_user_id = actor_user_id(&claims)?;
    let (context_type, context_id) =
        extract_optional_operator_context(&req, operator_identity_svc.as_ref().map(|svc| svc.get_ref().as_ref()))?;
    let actor_profile = load_user_with_context(
        auth_svc.get_ref().as_ref(),
        operator_identity_svc.as_ref().map(|svc| svc.get_ref().as_ref()),
        context_type.as_deref(),
        context_id.as_deref(),
        actor_user_id,
    )
    .await?;

    let created = svc
        .create(
            body.shift_date,
            &body.shift_code,
            body.from_user_id.as_deref(),
            &body.to_user_id,
            body.summary.clone(),
            &body.risk_level,
            body.items
                .iter()
                .map(|item| ShiftHandoverItemCreateInput {
                    item_type: item.item_type.clone(),
                    title: item.title.clone(),
                    detail: item.detail.clone(),
                    owner_user_id: item.owner_user_id.clone(),
                    due_at: item.due_at,
                    is_mandatory: item.is_mandatory,
                })
                .collect(),
            actor_user_id,
            actor_profile.as_ref().and_then(effective_operator_name_for_user),
            actor_profile.as_ref().and_then(resolve_operator_job_title_for_user),
        )
        .await
        .map_err(map_shift_handover_error)?;

    let fallbacks = load_user_fallbacks(
        auth_svc.get_ref().as_ref(),
        operator_identity_svc.as_ref().map(|svc| svc.get_ref().as_ref()),
        context_type.as_deref(),
        context_id.as_deref(),
        [&created.from_user_id, &created.to_user_id],
    )
    .await?;

    Ok(HttpResponse::Created().json(to_handover_response(created, &fallbacks)))
}

pub(crate) async fn list_handovers(
    svc: web::Data<Arc<ShiftHandoverService>>,
    auth_svc: web::Data<Arc<AuthService>>,
    operator_identity_svc: Option<web::Data<Arc<OperatorIdentityService>>>,
    req: HttpRequest,
    claims: JwtAuth,
    query: web::Query<ShiftHandoverListQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission(PermissionCatalog::SHIFT_HANDOVER_READ)?;
    let items = svc
        .list(
            query.shift_date,
            query.shift_code.as_deref(),
            query.status.as_deref(),
            query.from_user_id.as_deref(),
            query.to_user_id.as_deref(),
            query.limit.unwrap_or(50).clamp(1, 200),
            query.offset.unwrap_or(0).max(0),
        )
        .await
        .map_err(map_shift_handover_error)?;

    let (context_type, context_id) =
        extract_optional_operator_context(&req, operator_identity_svc.as_ref().map(|svc| svc.get_ref().as_ref()))?;
    let fallbacks = load_user_fallbacks_for_handovers(
        auth_svc.get_ref().as_ref(),
        operator_identity_svc.as_ref().map(|svc| svc.get_ref().as_ref()),
        context_type.as_deref(),
        context_id.as_deref(),
        &items,
    )
    .await?;
    let payload: Vec<ShiftHandoverResponse> = items
        .into_iter()
        .map(|handover| to_handover_response(handover, &fallbacks))
        .collect();
    Ok(HttpResponse::Ok().json(payload))
}

pub(crate) async fn list_candidates(
    auth_svc: web::Data<Arc<AuthService>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission(PermissionCatalog::SHIFT_HANDOVER_READ)?;
    let current_user_id = actor_user_id(&claims)?.to_string();
    let users = auth_svc.list_users().await.map_err(ApiError::from)?;

    let payload: Vec<ShiftHandoverCandidateResponse> = users
        .into_iter()
        .filter(|user| user.id != current_user_id)
        .map(|user| {
            let display_name = display_name_for_user(&user);
            let display_label = compose_operator_label(
                display_name.as_deref().or(Some(user.username.as_str())),
                user.job_title
                    .as_deref()
                    .or(if user.is_admin { Some("admin") } else { Some("用户") }),
            );

            ShiftHandoverCandidateResponse {
                user_id: user.id,
                username: user.username,
                display_name,
                job_title: user.job_title,
                display_label,
            }
        })
        .collect();

    Ok(HttpResponse::Ok().json(payload))
}

pub(crate) async fn preview_system_draft(
    svc: web::Data<Arc<ShiftHandoverService>>,
    claims: JwtAuth,
    query: web::Query<ShiftHandoverSystemDraftPreviewQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission(PermissionCatalog::SHIFT_HANDOVER_READ)?;
    let payload = svc
        .preview_system_draft(actor_user_id(&claims)?, query.to_user_id.as_deref())
        .await
        .map_err(map_shift_handover_error)?;

    Ok(HttpResponse::Ok().json(ApiResponse::ok_with_message(
        payload,
        "Shift handover system draft preview loaded",
    )))
}

pub(crate) async fn get_handover(
    svc: web::Data<Arc<ShiftHandoverService>>,
    auth_svc: web::Data<Arc<AuthService>>,
    operator_identity_svc: Option<web::Data<Arc<OperatorIdentityService>>>,
    req: HttpRequest,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission(PermissionCatalog::SHIFT_HANDOVER_READ)?;
    let handover_id = path.into_inner();
    let handover = svc.get(&handover_id).await.map_err(ApiError::from)?;
    let Some(handover) = handover else {
        return Err(ApiError::NotFound("shift handover not found".into()));
    };

    let (context_type, context_id) =
        extract_optional_operator_context(&req, operator_identity_svc.as_ref().map(|svc| svc.get_ref().as_ref()))?;
    let fallbacks = load_user_fallbacks(
        auth_svc.get_ref().as_ref(),
        operator_identity_svc.as_ref().map(|svc| svc.get_ref().as_ref()),
        context_type.as_deref(),
        context_id.as_deref(),
        [&handover.from_user_id, &handover.to_user_id],
    )
    .await?;

    Ok(HttpResponse::Ok().json(to_handover_response(handover, &fallbacks)))
}

pub(crate) async fn submit_handover(
    svc: web::Data<Arc<ShiftHandoverService>>,
    auth_svc: web::Data<Arc<AuthService>>,
    operator_identity_svc: Option<web::Data<Arc<OperatorIdentityService>>>,
    req: HttpRequest,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission(PermissionCatalog::SHIFT_HANDOVER_SUBMIT)?;
    let actor_user_id = actor_user_id(&claims)?;
    let handover = svc
        .submit(&path.into_inner(), actor_user_id, claims.0.is_admin.unwrap_or(false))
        .await
        .map_err(map_shift_handover_error)?;

    let Some(handover) = handover else {
        return Err(ApiError::NotFound("shift handover not found".into()));
    };

    let (context_type, context_id) =
        extract_optional_operator_context(&req, operator_identity_svc.as_ref().map(|svc| svc.get_ref().as_ref()))?;
    let fallbacks = load_user_fallbacks(
        auth_svc.get_ref().as_ref(),
        operator_identity_svc.as_ref().map(|svc| svc.get_ref().as_ref()),
        context_type.as_deref(),
        context_id.as_deref(),
        [&handover.from_user_id, &handover.to_user_id],
    )
    .await?;

    Ok(HttpResponse::Ok().json(to_handover_response(handover, &fallbacks)))
}

pub(crate) async fn acknowledge_item(
    svc: web::Data<Arc<ShiftHandoverService>>,
    claims: JwtAuth,
    path: web::Path<(String, String)>,
    body: web::Json<ShiftHandoverItemAcknowledgeRequest>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission(PermissionCatalog::SHIFT_HANDOVER_ACK)?;
    let actor_user_id = actor_user_id(&claims)?;
    let (handover_id, item_id) = path.into_inner();
    let item = svc
        .acknowledge_item(
            &handover_id,
            &item_id,
            actor_user_id,
            body.acknowledged,
            claims.0.is_admin.unwrap_or(false),
        )
        .await
        .map_err(map_shift_handover_error)?;

    let Some(item) = item else {
        return Err(ApiError::NotFound("shift handover item not found".into()));
    };

    Ok(HttpResponse::Ok().json(to_item_response(item)))
}

pub(crate) async fn acknowledge_handover(
    svc: web::Data<Arc<ShiftHandoverService>>,
    auth_svc: web::Data<Arc<AuthService>>,
    operator_identity_svc: Option<web::Data<Arc<OperatorIdentityService>>>,
    req: HttpRequest,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission(PermissionCatalog::SHIFT_HANDOVER_ACK)?;
    let actor_user_id = actor_user_id(&claims)?;
    let (context_type, context_id) =
        extract_optional_operator_context(&req, operator_identity_svc.as_ref().map(|svc| svc.get_ref().as_ref()))?;
    let actor_profile = load_user_with_context(
        auth_svc.get_ref().as_ref(),
        operator_identity_svc.as_ref().map(|svc| svc.get_ref().as_ref()),
        context_type.as_deref(),
        context_id.as_deref(),
        actor_user_id,
    )
    .await?;

    let handover = svc
        .complete(
            &path.into_inner(),
            actor_user_id,
            claims.0.is_admin.unwrap_or(false),
            actor_profile.as_ref().and_then(effective_operator_name_for_user),
            actor_profile.as_ref().and_then(resolve_operator_job_title_for_user),
        )
        .await
        .map_err(map_shift_handover_error)?;

    let Some(handover) = handover else {
        return Err(ApiError::NotFound("shift handover not found".into()));
    };

    let fallbacks = load_user_fallbacks(
        auth_svc.get_ref().as_ref(),
        operator_identity_svc.as_ref().map(|svc| svc.get_ref().as_ref()),
        context_type.as_deref(),
        context_id.as_deref(),
        [&handover.from_user_id, &handover.to_user_id],
    )
    .await?;

    Ok(HttpResponse::Ok().json(ApiResponse::ok_with_message(
        to_handover_response(handover, &fallbacks),
        "Shift handover acknowledged",
    )))
}
