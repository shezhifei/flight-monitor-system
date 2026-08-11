//! AI v2 路由
//!
//! Rust 继续承接 canonical 公共路径，但 HTTP 语义统一代理到 Python AI sidecar。

use actix_web::{web, HttpRequest, HttpResponse};

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::services::python_sidecar_proxy::{
    ai_sidecar_auth_for_path, ai_sidecar_timeout, ai_sidecar_url, forward_json_request, forward_request,
};

const AI_V2_CANONICAL_PREFIX: &str = "/api/v2/ai/v2";
const AI_V2_INTERNAL_PREFIX: &str = "/internal/ai/v1";

/// List all AI entities
async fn list_entities(req: HttpRequest, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    Ok(forward_ai_v2_request(&req, reqwest::Method::GET).await)
}

/// Get entity status
async fn get_entity_status(req: HttpRequest, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    Ok(forward_ai_v2_request(&req, reqwest::Method::GET).await)
}

/// Submit batch job for entity
async fn submit_batch_job(
    req: HttpRequest,
    claims: JwtAuth,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:execute")?;
    Ok(forward_ai_v2_json(&req, reqwest::Method::POST, &body.into_inner()).await)
}

fn ai_v2_target_url(req: &HttpRequest) -> String {
    let suffix = req.path().strip_prefix(AI_V2_CANONICAL_PREFIX).unwrap_or_default();
    let base = ai_sidecar_url();
    let path = format!("{AI_V2_INTERNAL_PREFIX}{suffix}");
    if req.query_string().trim().is_empty() {
        format!("{base}{path}")
    } else {
        format!("{base}{path}?{}", req.query_string())
    }
}

async fn forward_ai_v2_request(req: &HttpRequest, method: reqwest::Method) -> HttpResponse {
    let target = ai_v2_target_url(req);
    let internal_path = target.strip_prefix(&ai_sidecar_url()).unwrap_or(&target);
    forward_request(
        req,
        method,
        &target,
        ai_sidecar_auth_for_path(internal_path),
        ai_sidecar_timeout(),
    )
    .await
}

async fn forward_ai_v2_json(req: &HttpRequest, method: reqwest::Method, body: &serde_json::Value) -> HttpResponse {
    let target = ai_v2_target_url(req);
    let internal_path = target.strip_prefix(&ai_sidecar_url()).unwrap_or(&target);
    forward_json_request(
        req,
        method,
        &target,
        body,
        ai_sidecar_auth_for_path(internal_path),
        ai_sidecar_timeout(),
    )
    .await
}

/// Register AI v2 路由
fn configure_ai_v2_routes(scope: actix_web::Scope) -> actix_web::Scope {
    scope
        .route("/entities", web::get().to(list_entities))
        .route("/entities/{entity_id}/status", web::get().to(get_entity_status))
        .route("/batch/{entity_id}", web::post().to(submit_batch_job))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(configure_ai_v2_routes(web::scope("/api/v2/ai/v2")));
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;
    use fms_application::schemas::auth_schemas::TokenData;

    fn claims_with_permissions(permissions: &[&str]) -> JwtAuth {
        JwtAuth(TokenData {
            sub: Some("test_user".to_string()),
            email: None,
            username: Some("tester".to_string()),
            token_kind: Some("access".to_string()),
            is_admin: Some(false),
            permissions: permissions.iter().map(|value| value.to_string()).collect(),
            department: None,
            department_id: None,
            pv: None,
            iat: None,
            exp: None,
            iss: None,
            aud: None,
            ua_hash: None,
            ip_subnet_hash: None,
        })
    }

    #[actix_web::test]
    async fn ai_v2_canonical_path_preserves_target() {
        let req = TestRequest::with_uri("/api/v2/ai/v2/entities/entity-1/status").to_http_request();
        assert_eq!(
            ai_v2_target_url(&req),
            "http://localhost:9000/internal/ai/v1/entities/entity-1/status"
        );
    }

    #[test]
    fn ai_v2_batch_permission_requires_execute_or_wildcard() {
        assert!(claims_with_permissions(&["ai:execute"])
            .ensure_permission("ai:execute")
            .is_ok());
        assert!(claims_with_permissions(&["*"]).ensure_permission("ai:execute").is_ok());
        assert!(claims_with_permissions(&["ai:view"])
            .ensure_permission("ai:execute")
            .is_err());
    }
}
