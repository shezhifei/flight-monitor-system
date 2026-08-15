use actix_web::{web, HttpResponse};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use fms_application::services::ai_job_service::{AiJobService, AiJobServiceError};

#[derive(Debug, Deserialize)]
struct JobListQuery {
    status: Option<String>,
    #[allow(dead_code)]
    job_type: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

#[derive(Debug, Deserialize)]
struct CreateJobRequest {
    task_type: String,
    payload: Value,
    #[serde(default)]
    timeout_ms: Option<i64>,
}

fn default_limit() -> i64 {
    50
}

fn ok_resp(data: impl serde::Serialize) -> HttpResponse {
    HttpResponse::Ok().json(json!({ "success": true, "data": data }))
}

fn ensure_ai_view_permission(claims: &JwtAuth) -> Result<(), ApiError> {
    claims.ensure_permission("ai:view")
}

fn ensure_ai_chat_permission(claims: &JwtAuth) -> Result<(), ApiError> {
    claims.ensure_permission("ai:chat")
}

fn current_user_id(claims: &JwtAuth) -> String {
    claims
        .0
        .sub
        .clone()
        .or_else(|| claims.0.username.clone())
        .unwrap_or_else(|| "unknown_user".to_string())
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

/// POST /api/v2/ai/jobs — submit a new AI job (ADR-0004 async path).
///
/// Creates a job + run with the payload as `input_envelope`, leaves both
/// in `Pending` so the Python worker can lease via `SKIP LOCKED`.
/// Returns 202 Accepted with job_id / run_id.
async fn submit_job(
    service: web::Data<Arc<AiJobService>>,
    claims: JwtAuth,
    body: web::Json<CreateJobRequest>,
) -> Result<HttpResponse, ApiError> {
    ensure_ai_chat_permission(&claims)?;

    let user_id = current_user_id(&claims);
    let job = service
        .create_job(&body.task_type, Some(&user_id), None, None, None)
        .await
        .map_err(map_job_error)?;

    let run = service
        .create_run(&job.job_id, "python-ai-runtime", None, Some(body.payload.clone()))
        .await
        .map_err(map_job_error)?;

    Ok(HttpResponse::Accepted().json(json!({
        "success": true,
        "data": {
            "job_id": job.job_id,
            "run_id": run.run_id,
            "status": "pending",
            "created_at": job.created_at,
        }
    })))
}

/// DELETE /api/v2/ai/jobs/{job_id} — cancel a pending/running job (ADR-0004).
async fn cancel_job_handler(
    service: web::Data<Arc<AiJobService>>,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    ensure_ai_chat_permission(&claims)?;

    let job_id = path.into_inner();
    let job = service.cancel_job(&job_id, None).await.map_err(map_job_error)?;
    Ok(ok_resp(json!({
        "job_id": job.job_id,
        "status": job.status,
    })))
}

async fn list_jobs(
    service: web::Data<Arc<AiJobService>>,
    claims: JwtAuth,
    query: web::Query<JobListQuery>,
) -> Result<HttpResponse, ApiError> {
    ensure_ai_view_permission(&claims)?;

    let jobs = service
        .list_jobs(query.status.as_deref(), query.limit, query.offset)
        .await
        .map_err(map_job_error)?;
    Ok(ok_resp(jobs))
}

async fn get_job(
    service: web::Data<Arc<AiJobService>>,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    ensure_ai_view_permission(&claims)?;

    let job_id = path.into_inner();
    let job = service.get_job(&job_id).await.map_err(map_job_error)?;
    let runs = service.list_runs_for_job(&job_id).await.map_err(map_job_error)?;
    Ok(ok_resp(json!({
        "job": job,
        "runs_count": runs.len(),
        "runs_summary": runs.iter().map(|r| json!({
            "run_id": r.run_id,
            "status": r.status,
            "runtime_engine": r.runtime_engine,
            "created_at": r.created_at,
        })).collect::<Vec<_>>(),
    })))
}

async fn list_runs(
    service: web::Data<Arc<AiJobService>>,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    ensure_ai_view_permission(&claims)?;

    let job_id = path.into_inner();
    let runs = service.list_runs_for_job(&job_id).await.map_err(map_job_error)?;
    Ok(ok_resp(runs))
}

async fn get_run(
    service: web::Data<Arc<AiJobService>>,
    claims: JwtAuth,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, ApiError> {
    ensure_ai_view_permission(&claims)?;

    let (_job_id, run_id) = path.into_inner();
    let run = service.get_run(&run_id).await.map_err(map_job_error)?;
    Ok(ok_resp(run))
}

async fn list_run_events(
    service: web::Data<Arc<AiJobService>>,
    claims: JwtAuth,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, ApiError> {
    ensure_ai_view_permission(&claims)?;

    let (_job_id, run_id) = path.into_inner();
    let events = service.list_events_for_run(&run_id, 500).await.map_err(map_job_error)?;
    Ok(ok_resp(events))
}

async fn get_job_stats(service: web::Data<Arc<AiJobService>>, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    ensure_ai_view_permission(&claims)?;

    let stats = service.get_job_stats().await.map_err(map_job_error)?;
    Ok(ok_resp(stats))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/ai/jobs")
            .route("", web::get().to(list_jobs))
            .route("", web::post().to(submit_job))
            .route("/stats", web::get().to(get_job_stats))
            .route("/{job_id}", web::get().to(get_job))
            .route("/{job_id}", web::delete().to(cancel_job_handler))
            .route("/{job_id}/runs", web::get().to(list_runs))
            .route("/{job_id}/runs/{run_id}", web::get().to(get_run))
            .route("/{job_id}/runs/{run_id}/events", web::get().to(list_run_events)),
    );
}

#[cfg(test)]
mod tests {
    use super::ensure_ai_view_permission;
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
    fn ai_jobs_rejects_token_without_ai_view_permission() {
        let result = ensure_ai_view_permission(&claims(&["ai:execute"]));

        assert!(matches!(result, Err(crate::error::ApiError::Forbidden(_))));
    }

    #[test]
    fn ai_jobs_accepts_ai_view_permission() {
        let result = ensure_ai_view_permission(&claims(&["ai:view"]));

        assert!(result.is_ok());
    }

    #[test]
    fn ai_jobs_accepts_wildcard_permission() {
        let result = ensure_ai_view_permission(&claims(&["*"]));

        assert!(result.is_ok());
    }
}
