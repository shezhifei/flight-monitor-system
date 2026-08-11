use actix_web::{web, HttpRequest, HttpResponse};
use serde_json::Value;
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::routes::dispatch_resources::{created_resp, ok_resp};
use fms_application::schemas::dispatch_schemas::{
    DepartmentQualificationCatalogCreate, DepartmentQualificationCatalogResponse, DepartmentQualificationLevelCreate,
    DepartmentQualificationLevelResponse, DepartmentTaskTypeRequirementDraftCreate,
    DepartmentTaskTypeRequirementPublishRequest, DepartmentTaskTypeRequirementPublishResponse,
    DepartmentTaskTypeRequirementVersionResponse, DispatchRulePreviewRequest, DispatchRulePreviewResponse,
    DispatchRuleValidationRequest, DispatchRuleValidationResponse, FlightGenerationRuleCreate,
    FlightGenerationRuleResponse, GenerationAdjustmentRuleCreate, GenerationAdjustmentRuleResponse,
    QualificationGrantCreate, QualificationGrantResponse, TemporaryTaskTemplateCreate, TemporaryTaskTemplateResponse,
};
use fms_application::services::dispatch_resource_service::{
    extract_department_id_from_body, parse_comma_separated_ids, to_adjustment_rule_response,
    to_department_qualification_level_response, to_department_qualification_response, to_generation_rule_response,
    to_qualification_grant_response, to_task_type_requirement_version_response, to_temporary_task_template_response,
    PageQuery, QualificationGrantsQuery, QualificationLevelsQuery, RuleStatusQuery, TaskTypeRequirementVersionsQuery,
};
use fms_application::services::dispatch_rule_service::DispatchRuleService;

pub async fn list_department_qualifications(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchRuleService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    query: web::Query<PageQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let items = svc
        .list_qualifications(&path.into_inner(), query.include_inactive.unwrap_or(false))
        .await?;
    let payload: Vec<DepartmentQualificationCatalogResponse> =
        items.into_iter().map(to_department_qualification_response).collect();
    Ok(ok_resp(&req, payload))
}

pub async fn create_department_qualification(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchRuleService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<DepartmentQualificationCatalogCreate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc.create_qualification(&path.into_inner(), body.into_inner()).await?;
    Ok(created_resp(&req, to_department_qualification_response(saved)))
}

pub async fn list_department_qualification_levels(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchRuleService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    query: web::Query<QualificationLevelsQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let items = svc
        .list_levels(
            &path.into_inner(),
            query.qualification_code.as_deref(),
            query.include_inactive.unwrap_or(false),
        )
        .await?;
    let payload: Vec<DepartmentQualificationLevelResponse> = items
        .into_iter()
        .map(to_department_qualification_level_response)
        .collect();
    Ok(ok_resp(&req, payload))
}

pub async fn create_department_qualification_level(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchRuleService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<DepartmentQualificationLevelCreate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc.create_level(&path.into_inner(), body.into_inner()).await?;
    Ok(created_resp(&req, to_department_qualification_level_response(saved)))
}

pub async fn list_department_qualification_grants(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchRuleService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    query: web::Query<QualificationGrantsQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let user_ids = parse_comma_separated_ids(query.user_ids.as_deref());
    let items = svc
        .list_grants(
            &path.into_inner(),
            &user_ids,
            query.include_inactive.unwrap_or(false),
            Some(chrono::Utc::now()),
        )
        .await?;
    let payload: Vec<QualificationGrantResponse> = items.into_iter().map(to_qualification_grant_response).collect();
    Ok(ok_resp(&req, payload))
}

pub async fn create_department_qualification_grant(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchRuleService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<QualificationGrantCreate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc.create_grant(&path.into_inner(), body.into_inner()).await?;
    Ok(created_resp(&req, to_qualification_grant_response(saved)))
}

pub async fn list_department_task_type_requirement_versions(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchRuleService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    query: web::Query<TaskTypeRequirementVersionsQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let items = svc
        .list_requirement_versions(&path.into_inner(), query.task_type.as_deref(), query.status.as_deref())
        .await?;
    let payload: Vec<DepartmentTaskTypeRequirementVersionResponse> = items
        .into_iter()
        .map(to_task_type_requirement_version_response)
        .collect();
    Ok(ok_resp(&req, payload))
}

pub async fn create_department_task_type_requirement_draft(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchRuleService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<DepartmentTaskTypeRequirementDraftCreate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc
        .save_requirement_draft(&path.into_inner(), body.into_inner())
        .await?;
    Ok(created_resp(&req, to_task_type_requirement_version_response(saved)))
}

pub async fn publish_department_task_type_requirement(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchRuleService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<DepartmentTaskTypeRequirementPublishRequest>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let published = svc.publish_requirement(&path.into_inner(), body.into_inner()).await?;
    Ok(ok_resp(
        &req,
        DepartmentTaskTypeRequirementPublishResponse {
            published_version: to_task_type_requirement_version_response(published),
        },
    ))
}

pub async fn list_department_flight_generation_rules(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchRuleService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    query: web::Query<RuleStatusQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let items = svc
        .list_generation_rules(&path.into_inner(), query.status.as_deref())
        .await?;
    let payload: Vec<FlightGenerationRuleResponse> = items.into_iter().map(to_generation_rule_response).collect();
    Ok(ok_resp(&req, payload))
}

pub async fn create_department_flight_generation_rule(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchRuleService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<FlightGenerationRuleCreate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc.save_generation_rule(&path.into_inner(), body.into_inner()).await?;
    Ok(created_resp(&req, to_generation_rule_response(saved)))
}

pub async fn delete_department_flight_generation_rule(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchRuleService>>,
    claims: JwtAuth,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let (department_id, rule_id) = path.into_inner();
    let deleted = svc.delete_generation_rule(&department_id, &rule_id).await?;
    Ok(ok_resp(&req, deleted))
}

pub async fn list_department_generation_adjustment_rules(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchRuleService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    query: web::Query<RuleStatusQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let items = svc
        .list_adjustment_rules(&path.into_inner(), query.status.as_deref())
        .await?;
    let payload: Vec<GenerationAdjustmentRuleResponse> = items.into_iter().map(to_adjustment_rule_response).collect();
    Ok(ok_resp(&req, payload))
}

pub async fn create_department_generation_adjustment_rule(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchRuleService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<GenerationAdjustmentRuleCreate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc.save_adjustment_rule(&path.into_inner(), body.into_inner()).await?;
    Ok(created_resp(&req, to_adjustment_rule_response(saved)))
}

pub async fn list_department_temporary_task_templates(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchRuleService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    query: web::Query<PageQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let items = svc
        .list_temporary_task_templates(&path.into_inner(), query.include_inactive.unwrap_or(false))
        .await?;
    let payload: Vec<TemporaryTaskTemplateResponse> =
        items.into_iter().map(to_temporary_task_template_response).collect();
    Ok(ok_resp(&req, payload))
}

pub async fn create_department_temporary_task_template(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchRuleService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<TemporaryTaskTemplateCreate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc
        .save_temporary_task_template(&path.into_inner(), body.into_inner())
        .await?;
    Ok(created_resp(&req, to_temporary_task_template_response(saved)))
}

pub async fn validate_department_dispatch_rules(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchRuleService>>,
    claims: JwtAuth,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let body = body.into_inner();
    let department_id = extract_department_id_from_body(&body)?;
    let request: DispatchRuleValidationRequest =
        serde_json::from_value(body).map_err(|error| ApiError::ValidationError(error.to_string()))?;
    let payload = if let Some(generation_rule) = &request.generation_rule {
        svc.validate_generation_rule(&department_id, generation_rule, generation_rule.rule_id.as_deref())
            .await?
    } else {
        serde_json::json!({ "valid": true, "conflicts": [], "messages": [] })
    };
    let response = DispatchRuleValidationResponse {
        valid: payload.get("valid").and_then(Value::as_bool).unwrap_or(true),
        conflicts: payload
            .get("conflicts")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        messages: payload
            .get("messages")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
    };
    Ok(ok_resp(&req, response))
}

pub async fn preview_department_dispatch_rules(
    req: HttpRequest,
    svc: web::Data<Arc<DispatchRuleService>>,
    claims: JwtAuth,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let body = body.into_inner();
    let department_id = extract_department_id_from_body(&body)?;
    let request: DispatchRulePreviewRequest =
        serde_json::from_value(body).map_err(|error| ApiError::ValidationError(error.to_string()))?;
    let payload = svc.preview_dispatch_rules(&department_id, request).await?;
    let response = DispatchRulePreviewResponse {
        generated_orders: payload
            .get("generated_orders")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        applied_adjustments: payload
            .get("applied_adjustments")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        turnaround_constraints: payload
            .get("turnaround_constraints")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        conflicts: payload
            .get("conflicts")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        blocking_errors: payload
            .get("blocking_errors")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
    };
    Ok(ok_resp(&req, response))
}
