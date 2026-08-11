use actix_web::{web, HttpRequest, HttpResponse};

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::services::python_sidecar_proxy::{forward_ai_sidecar_request_deprecated, forward_ai_sidecar_sse_json};

use super::shared::NLQueryRequest;

pub(crate) async fn followup_natural_language_stream(
    req: HttpRequest,
    claims: JwtAuth,
    body: web::Json<NLQueryRequest>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:chat")?;
    let body_value = serde_json::to_value(&*body).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok(forward_ai_sidecar_sse_json(&req, reqwest::Method::POST, &body_value).await)
}

pub(crate) async fn get_query_suggestions(req: HttpRequest, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:chat")?;
    Ok(forward_ai_sidecar_request_deprecated(&req, reqwest::Method::GET).await)
}

pub(crate) async fn list_conversations(req: HttpRequest, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:chat")?;
    Ok(forward_ai_sidecar_request_deprecated(&req, reqwest::Method::GET).await)
}

pub(crate) async fn get_conversation_messages(req: HttpRequest, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:chat")?;
    Ok(forward_ai_sidecar_request_deprecated(&req, reqwest::Method::GET).await)
}

pub(crate) async fn end_conversation(req: HttpRequest, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:chat")?;
    Ok(forward_ai_sidecar_request_deprecated(&req, reqwest::Method::DELETE).await)
}
