pub(crate) use crate::error::ApiError;
pub(crate) use crate::middleware::service_identity::ServiceIdentity;
pub(crate) use actix_web::HttpResponse;
pub(crate) use fms_application::services::ai_job_service::AiJobService;
pub(crate) use serde::Deserialize;
pub(crate) use serde_json::{json, Value};
pub(crate) use std::sync::Arc;
#[derive(Deserialize)]
pub(crate) struct RunEventRequest {
    pub(crate) event_type: String,
    pub(crate) payload: Option<Value>,
}

#[derive(Deserialize)]
pub(crate) struct CompleteRunRequest {
    pub(crate) output_raw: Option<Value>,
    pub(crate) output_validated: Option<Value>,
    pub(crate) token_usage: Option<Value>,
}

#[derive(Deserialize)]
pub(crate) struct FailRunRequest {
    pub(crate) error_code: Option<String>,
    pub(crate) error_message: Option<String>,
    pub(crate) output_raw: Option<Value>,
}

#[derive(Deserialize)]
pub(crate) struct LeaseJobRequest {
    pub(crate) job_type: Option<String>,
    pub(crate) lease_owner: String,
    #[serde(default = "default_lease_seconds")]
    pub(crate) lease_seconds: i64,
}

#[derive(Deserialize)]
pub(crate) struct HeartbeatRequest {
    pub(crate) lease_owner: String,
    #[serde(default = "default_lease_seconds")]
    pub(crate) lease_seconds: i64,
}

fn default_lease_seconds() -> i64 {
    60
}

/// Pure function: returns true for terminal run statuses.
/// Used by both complete_run and fail_run handlers to guard against
/// double-terminal updates.
pub(crate) fn is_run_terminal(status: &str) -> bool {
    matches!(status, "succeeded" | "failed_terminal" | "cancelled")
}

/// Standard internal-face error body: `{ success, error_code, error, ...extras }`.
pub(crate) fn error_json(error_code: &str, message: &str, extra: Value) -> Value {
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
pub(crate) fn permissions_grant(permissions: &[String], required: &str) -> bool {
    if permissions.iter().any(|p| p == "*" || p == required) {
        return true;
    }
    if let Some((resource, _)) = required.split_once(':') {
        let wildcard = format!("{resource}:*");
        return permissions.iter().any(|p| p == &wildcard);
    }
    false
}

/// Maps authorization-context loader failures to the internal-face HTTP codes.
pub(crate) fn map_loader_error(
    err: fms_domain::ports::ai_auth_context_loader::AuthContextLoaderError,
    run_id: &str,
) -> ApiError {
    use fms_domain::ports::ai_auth_context_loader::AuthContextLoaderError;
    match err {
        AuthContextLoaderError::RunNotFound(_) => ApiError::NotFound(format!("AI_RUN_NOT_FOUND {run_id}")),
        AuthContextLoaderError::JobNotFound(_) | AuthContextLoaderError::RequesterNotFound(_) => {
            // Fail closed: if we cannot establish who requested the run, we must
            // not grant any action.
            ApiError::Forbidden("TOOL_ACTOR_PERMISSION_DENIED".into())
        }
        AuthContextLoaderError::EntityConfigNotFound(_) | AuthContextLoaderError::Internal(_) => {
            ApiError::Internal("internal error".into())
        }
    }
}
