use actix_web::{web, HttpRequest, HttpResponse};
use serde_json::json;
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::services::ai_run_event_payload::sanitize_event_payload_opt;
use crate::services::ai_run_event_types as evt;
use crate::services::ai_runtime_client::AiRuntimeClient;
use crate::services::python_sidecar_proxy::{forward_ai_sidecar_json_deprecated, forward_ai_sidecar_sse_json};
use fms_application::services::ai_context_service::AiContextService;
use fms_application::services::ai_job_service::AiJobService;
use fms_application::services::ai_proposal_ingest_service::AiProposalIngestService;
use fms_domain::models::ai_job::{AiJobStatus, AiRunStatus};
use fms_domain::models::ai_structured_output::AiStructuredOutput;

use super::shared::{current_user_id, target_objects_from_request, NLQueryRequest};

pub(crate) async fn query_natural_language(
    req: HttpRequest,
    claims: JwtAuth,
    body: web::Json<NLQueryRequest>,
    job_service: web::Data<Arc<AiJobService>>,
    context_service: web::Data<Arc<AiContextService>>,
    runtime_client: web::Data<Arc<AiRuntimeClient>>,
    proposal_ingest_service: web::Data<Arc<AiProposalIngestService>>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:chat")?;

    let streaming = body.streaming.unwrap_or(false);
    if streaming {
        let body_value = serde_json::to_value(&*body).map_err(|e| ApiError::BadRequest(e.to_string()))?;
        return Ok(forward_ai_sidecar_sse_json(&req, reqwest::Method::POST, &body_value).await);
    }

    let user_id = current_user_id(&claims);
    let roles: Vec<String> = claims.0.permissions.clone();

    let job = job_service
        .create_job("nl_query", Some(&user_id), None, None, None)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let target_objects = target_objects_from_request(&body);
    let mut envelope = context_service
        .build_envelope(
            &user_id,
            &roles,
            claims.0.department_id.as_deref(),
            "nl_query",
            &body.question,
            &target_objects,
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let run = job_service
        .create_run(&job.job_id, "python-ai-runtime", None, None)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let _ = job_service
        .append_event(&job.job_id, &run.run_id, evt::RUNTIME_STARTED, None)
        .await;

    envelope.job_id = job.job_id.clone();
    envelope.run_id = run.run_id.clone();

    let envelope_value = serde_json::to_value(&envelope).map_err(|e| ApiError::Internal(e.to_string()))?;

    job_service
        .update_run_input_envelope(&run.run_id, envelope_value.clone())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Async path (ADR-0004): return 202 Accepted immediately.
    // The job stays in Pending; the Python worker leases it via
    // `lease_pending` (SKIP LOCKED) and processes asynchronously.
    // Result is delivered via SSE (ai_execution topic) or polled via
    // GET /api/v2/ai/jobs/{job_id}.
    if body.async_mode.unwrap_or(false) {
        return Ok(HttpResponse::Accepted().json(json!({
            "success": true,
            "data": {
                "job_id": job.job_id,
                "run_id": run.run_id,
                "status": "pending",
                "created_at": job.created_at,
            }
        })));
    }

    job_service
        .transition_job(&job.job_id, AiJobStatus::Claimed)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    job_service
        .transition_run(&run.run_id, AiRunStatus::Claimed)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    job_service
        .transition_job(&job.job_id, AiJobStatus::Running)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    job_service
        .transition_run(&run.run_id, AiRunStatus::Running)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let python_resp = runtime_client.start_run(&req, &envelope_value).await;

    if !python_resp.status().is_success() {
        let status_code = python_resp.status();
        let body_bytes = actix_web::body::to_bytes(python_resp.into_body())
            .await
            .unwrap_or_default();
        let body_text = String::from_utf8_lossy(&body_bytes).into_owned();
        let degraded: bool = serde_json::from_str::<serde_json::Value>(&body_text)
            .ok()
            .and_then(|v| v.get("degraded").and_then(|d| d.as_bool()))
            .unwrap_or(true);

        let error_msg = format!("Python AI runtime returned {}", status_code);
        job_service
            .fail_run(&run.run_id, Some("runtime_error"), Some(&error_msg), None)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        job_service
            .transition_job(&job.job_id, AiJobStatus::FailedTerminal)
            .await
            .map_err(|e| {
                tracing::warn!("failed to transition job {} to failed_terminal: {}", job.job_id, e);
                ApiError::Internal(e.to_string())
            })?;

        let _ = job_service
            .append_event(
                &job.job_id,
                &run.run_id,
                evt::RUNTIME_COMPLETED,
                sanitize_event_payload_opt(Some(json!({"error_message": error_msg}))),
            )
            .await;

        return Ok(HttpResponse::ServiceUnavailable().json(json!({
            "success": false,
            "data": {
                "job_id": job.job_id,
                "run_id": run.run_id,
                "status": "failed_terminal",
                "degraded": degraded,
            },
            "message": format!("AI Runtime 返回错误: {}", status_code),
        })));
    }

    let body_bytes = actix_web::body::to_bytes(python_resp.into_body())
        .await
        .unwrap_or_default();
    let body_text = String::from_utf8_lossy(&body_bytes).into_owned();
    let python_body: serde_json::Value = serde_json::from_str(&body_text).unwrap_or(json!({}));

    let business_success = python_body.get("success").and_then(|v| v.as_bool()).unwrap_or(true);
    let run_status = python_body
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("succeeded")
        .to_string();
    let error_field = python_body.get("error").and_then(|v| v.as_str()).map(|s| s.to_string());
    let degraded = python_body.get("degraded").and_then(|v| v.as_bool()).unwrap_or(false);

    if !business_success || run_status == "failed" || run_status == "error" {
        let error_msg = error_field
            .unwrap_or_else(|| format!("Python AI runtime returned business failure: status={}", run_status));
        job_service
            .fail_run(
                &run.run_id,
                Some("business_failure"),
                Some(&error_msg),
                Some(python_body.clone()),
            )
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        job_service
            .transition_job(&job.job_id, AiJobStatus::FailedTerminal)
            .await
            .map_err(|e| {
                tracing::warn!("failed to transition job {} to failed_terminal: {}", job.job_id, e);
                ApiError::Internal(e.to_string())
            })?;

        let _ = job_service
            .append_event(
                &job.job_id,
                &run.run_id,
                evt::RUNTIME_COMPLETED,
                sanitize_event_payload_opt(Some(json!({"error_message": error_msg}))),
            )
            .await;

        return Ok(HttpResponse::UnprocessableEntity().json(json!({
            "success": false,
            "data": {
                "job_id": job.job_id,
                "run_id": run.run_id,
                "status": "failed_terminal",
                "degraded": degraded,
            },
            "message": error_msg,
        })));
    }

    let answer = python_body
        .get("answer")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let has_proposals = python_body
        .get("proposals")
        .and_then(|v| v.as_array())
        .map(|arr| !arr.is_empty())
        .unwrap_or(false);

    if has_proposals {
        let structured_output: AiStructuredOutput = serde_json::from_value(python_body.clone())
            .map_err(|e| ApiError::Internal(format!("invalid structured output: {}", e)))?;

        let _ = job_service
            .append_event(&job.job_id, &run.run_id, evt::PROPOSAL_INGEST_STARTED, None)
            .await;
        let ingest_result = proposal_ingest_service.ingest(structured_output, &envelope).await;

        if !ingest_result.success {
            let error_msg = format!("Proposal validation failed: {:?}", ingest_result.rejected_proposals);
            let _ = job_service
                .append_event(
                    &job.job_id,
                    &run.run_id,
                    evt::PROPOSAL_INGEST_FAILED,
                    sanitize_event_payload_opt(Some(json!({"error_message": error_msg}))),
                )
                .await;
            job_service
                .fail_run(
                    &run.run_id,
                    Some("proposal_validation_failed"),
                    Some(&error_msg),
                    Some(python_body.clone()),
                )
                .await
                .map_err(|e| ApiError::Internal(e.to_string()))?;

            let _ = job_service
                .append_event(
                    &job.job_id,
                    &run.run_id,
                    evt::RUNTIME_COMPLETED,
                    sanitize_event_payload_opt(Some(json!({"error_message": error_msg}))),
                )
                .await;

            return Ok(HttpResponse::UnprocessableEntity().json(json!({
                "success": false,
                "data": {
                    "job_id": job.job_id,
                    "run_id": run.run_id,
                    "status": "failed_terminal",
                    "degraded": false,
                },
                "message": error_msg,
                "rejected_proposals": ingest_result.rejected_proposals,
            })));
        }

        let _ = job_service
            .append_event(&job.job_id, &run.run_id, evt::PROPOSAL_INGEST_SUCCEEDED, None)
            .await;
        job_service
            .complete_run(&run.run_id, Some(python_body.clone()), None, None)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;

        let _ = job_service
            .append_event(&job.job_id, &run.run_id, evt::RUNTIME_COMPLETED, None)
            .await;

        return Ok(HttpResponse::Ok().json(json!({
            "success": true,
            "data": {
                "job_id": job.job_id,
                "run_id": run.run_id,
                "answer": answer,
                "status": "succeeded",
                "degraded": degraded,
                "created_proposal_ids": ingest_result.created_proposal_ids,
            }
        })));
    }

    let output_raw = serde_json::to_value(&python_body).ok();

    job_service
        .complete_run(&run.run_id, output_raw, None, None)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    job_service
        .transition_job(&job.job_id, AiJobStatus::Succeeded)
        .await
        .map_err(|e| {
            tracing::warn!("failed to transition job {} to succeeded: {}", job.job_id, e);
            ApiError::Internal(e.to_string())
        })?;

    let _ = job_service
        .append_event(&job.job_id, &run.run_id, evt::RUNTIME_COMPLETED, None)
        .await;

    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "data": {
            "job_id": job.job_id,
            "run_id": run.run_id,
            "answer": answer,
            "status": run_status,
            "degraded": degraded,
        }
    })))
}

pub(crate) async fn followup_natural_language(
    req: HttpRequest,
    claims: JwtAuth,
    body: web::Json<NLQueryRequest>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:chat")?;
    let body_value = serde_json::to_value(&*body).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok(forward_ai_sidecar_json_deprecated(&req, reqwest::Method::POST, &body_value).await)
}
