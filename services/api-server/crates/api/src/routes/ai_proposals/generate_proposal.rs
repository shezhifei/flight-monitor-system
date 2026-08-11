use super::*;

pub(crate) async fn generate_proposal(
    service: web::Data<Arc<AiActionProposalService>>,
    claims: JwtAuth,
    body: web::Json<ProposalGenerateRequest>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:execute")?;

    let req = GenerateProposalRequest {
        job_id: body.job_id.clone(),
        run_id: body.run_id.clone(),
        ontology_version: body.ontology_version.clone(),
        object_type: body.object_type.clone(),
        object_id: body.object_id.clone(),
        action_name: body.action_name.clone(),
        arguments: body.arguments.clone(),
        reasoning: body.reasoning.clone(),
        confidence: body.confidence,
        requester_user_id: Some(current_user_id(&claims)),
        requester_user_roles: current_permissions(&claims),
        requester_department_id: claims.0.department_id.clone(),
        correlation_id: body.correlation_id.clone(),
        idempotency_key: body.idempotency_key.clone(),
        expected_object_version: body.expected_object_version,
        risk_level: None,
        approval_policy: None,
        required_permissions: None,
    };

    let proposal = service.generate_proposal(req).await.map_err(map_proposal_error)?;

    Ok(ok_resp(proposal))
}

pub(crate) async fn validate_proposal(
    service: web::Data<Arc<AiActionProposalService>>,
    claims: JwtAuth,
    body: web::Json<ProposalValidateRequest>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:execute")?;

    let req = ValidateProposalRequest {
        proposal_id: body.proposal_id.clone(),
        before_snapshot: Some(body.before_snapshot.clone()),
        after_preview: Some(body.after_preview.clone()),
        constraint_results: body.constraint_results.clone(),
    };

    let proposal = service.validate_proposal(req).await.map_err(map_proposal_error)?;

    Ok(ok_resp(proposal))
}

pub(crate) async fn approve_proposal(
    service: web::Data<Arc<AiActionProposalService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<ProposalApproveRequest>,
) -> Result<HttpResponse, ApiError> {
    let req = ApproveProposalRequest {
        proposal_id: path.into_inner(),
        approver_id: current_user_id(&claims),
        approver_permissions: current_permissions(&claims),
        approver_department_id: claims.0.department_id.clone(),
        modified_arguments: if body.modified_arguments.is_object()
            && body
                .modified_arguments
                .as_object()
                .map(|o| !o.is_empty())
                .unwrap_or(false)
        {
            Some(body.modified_arguments.clone())
        } else {
            None
        },
    };

    let proposal = service.approve_proposal(req).await.map_err(map_proposal_error)?;

    Ok(ok_resp(proposal))
}

pub(crate) async fn reject_proposal(
    service: web::Data<Arc<AiActionProposalService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<ProposalRejectRequest>,
) -> Result<HttpResponse, ApiError> {
    let req = RejectProposalRequest {
        proposal_id: path.into_inner(),
        rejecter_id: current_user_id(&claims),
        reason: body.reason.clone(),
    };

    let proposal = service.reject_proposal(req).await.map_err(map_proposal_error)?;

    Ok(ok_resp(proposal))
}

pub(crate) async fn execute_proposal(
    service: web::Data<Arc<AiActionProposalService>>,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let req = ExecuteProposalRequest {
        proposal_id: path.into_inner(),
        executor_id: current_user_id(&claims),
        executor_permissions: current_permissions(&claims),
        executor_department_id: claims.0.department_id.clone(),
    };

    let proposal = service.execute_proposal(req).await.map_err(map_proposal_error)?;

    Ok(ok_resp(proposal))
}

pub(crate) async fn get_proposal(
    service: web::Data<Arc<AiActionProposalService>>,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;

    let proposal = service
        .get_proposal(&path.into_inner())
        .await
        .map_err(map_proposal_error)?;

    Ok(ok_resp(proposal))
}

pub(crate) async fn list_proposals(
    service: web::Data<Arc<AiActionProposalService>>,
    claims: JwtAuth,
    query: web::Query<ProposalListQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;

    let status = query
        .status
        .as_ref()
        .and_then(|s| fms_domain::models::ai_proposal::ActionProposalStatus::from_code(parse_status_code(s)));

    let q = ActionProposalQuery {
        job_id: query.job_id.clone(),
        run_id: query.run_id.clone(),
        object_type: query.object_type.clone(),
        object_id: query.object_id.clone(),
        action_name: query.action_name.clone(),
        status,
        risk_level: None,
        approval_policy: None,
        requester_user_id: None,
        pending_action_id: None,
        idempotency_key: None,
        created_after: None,
        created_before: None,
        limit: Some(query.limit),
        offset: Some(query.offset),
    };

    let proposals = service.list_proposals(&q).await.map_err(map_proposal_error)?;

    Ok(ok_resp(proposals))
}

pub(crate) async fn get_proposal_stats(
    service: web::Data<Arc<AiActionProposalService>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;

    let stats = service.get_stats().await.map_err(map_proposal_error)?;

    Ok(ok_resp(stats))
}

pub(crate) async fn expire_stale_proposals(
    service: web::Data<Arc<AiActionProposalService>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:execute")?;

    let count = service.expire_stale_proposals().await.map_err(map_proposal_error)?;

    Ok(ok_resp(json!({ "expired_count": count })))
}
