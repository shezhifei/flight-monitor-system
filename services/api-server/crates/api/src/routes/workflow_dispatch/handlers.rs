use actix_web::{web, HttpRequest, HttpResponse};
use serde_json::Value;
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::{JwtAuth, OptionalJwtAuth, WorkflowInternalToken};
use crate::middleware::permissions::PermissionCheck;
use crate::types::ConcreteWorkflowDispatchService as WorkflowDispatchService;
use fms_application::schemas::dispatch_schemas::{WorkflowDispatchAssignRequest, WorkflowDispatchCreateRequest};
use fms_application::services::auth_service::AuthService;
use fms_application::services::authorization_service::PermissionCatalog;
use fms_application::services::dispatch_query_service::DispatchQueryService;
use fms_domain::error::DomainError;

use super::responses::{stored_recommendations, workflow_dispatch_order_to_value};
use super::shared::{
    can_manage_dispatch_claims, department_scope, serde_payload_detail, service_unavailable,
    validate_assign_payload_shape, validate_trigger_payload_shape, validation_error_response,
    PublicWorkflowDispatchTriggerRequest,
};

pub(crate) async fn trigger_dispatch_from_workflow(
    svc: Option<web::Data<Arc<WorkflowDispatchService>>>,
    workflow_token: Option<web::Data<WorkflowInternalToken>>,
    request: HttpRequest,
    claims: OptionalJwtAuth,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    let Some(svc) = svc else {
        return Ok(service_unavailable("workflow dispatch service unavailable"));
    };

    let expected_token = workflow_token
        .as_ref()
        .and_then(|token| token.0.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let provided_token = request
        .headers()
        .get("X-Workflow-Token")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let claims_ref = claims.0.as_ref().map(|token| JwtAuth(token.clone()));
    let can_manage = can_manage_dispatch_claims(claims_ref.as_ref());

    let token_valid = match (expected_token, provided_token) {
        (Some(expected), Some(provided)) => provided == expected,
        (Some(_), None) => false,
        (None, _) => true,
    };

    if !token_valid && !can_manage {
        return Err(ApiError::Unauthorized("invalid workflow token".into()));
    }

    let raw_payload = body.into_inner();
    let shape_errors = validate_trigger_payload_shape(&raw_payload);
    if !shape_errors.is_empty() {
        return Ok(validation_error_response(shape_errors));
    }
    let public_payload: PublicWorkflowDispatchTriggerRequest = match serde_json::from_value(raw_payload.clone()) {
        Ok(value) => value,
        Err(error) => {
            return Ok(validation_error_response(vec![serde_payload_detail(
                &error,
                &raw_payload,
            )]));
        }
    };
    let payload: WorkflowDispatchCreateRequest = public_payload.into();
    if let Err(detail) = payload.validate() {
        return Ok(validation_error_response(detail));
    }

    let order = match svc.create_dispatch_from_workflow(payload).await {
        Ok(order) => order,
        Err(
            DomainError::ValidationError(_)
            | DomainError::BusinessRuleViolation(_)
            | DomainError::BusinessRuleViolationWithDetails { .. }
            | DomainError::InvalidStateTransition { .. },
        ) => {
            return Err(ApiError::BadRequest("invalid workflow dispatch payload".into()));
        }
        Err(DomainError::Internal(_)) => {
            return Err(ApiError::Internal("trigger dispatch failed".into()));
        }
        Err(error) => return Err(ApiError::from(error)),
    };
    Ok(HttpResponse::Ok().json(workflow_dispatch_order_to_value(&order)))
}

pub(crate) async fn assign_workflow_dispatch_order(
    svc: Option<web::Data<Arc<WorkflowDispatchService>>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    let Some(svc) = svc else {
        return Ok(service_unavailable("workflow dispatch service unavailable"));
    };
    claims.ensure_permission(PermissionCatalog::DISPATCH_ORDER_UPDATE)?;
    let actor_id = claims
        .0
        .sub
        .as_deref()
        .ok_or_else(|| ApiError::Unauthorized("未认证".into()))?;
    let raw_payload = body.into_inner();
    let shape_errors = validate_assign_payload_shape(&raw_payload);
    if !shape_errors.is_empty() {
        return Ok(validation_error_response(shape_errors));
    }
    let payload: WorkflowDispatchAssignRequest = match serde_json::from_value(raw_payload.clone()) {
        Ok(value) => value,
        Err(error) => {
            return Ok(validation_error_response(vec![serde_payload_detail(
                &error,
                &raw_payload,
            )]));
        }
    };
    if let Err(detail) = payload.validate() {
        return Ok(validation_error_response(detail));
    }

    let order = match svc
        .assign_dispatch_from_supervisor(&path.into_inner(), payload, actor_id)
        .await
    {
        Ok(order) => order,
        Err(
            DomainError::NotFound { .. }
            | DomainError::ValidationError(_)
            | DomainError::BusinessRuleViolation(_)
            | DomainError::BusinessRuleViolationWithDetails { .. }
            | DomainError::InvalidStateTransition { .. },
        ) => {
            return Err(ApiError::BadRequest("invalid workflow assignment request".into()));
        }
        Err(DomainError::Internal(_)) => {
            return Err(ApiError::Internal("assign workflow dispatch failed".into()));
        }
        Err(error) => return Err(ApiError::from(error)),
    };
    Ok(HttpResponse::Ok().json(workflow_dispatch_order_to_value(&order)))
}

pub(crate) async fn list_pending_workflow_dispatch_orders(
    query_svc: Option<web::Data<Arc<DispatchQueryService>>>,
    auth_svc: Option<web::Data<Arc<AuthService>>>,
    claims: JwtAuth,
    query: web::Query<super::shared::PageQuery>,
) -> Result<HttpResponse, ApiError> {
    let Some(query_svc) = query_svc else {
        return Ok(service_unavailable("dispatch query service unavailable"));
    };
    claims.ensure_permission(PermissionCatalog::DISPATCH_ORDER_READ)?;
    let department = department_scope(auth_svc.as_ref().map(|svc| svc.get_ref()), &claims).await?;
    let orders = query_svc
        .list_orders(
            None,
            None,
            Some("pending"),
            Some("workflow"),
            department.as_deref(),
            query.page.unwrap_or(1).max(1),
            query.page_size.unwrap_or(50).clamp(1, 200),
        )
        .await?;
    Ok(HttpResponse::Ok().json(orders.iter().map(workflow_dispatch_order_to_value).collect::<Vec<_>>()))
}

pub(crate) async fn get_workflow_dispatch_recommendations(
    workflow_svc: Option<web::Data<Arc<WorkflowDispatchService>>>,
    query_svc: Option<web::Data<Arc<DispatchQueryService>>>,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let Some(query_svc) = query_svc else {
        return Ok(service_unavailable("dispatch query service unavailable"));
    };
    let Some(workflow_svc) = workflow_svc else {
        return Ok(service_unavailable("workflow dispatch service unavailable"));
    };
    claims.ensure_permission(PermissionCatalog::DISPATCH_ORDER_READ)?;
    let order_id = path.into_inner();
    let Some(order) = query_svc.get_order(&order_id, true, None).await? else {
        return Err(ApiError::NotFound("dispatch order not found".into()));
    };

    let context = &order.workflow_context;
    let target_department = context
        .get("target_department")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let target_job_title = context
        .get("target_job_title")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let required_people = context
        .get("required_people")
        .and_then(|value| value.as_i64())
        .unwrap_or(1) as i32;

    let recommendations = if let Some(target_department) = target_department {
        match workflow_svc
            .recommend_assignees(target_department, &order.task_type, target_job_title, required_people)
            .await
        {
            Ok(items) => items,
            Err(_) => stored_recommendations(&order.recommended_assignees),
        }
    } else {
        stored_recommendations(&order.recommended_assignees)
    };

    Ok(HttpResponse::Ok().json(recommendations))
}
