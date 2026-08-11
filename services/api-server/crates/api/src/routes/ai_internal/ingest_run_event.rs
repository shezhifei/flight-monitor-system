use super::*;

pub(crate) async fn ingest_run_event(
    _service_identity: ServiceIdentity,
    service: web::Data<Arc<AiJobService>>,
    path: web::Path<String>,
    body: web::Json<RunEventRequest>,
) -> Result<HttpResponse, ApiError> {
    let run_id = path.into_inner();
    let run = service
        .get_run(&run_id)
        .await
        .map_err(|_| ApiError::Internal("internal error".into()))?;
    service
        .append_event(&run.job_id, &run_id, &body.event_type, body.payload.clone())
        .await
        .map_err(|_| ApiError::Internal("internal error".into()))?;
    Ok(HttpResponse::Ok().json(json!({"success": true})))
}

pub(crate) async fn complete_run(
    _service_identity: ServiceIdentity,
    service: web::Data<Arc<AiJobService>>,
    ingest_service: web::Data<Arc<fms_application::services::ai_proposal_ingest_service::AiProposalIngestService>>,
    _context_service: web::Data<Arc<fms_application::services::ai_context_service::AiContextService>>,
    path: web::Path<String>,
    body: web::Json<CompleteRunRequest>,
) -> Result<HttpResponse, ApiError> {
    let run_id = path.into_inner();
    let run = service
        .get_run(&run_id)
        .await
        .map_err(|_| ApiError::Internal("internal error".into()))?;

    if is_run_terminal(&run.status) {
        return Ok(HttpResponse::Conflict().json(json!({
            "success": false,
            "error": "Run already in terminal state",
            "data": {
                "run_id": run.run_id,
                "status": run.status,
            }
        })));
    }

    let input_envelope = run
        .input_envelope
        .as_ref()
        .ok_or_else(|| ApiError::ValidationError("Run has no input_envelope. Cannot complete run.".into()))?;

    let envelope: fms_domain::models::ai_context_envelope::ContextEnvelope =
        serde_json::from_value(input_envelope.clone())
            .map_err(|_| ApiError::Internal("invalid input envelope".into()))?;

    if envelope.job_id != run.job_id || envelope.run_id != run.run_id {
        return Err(ApiError::Conflict(
            "input_envelope job_id/run_id does not match database record".into(),
        ));
    }

    let mut validated_envelope = envelope;
    validated_envelope.job_id = run.job_id.clone();
    validated_envelope.run_id = run.run_id.clone();

    if let Some(raw) = &body.output_raw {
        if raw
            .get("proposals")
            .and_then(|p| p.as_array())
            .map_or(false, |a| !a.is_empty())
        {
            let output: fms_domain::models::ai_structured_output::AiStructuredOutput =
                serde_json::from_value(raw.clone())
                    .map_err(|_| ApiError::Internal("invalid structured output".into()))?;

            let ingest_result = ingest_service.ingest(output, &validated_envelope).await;

            if !ingest_result.success {
                return Ok(HttpResponse::UnprocessableEntity().json(json!({
                    "success": false,
                    "error": "Validation failed",
                    "details": ingest_result.rejected_proposals,
                })));
            }

            return Ok(HttpResponse::Ok().json(json!({
                "success": true,
                "created_proposal_ids": ingest_result.created_proposal_ids,
                "data": {
                    "run_id": run.run_id,
                    "job_id": run.job_id,
                    "status": "succeeded"
                }
            })));
        }
    }

    let run = service
        .complete_run(
            &run_id,
            body.output_raw.clone(),
            body.output_validated.clone(),
            body.token_usage.clone(),
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let job = service
        .transition_job(&run.job_id, fms_domain::models::ai_job::AiJobStatus::Succeeded)
        .await
        .map_err(|e| {
            tracing::warn!("failed to transition job to succeeded: {}", e);
            ApiError::Internal(e.to_string())
        })?;

    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "data": {
            "run_id": run.run_id,
            "status": run.status,
            "job_id": job.job_id,
            "job_status": job.status,
        }
    })))
}

pub(crate) async fn fail_run(
    _service_identity: ServiceIdentity,
    service: web::Data<Arc<AiJobService>>,
    path: web::Path<String>,
    body: web::Json<FailRunRequest>,
) -> Result<HttpResponse, ApiError> {
    let run_id = path.into_inner();
    let run = service
        .get_run(&run_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    if is_run_terminal(&run.status) {
        return Ok(HttpResponse::Conflict().json(json!({
            "success": false,
            "error": "Run already in terminal state",
            "data": {
                "run_id": run.run_id,
                "status": run.status,
            }
        })));
    }

    let run = service
        .fail_run(
            &run_id,
            body.error_code.as_deref(),
            body.error_message.as_deref(),
            body.output_raw.clone(),
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let job = service
        .transition_job(&run.job_id, fms_domain::models::ai_job::AiJobStatus::FailedTerminal)
        .await
        .map_err(|e| {
            tracing::warn!("failed to transition job to failed_terminal: {}", e);
            ApiError::Internal(e.to_string())
        })?;
    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "data": {
            "run_id": run.run_id,
            "status": run.status,
            "job_id": job.job_id,
            "job_status": job.status,
        }
    })))
}

/// POST /internal/ai/v1/jobs/lease — Python worker leases a pending job.
///
/// Uses `SKIP LOCKED` to atomically claim a Pending job and transition it
/// to Claimed. Returns `null` data when no job is available.
pub(crate) async fn lease_job(
    _service_identity: ServiceIdentity,
    service: web::Data<Arc<AiJobService>>,
    body: web::Json<LeaseJobRequest>,
) -> Result<HttpResponse, ApiError> {
    let job = service
        .lease_job(body.job_type.as_deref(), &body.lease_owner, body.lease_seconds)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(HttpResponse::Ok().json(json!({"success": true, "data": job})))
}

/// POST /internal/ai/v1/jobs/{job_id}/heartbeat — Python worker refreshes lease.
pub(crate) async fn heartbeat_job(
    _service_identity: ServiceIdentity,
    service: web::Data<Arc<AiJobService>>,
    path: web::Path<String>,
    body: web::Json<HeartbeatRequest>,
) -> Result<HttpResponse, ApiError> {
    let job_id = path.into_inner();
    let renewed = service
        .heartbeat_job(&job_id, &body.lease_owner, body.lease_seconds)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(HttpResponse::Ok().json(json!({"success": true, "data": {"renewed": renewed}})))
}

/// GET /internal/ai/v1/jobs/{job_id}/runs — list runs for a job (Python worker).
pub(crate) async fn list_job_runs(
    _service_identity: ServiceIdentity,
    service: web::Data<Arc<AiJobService>>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let job_id = path.into_inner();
    let runs = service
        .list_runs_for_job(&job_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(HttpResponse::Ok().json(json!({"success": true, "data": runs})))
}
