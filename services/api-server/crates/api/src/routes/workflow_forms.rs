use actix_web::{web, HttpResponse};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::sse::hub::SseHub;
use fms_application::schemas::response::ApiResponse;
use fms_application::schemas::workflow_form_schemas::{
    CreateWorkflowFormBindingRequest, CreateWorkflowFormTemplateRequest, SubmitWorkflowFormRequest,
};
use fms_application::services::auth_service::AuthService;
use fms_application::services::authorization_service::{AuthorizationService, PermissionCatalog};
use fms_application::services::workflow_form_service::{WorkflowFormActor, WorkflowFormService};

#[derive(Debug, Deserialize)]
struct TemplateLookupQuery {
    version: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct BindingListQuery {
    template_code: String,
}

fn service_unavailable() -> HttpResponse {
    HttpResponse::ServiceUnavailable().json(json!({
        "detail": "Workflow form service is not available"
    }))
}

fn actor_username(claims: &JwtAuth) -> Option<String> {
    claims
        .0
        .username
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn actor_user_id(claims: &JwtAuth) -> String {
    claims
        .0
        .sub
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| actor_username(claims))
        .unwrap_or_else(|| "unknown".to_string())
}

async fn build_actor(claims: &JwtAuth, auth_svc: Option<&AuthService>) -> Result<WorkflowFormActor, ApiError> {
    let user_id = actor_user_id(claims);
    let roles = if let Some(auth_svc) = auth_svc {
        auth_svc
            .find_user_by_id(&user_id)
            .await?
            .map(|user| user.roles)
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    Ok(WorkflowFormActor {
        user_id,
        username: actor_username(claims),
        operator_name: actor_username(claims),
        department_id: claims.viewer_department_id().map(str::to_string),
        department_name: claims.viewer_department_name().map(str::to_string),
        roles,
    })
}

async fn create_template(
    svc: Option<web::Data<Arc<WorkflowFormService>>>,
    claims: JwtAuth,
    body: web::Json<CreateWorkflowFormTemplateRequest>,
) -> Result<HttpResponse, ApiError> {
    let Some(svc) = svc else {
        return Ok(service_unavailable());
    };
    claims.ensure_authenticated()?;
    claims.ensure_grant(PermissionCatalog::WORKFLOW_DEFINITION_EDIT)?;

    let created = svc.create_template(body.into_inner(), &actor_user_id(&claims)).await?;
    Ok(HttpResponse::Created().json(ApiResponse::ok_with_message(
        fms_application::schemas::workflow_form_schemas::WorkflowFormTemplateResponse::from(created),
        "流程表单模板创建成功",
    )))
}

async fn create_binding(
    svc: Option<web::Data<Arc<WorkflowFormService>>>,
    claims: JwtAuth,
    body: web::Json<CreateWorkflowFormBindingRequest>,
) -> Result<HttpResponse, ApiError> {
    let Some(svc) = svc else {
        return Ok(service_unavailable());
    };
    claims.ensure_authenticated()?;
    claims.ensure_grant(PermissionCatalog::WORKFLOW_DEFINITION_EDIT)?;

    let created = svc.create_binding(body.into_inner()).await?;
    Ok(HttpResponse::Created().json(ApiResponse::ok_with_message(
        fms_application::schemas::workflow_form_schemas::WorkflowFormBindingResponse::from(created),
        "流程表单绑定创建成功",
    )))
}

async fn get_template(
    svc: Option<web::Data<Arc<WorkflowFormService>>>,
    path: web::Path<String>,
    query: web::Query<TemplateLookupQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let Some(svc) = svc else {
        return Ok(service_unavailable());
    };
    claims.ensure_authenticated()?;
    claims.ensure_grant(PermissionCatalog::WORKFLOW_DEFINITION_READ)?;

    let template = svc.get_template(&path.into_inner(), query.version).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok_with_message(
        fms_application::schemas::workflow_form_schemas::WorkflowFormTemplateResponse::from(template),
        "获取流程表单模板成功",
    )))
}

async fn list_bindings(
    svc: Option<web::Data<Arc<WorkflowFormService>>>,
    query: web::Query<BindingListQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let Some(svc) = svc else {
        return Ok(service_unavailable());
    };
    claims.ensure_authenticated()?;
    claims.ensure_grant(PermissionCatalog::WORKFLOW_DEFINITION_READ)?;

    let bindings = svc.list_bindings(&query.template_code).await?;
    let response: Vec<fms_application::schemas::workflow_form_schemas::WorkflowFormBindingResponse> =
        bindings.into_iter().map(Into::into).collect();
    Ok(HttpResponse::Ok().json(ApiResponse::ok_with_message(response, "获取流程表单绑定成功")))
}

async fn get_case_workflow_forms(
    svc: Option<web::Data<Arc<WorkflowFormService>>>,
    auth_svc: Option<web::Data<Arc<AuthService>>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let Some(svc) = svc else {
        return Ok(service_unavailable());
    };
    claims.ensure_authenticated()?;
    claims.ensure_grant(PermissionCatalog::BUSINESS_CASE_READ)?;

    let actor = build_actor(&claims, auth_svc.as_ref().map(|svc| svc.get_ref().as_ref())).await?;
    let response = svc
        .get_forms_for_case_workflow(
            &path.into_inner(),
            actor.department_id.as_deref(),
            actor.department_name.as_deref(),
            &actor.roles,
        )
        .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok_with_message(response, "获取流程表单成功")))
}

async fn submit_case_workflow_form(
    svc: Option<web::Data<Arc<WorkflowFormService>>>,
    auth_svc: Option<web::Data<Arc<AuthService>>>,
    sse_hub: Option<web::Data<Arc<SseHub>>>,
    path: web::Path<(String, String)>,
    claims: JwtAuth,
    body: web::Json<SubmitWorkflowFormRequest>,
) -> Result<HttpResponse, ApiError> {
    let Some(svc) = svc else {
        return Ok(service_unavailable());
    };
    claims.ensure_authenticated()?;
    claims.ensure_grant(PermissionCatalog::WORKFLOW_RUN_ACT)?;

    let (case_id, form_code) = path.into_inner();
    let body = body.into_inner();
    let actor = build_actor(&claims, auth_svc.as_ref().map(|svc| svc.get_ref().as_ref())).await?;
    let result = svc
        .submit_task_form(&case_id, &form_code, &body.task_id, body.data, &actor)
        .await?;

    if let Some(hub) = sse_hub.as_ref().map(|hub| hub.get_ref()) {
        hub.broadcast_event(
            "business_cases",
            Some("form_submitted"),
            json!({
                "event": "form_submitted",
                "case_id": result.case_id,
                "form_code": result.form_code,
                "submission_id": result.submission_id,
                "business_case": result.business_case,
            }),
        )
        .await;
    }

    Ok(HttpResponse::Ok().json(ApiResponse::ok_with_message(result, "表单提交成功")))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/api/v2/workflow-forms/templates", web::post().to(create_template))
        .route(
            "/api/v2/workflow-forms/templates/{form_code}",
            web::get().to(get_template),
        )
        .route("/api/v2/workflow-forms/bindings", web::post().to(create_binding))
        .route("/api/v2/workflow-forms/bindings", web::get().to(list_bindings))
        .route(
            "/api/v2/business_cases/{case_id}/workflow/forms",
            web::get().to(get_case_workflow_forms),
        )
        .route(
            "/api/v2/business_cases/{case_id}/workflow/forms/{form_code}/submit",
            web::post().to(submit_case_workflow_form),
        );
}

#[cfg(test)]
mod tests {
    use super::configure;
    use actix_web::{body::to_bytes, http::StatusCode, test, web, App};
    use chrono::Utc;
    use jsonwebtoken::{encode, EncodingKey, Header};

    use crate::middleware::jwt::JwtSecret;
    use fms_application::schemas::auth_schemas::TokenData;

    fn bearer_token() -> String {
        encode(
            &Header::default(),
            &TokenData {
                sub: Some("user-1".to_string()),
                email: None,
                username: Some("dispatcher".to_string()),
                token_kind: Some("access".to_string()),
                is_admin: Some(false),
                permissions: vec!["flight:read".to_string()],
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
    async fn get_forms_returns_503_when_service_missing() {
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
                .uri("/api/v2/business_cases/case-1/workflow/forms")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = to_bytes(response.into_body()).await.expect("read body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json body");
        assert_eq!(payload["detail"], "Workflow form service is not available");
    }

    #[actix_web::test]
    async fn get_template_returns_503_when_service_missing() {
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
                .uri("/api/v2/workflow-forms/templates/ground_confirm_form")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
