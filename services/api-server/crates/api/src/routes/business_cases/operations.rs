use actix_web::{web, HttpRequest, HttpResponse};
use serde_json::json;
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::sse::hub::SseHub;
use crate::types::ConcreteBusinessCaseService as BusinessCaseService;
use fms_application::services::auth_service::AuthService;
use fms_application::services::authorization_service::PermissionCatalog;
use fms_application::services::cache_invalidation_service::CacheInvalidationService;
use fms_application::services::flight_cache_service::FlightCacheService;
use fms_application::services::flight_runtime_service::FlightRuntimeService;
use fms_application::services::operator_identity_service::OperatorIdentityService;

use super::shared::{
    actor_name, broadcast_business_case_event, build_submitted_operator_name, ensure_authenticated, ensure_grant,
    extract_optional_operator_context, invalidate_business_case_flight_list_caches, normalized_client_action_id,
    ok_resp, refresh_related_flight_cache, viewer_department_id, viewer_department_name, AppendRequest,
    StatusUpdateRequest,
};

#[allow(clippy::too_many_arguments)]
pub(crate) async fn append_to_case(
    svc: web::Data<Arc<BusinessCaseService>>,
    auth_svc: web::Data<Arc<AuthService>>,
    flight_runtime_svc: web::Data<Arc<FlightRuntimeService>>,
    flight_cache_svc: Option<web::Data<Arc<FlightCacheService>>>,
    operator_identity_svc: Option<web::Data<Arc<OperatorIdentityService>>>,
    sse_hub: Option<web::Data<Arc<SseHub>>>,
    cache_invalidation: Option<web::Data<Arc<CacheInvalidationService>>>,
    path: web::Path<String>,
    req: HttpRequest,
    claims: JwtAuth,
    body: web::Json<AppendRequest>,
) -> Result<HttpResponse, ApiError> {
    ensure_authenticated(&claims)?;
    let case_id = path.into_inner();
    let Some(_existing_case) = svc
        .get_accessible(&case_id, viewer_department_id(&claims), viewer_department_name(&claims))
        .await?
    else {
        return Err(ApiError::NotFound("业务事项不存在或参数无效".into()));
    };
    ensure_grant(&claims, PermissionCatalog::BUSINESS_CASE_APPEND)?;
    let actor = actor_name(&claims);
    let payload = body.into_inner();
    let content = payload.content.trim();
    if content.is_empty() {
        return Err(ApiError::NotFound("业务事项不存在或参数无效".into()));
    }
    let client_action_id = normalized_client_action_id(&req, payload.client_action_id.clone());

    let (context_type, context_id) =
        extract_optional_operator_context(&req, operator_identity_svc.as_ref().map(|svc| svc.get_ref().as_ref()))?;
    let submitted_operator_name = if let (Some(operator_identity_svc), Some(user_id)) =
        (operator_identity_svc.as_ref(), claims.0.sub.as_deref())
    {
        match auth_svc.find_user_by_id(user_id).await? {
            Some(user) => operator_identity_svc
                .enrich_user_response(user, context_type.as_deref(), context_id.as_deref())
                .await
                .ok()
                .and_then(|item| item.effective_operator_name)
                .or_else(|| build_submitted_operator_name(&claims)),
            None => build_submitted_operator_name(&claims),
        }
    } else {
        build_submitted_operator_name(&claims)
    };

    let Some(append_result) = svc
        .append_case_if_accessible(
            &case_id,
            content,
            actor,
            submitted_operator_name.clone(),
            actor,
            payload.mention_user_ids,
            client_action_id,
            viewer_department_id(&claims),
            viewer_department_name(&claims),
        )
        .await?
    else {
        return Err(ApiError::NotFound("业务事项不存在或参数无效".into()));
    };
    let case = append_result.case;

    if append_result.inserted {
        refresh_related_flight_cache(
            Some(&case.flight_id),
            flight_runtime_svc.get_ref(),
            flight_cache_svc.as_ref().map(|svc| svc.get_ref()),
        )
        .await;
        invalidate_business_case_flight_list_caches(
            cache_invalidation.as_ref().map(|data| data.get_ref()),
            Some(&case.flight_id),
        )
        .await;

        broadcast_business_case_event(
            sse_hub.as_ref().map(|hub| hub.get_ref()),
            "business_case.updated",
            json!({
                "event": "business_case.updated",
                "case_id": case.case_id,
                "flight_id": case.flight_id,
                "changed_fields": ["append_entries"],
                "append_id": append_result.append.append_id,
            }),
        )
        .await;
    }

    Ok(HttpResponse::Created().json(json!({
        "success": true,
        "data": case,
        "message": "业务事项追加成功",
    })))
}

pub(crate) async fn update_business_case_status(
    svc: web::Data<Arc<BusinessCaseService>>,
    flight_runtime_svc: web::Data<Arc<FlightRuntimeService>>,
    flight_cache_svc: Option<web::Data<Arc<FlightCacheService>>>,
    sse_hub: Option<web::Data<Arc<SseHub>>>,
    cache_invalidation: Option<web::Data<Arc<CacheInvalidationService>>>,
    path: web::Path<String>,
    claims: JwtAuth,
    body: web::Json<StatusUpdateRequest>,
) -> Result<HttpResponse, ApiError> {
    ensure_authenticated(&claims)?;
    ensure_grant(&claims, PermissionCatalog::BUSINESS_CASE_STATUS_TRANSITION)?;
    let case_id = path.into_inner();
    let Some(_existing_case) = svc
        .get_accessible(&case_id, viewer_department_id(&claims), viewer_department_name(&claims))
        .await?
    else {
        return Err(ApiError::NotFound("业务事项不存在".into()));
    };
    ensure_grant(&claims, PermissionCatalog::BUSINESS_CASE_STATUS_TRANSITION)?;

    let updated = svc
        .update_status_if_accessible(
            &case_id,
            &body.status,
            actor_name(&claims),
            viewer_department_id(&claims),
            viewer_department_name(&claims),
        )
        .await?;
    if !updated {
        let Some(case) = svc
            .get_accessible(&case_id, viewer_department_id(&claims), viewer_department_name(&claims))
            .await?
        else {
            return Err(ApiError::NotFound("业务事项不存在或参数无效".into()));
        };
        return Ok(ok_resp(case, "业务事项状态未变化"));
    }

    let Some(case) = svc
        .get_accessible(&case_id, viewer_department_id(&claims), viewer_department_name(&claims))
        .await?
    else {
        return Err(ApiError::NotFound("业务事项不存在".into()));
    };

    refresh_related_flight_cache(
        Some(&case.flight_id),
        flight_runtime_svc.get_ref(),
        flight_cache_svc.as_ref().map(|svc| svc.get_ref()),
    )
    .await;
    invalidate_business_case_flight_list_caches(
        cache_invalidation.as_ref().map(|data| data.get_ref()),
        Some(&case.flight_id),
    )
    .await;

    broadcast_business_case_event(
        sse_hub.as_ref().map(|hub| hub.get_ref()),
        "business_case.updated",
        json!({
            "event": "business_case.updated",
            "case_id": case.case_id,
            "flight_id": case.flight_id,
            "changed_fields": ["status"],
        }),
    )
    .await;

    Ok(ok_resp(case, "业务事项状态更新成功"))
}

pub(crate) async fn acknowledge_append(
    svc: web::Data<Arc<BusinessCaseService>>,
    cache_invalidation: Option<web::Data<Arc<CacheInvalidationService>>>,
    path: web::Path<(String, String)>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_authenticated(&claims)?;
    let (case_id, append_id) = path.into_inner();
    let user_id = claims.0.sub.as_deref().unwrap_or("unknown");
    let existing_case = svc
        .get_accessible(&case_id, viewer_department_id(&claims), viewer_department_name(&claims))
        .await?;

    let Some(result) = svc
        .acknowledge_append_if_accessible(
            &case_id,
            &append_id,
            user_id,
            viewer_department_id(&claims),
            viewer_department_name(&claims),
        )
        .await?
    else {
        return Err(ApiError::NotFound("业务事项不存在或参数无效".into()));
    };
    invalidate_business_case_flight_list_caches(
        cache_invalidation.as_ref().map(|data| data.get_ref()),
        existing_case.as_ref().map(|case| case.flight_id.as_str()),
    )
    .await;
    Ok(ok_resp(result, "确认成功"))
}

pub(crate) async fn delete_business_case(
    svc: web::Data<Arc<BusinessCaseService>>,
    flight_runtime_svc: web::Data<Arc<FlightRuntimeService>>,
    flight_cache_svc: Option<web::Data<Arc<FlightCacheService>>>,
    sse_hub: Option<web::Data<Arc<SseHub>>>,
    cache_invalidation: Option<web::Data<Arc<CacheInvalidationService>>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_authenticated(&claims)?;
    let case_id = path.into_inner();
    let Some(case) = svc
        .get_accessible(&case_id, viewer_department_id(&claims), viewer_department_name(&claims))
        .await?
    else {
        return Err(ApiError::NotFound("业务事项不存在或参数无效".into()));
    };
    ensure_grant(&claims, PermissionCatalog::BUSINESS_CASE_DELETE)?;
    let flight_id = Some(case.flight_id.clone());
    let deleted = svc
        .delete_if_accessible(&case_id, viewer_department_id(&claims), viewer_department_name(&claims))
        .await?;
    if !deleted {
        return Err(ApiError::NotFound("业务事项不存在或参数无效".into()));
    }

    refresh_related_flight_cache(
        flight_id.as_deref(),
        flight_runtime_svc.get_ref(),
        flight_cache_svc.as_ref().map(|svc| svc.get_ref()),
    )
    .await;
    invalidate_business_case_flight_list_caches(
        cache_invalidation.as_ref().map(|data| data.get_ref()),
        flight_id.as_deref(),
    )
    .await;

    broadcast_business_case_event(
        sse_hub.as_ref().map(|hub| hub.get_ref()),
        "business_case.deleted",
        json!({
            "event": "business_case.deleted",
            "case_id": case_id,
        }),
    )
    .await;

    Ok(ok_resp(serde_json::Value::Null, "业务事项删除成功"))
}
