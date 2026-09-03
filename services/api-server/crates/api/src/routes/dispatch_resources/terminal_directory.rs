//! 空间目录（航站楼/登机口/行李转盘）读写与只读上下文路由。
//!
//! 规则见 `docs/plans/2026-08-25-ontology-team-equipment-personnel-design.md`：
//! - 新建口/转盘 `terminal_id` 必填，目录行 + 成员行原子建立。
//! - 停用楼 / 移出成员：有未结束占用 → 409（经 `DomainError::Conflict`）。
//! - `get_context` 为只读动作（`dispatch:view`）。

use actix_web::{web, HttpRequest, HttpResponse};
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::routes::dispatch_resources::{created_resp, ok_resp};
use fms_application::services::terminal_resource_service::{
    CarouselCreate, CarouselUpdate, GateCreate, GateUpdate, StandCreate, StandUpdate, TerminalCreate,
    TerminalListQuery, TerminalUpdate,
};
use fms_application::types::ConcreteTerminalResourceService;

type TerminalSvc = Arc<ConcreteTerminalResourceService>;

// ──────────────────────────────────────────────── Terminal 主数据 ──
pub async fn list_terminals(
    req: HttpRequest,
    svc: web::Data<TerminalSvc>,
    claims: JwtAuth,
    query: web::Query<TerminalListQuery>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let items = svc.list_terminals(query.include_inactive.unwrap_or(false)).await?;
    Ok(ok_resp(&req, items))
}

pub async fn get_terminal(
    req: HttpRequest,
    svc: web::Data<TerminalSvc>,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let item = svc.get_terminal(&path.into_inner()).await?;
    match item {
        Some(terminal) => Ok(ok_resp(&req, terminal)),
        None => Err(ApiError::NotFound("航站楼不存在".into())),
    }
}

pub async fn create_terminal(
    req: HttpRequest,
    svc: web::Data<TerminalSvc>,
    claims: JwtAuth,
    body: web::Json<TerminalCreate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc.create_terminal(body.into_inner()).await?;
    Ok(created_resp(&req, saved))
}

pub async fn update_terminal(
    req: HttpRequest,
    svc: web::Data<TerminalSvc>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<TerminalUpdate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc.update_terminal(&path.into_inner(), body.into_inner()).await?;
    Ok(ok_resp(&req, saved))
}

/// 停用楼。存在未结束占用/分配 → 409 带明细。
pub async fn deactivate_terminal(
    req: HttpRequest,
    svc: web::Data<TerminalSvc>,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc.deactivate_terminal(&path.into_inner()).await?;
    Ok(ok_resp(&req, saved))
}

// ──────────────────────────────────────────────── Gate 目录 ──
pub async fn create_gate(
    req: HttpRequest,
    svc: web::Data<TerminalSvc>,
    claims: JwtAuth,
    body: web::Json<GateCreate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc.create_gate(body.into_inner()).await?;
    Ok(created_resp(&req, saved))
}

pub async fn update_gate(
    req: HttpRequest,
    svc: web::Data<TerminalSvc>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<GateUpdate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc.update_gate(&path.into_inner(), body.into_inner()).await?;
    Ok(ok_resp(&req, saved))
}

/// 停用登机口。存在未结束分配 → 409。
pub async fn deactivate_gate(
    req: HttpRequest,
    svc: web::Data<TerminalSvc>,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc.deactivate_gate(&path.into_inner()).await?;
    Ok(ok_resp(&req, saved))
}

// ──────────────────────────────────────────────── Carousel 目录 ──
pub async fn create_carousel(
    req: HttpRequest,
    svc: web::Data<TerminalSvc>,
    claims: JwtAuth,
    body: web::Json<CarouselCreate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc.create_carousel(body.into_inner()).await?;
    Ok(created_resp(&req, saved))
}

pub async fn update_carousel(
    req: HttpRequest,
    svc: web::Data<TerminalSvc>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<CarouselUpdate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc.update_carousel(&path.into_inner(), body.into_inner()).await?;
    Ok(ok_resp(&req, saved))
}

/// 停用行李转盘。存在未结束分配 → 409。
pub async fn deactivate_carousel(
    req: HttpRequest,
    svc: web::Data<TerminalSvc>,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc.deactivate_carousel(&path.into_inner()).await?;
    Ok(ok_resp(&req, saved))
}

// ──────────────────────────────────────────────── Stand 目录 ──
pub async fn create_stand(
    req: HttpRequest,
    svc: web::Data<TerminalSvc>,
    claims: JwtAuth,
    body: web::Json<StandCreate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc.create_stand(body.into_inner()).await?;
    Ok(created_resp(&req, saved))
}

pub async fn update_stand(
    req: HttpRequest,
    svc: web::Data<TerminalSvc>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<StandUpdate>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc.update_stand(&path.into_inner(), body.into_inner()).await?;
    Ok(ok_resp(&req, saved))
}

/// 停用机位。存在未结束占用 → 409。
pub async fn deactivate_stand(
    req: HttpRequest,
    svc: web::Data<TerminalSvc>,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let saved = svc.deactivate_stand(&path.into_inner()).await?;
    Ok(ok_resp(&req, saved))
}

// ──────────────────────────────────────────────── 成员关系 ──
/// 把既有机位挂到某座启用楼。
pub async fn add_stand_member(
    req: HttpRequest,
    svc: web::Data<TerminalSvc>,
    claims: JwtAuth,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let (terminal_id, stand_id) = path.into_inner();
    svc.add_stand_member(&terminal_id, &stand_id).await?;
    Ok(ok_resp(
        &req,
        serde_json::json!({ "terminal_id": terminal_id, "stand_id": stand_id }),
    ))
}

/// 从楼里移出机位；有未结束占用 → 409。
pub async fn remove_stand_member(
    req: HttpRequest,
    svc: web::Data<TerminalSvc>,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let stand_id = path.into_inner();
    svc.remove_stand_member(&stand_id).await?;
    Ok(ok_resp(&req, serde_json::json!({ "stand_id": stand_id })))
}

pub async fn add_gate_member(
    req: HttpRequest,
    svc: web::Data<TerminalSvc>,
    claims: JwtAuth,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let (terminal_id, gate_id) = path.into_inner();
    svc.add_gate_member(&terminal_id, &gate_id).await?;
    Ok(ok_resp(
        &req,
        serde_json::json!({ "terminal_id": terminal_id, "gate_id": gate_id }),
    ))
}

pub async fn remove_gate_member(
    req: HttpRequest,
    svc: web::Data<TerminalSvc>,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let gate_id = path.into_inner();
    svc.remove_gate_member(&gate_id).await?;
    Ok(ok_resp(&req, serde_json::json!({ "gate_id": gate_id })))
}

pub async fn add_carousel_member(
    req: HttpRequest,
    svc: web::Data<TerminalSvc>,
    claims: JwtAuth,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let (terminal_id, carousel_id) = path.into_inner();
    svc.add_carousel_member(&terminal_id, &carousel_id).await?;
    Ok(ok_resp(
        &req,
        serde_json::json!({ "terminal_id": terminal_id, "carousel_id": carousel_id }),
    ))
}

pub async fn remove_carousel_member(
    req: HttpRequest,
    svc: web::Data<TerminalSvc>,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:manage")?;
    let carousel_id = path.into_inner();
    svc.remove_carousel_member(&carousel_id).await?;
    Ok(ok_resp(&req, serde_json::json!({ "carousel_id": carousel_id })))
}

// ──────────────────────────────────────────────── 只读上下文 ──
/// 只读动作：楼 + 三类成员目录行。楼不存在 → 404。
pub async fn get_context(
    req: HttpRequest,
    svc: web::Data<TerminalSvc>,
    claims: JwtAuth,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let directory = svc.get_context(&path.into_inner()).await?;
    match directory {
        Some(directory) => Ok(ok_resp(&req, directory)),
        None => Err(ApiError::NotFound("航站楼不存在".into())),
    }
}
