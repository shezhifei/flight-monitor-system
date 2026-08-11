pub(crate) use crate::error::ApiError;
pub(crate) use crate::middleware::service_identity::ServiceIdentity;
pub(crate) use actix_web::{web, HttpResponse};
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
