use actix_web::{web, HttpRequest, HttpResponse};
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::routes::dispatch_resources::{created_resp, ok_resp, MessageResponse};
use fms_application::schemas::dispatch_schemas::{
    PositionUpdate, TeamCreate, TeamMemberAdd, TeamMemberResponse, TeamResponse, TeamUpdate,
};
use fms_application::types::ConcreteDispatchResourceService;
use fms_application::services::dispatch_resource_service::{
    to_member_response, to_team_response, TeamDetailQuery, TeamListQuery, TeamMembersQuery,
    TeamStatusQuery,
};

pub async fn list_teams(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    query: web::Query<TeamListQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("team:view")?;
    let items = svc
        .list_teams(
            query.include_inactive.unwrap_or(false),
            query.team_type_id.as_deref(),
            query.terminal.as_deref(),
            query.page.unwrap_or(1),
            query.page_size.unwrap_or(50),
        )
        .await?;
    let payload: Vec<TeamResponse> = items.into_iter().map(to_team_response).collect();
    Ok(ok_resp(&req, payload))
}

pub async fn get_team(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    query: web::Query<TeamDetailQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("team:view")?;
    let item = svc
        .get_team(&path.into_inner(), query.load_members.unwrap_or(true))
        .await?;
    match item {
        Some(team) => Ok(ok_resp(&req, to_team_response(team))),
        None => Err(ApiError::NotFound("班组不存在".into())),
    }
}

pub async fn create_team(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    body: web::Json<TeamCreate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("team:manage")?;
    let saved = svc.create_team(body.into_inner()).await?;
    Ok(created_resp(&req, to_team_response(saved)))
}

pub async fn update_team(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<TeamUpdate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("team:manage")?;
    let saved = svc.update_team(&path.into_inner(), body.into_inner()).await?;
    Ok(ok_resp(&req, to_team_response(saved)))
}

pub async fn delete_team(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("team:manage")?;
    svc.delete_team(&path.into_inner()).await?;
    Ok(ok_resp(
        &req,
        MessageResponse {
            message: "班组已删除".into(),
        },
    ))
}

pub async fn update_team_position(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<PositionUpdate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("team:manage")?;
    svc.update_team_position(&path.into_inner(), body.into_inner()).await?;
    Ok(ok_resp(
        &req,
        MessageResponse {
            message: "位置已更新".into(),
        },
    ))
}

pub async fn update_team_status(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    query: web::Query<TeamStatusQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("team:manage")?;
    svc.update_team_status(&path.into_inner(), &query.status).await?;
    Ok(ok_resp(
        &req,
        MessageResponse {
            message: "状态已更新".into(),
        },
    ))
}

pub async fn list_team_members(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    query: web::Query<TeamMembersQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("team:view")?;
    let items = svc
        .list_team_members(&path.into_inner(), query.include_inactive.unwrap_or(false))
        .await?;
    let payload: Vec<TeamMemberResponse> = items.into_iter().map(to_member_response).collect();
    Ok(ok_resp(&req, payload))
}

pub async fn add_team_member(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<TeamMemberAdd>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("team:manage")?;
    let saved = svc.add_team_member(&path.into_inner(), body.into_inner()).await?;
    Ok(created_resp(&req, to_member_response(saved)))
}

pub async fn remove_team_member(
    req: HttpRequest,
    svc: web::Data<Arc<ConcreteDispatchResourceService>>,
    claims: JwtAuth,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("team:manage")?;
    let (team_id, user_id) = path.into_inner();
    svc.remove_team_member(&team_id, &user_id).await?;
    Ok(ok_resp(
        &req,
        MessageResponse {
            message: "成员已移除".into(),
        },
    ))
}
