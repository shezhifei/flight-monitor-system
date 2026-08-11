pub(crate) use crate::error::ApiError;
pub(crate) use crate::middleware::jwt::JwtAuth;
pub(crate) use actix_web::{web, HttpResponse};
pub(crate) use fms_application::services::authorization_service::{
    AuthorizationService, PermissionCatalog, ScopeLevel,
};
pub(crate) use fms_application::services::business_case_type_service::BusinessCaseTypeService;
pub(crate) use fms_domain::models::business_case::BusinessCaseType;
pub(crate) use fms_domain::models::business_case::VisibilityScope;
pub(crate) use serde::Deserialize;
pub(crate) use serde_json::json;
pub(crate) use std::sync::Arc;
#[derive(Debug, Deserialize)]
pub(crate) struct ListQuery {
    pub(crate) active_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateRequest {
    pub(crate) code: String,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    #[serde(default)]
    pub(crate) visibility_scope: Option<VisibilityScope>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StatusUpdateRequest {
    pub(crate) is_active: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AiExtractionConfigUpdateRequest {
    pub(crate) ai_extraction_config: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CasePropertiesUpdateRequest {
    pub(crate) case_properties: serde_json::Value,
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

pub(crate) fn ensure_grant(claims: &JwtAuth, permission: &str) -> Result<(), ApiError> {
    if AuthorizationService::has_grant(&claims.0, permission) || has_resource_wildcard(claims, permission) {
        return Ok(());
    }
    Err(ApiError::Forbidden(format!("缺少权限: {permission}")))
}

pub(crate) fn ensure_scope_grant(claims: &JwtAuth, permission: &str, scope: ScopeLevel) -> Result<(), ApiError> {
    if AuthorizationService::scope_grant(&claims.0, permission, scope) || has_resource_wildcard(claims, permission) {
        return Ok(());
    }
    Err(ApiError::Forbidden(format!("缺少权限: {permission} @ {:?}", scope)))
}

pub(crate) fn ok_resp(data: impl serde::Serialize) -> HttpResponse {
    HttpResponse::Ok().json(json!({ "success": true, "data": data }))
}

pub(crate) fn viewer_department_id(claims: &JwtAuth) -> Option<&str> {
    AuthorizationService::department_id(&claims.0)
}

pub(crate) fn viewer_department_name(claims: &JwtAuth) -> Option<&str> {
    AuthorizationService::department_name(&claims.0)
}

pub(crate) fn case_type_scope_level(item: &BusinessCaseType) -> ScopeLevel {
    match item.visibility_scope {
        VisibilityScope::Department => ScopeLevel::Department,
        VisibilityScope::Common => ScopeLevel::Common,
    }
}

pub(crate) async fn list_case_types(
    svc: web::Data<Arc<BusinessCaseTypeService>>,
    query: web::Query<ListQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_authenticated(&claims)?;
    ensure_grant(&claims, PermissionCatalog::WORKFLOW_DEFINITION_READ)?;
    let items = svc
        .list_case_types_for_viewer(
            query.active_only.unwrap_or(true),
            viewer_department_id(&claims),
            viewer_department_name(&claims),
        )
        .await
        .map_err(ApiError::from)?;
    Ok(ok_resp(items))
}

pub(crate) async fn create_case_type(
    svc: web::Data<Arc<BusinessCaseTypeService>>,
    claims: JwtAuth,
    body: web::Json<CreateRequest>,
) -> Result<HttpResponse, ApiError> {
    ensure_authenticated(&claims)?;
    let payload = body.into_inner();
    let resolved_visibility_scope = payload.visibility_scope.unwrap_or(VisibilityScope::Department);
    ensure_scope_grant(
        &claims,
        PermissionCatalog::WORKFLOW_DEFINITION_EDIT,
        match resolved_visibility_scope {
            VisibilityScope::Department => ScopeLevel::Department,
            VisibilityScope::Common => ScopeLevel::Common,
        },
    )?;
    let item = svc
        .create_case_type_for_viewer(
            &payload.code,
            &payload.name,
            payload.description.as_deref(),
            resolved_visibility_scope,
            viewer_department_id(&claims),
            viewer_department_name(&claims),
        )
        .await
        .map_err(ApiError::from)?;
    Ok(HttpResponse::Created().json(json!({ "success": true, "data": item })))
}

pub(crate) async fn save_case_type_bpmn(
    svc: web::Data<Arc<BusinessCaseTypeService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, ApiError> {
    ensure_authenticated(&claims)?;
    let code = path.into_inner();
    let Some(case_type) = svc
        .find_by_code_for_viewer(&code, viewer_department_id(&claims), viewer_department_name(&claims))
        .await
        .map_err(ApiError::from)?
    else {
        return Ok(HttpResponse::NotFound().json(json!({
            "detail": format!("业务事项类型 {code} 不存在")
        })));
    };
    ensure_scope_grant(
        &claims,
        PermissionCatalog::WORKFLOW_DEFINITION_EDIT,
        case_type_scope_level(&case_type),
    )?;
    let payload = body.into_inner();
    let bpmn_xml = payload
        .get("bpmn_xml")
        .and_then(serde_json::Value::as_str)
        .or_else(|| payload.get("xml").and_then(serde_json::Value::as_str))
        .unwrap_or_default();
    if bpmn_xml.is_empty() {
        return Ok(HttpResponse::UnprocessableEntity().json(json!({
            "detail": "缺少 bpmn_xml 参数"
        })));
    }

    let updated = svc
        .save_bpmn_xml_if_accessible(
            &code,
            bpmn_xml,
            payload.get("description").and_then(serde_json::Value::as_str),
            viewer_department_id(&claims),
            viewer_department_name(&claims),
            AuthorizationService::scope_grant(
                &claims.0,
                PermissionCatalog::WORKFLOW_DEFINITION_EDIT,
                ScopeLevel::Common,
            ) || has_resource_wildcard(&claims, PermissionCatalog::WORKFLOW_DEFINITION_EDIT),
        )
        .await
        .map_err(ApiError::from)?;

    if !updated {
        return Ok(HttpResponse::NotFound().json(json!({
            "detail": format!("业务事项类型 {code} 不存在")
        })));
    }

    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "message": format!("BPMN 已保存至 {code}"),
    })))
}

pub(crate) async fn update_case_type_status(
    svc: web::Data<Arc<BusinessCaseTypeService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<StatusUpdateRequest>,
) -> Result<HttpResponse, ApiError> {
    ensure_authenticated(&claims)?;
    let code = path.into_inner();
    let Some(case_type) = svc
        .find_by_code_for_viewer(&code, viewer_department_id(&claims), viewer_department_name(&claims))
        .await
        .map_err(ApiError::from)?
    else {
        return Ok(HttpResponse::NotFound().json(json!({
            "detail": format!("业务事项类型 {code} 不存在或无变更")
        })));
    };
    let permission = if body.is_active {
        PermissionCatalog::WORKFLOW_DEFINITION_PUBLISH
    } else {
        PermissionCatalog::WORKFLOW_DEFINITION_DEPRECATE
    };
    ensure_scope_grant(&claims, permission, case_type_scope_level(&case_type))?;
    let updated = svc
        .update_status_if_accessible(
            &code,
            body.is_active,
            viewer_department_id(&claims),
            viewer_department_name(&claims),
            AuthorizationService::scope_grant(&claims.0, permission, ScopeLevel::Common)
                || has_resource_wildcard(&claims, permission),
        )
        .await
        .map_err(ApiError::from)?;

    if !updated {
        return Ok(HttpResponse::NotFound().json(json!({
            "detail": format!("业务事项类型 {code} 不存在或无变更")
        })));
    }

    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "message": "状态更新成功"
    })))
}

pub(crate) async fn update_case_type_ai_extraction_config(
    svc: web::Data<Arc<BusinessCaseTypeService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<AiExtractionConfigUpdateRequest>,
) -> Result<HttpResponse, ApiError> {
    ensure_authenticated(&claims)?;
    let code = path.into_inner();
    let Some(case_type) = svc
        .find_by_code_for_viewer(&code, viewer_department_id(&claims), viewer_department_name(&claims))
        .await
        .map_err(ApiError::from)?
    else {
        return Ok(HttpResponse::NotFound().json(json!({
            "detail": format!("业务事项类型 {code} 不存在")
        })));
    };

    ensure_scope_grant(
        &claims,
        PermissionCatalog::WORKFLOW_DEFINITION_EDIT,
        case_type_scope_level(&case_type),
    )?;

    let payload = body.into_inner();
    let updated = svc
        .update_ai_extraction_config_if_accessible(
            &code,
            payload.ai_extraction_config,
            viewer_department_id(&claims),
            viewer_department_name(&claims),
            AuthorizationService::scope_grant(
                &claims.0,
                PermissionCatalog::WORKFLOW_DEFINITION_EDIT,
                ScopeLevel::Common,
            ) || has_resource_wildcard(&claims, PermissionCatalog::WORKFLOW_DEFINITION_EDIT),
        )
        .await
        .map_err(ApiError::from)?;

    if updated.is_none() {
        return Ok(HttpResponse::NotFound().json(json!({
            "detail": format!("业务事项类型 {code} 不存在")
        })));
    }

    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "message": format!("AI 抽取配置已保存至 {code}"),
        "data": updated,
    })))
}

pub(crate) async fn update_case_type_case_properties(
    svc: web::Data<Arc<BusinessCaseTypeService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<CasePropertiesUpdateRequest>,
) -> Result<HttpResponse, ApiError> {
    ensure_authenticated(&claims)?;
    let code = path.into_inner();
    let Some(case_type) = svc
        .find_by_code_for_viewer(&code, viewer_department_id(&claims), viewer_department_name(&claims))
        .await
        .map_err(ApiError::from)?
    else {
        return Ok(HttpResponse::NotFound().json(json!({
            "detail": format!("业务事项类型 {code} 不存在")
        })));
    };

    ensure_scope_grant(
        &claims,
        PermissionCatalog::WORKFLOW_DEFINITION_EDIT,
        case_type_scope_level(&case_type),
    )?;

    let payload = body.into_inner();
    let updated = svc
        .update_case_properties_if_accessible(
            &code,
            payload.case_properties,
            viewer_department_id(&claims),
            viewer_department_name(&claims),
            AuthorizationService::scope_grant(
                &claims.0,
                PermissionCatalog::WORKFLOW_DEFINITION_EDIT,
                ScopeLevel::Common,
            ) || has_resource_wildcard(&claims, PermissionCatalog::WORKFLOW_DEFINITION_EDIT),
        )
        .await
        .map_err(ApiError::from)?;

    if updated.is_none() {
        return Ok(HttpResponse::NotFound().json(json!({
            "detail": format!("业务事项类型 {code} 不存在")
        })));
    }

    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "message": format!("业务规则配置已保存至 {code}"),
        "data": updated,
    })))
}
