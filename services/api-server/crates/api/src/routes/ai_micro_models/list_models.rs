use super::*;

pub(crate) async fn list_models(
    registry: web::Data<Arc<MicroModelRegistry>>,
    claims: JwtAuth,
    query: web::Query<ModelListQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;

    let models = if let Some(category_str) = &query.category {
        let category = fms_domain::models::micro_model::MicroModelCategory::from_str_loose(category_str);
        match category {
            Some(cat) => registry.list_by_category(cat),
            None => registry.list_all(),
        }
    } else if query.proposal_capable == Some(true) {
        registry.list_proposal_capable()
    } else {
        registry.list_all()
    };

    let result: Vec<Value> = models
        .into_iter()
        .map(|m| {
            let enabled = registry.is_enabled(&m.model_id);
            json!({
                "model_id": m.model_id,
                "name": m.name,
                "description": m.description,
                "category": m.category.label(),
                "execution_mode": m.execution_mode.label(),
                "ontology_objects": m.ontology_objects,
                "proposal_capable": m.proposal_capable,
                "timeout_ms": m.timeout_ms,
                "version": m.version,
                "enabled": enabled,
                "feature_flag": m.feature_flag,
                "evaluation_dataset_id": m.evaluation_dataset_id,
            })
        })
        .collect();

    Ok(ok_resp(result))
}

pub(crate) async fn get_model(
    registry: web::Data<Arc<MicroModelRegistry>>,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;

    let model_id = path.into_inner();
    match registry.get(&model_id) {
        Some(m) => {
            let enabled = registry.is_enabled(&m.model_id);
            Ok(ok_resp(json!({
                "model_id": m.model_id,
                "name": m.name,
                "description": m.description,
                "category": m.category.label(),
                "execution_mode": m.execution_mode.label(),
                "ontology_objects": m.ontology_objects,
                "input_schema": m.input_schema,
                "output_schema": m.output_schema,
                "advisory_output": m.advisory_output,
                "proposal_capable": m.proposal_capable,
                "timeout_ms": m.timeout_ms,
                "max_retries": m.max_retries,
                "version": m.version,
                "allowed_actions": m.allowed_actions,
                "enabled": enabled,
                "feature_flag": m.feature_flag,
                "evaluation_dataset_id": m.evaluation_dataset_id,
            })))
        }
        None => Err(ApiError::NotFound(format!("model not found: {}", model_id))),
    }
}

pub(crate) async fn execute_model(
    registry: web::Data<Arc<MicroModelRegistry>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<ExecuteRequest>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:execute")?;

    let model_id = path.into_inner();

    // Verify model exists
    let spec = registry
        .get(&model_id)
        .ok_or_else(|| ApiError::NotFound(format!("model not found: {}", model_id)))?;

    // Feature flag gate
    if !registry.is_enabled(&model_id) {
        let flag_name = spec.feature_flag.as_deref().unwrap_or("unknown");
        return Err(ApiError::Forbidden(format!(
            "micro-model {} is disabled (feature flag {} is not enabled)",
            model_id, flag_name
        )));
    }

    let execution_id = format!("exec_{}", ulid::Ulid::new());
    let executor = MicroModelExecutor::new(Arc::clone(&registry));

    match executor.execute(&model_id, &body.input) {
        Ok(result) => {
            let input_snapshot = if body.include_input_snapshot {
                Some(body.input.clone())
            } else {
                None
            };

            let response = MicroModelExecuteResponse {
                execution_id,
                model_id: model_id.clone(),
                model_version: result.model_version,
                status: "success".to_string(),
                output: result.output,
                execution_time_ms: result.execution_time_ms,
                proposal_candidates: if body.generate_proposals {
                    result.proposal_candidates
                } else {
                    vec![]
                },
                canonical_proposals_created: vec![],
                input_snapshot,
                error: None,
            };

            Ok(ok_resp(response))
        }
        Err(err) => {
            // Distinguish input validation errors from other failures
            if err.contains("invalid") && err.contains("input") {
                Err(ApiError::BadRequest(err))
            } else {
                let response = MicroModelExecuteResponse {
                    execution_id,
                    model_id: model_id.clone(),
                    model_version: spec.version.clone(),
                    status: "failed".to_string(),
                    output: Value::Null,
                    execution_time_ms: 0,
                    proposal_candidates: vec![],
                    canonical_proposals_created: vec![],
                    input_snapshot: None,
                    error: Some(err),
                };

                Ok(ok_resp(response))
            }
        }
    }
}
