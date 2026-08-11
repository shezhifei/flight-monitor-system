use actix_web::{web, HttpRequest, HttpResponse};
use serde_json::json;
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::routes::workflow_actor::resolve_workflow_actor;
use crate::sse::hub::SseHub;
use crate::types::ConcreteBusinessCaseService as BusinessCaseService;
use fms_application::services::auth_service::AuthService;
use fms_application::services::authorization_service::PermissionCatalog;
use fms_application::services::business_case_service::BusinessCaseUpdatePayload;
use fms_application::services::business_case_workflow_service::BusinessCaseWorkflowService;
use fms_application::services::cache_invalidation_service::CacheInvalidationService;
use fms_application::services::flight_cache_service::FlightCacheService;
use fms_application::services::flight_runtime_service::FlightRuntimeService;
use fms_application::services::flight_service::FlightService;
use fms_application::services::operator_identity_service::OperatorIdentityService;
use fms_runtime::spawn_tracked::spawn_tracked;

use super::shared::{
    actor_name, broadcast_business_case_event, ensure_authenticated, ensure_grant, extract_optional_operator_context,
    invalidate_business_case_flight_list_caches, ok_resp, refresh_related_flight_cache, resolve_flight_no,
    trigger_business_case_workflow_best_effort, viewer_department_id, viewer_department_name, CreateRequest, ListQuery,
    UpdateRequest,
};

pub(crate) async fn list_business_cases(
    svc: web::Data<Arc<BusinessCaseService>>,
    query: web::Query<ListQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_authenticated(&claims)?;
    ensure_grant(&claims, PermissionCatalog::BUSINESS_CASE_READ)?;
    let cases = svc
        .list_filtered_for_viewer(
            query.flight_id.as_deref(),
            query.case_type.as_deref(),
            query.status.as_deref(),
            viewer_department_id(&claims),
            viewer_department_name(&claims),
        )
        .await?;

    Ok(HttpResponse::Ok().json(
        cases
            .into_iter()
            .map(|case| {
                json!({
                    "success": true,
                    "data": case,
                    "message": "获取成功",
                })
            })
            .collect::<Vec<_>>(),
    ))
}

pub(crate) async fn get_business_case(
    svc: web::Data<Arc<BusinessCaseService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_authenticated(&claims)?;
    ensure_grant(&claims, PermissionCatalog::BUSINESS_CASE_READ)?;
    let case_id = path.into_inner();
    let Some(case) = svc
        .get_accessible(&case_id, viewer_department_id(&claims), viewer_department_name(&claims))
        .await?
    else {
        return Err(ApiError::NotFound("业务事项不存在".into()));
    };
    Ok(ok_resp(case, "获取成功"))
}

pub(crate) async fn create_business_case(
    svc: web::Data<Arc<BusinessCaseService>>,
    flight_svc: web::Data<Arc<FlightService>>,
    flight_runtime_svc: web::Data<Arc<FlightRuntimeService>>,
    flight_cache_svc: Option<web::Data<Arc<FlightCacheService>>>,
    auth_svc: Option<web::Data<Arc<AuthService>>>,
    sse_hub: Option<web::Data<Arc<SseHub>>>,
    workflow_svc: Option<web::Data<Arc<BusinessCaseWorkflowService>>>,
    operator_identity_svc: Option<web::Data<Arc<OperatorIdentityService>>>,
    cache_invalidation: Option<web::Data<Arc<CacheInvalidationService>>>,
    req: HttpRequest,
    claims: JwtAuth,
    body: web::Json<CreateRequest>,
) -> Result<HttpResponse, ApiError> {
    ensure_authenticated(&claims)?;
    let (context_type, context_id) =
        extract_optional_operator_context(&req, operator_identity_svc.as_ref().map(|svc| svc.get_ref().as_ref()))?;
    let actor = resolve_workflow_actor(
        &claims,
        auth_svc.as_ref().map(|svc| svc.get_ref().as_ref()),
        operator_identity_svc.as_ref().map(|svc| svc.get_ref().as_ref()),
        context_type,
        context_id,
    )
    .await?;
    let payload = body.into_inner();
    let CreateRequest {
        case_type,
        flight_id,
        description,
        visibility_scope,
        status,
        context,
    } = payload;
    let requested_visibility_scope = visibility_scope.unwrap_or_default();
    ensure_grant(&claims, PermissionCatalog::BUSINESS_CASE_CREATE)?;

    let flight = flight_svc
        .get_flight(&flight_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("航班不存在: {flight_id}")))?;
    let flight_no = resolve_flight_no(&flight);

    let case = svc
        .create_for_viewer(
            &case_type,
            &flight_id,
            &flight_no,
            &description,
            context,
            status.as_deref(),
            actor_name(&claims),
            requested_visibility_scope,
            viewer_department_id(&claims),
            viewer_department_name(&claims),
        )
        .await?;

    if let Some(workflow_svc) = workflow_svc.as_ref().map(|svc| Arc::clone(svc.get_ref())) {
        let workflow_case_type = case.case_type.clone();
        let workflow_case_id = case.case_id.clone();
        let workflow_actor = actor.clone();
        spawn_tracked("business_case:workflow_best_effort", async move {
            trigger_business_case_workflow_best_effort(
                workflow_svc,
                workflow_case_type,
                workflow_case_id,
                workflow_actor,
            )
            .await;
        });
    }

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
        "business_case.created",
        json!({
            "event": "business_case.created",
            "case_id": case.case_id,
            "case_type": case.case_type,
            "flight_id": case.flight_id,
        }),
    )
    .await;

    Ok(HttpResponse::Created().json(json!({
        "success": true,
        "data": case,
        "message": "业务事项创建成功",
    })))
}

pub(crate) async fn update_business_case(
    svc: web::Data<Arc<BusinessCaseService>>,
    flight_runtime_svc: web::Data<Arc<FlightRuntimeService>>,
    flight_cache_svc: Option<web::Data<Arc<FlightCacheService>>>,
    sse_hub: Option<web::Data<Arc<SseHub>>>,
    cache_invalidation: Option<web::Data<Arc<CacheInvalidationService>>>,
    path: web::Path<String>,
    claims: JwtAuth,
    body: web::Json<UpdateRequest>,
) -> Result<HttpResponse, ApiError> {
    ensure_authenticated(&claims)?;
    let case_id = path.into_inner();
    let Some(_existing_case) = svc
        .get_accessible(&case_id, viewer_department_id(&claims), viewer_department_name(&claims))
        .await?
    else {
        return Err(ApiError::NotFound("业务事项不存在或参数无效".into()));
    };
    ensure_grant(&claims, PermissionCatalog::BUSINESS_CASE_UPDATE)?;
    let actor = actor_name(&claims);
    let payload = body.into_inner();
    let changed_fields = [
        ("case_type", payload.case_type.is_some()),
        ("description", payload.description.is_some()),
        ("context", payload.context.is_some()),
        ("status", payload.status.is_some()),
        ("stand", payload.stand.is_some()),
        ("gate", payload.gate.is_some()),
    ]
    .into_iter()
    .filter_map(|(field, changed)| changed.then_some(field))
    .collect::<Vec<_>>();
    let update = BusinessCaseUpdatePayload {
        case_type: payload.case_type,
        description: payload.description,
        context: payload.context,
        status: payload.status,
        stand: payload.stand,
        gate: payload.gate,
    };

    let Some(case) = svc
        .update_case_if_accessible(
            &case_id,
            update,
            actor,
            viewer_department_id(&claims),
            viewer_department_name(&claims),
        )
        .await?
    else {
        return Err(ApiError::NotFound("业务事项不存在或参数无效".into()));
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
            "changed_fields": changed_fields,
        }),
    )
    .await;

    Ok(ok_resp(case, "业务事项更新成功"))
}
