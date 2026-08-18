//! Internal ontology action endpoints for the agent loop.
//!
//! These endpoints let the Python sidecar execute registered ontology
//! read/advisory actions on behalf of a run, without going through the public
//! user-facing `/api/v2/ai/ontology/actions/*` routes (which require a user
//! JWT). Authentication is Service Identity; authorization is recomputed from
//! the requester persisted on the run's job — the sidecar-supplied body never
//! conveys trust.
//!
//! Contract (frozen, asserted in tests):
//! ```text
//! POST /internal/ai/v1/ontology/actions/read
//! POST /internal/ai/v1/ontology/actions/advisory
//! { "run_id": "run_...", "action_name": "flight.get_context", "arguments": {...} }
//! ```
//!
//! Failure codes (HTTP + JSON `error_code`):
//! - no/bad Service Identity            → 401
//! - unknown read/advisory action       → 400 `unknown read action` / `unknown advisory action`
//! - run_id not found                   → 404 `AI_RUN_NOT_FOUND`
//! - requester lacks required permission→ 403 `TOOL_ACTOR_PERMISSION_DENIED`
//! - object not found                   → 404 (reuses `OntologyActionError::NotFound`)

use crate::routes::ai_ontology::{dispatch_advisory_action, dispatch_read_action};
use crate::error::ApiError;
use crate::middleware::service_identity::ServiceIdentity;
use actix_web::{web, HttpResponse};
use fms_application::services::ai_job_service::{AiJobService, AiJobServiceError};
use fms_application::services::ontology_actions::{
    advisory_action_permission, read_action_permission, OntologyActionError, OntologyActionServices,
};
use fms_domain::ports::ai_auth_context_loader::{AuthContextLoaderError, RunAuthorizationContextLoader};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

/// Internal tool-call identifier used when loading the authorization context.
/// Ontology actions are not persisted as `ai_tool_calls` rows by this path, so
/// a stable synthetic key is sufficient for the loader's bookkeeping.
const INTERNAL_ONTOLOGY_TOOL_CALL_PK: &str = "internal-ontology-action";

#[derive(Debug, Deserialize)]
pub(crate) struct InternalOntologyActionRequest {
    pub(crate) run_id: String,
    pub(crate) action_name: String,
    #[serde(default = "default_arguments")]
    pub(crate) arguments: Value,
}

fn default_arguments() -> Value {
    json!({})
}

fn error_json(error_code: &str, message: &str, extra: Value) -> Value {
    let mut body = json!({
        "success": false,
        "error_code": error_code,
        "error": message,
    });
    if let (Some(obj), Some(extra)) = (body.as_object_mut(), extra.as_object()) {
        for (k, v) in extra {
            obj.insert(k.clone(), v.clone());
        }
    }
    body
}

/// Returns true when the persisted requester permissions satisfy `required`.
/// Matches the user-facing `PermissionCheck` semantics: exact grant, global
/// `*`, or a resource-level wildcard (`resource:*`).
fn permissions_grant(permissions: &[String], required: &str) -> bool {
    if permissions.iter().any(|p| p == "*" || p == required) {
        return true;
    }
    if let Some((resource, _)) = required.split_once(':') {
        let wildcard = format!("{resource}:*");
        return permissions.iter().any(|p| p == &wildcard);
    }
    false
}

fn map_loader_error(err: AuthContextLoaderError, run_id: &str) -> ApiError {
    match err {
        AuthContextLoaderError::RunNotFound(_) => ApiError::NotFound(format!("AI_RUN_NOT_FOUND {run_id}")),
        AuthContextLoaderError::JobNotFound(_) | AuthContextLoaderError::RequesterNotFound(_) => {
            // Fail closed: if we cannot establish who requested the run, we must
            // not grant any ontology action.
            ApiError::Forbidden("TOOL_ACTOR_PERMISSION_DENIED".into())
        }
        AuthContextLoaderError::EntityConfigNotFound(_) | AuthContextLoaderError::Internal(_) => {
            ApiError::Internal("internal error".into())
        }
    }
}

fn map_action_error(error: OntologyActionError) -> ApiError {
    match error {
        OntologyActionError::InvalidArguments(msg) => ApiError::BadRequest(msg),
        OntologyActionError::NotFound(msg) => ApiError::NotFound(msg),
        OntologyActionError::Repository(msg) | OntologyActionError::Internal(msg) => ApiError::Internal(msg),
    }
}

/// Common core for the internal read/advisory endpoints. Validates the action
/// whitelist, resolves the run, recomputes the requester's permissions from
/// Rust-persisted data, enforces them, and dispatches to the shared service.
async fn execute_internal_action(
    job_service: &AiJobService,
    auth_loader: &Arc<dyn RunAuthorizationContextLoader + Send + Sync>,
    actions: &OntologyActionServices,
    body: InternalOntologyActionRequest,
    required_permission: Option<&'static str>,
    is_advisory: bool,
) -> Result<HttpResponse, ApiError> {
    let kind = if is_advisory { "advisory" } else { "read" };

    // 1. Whitelist the action name up front so unknown actions are rejected
    //    without touching storage (and independent of run state).
    let required_permission = required_permission.ok_or_else(|| {
        ApiError::BadRequest(format!("unknown {kind} action: {}", body.action_name))
    })?;

    // 2. Resolve the run. Missing run → 404 AI_RUN_NOT_FOUND.
    let run = match job_service.get_run(&body.run_id).await {
        Ok(run) => run,
        Err(AiJobServiceError::NotFound(_)) => {
            return Ok(HttpResponse::NotFound().json(error_json(
                "AI_RUN_NOT_FOUND",
                &format!("run not found: {}", body.run_id),
                json!({ "run_id": body.run_id }),
            )));
        }
        Err(_) => return Err(ApiError::Internal("internal error".into())),
    };

    // 3. Recompute the requester's permissions from Rust-persisted data. The
    //    sidecar body is never trusted for authorization.
    let context = auth_loader
        .load_context(
            &body.run_id,
            &run.job_id,
            INTERNAL_ONTOLOGY_TOOL_CALL_PK,
            &body.action_name,
            &body.arguments,
        )
        .await
        .map_err(|err| map_loader_error(err, &body.run_id))?;

    // 4. Enforce the action's required permission against the requester.
    if !permissions_grant(&context.requester_permissions, required_permission) {
        return Ok(HttpResponse::Forbidden().json(error_json(
            "TOOL_ACTOR_PERMISSION_DENIED",
            &format!(
                "requester lacks permission '{}' for action '{}'",
                required_permission, body.action_name
            ),
            json!({ "action_name": body.action_name, "required_permission": required_permission }),
        )));
    }

    // 5. Dispatch through the shared service surface (same as the public face).
    let result = if is_advisory {
        dispatch_advisory_action(actions, &body.action_name, &body.arguments).await
    } else {
        dispatch_read_action(actions, &body.action_name, &body.arguments).await
    };
    result
        .map(|value| HttpResponse::Ok().json(value))
        .map_err(map_action_error)
}

pub(crate) async fn execute_read_action_internal(
    _service_identity: ServiceIdentity,
    job_service: web::Data<Arc<AiJobService>>,
    auth_loader: web::Data<Arc<dyn RunAuthorizationContextLoader + Send + Sync>>,
    actions: web::Data<Arc<OntologyActionServices>>,
    body: web::Json<InternalOntologyActionRequest>,
) -> Result<HttpResponse, ApiError> {
    let required = read_action_permission(&body.action_name);
    execute_internal_action(
        &job_service,
        auth_loader.get_ref(),
        &actions,
        body.into_inner(),
        required,
        false,
    )
    .await
}

pub(crate) async fn execute_advisory_action_internal(
    _service_identity: ServiceIdentity,
    job_service: web::Data<Arc<AiJobService>>,
    auth_loader: web::Data<Arc<dyn RunAuthorizationContextLoader + Send + Sync>>,
    actions: web::Data<Arc<OntologyActionServices>>,
    body: web::Json<InternalOntologyActionRequest>,
) -> Result<HttpResponse, ApiError> {
    let required = advisory_action_permission(&body.action_name);
    execute_internal_action(
        &job_service,
        auth_loader.get_ref(),
        &actions,
        body.into_inner(),
        required,
        true,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permissions_grant_matches_exact() {
        assert!(permissions_grant(&["flight:read".to_string()], "flight:read"));
        assert!(!permissions_grant(&["flight:read".to_string()], "dispatch:read"));
    }

    #[test]
    fn permissions_grant_matches_global_wildcard() {
        assert!(permissions_grant(&["*".to_string()], "dispatch:read"));
    }

    #[test]
    fn permissions_grant_matches_resource_wildcard() {
        assert!(permissions_grant(&["flight:*".to_string()], "flight:read"));
        assert!(!permissions_grant(&["dispatch:*".to_string()], "flight:read"));
    }

    #[test]
    fn permissions_grant_denies_empty() {
        assert!(!permissions_grant(&[], "flight:read"));
    }

    #[test]
    fn request_defaults_arguments_to_empty_object() {
        let raw = r#"{"run_id":"run_1","action_name":"flight.search"}"#;
        let req: InternalOntologyActionRequest = serde_json::from_str(raw).expect("deserialize");
        assert_eq!(req.run_id, "run_1");
        assert_eq!(req.action_name, "flight.search");
        assert_eq!(req.arguments, json!({}));
    }

    #[test]
    fn error_json_carries_error_code_and_extras() {
        let body = error_json(
            "TOOL_ACTOR_PERMISSION_DENIED",
            "denied",
            json!({ "action_name": "flight.get_context" }),
        );
        assert_eq!(body["success"], false);
        assert_eq!(body["error_code"], "TOOL_ACTOR_PERMISSION_DENIED");
        assert_eq!(body["error"], "denied");
        assert_eq!(body["action_name"], "flight.get_context");
    }
}
