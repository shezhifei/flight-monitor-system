use actix_web::{web, HttpRequest, HttpResponse};
use serde_json::json;
use std::sync::Arc;

use super::workflow_actor::resolve_workflow_actor;
use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::services::runtime_error_monitor::record_service_unavailable_background;
use fms_application::schemas::business_case_workflow_schemas::BusinessCaseWorkflowStartRequest;
use fms_application::schemas::response::ApiResponse;
use fms_application::services::auth_service::AuthService;
use fms_application::services::authorization_service::{AuthorizationService, PermissionCatalog};
use fms_application::services::business_case_workflow_service::BusinessCaseWorkflowService;
use fms_application::services::cache_invalidation_service::{CacheInvalidationKey, CacheInvalidationService};
use fms_application::services::operator_identity_service::OperatorIdentityService;
use fms_domain::error::DomainError;

fn orchestrator_unavailable() -> HttpResponse {
    record_service_unavailable_background(
        "Business case workflow orchestrator is not available",
        "business_case_workflow_orchestrator",
        "infrastructure",
    );
    HttpResponse::ServiceUnavailable().json(json!({
        "detail": "Business case workflow orchestrator is not available"
    }))
}

fn map_start_workflow_detail(error: DomainError) -> String {
    match error {
        DomainError::BusinessRuleViolation(message)
        | DomainError::ValidationError(message)
        | DomainError::Internal(message)
        | DomainError::Conflict(message)
        | DomainError::ConcurrencyConflict(message)
        | DomainError::PermissionDenied(message)
        | DomainError::Unauthorized(message) => message,
        DomainError::BusinessRuleViolationWithDetails { message, .. } => message,
        DomainError::InvalidStateTransition { from, to } => {
            format!("非法状态转换: {from} → {to}")
        }
        DomainError::NotFound { entity_type, id } => {
            if entity_type == "flight" {
                format!("Flight not found: {id}")
            } else {
                format!("实体未找到: {entity_type} (id={id})")
            }
        }
    }
}

fn raw_detail(status: actix_web::http::StatusCode, message: impl Into<String>) -> HttpResponse {
    HttpResponse::build(status).json(json!({ "detail": message.into() }))
}

async fn invalidate_workflow_business_case_caches(
    cache_invalidation: Option<&Arc<CacheInvalidationService>>,
    flight_id: &str,
) {
    let Some(cache_invalidation) = cache_invalidation else {
        return;
    };
    let event = cache_invalidation.flight_event(
        flight_id,
        [
            CacheInvalidationKey::FlightRuntimeProjection,
            CacheInvalidationKey::FlightListResponse,
        ],
    );
    cache_invalidation.invalidate_and_publish(event).await;
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

async fn start_business_case_workflow(
    svc: Option<web::Data<Arc<BusinessCaseWorkflowService>>>,
    cache_invalidation: Option<web::Data<Arc<CacheInvalidationService>>>,
    path: web::Path<String>,
    body: web::Json<BusinessCaseWorkflowStartRequest>,
    auth_svc: Option<web::Data<Arc<AuthService>>>,
    operator_identity_svc: Option<web::Data<Arc<OperatorIdentityService>>>,
    req: HttpRequest,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let Some(svc) = svc else {
        return Ok(orchestrator_unavailable());
    };
    claims.ensure_authenticated()?;
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
    match svc
        .start_workflow_for_viewer(
            &path.into_inner(),
            body.into_inner(),
            &actor,
            claims.viewer_department_id(),
            claims.viewer_department_name(),
        )
        .await
    {
        Ok(detail) => {
            invalidate_workflow_business_case_caches(
                cache_invalidation.as_ref().map(|data| data.get_ref()),
                &detail.business_case.flight_id,
            )
            .await;
            Ok(HttpResponse::Created().json(ApiResponse::ok_with_message(detail, "业务事项流程启动成功")))
        }
        Err(error) => Ok(raw_detail(
            actix_web::http::StatusCode::BAD_REQUEST,
            map_start_workflow_detail(error),
        )),
    }
}

async fn get_business_case_workflow_run(
    svc: Option<web::Data<Arc<BusinessCaseWorkflowService>>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let Some(svc) = svc else {
        return Ok(orchestrator_unavailable());
    };
    claims.ensure_authenticated()?;
    claims.ensure_grant(PermissionCatalog::WORKFLOW_RUN_READ)?;
    let run_id = path.into_inner();
    match svc
        .get_run_details_for_viewer(&run_id, claims.viewer_department_id(), claims.viewer_department_name())
        .await
    {
        Ok(Some(detail)) => {
            Ok(HttpResponse::Ok().json(ApiResponse::ok_with_message(detail, "获取业务事项流程运行成功")))
        }
        Ok(None) => Ok(raw_detail(
            actix_web::http::StatusCode::NOT_FOUND,
            "业务事项流程运行不存在",
        )),
        Err(_) => Ok(raw_detail(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            "获取业务事项流程运行失败",
        )),
    }
}

async fn get_business_case_workflow_by_case(
    svc: Option<web::Data<Arc<BusinessCaseWorkflowService>>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let Some(svc) = svc else {
        return Ok(orchestrator_unavailable());
    };
    claims.ensure_authenticated()?;
    claims.ensure_grant(PermissionCatalog::WORKFLOW_RUN_READ)?;
    let case_id = path.into_inner();
    match svc
        .get_case_workflow_for_viewer(&case_id, claims.viewer_department_id(), claims.viewer_department_name())
        .await
    {
        Ok(Some(detail)) => {
            Ok(HttpResponse::Ok().json(ApiResponse::ok_with_message(detail, "获取业务事项关联流程成功")))
        }
        Ok(None) => Ok(raw_detail(
            actix_web::http::StatusCode::NOT_FOUND,
            "业务事项未关联流程运行",
        )),
        Err(_) => Ok(raw_detail(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            "获取业务事项关联流程失败",
        )),
    }
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route(
        "/api/v2/business-case-workflows/{template_code}/start",
        web::post().to(start_business_case_workflow),
    )
    .route(
        "/api/v2/business-case-workflows/runs/{run_id}",
        web::get().to(get_business_case_workflow_run),
    )
    .route(
        "/api/v2/business_cases/{case_id}/workflow",
        web::get().to(get_business_case_workflow_by_case),
    );
}

#[cfg(test)]
mod tests {
    use super::configure;
    use super::map_start_workflow_detail;
    use crate::routes::workflow_actor::build_workflow_actor;
    use actix_web::{body::to_bytes, http::StatusCode, test, web, App};
    use chrono::Utc;
    use jsonwebtoken::{encode, EncodingKey, Header};

    use crate::middleware::jwt::{JwtAuth, JwtSecret};
    use fms_application::schemas::auth_schemas::TokenData;
    use fms_domain::error::DomainError;

    fn bearer_token() -> String {
        encode(
            &Header::default(),
            &TokenData {
                sub: Some("admin-1".to_string()),
                email: None,
                username: Some("dispatcher".to_string()),
                token_kind: Some("access".to_string()),
                is_admin: Some(true),
                permissions: vec!["flight:manage".to_string()],
                department: Some("ops".to_string()),
                department_id: Some("ops-1".to_string()),
                pv: Some(1),
                iat: Some(Utc::now().timestamp()),
                exp: Some((Utc::now() + chrono::Duration::hours(1)).timestamp()),
                iss: None,
                aud: None,
                ua_hash: None,
                ip_subnet_hash: None,
            },
            &EncodingKey::from_secret(b"test-secret"),
        )
        .expect("test jwt should encode")
    }

    #[actix_web::test]
    async fn get_run_returns_503_when_orchestrator_missing() {
        let token = bearer_token();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
                .configure(configure),
        )
        .await;

        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v2/business-case-workflows/runs/run-1")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = to_bytes(response.into_body()).await.expect("read body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json body");
        assert_eq!(
            payload["detail"],
            "Business case workflow orchestrator is not available"
        );
        println!("business case workflow 503 payload: {}", payload);
    }

    #[actix_web::test]
    async fn start_workflow_errors_normalize_to_python_bad_request_semantics() {
        let not_found = map_start_workflow_detail(DomainError::NotFound {
            entity_type: "flight",
            id: "MU123".to_string(),
        });
        let validation =
            map_start_workflow_detail(DomainError::ValidationError("template_code is required".to_string()));
        let internal = map_start_workflow_detail(DomainError::Internal("Flowable service unavailable".to_string()));

        assert_eq!(not_found, "Flight not found: MU123");
        assert_eq!(validation, "template_code is required");
        assert_eq!(internal, "Flowable service unavailable");
    }

    #[actix_web::test]
    async fn workflow_actor_keeps_operator_context_headers() {
        let claims = JwtAuth(TokenData {
            sub: Some("user-1".to_string()),
            email: None,
            username: Some("dispatcher".to_string()),
            token_kind: Some("access".to_string()),
            is_admin: Some(false),
            permissions: vec![],
            department: None,
            department_id: None,
            pv: Some(1),
            iat: None,
            exp: None,
            iss: None,
            aud: None,
            ua_hash: None,
            ip_subnet_hash: None,
        });

        let actor = build_workflow_actor(
            &claims,
            None,
            Some("web_client".to_string()),
            Some("console-1".to_string()),
        );

        assert_eq!(actor.context_type.as_deref(), Some("web_client"));
        assert_eq!(actor.context_id.as_deref(), Some("console-1"));
        assert_eq!(actor.username.as_deref(), Some("dispatcher"));
    }
}
