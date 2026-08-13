use actix_web::{HttpRequest, HttpResponse};

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::services::python_sidecar_proxy::forward_ai_sidecar_request_deprecated;

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
