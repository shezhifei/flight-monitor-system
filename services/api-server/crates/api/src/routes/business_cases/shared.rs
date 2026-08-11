use actix_web::{HttpRequest, HttpResponse};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::warn;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::routes::flights::invalidate_flight_list_response_cache;
use crate::sse::hub::SseHub;
use crate::types::ConcreteBusinessCaseService as BusinessCaseService;
use fms_application::services::authorization_service::{AuthorizationService, PermissionCatalog};
use fms_application::services::business_case_workflow_service::{BusinessCaseWorkflowService, WorkflowActor};
use fms_application::services::cache_invalidation_service::{CacheInvalidationKey, CacheInvalidationService};
use fms_application::services::flight_cache_service::FlightCacheService;
use fms_application::services::flight_runtime_service::FlightRuntimeService;
use fms_application::services::operator_identity_service::OperatorIdentityService;
use fms_domain::models::business_case::VisibilityScope;

#[derive(Debug, Deserialize)]
pub(crate) struct ListQuery {
    pub(crate) flight_id: Option<String>,
    pub(crate) case_type: Option<String>,
    pub(crate) status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateRequest {
    pub(crate) case_type: String,
    pub(crate) flight_id: String,
    pub(crate) description: String,
    #[serde(default)]
    pub(crate) visibility_scope: Option<VisibilityScope>,
    #[serde(default)]
    pub(crate) status: Option<String>,
    #[serde(default)]
    pub(crate) context: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct UpdateRequest {
    pub(crate) case_type: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) context: Option<HashMap<String, serde_json::Value>>,
    pub(crate) status: Option<String>,
    pub(crate) stand: Option<String>,
    pub(crate) gate: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AppendRequest {
    pub(crate) content: String,
    #[serde(default)]
    pub(crate) mention_user_ids: Vec<String>,
    #[serde(default)]
    pub(crate) client_action_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StatusUpdateRequest {
    pub(crate) status: String,
}

pub(crate) fn ensure_authenticated(claims: &JwtAuth) -> Result<(), ApiError> {
    if AuthorizationService::is_authenticated(&claims.0) {
        Ok(())
    } else {
        Err(ApiError::Unauthorized("未认证".into()))
    }
}

pub(crate) fn has_resource_wildcard(claims: &JwtAuth, permission: &str) -> bool {
    permission
        .split_once('.')
        .map(|(resource, _)| format!("{resource}.*"))
        .map(|wildcard| claims.0.permissions.iter().any(|item| item == &wildcard))
        .unwrap_or(false)
}

pub(crate) fn has_grant(claims: &JwtAuth, permission: &str) -> bool {
    AuthorizationService::has_grant(&claims.0, permission) || has_resource_wildcard(claims, permission)
}

pub(crate) fn ensure_grant(claims: &JwtAuth, permission: &str) -> Result<(), ApiError> {
    if has_grant(claims, permission) {
        return Ok(());
    }
    Err(ApiError::Forbidden(format!("缺少权限: {permission}")))
}

pub(crate) fn actor_name(claims: &JwtAuth) -> &str {
    claims
        .0
        .username
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or(claims.0.sub.as_deref())
        .unwrap_or("unknown")
}

pub(crate) fn viewer_department_id(claims: &JwtAuth) -> Option<&str> {
    AuthorizationService::department_id(&claims.0)
}

pub(crate) fn viewer_department_name(claims: &JwtAuth) -> Option<&str> {
    AuthorizationService::department_name(&claims.0)
}

pub(crate) fn ok_resp(data: impl serde::Serialize, message: &str) -> HttpResponse {
    HttpResponse::Ok().json(json!({
        "success": true,
        "data": data,
        "message": message,
    }))
}

pub(crate) async fn broadcast_business_case_event(hub: Option<&Arc<SseHub>>, event: &str, payload: serde_json::Value) {
    let Some(hub) = hub else {
        return;
    };
    hub.broadcast_event("business_cases", Some(event), payload).await;
}

pub(crate) async fn trigger_business_case_workflow_best_effort(
    workflow_svc: Arc<BusinessCaseWorkflowService>,
    case_type: String,
    case_id: String,
    actor: WorkflowActor,
) {
    if let Err(error) = workflow_svc
        .attach_existing_case_to_workflow(&case_type, &case_id, &actor)
        .await
    {
        warn!(
            case_id = %case_id,
            case_type = %case_type,
            error = %error,
            "business case workflow auto-attach failed"
        );
    }
}

pub(crate) async fn refresh_related_flight_cache(
    flight_id: Option<&str>,
    runtime_svc: &Arc<FlightRuntimeService>,
    cache_svc: Option<&Arc<FlightCacheService>>,
) {
    let Some(flight_id) = flight_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };
    let Some(cache_svc) = cache_svc else {
        return;
    };

    match runtime_svc.build_cached_flight(flight_id).await {
        Ok(Some(flight)) => cache_svc.refresh_single_flight_cache(&flight).await,
        Ok(None) => warn!(flight_id, "skip related flight cache refresh because flight is missing"),
        Err(error) => warn!(
            flight_id,
            error = %error,
            "failed to refresh related single flight cache for business case write"
        ),
    }
}

pub(crate) async fn invalidate_business_case_flight_list_caches(
    cache_invalidation: Option<&Arc<CacheInvalidationService>>,
    flight_id: Option<&str>,
) {
    if let Some(cache_invalidation) = cache_invalidation {
        let event = match flight_id.map(str::trim).filter(|value| !value.is_empty()) {
            Some(flight_id) => cache_invalidation.flight_event(
                flight_id,
                [
                    CacheInvalidationKey::FlightRuntimeProjection,
                    CacheInvalidationKey::FlightListResponse,
                ],
            ),
            None => cache_invalidation.flight_list_event([CacheInvalidationKey::FlightListResponse]),
        };
        cache_invalidation.invalidate_and_publish(event).await;
    } else {
        invalidate_flight_list_response_cache().await;
    }
}

pub(crate) fn normalized_client_action_id(req: &HttpRequest, body_value: Option<String>) -> Option<String> {
    body_value
        .as_deref()
        .or_else(|| {
            req.headers()
                .get("Idempotency-Key")
                .and_then(|value| value.to_str().ok())
        })
        .or_else(|| {
            req.headers()
                .get("X-Idempotency-Key")
                .and_then(|value| value.to_str().ok())
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(crate) fn build_submitted_operator_name(claims: &JwtAuth) -> Option<String> {
    let base = claims
        .0
        .username
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| claims.0.sub.as_deref().map(str::trim).filter(|value| !value.is_empty()))?;

    Some(base.to_string())
}

pub(crate) fn extract_optional_operator_context(
    req: &HttpRequest,
    svc: Option<&OperatorIdentityService>,
) -> Result<(Option<String>, Option<String>), ApiError> {
    let context_type = req
        .headers()
        .get("X-Operator-Context-Type")
        .and_then(|value| value.to_str().ok());
    let context_id = req
        .headers()
        .get("X-Operator-Context-Id")
        .and_then(|value| value.to_str().ok());

    match svc {
        Some(svc) => svc.normalize_context(context_type, context_id).map_err(ApiError::from),
        None => Ok((None, None)),
    }
}

pub(crate) fn resolve_flight_no(flight: &fms_application::schemas::flight_schemas::FlightResponse) -> String {
    flight
        .flight_number
        .clone()
        .or_else(|| {
            flight
                .outbound_leg
                .as_ref()
                .map(|leg| leg.flight_no.clone())
                .filter(|value| !value.trim().is_empty())
        })
        .or_else(|| {
            flight
                .inbound_leg
                .as_ref()
                .map(|leg| leg.flight_no.clone())
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_default()
}
