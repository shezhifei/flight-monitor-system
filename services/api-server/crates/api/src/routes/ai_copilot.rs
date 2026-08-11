//! AI Copilot business-case draft routes.

use actix_web::{web, HttpRequest, HttpResponse};
use serde_json::json;
use std::sync::Arc;

use super::workflow_actor::resolve_workflow_actor;
use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::types::ConcreteAiCopilotBusinessCaseBatchRepository;
use fms_application::services::ai_business_case_copilot_service::{
    AiBusinessCaseCopilotService, AiCopilotBatchAccess, AiCopilotCommitRequest, AiCopilotDraftRequest,
    AiCopilotFailedBatchResolutionRequest,
};
use fms_application::services::auth_service::AuthService;
use fms_application::services::authorization_service::{AuthorizationService, PermissionCatalog, ScopeLevel};
use fms_application::services::operator_identity_service::OperatorIdentityService;
use fms_domain::models::ai_copilot::AiCopilotBatchStatus;
use fms_domain::models::business_case::VisibilityScope;

type CopilotService = AiBusinessCaseCopilotService<ConcreteAiCopilotBusinessCaseBatchRepository>;

#[derive(Debug, serde::Deserialize)]
struct BatchListQuery {
    status: Option<String>,
    workflow_dispatch_status: Option<String>,
    #[serde(default = "default_batch_list_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

#[derive(Debug, serde::Deserialize)]
struct OperationalMetricsQuery {
    #[serde(default = "default_recent_error_limit")]
    recent_error_limit: i64,
    #[serde(default = "default_workflow_dispatch_max_attempts")]
    max_workflow_dispatch_attempts: i32,
}

fn default_batch_list_limit() -> i64 {
    50
}

fn default_recent_error_limit() -> i64 {
    10
}

fn default_workflow_dispatch_max_attempts() -> i32 {
    5
}

fn parse_batch_status(value: &str) -> Result<AiCopilotBatchStatus, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "draft" => Ok(AiCopilotBatchStatus::Draft),
        "committing" => Ok(AiCopilotBatchStatus::Committing),
        "committed" => Ok(AiCopilotBatchStatus::Committed),
        "failed" => Ok(AiCopilotBatchStatus::Failed),
        "failed_resolved" => Ok(AiCopilotBatchStatus::FailedResolved),
        "expired" => Ok(AiCopilotBatchStatus::Expired),
        _ => Err(ApiError::BadRequest(format!("无效批次状态: {value}"))),
    }
}

fn parse_workflow_dispatch_status(value: &str) -> Result<String, ApiError> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "not_required" | "pending" | "failed" | "succeeded" => Ok(normalized),
        _ => Err(ApiError::BadRequest(format!("无效流程派发状态: {value}"))),
    }
}

fn actor_name(claims: &JwtAuth) -> &str {
    claims
        .0
        .username
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or(claims.0.sub.as_deref())
        .unwrap_or("unknown")
}

fn batch_actor_keys(claims: &JwtAuth) -> Vec<String> {
    [
        Some(actor_name(claims)),
        claims.0.username.as_deref(),
        claims.0.sub.as_deref(),
        claims.0.email.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .filter(|value| !value.is_empty())
    .map(ToOwned::to_owned)
    .collect()
}

fn owner_batch_access(claims: &JwtAuth) -> AiCopilotBatchAccess {
    AiCopilotBatchAccess::for_actor_keys(batch_actor_keys(claims))
}

fn ensure_copilot_ops_grant(claims: &JwtAuth) -> Result<(), ApiError> {
    claims.ensure_grant(PermissionCatalog::SYSTEM_OPS_ADMIN)
}

fn ok_resp(data: impl serde::Serialize, message: &str) -> HttpResponse {
    HttpResponse::Ok().json(json!({
        "success": true,
        "data": data,
        "message": message,
    }))
}

fn extract_optional_operator_context(
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

async fn create_business_case_draft(
    svc: web::Data<Arc<CopilotService>>,
    claims: JwtAuth,
    body: web::Json<AiCopilotDraftRequest>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_authenticated()?;
    claims.ensure_grant("ai:media")?;
    claims.ensure_grant(PermissionCatalog::FLIGHT_READ)?;

    let include_common_case_types = AuthorizationService::scope_grant(
        &claims.0,
        PermissionCatalog::WORKFLOW_DEFINITION_READ,
        ScopeLevel::Common,
    ) || claims.has_resource_wildcard(PermissionCatalog::WORKFLOW_DEFINITION_READ);

    let response = svc
        .draft_from_transcript(
            body.into_inner(),
            actor_name(&claims),
            claims.viewer_department_id(),
            claims.viewer_department_name(),
            include_common_case_types,
        )
        .await?;
    Ok(ok_resp(response, "草稿生成成功"))
}

async fn diagnose_business_case_draft(
    svc: web::Data<Arc<CopilotService>>,
    claims: JwtAuth,
    body: web::Json<AiCopilotDraftRequest>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_authenticated()?;
    claims.ensure_grant("ai:media")?;
    claims.ensure_grant(PermissionCatalog::FLIGHT_READ)?;

    let include_common_case_types = AuthorizationService::scope_grant(
        &claims.0,
        PermissionCatalog::WORKFLOW_DEFINITION_READ,
        ScopeLevel::Common,
    ) || claims.has_resource_wildcard(PermissionCatalog::WORKFLOW_DEFINITION_READ);

    let response = svc
        .diagnose_draft_from_transcript(
            body.into_inner(),
            claims.viewer_department_id(),
            claims.viewer_department_name(),
            include_common_case_types,
        )
        .await?;
    Ok(ok_resp(response, "草稿诊断完成"))
}

async fn commit_business_case_batch(
    svc: web::Data<Arc<CopilotService>>,
    auth_svc: Option<web::Data<Arc<AuthService>>>,
    operator_identity_svc: Option<web::Data<Arc<OperatorIdentityService>>>,
    req: HttpRequest,
    path: web::Path<String>,
    claims: JwtAuth,
    body: web::Json<AiCopilotCommitRequest>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_authenticated()?;
    claims.ensure_grant(PermissionCatalog::BUSINESS_CASE_CREATE)?;
    claims.ensure_grant(PermissionCatalog::WORKFLOW_RUN_START)?;

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

    let include_common_case_types =
        AuthorizationService::scope_grant(&claims.0, PermissionCatalog::BUSINESS_CASE_CREATE, ScopeLevel::Common)
            || claims.has_resource_wildcard(PermissionCatalog::BUSINESS_CASE_CREATE);

    let response = svc
        .commit_batch(
            &path.into_inner(),
            body.into_inner(),
            owner_batch_access(&claims),
            actor,
            VisibilityScope::default(),
            claims.viewer_department_id(),
            claims.viewer_department_name(),
            include_common_case_types,
        )
        .await?;
    Ok(ok_resp(response, "批量创建成功"))
}

async fn get_business_case_batch_status(
    svc: web::Data<Arc<CopilotService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_authenticated()?;
    claims.ensure_grant("ai:media")?;
    claims.ensure_grant(PermissionCatalog::FLIGHT_READ)?;

    let response = svc
        .get_batch_status(&path.into_inner(), owner_batch_access(&claims))
        .await?;
    Ok(ok_resp(response, "批次状态读取成功"))
}

async fn list_business_case_batches(
    svc: web::Data<Arc<CopilotService>>,
    query: web::Query<BatchListQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_authenticated()?;
    claims.ensure_grant("ai:media")?;
    claims.ensure_grant(PermissionCatalog::FLIGHT_READ)?;

    let status = query.status.as_deref().map(parse_batch_status).transpose()?;
    let workflow_dispatch_status = query
        .workflow_dispatch_status
        .as_deref()
        .map(parse_workflow_dispatch_status)
        .transpose()?;
    let response = svc
        .list_batches(
            status,
            workflow_dispatch_status.as_deref(),
            query.limit,
            query.offset,
            owner_batch_access(&claims),
        )
        .await?;
    Ok(ok_resp(response, "批次列表读取成功"))
}

async fn get_business_case_operational_metrics(
    svc: web::Data<Arc<CopilotService>>,
    query: web::Query<OperationalMetricsQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_authenticated()?;
    claims.ensure_grant("ai:media")?;
    ensure_copilot_ops_grant(&claims)?;

    let response = svc
        .operational_metrics(query.max_workflow_dispatch_attempts, query.recent_error_limit)
        .await?;
    Ok(ok_resp(response, "运行指标读取成功"))
}

async fn retry_business_case_batch_workflow_dispatch(
    svc: web::Data<Arc<CopilotService>>,
    auth_svc: Option<web::Data<Arc<AuthService>>>,
    operator_identity_svc: Option<web::Data<Arc<OperatorIdentityService>>>,
    req: HttpRequest,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_authenticated()?;
    claims.ensure_grant("ai:media")?;
    ensure_copilot_ops_grant(&claims)?;

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

    let response = svc
        .retry_workflow_dispatch(&path.into_inner(), actor, AiCopilotBatchAccess::unrestricted())
        .await?;
    Ok(ok_resp(response, "流程派发重试完成"))
}

async fn resolve_failed_business_case_batch(
    svc: web::Data<Arc<CopilotService>>,
    path: web::Path<String>,
    claims: JwtAuth,
    body: web::Json<AiCopilotFailedBatchResolutionRequest>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_authenticated()?;
    claims.ensure_grant("ai:media")?;
    ensure_copilot_ops_grant(&claims)?;

    let response = svc
        .resolve_failed_batch(
            &path.into_inner(),
            body.into_inner(),
            actor_name(&claims),
            AiCopilotBatchAccess::unrestricted(),
        )
        .await?;
    Ok(ok_resp(response, "失败批次处理完成"))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/ai/copilot")
            .route("/business-case-drafts", web::post().to(create_business_case_draft))
            .route(
                "/business-case-draft-diagnostics",
                web::post().to(diagnose_business_case_draft),
            )
            .route("/business-case-batches", web::get().to(list_business_case_batches))
            .route(
                "/business-case-operational-metrics",
                web::get().to(get_business_case_operational_metrics),
            )
            .route(
                "/business-case-batches/{batch_id}",
                web::get().to(get_business_case_batch_status),
            )
            .route(
                "/business-case-batches/{batch_id}/failed-resolution",
                web::post().to(resolve_failed_business_case_batch),
            )
            .route(
                "/business-case-batches/{batch_id}/workflow-dispatch/retry",
                web::post().to(retry_business_case_batch_workflow_dispatch),
            )
            .route(
                "/business-case-batches/{batch_id}/commit",
                web::post().to(commit_business_case_batch),
            ),
    );
}

#[cfg(test)]
mod tests {
    use crate::middleware::jwt::JwtAuth;
    use crate::middleware::permissions::PermissionCheck;
    use fms_application::schemas::auth_schemas::TokenData;
    use fms_application::services::authorization_service::PermissionCatalog;

    fn claims(permissions: &[&str]) -> JwtAuth {
        JwtAuth(TokenData {
            sub: Some("user-1".to_string()),
            email: None,
            username: Some("dispatcher".to_string()),
            token_kind: Some("access".to_string()),
            is_admin: Some(false),
            permissions: permissions.iter().map(|value| value.to_string()).collect(),
            department: None,
            department_id: None,
            pv: Some(1),
            iat: None,
            exp: None,
            iss: None,
            aud: None,
            ua_hash: None,
            ip_subnet_hash: None,
        })
    }

    #[test]
    fn copilot_permission_helper_accepts_resource_wildcard() {
        assert!(claims(&["business_case.*"]).has_grant("business_case.create"));
    }

    #[test]
    fn copilot_permission_helper_keeps_ai_media_explicit() {
        assert!(claims(&["ai:media"]).has_grant("ai:media"));
        assert!(!claims(&["ai.*"]).has_grant("ai:media"));
    }

    #[test]
    fn operational_metrics_read_permissions_require_media_and_ops_admin() {
        let allowed = claims(&["ai:media", PermissionCatalog::SYSTEM_OPS_ADMIN]);
        assert!(allowed.has_grant("ai:media"));
        assert!(allowed.has_grant(PermissionCatalog::SYSTEM_OPS_ADMIN));

        let missing_media = claims(&[PermissionCatalog::SYSTEM_OPS_ADMIN]);
        assert!(!missing_media.has_grant("ai:media"));
        assert!(missing_media.has_grant(PermissionCatalog::SYSTEM_OPS_ADMIN));

        let missing_flight_read = claims(&["ai:media"]);
        assert!(missing_flight_read.has_grant("ai:media"));
        assert!(!missing_flight_read.has_grant(PermissionCatalog::SYSTEM_OPS_ADMIN));
    }

    #[test]
    fn copilot_ops_permission_accepts_system_admin_alias() {
        let allowed = claims(&["system:admin"]);
        assert!(allowed.has_grant(PermissionCatalog::SYSTEM_OPS_ADMIN));
    }
}
