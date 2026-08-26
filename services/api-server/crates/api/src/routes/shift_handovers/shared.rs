//! Shift handover routes aligned with the Python API.

pub(crate) use actix_web::{web, HttpRequest, HttpResponse};
pub(crate) use chrono::NaiveDate;
pub(crate) use fms_application::schemas::auth_schemas::UserResponse;
pub(crate) use fms_application::schemas::response::ApiResponse;
pub(crate) use fms_application::schemas::shift_handover_schemas::{
    ShiftHandoverCandidateResponse, ShiftHandoverCompleteRequest, ShiftHandoverCreateRequest,
    ShiftHandoverItemAcknowledgeRequest, ShiftHandoverItemResponse, ShiftHandoverResponse,
};
pub(crate) use fms_application::services::auth_service::AuthService;
pub(crate) use fms_application::services::authorization_service::PermissionCatalog;
pub(crate) use fms_application::services::operator_identity_service::OperatorIdentityService;
pub(crate) use fms_application::services::shift_handover_service::{
    ShiftHandoverItemCreateInput, ShiftHandoverService,
};
pub(crate) use fms_domain::error::DomainError;
pub(crate) use fms_domain::models::shift_handover::{ShiftHandover, ShiftHandoverItem};
pub(crate) use serde::Deserialize;
pub(crate) use std::collections::HashMap;
pub(crate) use std::sync::Arc;

pub(crate) use crate::error::ApiError;
pub(crate) use crate::middleware::jwt::JwtAuth;
pub(crate) use crate::middleware::permissions::PermissionCheck;
#[derive(Debug, serde::Deserialize)]
pub struct ShiftHandoverListQuery {
    pub shift_date: Option<NaiveDate>,
    pub shift_code: Option<String>,
    pub status: Option<String>,
    pub from_user_id: Option<String>,
    pub to_user_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ShiftHandoverSystemDraftPreviewQuery {
    pub to_user_id: Option<String>,
}

#[derive(Clone)]
pub(crate) struct UserFallback {
    pub(crate) name: Option<String>,
    pub(crate) job_title: Option<String>,
    pub(crate) label: Option<String>,
}

pub(crate) async fn load_user_fallbacks<'a, I>(
    auth_svc: &AuthService,
    operator_identity_svc: Option<&OperatorIdentityService>,
    context_type: Option<&str>,
    context_id: Option<&str>,
    user_ids: I,
) -> Result<HashMap<String, UserFallback>, ApiError>
where
    I: IntoIterator<Item = &'a String>,
{
    let mut fallbacks = HashMap::new();
    for user_id in user_ids {
        let normalized = user_id.trim();
        if normalized.is_empty() || fallbacks.contains_key(normalized) {
            continue;
        }
        if let Some(user) =
            load_user_with_context(auth_svc, operator_identity_svc, context_type, context_id, normalized).await?
        {
            fallbacks.insert(normalized.to_string(), user_fallback_from_user(user));
        }
    }
    Ok(fallbacks)
}

pub(crate) fn actor_user_id(claims: &JwtAuth) -> Result<&str, ApiError> {
    claims
        .0
        .sub
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ApiError::Unauthorized("missing authenticated user".into()))
}

pub(crate) fn map_shift_handover_error(error: DomainError) -> ApiError {
    match error {
        DomainError::ValidationError(message) => ApiError::BadRequest(message),
        DomainError::PermissionDenied(message) => ApiError::Forbidden(message),
        DomainError::Conflict(message) => ApiError::Conflict(message),
        DomainError::NotFound { entity_type, id } => ApiError::NotFound(format!("{entity_type} (id={id}) not found")),
        DomainError::Unauthorized(message) => ApiError::Unauthorized(message),
        DomainError::InvalidStateTransition { from, to } => {
            ApiError::Conflict(format!("invalid state transition: {from} -> {to}"))
        }
        DomainError::BusinessRuleViolation(message) => ApiError::Conflict(message),
        DomainError::BusinessRuleViolationWithDetails { message, .. } => ApiError::Conflict(message),
        DomainError::ConcurrencyConflict(message) => ApiError::Conflict(message),
        DomainError::Internal(message) => ApiError::Internal(message),
    }
}

pub(crate) fn invalid_shift_handover_request(_: DomainError) -> ApiError {
    ApiError::BadRequest("invalid shift handover request".into())
}

pub(crate) fn to_handover_response(
    handover: ShiftHandover,
    fallbacks: &HashMap<String, UserFallback>,
) -> ShiftHandoverResponse {
    let from_fallback = fallbacks.get(&handover.from_user_id);
    let to_fallback = fallbacks.get(&handover.to_user_id);
    let from_operator_name = handover
        .from_operator_name
        .clone()
        .or_else(|| from_fallback.and_then(|value| value.name.clone()));
    let from_operator_job_title = handover
        .from_operator_job_title
        .clone()
        .or_else(|| from_fallback.and_then(|value| value.job_title.clone()));
    let to_operator_name = handover
        .to_operator_name
        .clone()
        .or_else(|| to_fallback.and_then(|value| value.name.clone()));
    let to_operator_job_title = handover
        .to_operator_job_title
        .clone()
        .or_else(|| to_fallback.and_then(|value| value.job_title.clone()));

    ShiftHandoverResponse {
        handover_id: handover.handover_id,
        shift_date: handover.shift_date,
        shift_code: handover.shift_code,
        from_user_id: handover.from_user_id,
        to_user_id: handover.to_user_id,
        position_user_id: handover.position_user_id,
        from_operator_name: from_operator_name.clone(),
        from_operator_job_title: from_operator_job_title.clone(),
        from_operator_label: handover.from_operator_label.or_else(|| {
            compose_operator_label_opt(from_operator_name.as_deref(), from_operator_job_title.as_deref())
                .or_else(|| from_fallback.and_then(|value| value.label.clone()))
        }),
        to_operator_name: to_operator_name.clone(),
        to_operator_job_title: to_operator_job_title.clone(),
        to_operator_label: handover.to_operator_label.or_else(|| {
            compose_operator_label_opt(to_operator_name.as_deref(), to_operator_job_title.as_deref())
                .or_else(|| to_fallback.and_then(|value| value.label.clone()))
        }),
        status: handover.status,
        summary: handover.summary,
        risk_level: handover.risk_level,
        signed_at: handover.signed_at,
        submitted_at: handover.submitted_at,
        created_at: handover.created_at,
        updated_at: handover.updated_at,
        items: handover.items.into_iter().map(to_item_response).collect(),
    }
}

pub(crate) async fn load_user_fallbacks_for_handovers(
    auth_svc: &AuthService,
    operator_identity_svc: Option<&OperatorIdentityService>,
    context_type: Option<&str>,
    context_id: Option<&str>,
    handovers: &[ShiftHandover],
) -> Result<HashMap<String, UserFallback>, ApiError> {
    let user_ids = handovers
        .iter()
        .flat_map(|handover| [&handover.from_user_id, &handover.to_user_id])
        .collect::<Vec<_>>();
    load_user_fallbacks(auth_svc, operator_identity_svc, context_type, context_id, user_ids).await
}

pub(crate) async fn load_user_with_context(
    auth_svc: &AuthService,
    operator_identity_svc: Option<&OperatorIdentityService>,
    context_type: Option<&str>,
    context_id: Option<&str>,
    user_id: &str,
) -> Result<Option<UserResponse>, ApiError> {
    let Some(user) = auth_svc.find_user_by_id(user_id).await.map_err(ApiError::from)? else {
        return Ok(None);
    };
    let Some(operator_identity_svc) = operator_identity_svc else {
        return Ok(Some(user));
    };

    operator_identity_svc
        .enrich_user_response(user, context_type, context_id)
        .await
        .map(Some)
        .map_err(ApiError::from)
}

pub(crate) fn user_fallback_from_user(user: UserResponse) -> UserFallback {
    let name = effective_operator_name_for_user(&user);
    let job_title = resolve_operator_job_title_for_user(&user);
    let label = user
        .effective_operator_label
        .clone()
        .or_else(|| compose_operator_label_opt(name.as_deref(), job_title.as_deref()));
    UserFallback { name, job_title, label }
}

pub(crate) fn extract_optional_operator_context(
    req: &HttpRequest,
    svc: Option<&OperatorIdentityService>,
) -> Result<(Option<String>, Option<String>), ApiError> {
    let Some(svc) = svc else {
        return Ok((None, None));
    };

    let context_type = req
        .headers()
        .get("X-Operator-Context-Type")
        .and_then(|value| value.to_str().ok());
    let context_id = req
        .headers()
        .get("X-Operator-Context-Id")
        .and_then(|value| value.to_str().ok());
    svc.normalize_context(context_type, context_id).map_err(ApiError::from)
}

pub(crate) fn to_item_response(item: ShiftHandoverItem) -> ShiftHandoverItemResponse {
    ShiftHandoverItemResponse {
        item_id: item.item_id,
        handover_id: item.handover_id,
        item_type: item.item_type,
        title: item.title,
        detail: item.detail,
        owner_user_id: item.owner_user_id,
        due_at: item.due_at,
        is_mandatory: item.is_mandatory,
        acknowledged: item.acknowledged,
        acknowledged_at: item.acknowledged_at,
        acknowledged_by: item.acknowledged_by,
        created_at: item.created_at,
        updated_at: item.updated_at,
    }
}

pub(crate) fn display_name_for_user(user: &fms_application::schemas::auth_schemas::UserResponse) -> Option<String> {
    user.display_name.clone().or_else(|| {
        if user.username.trim().is_empty() {
            None
        } else {
            Some(user.username.clone())
        }
    })
}

pub(crate) fn effective_operator_name_for_user(
    user: &fms_application::schemas::auth_schemas::UserResponse,
) -> Option<String> {
    user.effective_operator_name
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| display_name_for_user(user))
}

pub(crate) fn resolve_operator_job_title_for_user(
    user: &fms_application::schemas::auth_schemas::UserResponse,
) -> Option<String> {
    user.job_title
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| user.roles.iter().find(|value| !value.trim().is_empty()).cloned())
        .or_else(|| {
            if user.is_admin {
                Some("admin".to_string())
            } else {
                Some("用户".to_string())
            }
        })
}

pub(crate) fn compose_operator_label_opt(name: Option<&str>, title: Option<&str>) -> Option<String> {
    let resolved_name = name.and_then(non_empty);
    let resolved_title = title.and_then(non_empty);
    match (resolved_name, resolved_title) {
        (Some(name), Some(title)) => Some(format!("{name}-{title}")),
        (Some(name), None) => Some(name.to_string()),
        (None, Some(title)) => Some(title.to_string()),
        (None, None) => None,
    }
}

pub(crate) fn compose_operator_label(name: Option<&str>, title: Option<&str>) -> String {
    compose_operator_label_opt(name, title).unwrap_or_default()
}

pub(crate) fn non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}
