use actix_web::{web, HttpRequest, HttpResponse};
use serde::Deserialize;
use serde_json::json;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::services::python_sidecar_proxy::{
    ai_sidecar_auth_for_path, ai_sidecar_timeout, ai_sidecar_url, forward_request,
};

#[derive(Debug, Deserialize)]
pub struct ToolsExplainQuery {
    pub entity: String,
    pub tool: String,
    #[serde(default)]
    pub task_type: Option<String>,
}

/// GET /internal/ai/v1/tools/explain?entity=<id>&tool=<name>[&task_type=...]
///
/// Debug endpoint: returns the full governance decision chain for one
/// entity × tool pair. Proxies to the Python sidecar which re-runs
/// capability_resolver (no new storage). Permission model matches
/// runtime_health (`ai:view`).
pub async fn tools_explain(
    query: web::Query<ToolsExplainQuery>,
    req: HttpRequest,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;

    let entity = query.entity.trim();
    let tool = query.tool.trim();
    if entity.is_empty() || tool.is_empty() {
        return Ok(HttpResponse::BadRequest().json(json!({
            "success": false,
            "error": "Query params 'entity' and 'tool' are required",
            "code": "MISSING_QUERY_PARAMS",
        })));
    }

    let internal_path = "/internal/ai/v1/tools/explain";
    let target = if req.query_string().trim().is_empty() {
        format!("{}{}", ai_sidecar_url(), internal_path)
    } else {
        format!(
            "{}{}?{}",
            ai_sidecar_url(),
            internal_path,
            req.query_string()
        )
    };

    Ok(forward_request(
        &req,
        reqwest::Method::GET,
        &target,
        ai_sidecar_auth_for_path(internal_path),
        ai_sidecar_timeout(),
    )
    .await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_requires_entity_and_tool_fields() {
        let raw = r#"{"entity":"ent-1","tool":"list_flights"}"#;
        let q: ToolsExplainQuery = serde_json::from_str(raw).expect("deserialize");
        assert_eq!(q.entity, "ent-1");
        assert_eq!(q.tool, "list_flights");
        assert!(q.task_type.is_none());
    }

    #[test]
    fn query_accepts_optional_task_type() {
        let raw = r#"{"entity":"ent-1","tool":"assign_gate","task_type":"dispatch_ops"}"#;
        let q: ToolsExplainQuery = serde_json::from_str(raw).expect("deserialize");
        assert_eq!(q.task_type.as_deref(), Some("dispatch_ops"));
    }
}
