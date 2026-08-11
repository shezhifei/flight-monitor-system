use actix_web::{web, HttpRequest, HttpResponse};
use serde_json::Value;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::services::python_sidecar_proxy::{forward_ai_sidecar_json, forward_ai_sidecar_request};

async fn create_eval_job(req: HttpRequest, body: web::Json<Value>, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:chat")?;
    Ok(forward_ai_sidecar_json(&req, reqwest::Method::POST, &body.into_inner()).await)
}

async fn list_eval_jobs(req: HttpRequest, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:chat")?;
    Ok(forward_ai_sidecar_request(&req, reqwest::Method::GET).await)
}

async fn get_eval_job(req: HttpRequest, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:chat")?;
    Ok(forward_ai_sidecar_request(&req, reqwest::Method::GET).await)
}

async fn cancel_eval_job(req: HttpRequest, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:chat")?;
    Ok(forward_ai_sidecar_request(&req, reqwest::Method::POST).await)
}

async fn compare_eval_profiles(req: HttpRequest, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:chat")?;
    Ok(forward_ai_sidecar_request(&req, reqwest::Method::GET).await)
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/ai/eval")
            .route("/jobs", web::post().to(create_eval_job))
            .route("/jobs", web::get().to(list_eval_jobs))
            .route("/jobs/{job_id}", web::get().to(get_eval_job))
            .route("/jobs/{job_id}/cancel", web::post().to(cancel_eval_job))
            .route("/jobs/{job_id}/compare", web::get().to(compare_eval_profiles)),
    );
}
