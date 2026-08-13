use actix_web::{web, HttpRequest, HttpResponse};
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::routes::dispatch_resources::{created_resp, ok_resp, MessageResponse};
use fms_application::schemas::dispatch_schemas::{TaskTypeCreate, TaskTypeResponse};
use fms_application::services::dispatch_resource_service::{to_task_type_response, StepListQuery};
use fms_application::types::ConcreteDispatchResourceService;

pub async fn list_task_types(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    query: web::Query<StepListQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let items = svc
        .list_task_types(
            query.category.as_deref(),
            query.page.unwrap_or(1),
            query.page_size.unwrap_or(100),
        )
        .await?;
    let payload: Vec<TaskTypeResponse> = items.into_iter().map(to_task_type_response).collect();
    Ok(ok_resp(&req, payload))
}

pub async fn create_task_type(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    body: web::Json<TaskTypeCreate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc.create_task_type(body.into_inner()).await?;
    Ok(created_resp(&req, to_task_type_response(saved)))
}

pub async fn delete_task_type(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    svc.delete_task_type(&path.into_inner()).await?;
    Ok(ok_resp(
        &req,
        MessageResponse {
            message: "任务类型已删除".into(),
        },
    ))
}
