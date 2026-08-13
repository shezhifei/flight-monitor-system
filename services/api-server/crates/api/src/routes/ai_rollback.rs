//! Rollback API (`/api/v2/ai/proposals/{proposal_id}/...`).
//!
//! Endpoints (consumed by the operations UI and the agent self-test
//! scripts):
//!
//! * `GET  /api/v2/ai/proposals/{proposal_id}/compensation-plan`
//!   — list compensation plans for the proposal.
//! * `POST /api/v2/ai/proposals/{proposal_id}/rollback`
//!   — enqueue a compensation execution. Self-approved when
//!   `compensation.requires_approval` is `false`.
//! * `POST /api/v2/ai/proposals/{proposal_id}/compensation/{compensation_id}/approve`
//!   — explicit approver gate.
//!
//! The handlers re-validate the executor / approver against the
//! required permissions on the underlying action, refuse to roll back
//! when the object's current version has drifted past the snapshot
//! stored in the receipt, and append (never mutate) the audit trail
//! via a fresh `ai_action_receipts` row produced by the
//! compensation execution path.

use actix_web::{web, HttpResponse};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;

use fms_application::services::ai_runtime_service::rollback_service::{RollbackError, RollbackService};
use fms_domain::models::ai_execution::AiCompensationStatus;

#[derive(Debug, Deserialize)]
pub(crate) struct RollbackRequest {
    pub compensation_id: String,
    #[serde(default)]
    pub approver_user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ApproveCompensationRequest {
    pub approver_user_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CompensationPlanQuery {
    #[serde(default)]
    pub status: Option<String>,
}

fn ok_resp(data: impl serde::Serialize) -> HttpResponse {
    HttpResponse::Ok().json(json!({ "success": true, "data": data }))
}

fn accepted_resp(data: impl serde::Serialize) -> HttpResponse {
    HttpResponse::Accepted().json(json!({ "success": true, "data": data }))
}

fn ensure_ai_execute_permission(claims: &JwtAuth) -> Result<(), ApiError> {
    claims.ensure_permission("ai:execute")
}

fn current_user_id(claims: &JwtAuth) -> String {
    claims
        .0
        .sub
        .clone()
        .or_else(|| claims.0.username.clone())
        .unwrap_or_else(|| "unknown_user".to_string())
}

fn current_permissions(claims: &JwtAuth) -> Vec<String> {
    let mut permissions = claims.0.permissions.clone();
    if claims.0.is_admin.unwrap_or(false) && !permissions.iter().any(|p| p == "*") {
        permissions.push("*".to_string());
    }
    permissions
}

fn map_rollback_error(err: RollbackError) -> ApiError {
    match err {
        RollbackError::ProposalNotFound { proposal_id } => {
            ApiError::NotFound(format!("proposal {proposal_id} not found"))
        }
        RollbackError::CompensationNotFound { compensation_id } => {
            ApiError::NotFound(format!("compensation plan {compensation_id} not found"))
        }
        RollbackError::CompensationNotPlanned {
            compensation_id,
            status,
        } => ApiError::Conflict(format!(
            "compensation {compensation_id} is in status {status} and cannot transition"
        )),
        RollbackError::ApproverNotPermitted { approver_id } => ApiError::Forbidden(format!(
            "approver {approver_id} is not permitted to approve this rollback"
        )),
        RollbackError::ObjectVersionConflict {
            object_type,
            object_id,
            expected_version,
            current_version,
        } => ApiError::Conflict(format!(
            "object version drift for {object_type} {object_id}: expected {expected_version}, current {current_version}"
        )),
        RollbackError::Irreversible => ApiError::Conflict(
            "compensation plan is irreversible; rollback is not possible, generate a correction proposal instead"
                .to_string(),
        ),
        RollbackError::Planner(err) => ApiError::Internal(err.to_string()),
        RollbackError::Repository(err) => ApiError::Internal(err.to_string()),
        RollbackError::DomainExecutorUnavailable(msg) => ApiError::Internal(msg),
        RollbackError::DomainExecutorFailed(msg) => ApiError::Internal(msg),
        RollbackError::Internal(msg) => ApiError::Internal(msg),
    }
}

fn parse_status_filter(value: Option<&str>) -> Result<Option<AiCompensationStatus>, ApiError> {
    match value {
        None => Ok(None),
        Some(raw) => AiCompensationStatus::from_str(raw)
            .map(Some)
            .ok_or_else(|| ApiError::BadRequest(format!("unknown compensation status '{raw}'"))),
    }
}

async fn list_compensation_plans(
    rollback: web::Data<Arc<RollbackService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    query: web::Query<CompensationPlanQuery>,
) -> Result<HttpResponse, ApiError> {
    ensure_ai_execute_permission(&claims)?;
    let proposal_id = path.into_inner();
    let mut plans = rollback
        .list_compensations_for_proposal(&proposal_id)
        .await
        .map_err(map_rollback_error)?;
    if let Some(status) = parse_status_filter(query.status.as_deref())? {
        plans.retain(|p| p.status == status);
    }
    let receipts = rollback
        .list_receipts_for_proposal(&proposal_id)
        .await
        .map_err(map_rollback_error)?;
    let items: Vec<_> = plans
        .iter()
        .map(|plan| {
            json!({
                "compensation_id": plan.compensation_id,
                "receipt_id": plan.receipt_id,
                "proposal_id": plan.proposal_id,
                "status": plan.status.as_str(),
                "mode": plan.mode.as_str(),
                "plan": plan.plan,
                "requires_approval": plan.requires_approval,
                "approved_by": plan.approved_by,
                "approved_at": plan.approved_at,
                "executed_by": plan.executed_by,
                "executed_at": plan.executed_at,
                "execution_result": plan.execution_result,
                "execution_error": plan.execution_error,
                "created_at": plan.created_at,
                "updated_at": plan.updated_at,
            })
        })
        .collect();
    let receipt_items: Vec<_> = receipts
        .iter()
        .map(|r| {
            json!({
                "receipt_id": r.receipt_id,
                "proposal_id": r.proposal_id,
                "run_id": r.run_id,
                "object_type": r.object_type,
                "object_id": r.object_id,
                "action_name": r.action_name,
                "idempotency_key": r.idempotency_key,
                "before_checkpoint_id": r.before_checkpoint_id,
                "after_checkpoint_id": r.after_checkpoint_id,
                "executed_by": r.executed_by,
                "executed_at": r.executed_at,
            })
        })
        .collect();
    Ok(ok_resp(json!({
        "proposal_id": proposal_id,
        "plans": items,
        "receipts": receipt_items,
    })))
}

async fn rollback_proposal(
    rollback: web::Data<Arc<RollbackService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<RollbackRequest>,
) -> Result<HttpResponse, ApiError> {
    ensure_ai_execute_permission(&claims)?;
    let proposal_id = path.into_inner();
    let body = body.into_inner();
    let permissions = current_permissions(&claims);
    let actor = current_user_id(&claims);

    let plan = rollback
        .plan_repo()
        .get(&body.compensation_id)
        .await
        .map_err(RollbackError::from)
        .map_err(map_rollback_error)?
        .ok_or_else(|| ApiError::NotFound(format!("compensation plan {} not found", body.compensation_id)))?;
    if plan.proposal_id != proposal_id {
        return Err(ApiError::BadRequest(format!(
            "compensation {} does not belong to proposal {}",
            body.compensation_id, proposal_id
        )));
    }

    if plan.requires_approval {
        let approver = body.approver_user_id.clone().unwrap_or_else(|| actor.clone());
        let _approved = rollback
            .approve_compensation(&body.compensation_id, &approver, &permissions)
            .await
            .map_err(map_rollback_error)?;
    }

    let after = rollback
        .execute_compensation(&body.compensation_id, &actor)
        .await
        .map_err(map_rollback_error)?;

    Ok(accepted_resp(json!({
        "proposal_id": proposal_id,
        "compensation_id": after.compensation_id,
        "receipt_id": after.receipt_id,
        "status": after.status.as_str(),
        "mode": after.mode.as_str(),
        "executed_by": after.executed_by,
        "executed_at": after.executed_at,
        "execution_result": after.execution_result,
    })))
}

async fn approve_compensation(
    rollback: web::Data<Arc<RollbackService>>,
    claims: JwtAuth,
    path: web::Path<(String, String)>,
    body: web::Json<ApproveCompensationRequest>,
) -> Result<HttpResponse, ApiError> {
    ensure_ai_execute_permission(&claims)?;
    let (_proposal_id, compensation_id) = path.into_inner();
    let permissions = current_permissions(&claims);
    let plan = rollback
        .approve_compensation(&compensation_id, &body.approver_user_id, &permissions)
        .await
        .map_err(map_rollback_error)?;
    Ok(ok_resp(json!({
        "compensation_id": plan.compensation_id,
        "proposal_id": plan.proposal_id,
        "status": plan.status.as_str(),
        "approved_by": plan.approved_by,
        "approved_at": plan.approved_at,
    })))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/ai")
            .route(
                "/proposals/{proposal_id}/compensation-plan",
                web::get().to(list_compensation_plans),
            )
            .route("/proposals/{proposal_id}/rollback", web::post().to(rollback_proposal))
            .route(
                "/proposals/{proposal_id}/compensation/{compensation_id}/approve",
                web::post().to(approve_compensation),
            ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_filter_recognizes_known_values() {
        for raw in ["planned", "approved", "executing", "succeeded", "failed", "cancelled"] {
            let parsed = parse_status_filter(Some(raw)).unwrap();
            assert!(parsed.is_some());
        }
        assert!(parse_status_filter(Some("garbage")).is_err());
        assert!(parse_status_filter(None).unwrap().is_none());
    }
}
