//! Internal read-only replan snapshot endpoint for the agent loop (Task I1).
//!
//! Lets the Python sidecar fetch the deterministic replan snapshot that the
//! browser solver consumes, on behalf of a run, so `dispatch_ops` proposals
//! can be grounded in solver candidates. Authentication is Service Identity;
//! authorization is recomputed from the requester persisted on the run's job
//! — the sidecar-supplied body never conveys trust (same model as the
//! ontology internal endpoints, Task F1).
//!
//! Read-only by construction: the mutating `replan-apply` surface stays
//! user-JWT-only and is never exposed on this internal face.
//!
//! Contract (frozen, asserted in tests):
//! ```text
//! POST /internal/ai/v1/dispatch/replan-snapshot
//! { "run_id": "run_...", "window_start": "...", "window_end": "...",
//!   "strategy": "balanced", "max_suggestions": 20, "order_ids": [] }
//! ```
//!
//! Failure codes (HTTP + JSON `error_code`):
//! - no/bad Service Identity            → 401
//! - inverted window                    → 400
//! - unknown strategy / bad bounds      → 422 (ValidationError, same as public face)
//! - run_id not found                   → 404 `AI_RUN_NOT_FOUND`
//! - requester lacks `dispatch:read`    → 403 `TOOL_ACTOR_PERMISSION_DENIED`

use crate::error::ApiError;
use crate::middleware::service_identity::ServiceIdentity;
use crate::routes::dispatch::shared::public_replan_snapshot_payload;
use actix_web::{web, HttpResponse};
use chrono::{DateTime, Utc};
use fms_application::schemas::dispatch_schemas::DispatchReplanSnapshotQuery;
use fms_application::services::ai_job_service::{AiJobService, AiJobServiceError};
use fms_application::services::dispatch_frontend_replan_service::DispatchFrontendReplanService;
use fms_domain::ports::ai_auth_context_loader::RunAuthorizationContextLoader;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use super::shared::{error_json, map_loader_error, permissions_grant};

/// Internal tool-call identifier used when loading the authorization context.
/// This path does not persist `ai_tool_calls` rows, so a stable synthetic key
/// is sufficient for the loader's bookkeeping.
const INTERNAL_REPLAN_SNAPSHOT_TOOL_CALL_PK: &str = "internal-replan-snapshot";

/// Agent-facing tool name recorded in the authorization context.
const SOLVER_CANDIDATE_TOOL_NAME: &str = "dispatch.list_solver_candidates";

/// Run permission required to read replan snapshots. Matches the dispatch
/// read ontology actions (e.g. `dispatch.get_status`).
const REQUIRED_PERMISSION: &str = "dispatch:read";

#[derive(Debug, Deserialize)]
pub(crate) struct InternalReplanSnapshotRequest {
    pub(crate) run_id: String,
    pub(crate) window_start: DateTime<Utc>,
    pub(crate) window_end: DateTime<Utc>,
    /// Omitted → the shared schema default applies (same as the public face).
    #[serde(default)]
    pub(crate) strategy: Option<String>,
    #[serde(default)]
    pub(crate) max_suggestions: Option<i64>,
    /// Optional filter hint carried for schema symmetry with the agent tool
    /// schema; the snapshot itself always covers the full window.
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) order_ids: Vec<String>,
}

pub(crate) async fn replan_snapshot_internal(
    _service_identity: ServiceIdentity,
    job_service: web::Data<Arc<AiJobService>>,
    auth_loader: web::Data<Arc<dyn RunAuthorizationContextLoader + Send + Sync>>,
    replan_svc: web::Data<Arc<DispatchFrontendReplanService>>,
    body: web::Json<InternalReplanSnapshotRequest>,
) -> Result<HttpResponse, ApiError> {
    let body = body.into_inner();

    // 1. Build the shared query through serde so the schema defaults apply,
    //    then normalize: bad input fails before any storage access, matching
    //    the public face's validation order.
    let mut raw = json!({
        "window_start": body.window_start,
        "window_end": body.window_end,
    });
    if let Some(strategy) = &body.strategy {
        raw["strategy"] = json!(strategy);
    }
    if let Some(max_suggestions) = body.max_suggestions {
        raw["max_suggestions"] = json!(max_suggestions);
    }
    let query: DispatchReplanSnapshotQuery =
        serde_json::from_value(raw).map_err(|err| ApiError::ValidationError(err.to_string()))?;
    let query = query.normalize().map_err(ApiError::ValidationError)?;
    if query.window_end <= query.window_start {
        return Err(ApiError::BadRequest("window_end 必须晚于 window_start".to_string()));
    }

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
    let arguments = json!({
        "window_start": body.window_start,
        "window_end": body.window_end,
    });
    let context = auth_loader
        .load_context(
            &body.run_id,
            &run.job_id,
            INTERNAL_REPLAN_SNAPSHOT_TOOL_CALL_PK,
            SOLVER_CANDIDATE_TOOL_NAME,
            &arguments,
        )
        .await
        .map_err(|err| map_loader_error(err, &body.run_id))?;

    // 4. Enforce `dispatch:read` against the requester.
    if !permissions_grant(&context.requester_permissions, REQUIRED_PERMISSION) {
        return Ok(HttpResponse::Forbidden().json(error_json(
            "TOOL_ACTOR_PERMISSION_DENIED",
            &format!("requester lacks permission '{REQUIRED_PERMISSION}' for {SOLVER_CANDIDATE_TOOL_NAME}"),
            json!({ "required_permission": REQUIRED_PERMISSION }),
        )));
    }

    // 5. Build the snapshot through the same application service the public
    //    face uses; response shape matches `public_replan_snapshot_payload`.
    let payload = replan_svc
        .build_snapshot(
            query.window_start,
            query.window_end,
            query.strategy.clone(),
            query.max_suggestions,
        )
        .await?;
    Ok(HttpResponse::Ok().json(public_replan_snapshot_payload(&payload)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_deserializes_with_optional_fields() {
        let raw = r#"{
            "run_id": "run_1",
            "window_start": "2026-08-18T00:00:00Z",
            "window_end": "2026-08-18T06:00:00Z"
        }"#;
        let req: InternalReplanSnapshotRequest = serde_json::from_str(raw).expect("deserialize");
        assert_eq!(req.run_id, "run_1");
        assert!(req.strategy.is_none());
        assert!(req.max_suggestions.is_none());
        assert!(req.order_ids.is_empty());
    }

    #[test]
    fn request_carries_order_ids_when_provided() {
        let raw = r#"{
            "run_id": "run_1",
            "window_start": "2026-08-18T00:00:00Z",
            "window_end": "2026-08-18T06:00:00Z",
            "strategy": "stability",
            "order_ids": ["o-1", "o-2"]
        }"#;
        let req: InternalReplanSnapshotRequest = serde_json::from_str(raw).expect("deserialize");
        assert_eq!(req.strategy.as_deref(), Some("stability"));
        assert_eq!(req.order_ids, vec!["o-1".to_string(), "o-2".to_string()]);
    }

    #[test]
    fn request_requires_window_bounds() {
        let raw = r#"{"run_id": "run_1", "window_start": "2026-08-18T00:00:00Z"}"#;
        assert!(serde_json::from_str::<InternalReplanSnapshotRequest>(raw).is_err());
    }

    #[test]
    fn required_permission_is_dispatch_read() {
        assert_eq!(REQUIRED_PERMISSION, "dispatch:read");
    }
}
