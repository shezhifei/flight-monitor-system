//! `POST /api/v2/ai/runs/{run_id}/resume` — re-queue a `ResumeRun`
//! command from the latest `BeforeTool` / `AfterTool` checkpoint (or a
//! caller-supplied `from_checkpoint_id`).
//!
//! `GET /api/v2/ai/jobs/{job_id}/runs/{run_id}/checkpoints` — list
//! the run's checkpoints for ops/UI readouts.
//!
//! Both endpoints are read-mostly and live in this file. Rollback
//! goes through `AiActionProposalService` on the compensation routes.

use actix_web::{web, HttpResponse};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use fms_application::services::ai_job_service::{AiJob, AiJobServiceError, AiRun};
use fms_application::services::ai_runtime_service::ai_execution_control_service::{
    AiExecutionControlService, ControlServiceError,
};
use fms_domain::models::ai_execution::AiRunCheckpointRecord;
use fms_domain::models::ai_job::AiRunStatus;

fn ok_resp(data: impl serde::Serialize) -> HttpResponse {
    HttpResponse::Ok().json(json!({ "success": true, "data": data }))
}

fn accepted_resp(data: impl serde::Serialize) -> HttpResponse {
    HttpResponse::Accepted().json(json!({ "success": true, "data": data }))
}

fn ensure_ai_view_permission(claims: &JwtAuth) -> Result<(), ApiError> {
    claims.ensure_permission("ai:view")
}

fn ensure_ai_chat_permission(claims: &JwtAuth) -> Result<(), ApiError> {
    claims.ensure_permission("ai:chat")
}

fn map_job_error(err: AiJobServiceError) -> ApiError {
    match err {
        AiJobServiceError::NotFound(id) => ApiError::NotFound(id),
        AiJobServiceError::Validation(msg) => ApiError::BadRequest(msg),
        AiJobServiceError::Conflict(msg) => ApiError::Conflict(msg),
        AiJobServiceError::ConcurrencyLimitExceeded { .. } => ApiError::Conflict(err.to_string()),
        AiJobServiceError::Internal(msg) => ApiError::Internal(msg),
    }
}

fn map_control_error(err: ControlServiceError) -> ApiError {
    match err {
        ControlServiceError::PayloadParse(msg) => ApiError::BadRequest(msg),
        ControlServiceError::AuthorizationContext(msg) => ApiError::BadRequest(msg),
        ControlServiceError::Authorization(err) => ApiError::Internal(err.to_string()),
        ControlServiceError::Repository(err) => ApiError::Internal(err.to_string()),
        ControlServiceError::InvalidState(msg) => ApiError::Conflict(msg),
    }
}

#[derive(Debug, Deserialize)]
struct ResumeRunRequest {
    #[serde(default)]
    from_checkpoint_id: Option<String>,
}

fn is_resumable(run: &AiRun) -> Result<(), ApiError> {
    let status = AiRunStatus::from_str(&run.status)
        .ok_or_else(|| ApiError::Internal(format!("unknown run status in DB: {}", run.status)))?;
    match status {
        AiRunStatus::Pending
        | AiRunStatus::Claimed
        | AiRunStatus::Running
        | AiRunStatus::FailedRecoverable
        | AiRunStatus::Stale
        | AiRunStatus::TimedOut => Ok(()),
        _ => Err(ApiError::Conflict(format!(
            "run {} is in status {} and is not resumable",
            run.run_id, status
        ))),
    }
}

async fn resolve_resume_target(
    control: &AiExecutionControlService,
    run_id: &str,
    from_checkpoint_id: Option<&str>,
) -> Result<AiRunCheckpointRecord, ApiError> {
    let run_checkpoints = control.list_all_checkpoints(run_id).await.map_err(map_control_error)?;
    if let Some(checkpoint_id) = from_checkpoint_id {
        let target = run_checkpoints
            .into_iter()
            .find(|row| row.checkpoint_id == checkpoint_id)
            .ok_or_else(|| ApiError::NotFound(format!("checkpoint {checkpoint_id} not found for run {run_id}")))?;
        return Ok(target);
    }
    control
        .latest_recoverable_checkpoint(run_id)
        .await
        .map_err(map_control_error)?
        .ok_or_else(|| {
            ApiError::NotFound(format!(
                "no recoverable BeforeTool/AfterTool checkpoint for run {run_id}"
            ))
        })
}

async fn resume_run(
    service: web::Data<Arc<fms_application::services::ai_job_service::AiJobService>>,
    control: web::Data<Arc<AiExecutionControlService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: Option<web::Json<ResumeRunRequest>>,
) -> Result<HttpResponse, ApiError> {
    ensure_ai_chat_permission(&claims)?;

    let run_id = path.into_inner();
    let run = service.get_run(&run_id).await.map_err(map_job_error)?;
    is_resumable(&run)?;

    let job: AiJob = service.get_job(&run.job_id).await.map_err(map_job_error)?;
    let requester_user_id = job
        .requester_user_id
        .clone()
        .ok_or_else(|| ApiError::BadRequest("run has no requester_user_id; cannot resume".to_string()))?;

    let explicit = body.as_ref().and_then(|b| b.from_checkpoint_id.clone());
    let checkpoint = resolve_resume_target(control.get_ref(), &run.run_id, explicit.as_deref()).await?;

    let command = control
        .enqueue_resume_run(&run.job_id, &run.run_id, &checkpoint, &requester_user_id)
        .await
        .map_err(map_control_error)?;

    Ok(accepted_resp(json!({
        "run_id": run.run_id,
        "job_id": run.job_id,
        "command_id": command.command_id,
        "command_type": command.command_type,
        "command_sequence": command.command_sequence,
        "checkpoint": {
            "checkpoint_id": checkpoint.checkpoint_id,
            "sequence_no": checkpoint.sequence_no,
            "checkpoint_type": checkpoint.checkpoint_type.as_str(),
            "tool_call_pk": checkpoint.tool_call_pk,
            "proposal_id": checkpoint.proposal_id,
            "snapshot_hash": checkpoint.snapshot_hash,
            "snapshot": checkpoint.snapshot,
        },
        "requester_user_id": requester_user_id,
    })))
}

async fn list_checkpoints(
    service: web::Data<Arc<fms_application::services::ai_job_service::AiJobService>>,
    control: web::Data<Arc<AiExecutionControlService>>,
    claims: JwtAuth,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, ApiError> {
    ensure_ai_view_permission(&claims)?;

    let (job_id, run_id) = path.into_inner();
    let _ = service.get_job(&job_id).await.map_err(map_job_error)?;
    let run = service.get_run(&run_id).await.map_err(map_job_error)?;
    if run.job_id != job_id {
        return Err(ApiError::BadRequest(format!(
            "run {run_id} does not belong to job {job_id}"
        )));
    }

    let checkpoints = control.list_all_checkpoints(&run_id).await.map_err(map_control_error)?;
    let items: Vec<_> = checkpoints
        .into_iter()
        .map(|row| {
            json!({
                "checkpoint_id": row.checkpoint_id,
                "run_id": row.run_id,
                "job_id": row.job_id,
                "sequence_no": row.sequence_no,
                "checkpoint_type": row.checkpoint_type.as_str(),
                "tool_call_pk": row.tool_call_pk,
                "proposal_id": row.proposal_id,
                "snapshot_hash": row.snapshot_hash,
                "snapshot_size_bytes": row.snapshot_size_bytes,
                "created_at": row.created_at,
            })
        })
        .collect();
    Ok(ok_resp(json!({ "items": items, "total": items.len() })))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/ai")
            .route("/runs/{run_id}/resume", web::post().to(resume_run))
            .route(
                "/jobs/{job_id}/runs/{run_id}/checkpoints",
                web::get().to(list_checkpoints),
            ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::middleware::jwt::JwtAuth;
    use fms_application::schemas::auth_schemas::TokenData;

    fn claims(permissions: &[&str]) -> JwtAuth {
        JwtAuth(TokenData {
            sub: Some("user-1".to_string()),
            email: None,
            username: Some("tester".to_string()),
            token_kind: Some("access".to_string()),
            is_admin: Some(false),
            permissions: permissions.iter().map(|value| value.to_string()).collect(),
            department: None,
            department_id: None,
            pv: Some(1),
            iat: None,
            exp: None,
            iss: None,
            aud: None,
            ua_hash: None,
            ip_subnet_hash: None,
        })
    }

    #[test]
    fn ai_resume_rejects_token_without_ai_chat_permission() {
        let result = ensure_ai_chat_permission(&claims(&["ai:view"]));
        assert!(matches!(result, Err(crate::error::ApiError::Forbidden(_))));
    }

    #[test]
    fn ai_resume_accepts_ai_chat_permission() {
        let result = ensure_ai_chat_permission(&claims(&["ai:chat"]));
        assert!(result.is_ok());
    }

    #[test]
    fn ai_resume_accepts_wildcard_permission() {
        let result = ensure_ai_chat_permission(&claims(&["*"]));
        assert!(result.is_ok());
    }

    #[test]
    fn ai_checkpoints_list_requires_ai_view_permission() {
        let result = ensure_ai_view_permission(&claims(&["ai:chat"]));
        assert!(matches!(result, Err(crate::error::ApiError::Forbidden(_))));
    }

    fn sample_run(status: AiRunStatus) -> AiRun {
        AiRun {
            run_id: "run-1".into(),
            job_id: "job-1".into(),
            runtime_engine: "python-ai-runtime".into(),
            model_id: None,
            status: status.as_str().to_string(),
            input_envelope: None,
            output_raw: None,
            output_validated: None,
            token_usage: None,
            started_at: None,
            finished_at: None,
            error_code: None,
            error_message: None,
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn is_resumable_accepts_pending_running_failed_recoverable_stale_timed_out() {
        for status in [
            AiRunStatus::Pending,
            AiRunStatus::Claimed,
            AiRunStatus::Running,
            AiRunStatus::FailedRecoverable,
            AiRunStatus::Stale,
            AiRunStatus::TimedOut,
        ] {
            let run = sample_run(status);
            assert!(is_resumable(&run).is_ok(), "status {status:?} must be resumable");
        }
    }

    #[test]
    fn is_resumable_rejects_succeeded_failed_terminal_cancelled() {
        for status in [
            AiRunStatus::Succeeded,
            AiRunStatus::FailedTerminal,
            AiRunStatus::Cancelled,
        ] {
            let run = sample_run(status);
            assert!(is_resumable(&run).is_err(), "status {status:?} must not be resumable");
        }
    }
}
