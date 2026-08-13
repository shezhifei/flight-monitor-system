use actix_web::{web, HttpRequest, HttpResponse};
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::services::python_sidecar_proxy::{ai_sidecar_auth_for_path, ai_sidecar_timeout, forward_request};
use crate::sse::hub::SseHub;
use fms_application::schemas::ai_schemas::{
    ConnectionProbeRequest, EntityConfigUpdate, EntityToolsUpdateRequest, SystemPromptUpdate,
};
use fms_application::services::ai_route_service::AiRouteService;
use fms_application::services::ai_runtime_service::AiRuntimeService;

use super::shared::*;

pub async fn capabilities(svc: web::Data<Arc<AiRouteService>>, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    let payload = svc
        .capabilities(claims.has_permission("ai:execute"), claims.has_permission("ai:chat"))
        .await
        .map_err(map_route_error)?;
    Ok(ok_resp(payload))
}

pub async fn list_tools(
    svc: web::Data<Arc<AiRouteService>>,
    query: web::Query<ListToolsQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    svc.validate_invocation_mode(query.invocation_mode.as_deref())
        .map_err(map_route_error)?;
    let payload = svc
        .list_tools(query.category.as_deref())
        .await
        .map_err(map_route_error)?;
    Ok(ok_resp(payload))
}

pub async fn execute_tool(
    svc: web::Data<Arc<AiRouteService>>,
    runtime: web::Data<Arc<AiRuntimeService>>,
    sse_hub: web::Data<Arc<SseHub>>,
    claims: JwtAuth,
    body: web::Json<ToolExecuteRequest>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:execute")?;
    let spec = match svc.find_tool_spec(&body.tool_name, &body.tool_args).await {
        Ok(spec) => spec,
        Err(AiRouteError::Domain(fms_domain::error::DomainError::NotFound { .. })) => {
            return Ok(raw_detail(
                actix_web::http::StatusCode::NOT_FOUND,
                format!("工具不存在: {}", body.tool_name),
            ))
        }
        Err(error) => return Err(map_route_error(error)),
    };
    let payload = runtime
        .execute_tool(
            spec,
            body.tool_args.clone(),
            Some(current_user_id(&claims)),
            current_user_roles(&claims),
        )
        .await;
    let status = payload
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("error");
    let accepted = payload
        .get("success")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
        || status == "pending_approval";
    let execution_id = payload.get("execution_id").cloned().unwrap_or(serde_json::Value::Null);
    let response = serde_json::json!({
        "success": accepted,
        "accepted": accepted,
        "status": status,
        "code": payload.get("code"),
        "message": payload.get("message"),
        "recoverable": payload.get("recoverable"),
        "retryable": payload.get("retryable"),
        "execution_id": execution_id,
        "tool_name": body.tool_name,
        "severity": payload.get("severity"),
        "approval_required": payload.get("approval_required"),
        "approval_id": payload.get("approval_id"),
        "result_data": payload.get("data"),
        "error": payload.get("error"),
        "meta": payload.get("meta").cloned().unwrap_or_else(|| serde_json::json!({ "contract_version": "2.0" })),
    });
    broadcast_ai_event(
        &sse_hub,
        if status == "pending_approval" {
            "tool_pending_approval"
        } else {
            "tool_executed"
        },
        serde_json::json!({
            "status": status,
            "tool_name": body.tool_name,
            "execution_id": execution_id,
            "payload": payload,
        }),
    )
    .await;
    Ok(HttpResponse::Ok().json(response))
}

pub async fn list_tool_categories(
    svc: web::Data<Arc<AiRouteService>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    Ok(ok_resp(svc.list_tool_categories().await))
}

pub async fn list_entities(svc: web::Data<Arc<AiRouteService>>, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    Ok(ok_resp(svc.list_entities().await.map_err(map_route_error)?))
}

pub async fn get_entity(
    svc: web::Data<Arc<AiRouteService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    let entity_id = path.into_inner();
    match svc.get_entity(&entity_id).await {
        Ok(Some(payload)) => Ok(ok_resp(payload)),
        Ok(None) => Err(ApiError::NotFound("Entity config not found".into())),
        Err(error) => Err(map_route_error(error)),
    }
}

pub async fn update_entity(
    svc: web::Data<Arc<AiRouteService>>,
    path: web::Path<String>,
    claims: JwtAuth,
    body: web::Json<EntityConfigUpdate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:config")?;
    let payload = svc
        .update_entity(&path.into_inner(), body.into_inner())
        .await
        .map_err(map_route_error)?;
    Ok(ok_resp_with_message(payload, "配置更新成功"))
}

pub async fn test_connection(
    req: HttpRequest,
    svc: web::Data<Arc<AiRouteService>>,
    claims: JwtAuth,
    body: web::Json<ConnectionProbeRequest>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:config")?;
    let probe_req = body.into_inner();
    let include_caps = probe_req.include_capabilities;
    let entity_id = probe_req.entity_id.clone();
    let mut payload = svc.test_connection_base(probe_req).await.map_err(map_route_error)?;

    if include_caps {
        if let Some(eid) = entity_id {
            let base = ai_sidecar_base_url();
            let target = format!("{base}/internal/ai/v1/entities/{eid}/capabilities");
            let internal_path = target.strip_prefix(&base).unwrap_or(&target);
            let caps_resp = forward_request(
                &req,
                reqwest::Method::GET,
                &target,
                ai_sidecar_auth_for_path(internal_path),
                ai_sidecar_timeout(),
            )
            .await;
            if caps_resp.status().is_success() {
                if let Ok(body_bytes) = actix_web::body::to_bytes(caps_resp.into_body()).await {
                    if let Ok(caps_json) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                        let caps_data = caps_json.get("data").cloned().unwrap_or(caps_json);
                        if let Some(obj) = payload.as_object_mut() {
                            obj.insert("capabilities".to_string(), caps_data);
                        }
                    }
                }
            }
        }
    }

    Ok(ok_resp_with_message(payload, "连通性测试通过"))
}

pub async fn list_models(svc: web::Data<Arc<AiRouteService>>, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    Ok(ok_resp(svc.list_models().await))
}

pub async fn get_entity_prompt(
    svc: web::Data<Arc<AiRouteService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    let entity_id = path.into_inner();
    match svc.get_entity_prompt(&entity_id).await {
        Ok(Some(payload)) => Ok(ok_resp(payload)),
        Ok(None) => Err(ApiError::NotFound(format!("实体不存在: {entity_id}"))),
        Err(error) => Err(map_route_error(error)),
    }
}

pub async fn update_entity_prompt(
    svc: web::Data<Arc<AiRouteService>>,
    path: web::Path<String>,
    claims: JwtAuth,
    body: web::Json<SystemPromptUpdate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:config")?;
    svc.update_entity_prompt(&path.into_inner(), body.into_inner())
        .await
        .map_err(map_route_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "系统提示词已保存"
    })))
}

pub async fn registry_status(svc: web::Data<Arc<AiRouteService>>, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    Ok(ok_resp(svc.registry_status().await))
}

pub async fn registry_initialize(
    svc: web::Data<Arc<AiRouteService>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:config")?;
    Ok(ok_resp_with_message(
        svc.registry_initialize().await,
        "工具注册表已就绪",
    ))
}

pub async fn get_entity_tools(
    svc: web::Data<Arc<AiRouteService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    let entity_id = path.into_inner();
    match svc.get_entity_tools(&entity_id).await {
        Ok(Some(payload)) => Ok(ok_resp(payload)),
        Ok(None) => Err(ApiError::NotFound(format!("实体不存在: {entity_id}"))),
        Err(error) => Err(map_route_error(error)),
    }
}

pub async fn update_entity_tools(
    svc: web::Data<Arc<AiRouteService>>,
    path: web::Path<String>,
    claims: JwtAuth,
    body: web::Json<EntityToolsUpdateRequest>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:config")?;
    let payload = svc
        .update_entity_tools(&path.into_inner(), body.into_inner())
        .await
        .map_err(map_route_error)?;
    Ok(ok_resp_with_message(payload, "工具权限已保存"))
}
