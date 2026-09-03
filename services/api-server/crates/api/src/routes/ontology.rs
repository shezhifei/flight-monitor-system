//! 本体 V1 业务 API（ONTOLOGY_V1.md §3–§7）
//!
//! 与 `/api/v2/ai/ontology`（AI schema 快照）分离：本模块面向运行控制写路径。

use std::sync::Arc;

use actix_web::{web, HttpResponse};
use serde_json::json;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use fms_application::schemas::ontology_schemas::{
    AdjustCarouselRequest, AdjustGateRequest, AdjustStandRequest, AllocateCarouselRequest, AllocateGateRequest,
    AllocateStandRequest, AutoLinkScanRequest, BreakTurnaroundLinkRequest, ConfirmDraftFlightsRequest,
    CreateSuggestionRequest, CreateTurnaroundLinkRequest, ReassignAircraftRequest, ReleaseResourceRequest,
    SuggestionAcceptRequest, SuggestionQuery, SuggestionRejectRequest,
};
use fms_application::services::ontology_service::{OntologyError, OntologyService};

fn actor_id(claims: &JwtAuth) -> String {
    claims
        .0
        .username
        .as_deref()
        .or(claims.0.sub.as_deref())
        .unwrap_or("unknown")
        .to_string()
}

fn map_ontology_error(error: OntologyError) -> ApiError {
    match error {
        OntologyError::Validation(message) => ApiError::BadRequest(message),
        OntologyError::NotFound(message) => ApiError::NotFound(message),
        OntologyError::Conflict(message) => ApiError::Conflict(message),
        OntologyError::Forbidden(message) => ApiError::Forbidden(message),
        OntologyError::Internal(message) => ApiError::Internal(message),
    }
}

async fn reassign_aircraft(
    svc: web::Data<Arc<OntologyService>>,
    body: web::Json<ReassignAircraftRequest>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ontology.aircraft.reassign")?;
    let permissions = claims.0.permissions.clone();
    let is_admin = claims.0.is_admin.unwrap_or(false);
    let result = svc
        .reassign_aircraft(body.into_inner(), &actor_id(&claims), &permissions, is_admin)
        .await
        .map_err(map_ontology_error)?;
    Ok(HttpResponse::Ok().json(json!({ "success": true, "data": result })))
}

async fn accept_suggestion(
    svc: web::Data<Arc<OntologyService>>,
    path: web::Path<String>,
    body: web::Json<SuggestionAcceptRequest>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    // 具体 stand/gate 权限由服务层按 kind 二次校验（不变量 12）。
    // 权限和审计身份必须来自已验证的 JWT，不能信任请求体中的同名字段。
    let request = canonical_accept_request(body.into_inner(), &claims);
    let result = svc
        .accept_suggestion(&path.into_inner(), request)
        .await
        .map_err(map_ontology_error)?;
    Ok(HttpResponse::Ok().json(json!({ "success": true, "data": result })))
}

fn canonical_accept_request(mut request: SuggestionAcceptRequest, claims: &JwtAuth) -> SuggestionAcceptRequest {
    request.actor_permissions = claims.0.permissions.clone();
    if claims.0.is_admin.unwrap_or(false) && !request.actor_permissions.iter().any(|p| p == "*") {
        request.actor_permissions.push("*".to_string());
    }
    request.accepted_by = actor_id(claims);
    request
}

async fn reject_suggestion(
    svc: web::Data<Arc<OntologyService>>,
    path: web::Path<String>,
    body: web::Json<SuggestionRejectRequest>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ontology.suggestion.reject")?;
    let mut request = body.into_inner();
    request.rejected_by = actor_id(&claims);
    let result = svc
        .reject_suggestion(&path.into_inner(), request)
        .await
        .map_err(map_ontology_error)?;
    Ok(HttpResponse::Ok().json(json!({ "success": true, "data": result })))
}

async fn list_suggestions(
    svc: web::Data<Arc<OntologyService>>,
    query: web::Query<SuggestionQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ontology.read")?;
    let items = svc
        .list_suggestions(query.into_inner())
        .await
        .map_err(map_ontology_error)?;
    Ok(HttpResponse::Ok().json(json!({ "success": true, "data": items })))
}

async fn confirm_drafts(
    svc: web::Data<Arc<OntologyService>>,
    body: web::Json<ConfirmDraftFlightsRequest>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ontology.plan.confirm")?;
    let mut request = body.into_inner();
    request.confirmed_by = actor_id(&claims);
    let result = svc.confirm_draft_flights(request).await.map_err(map_ontology_error)?;
    Ok(HttpResponse::Ok().json(json!({ "success": true, "data": result })))
}

async fn flight_resource_view(
    svc: web::Data<Arc<OntologyService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ontology.read")?;
    let view = svc
        .flight_resource_view(&path.into_inner())
        .await
        .map_err(map_ontology_error)?;
    Ok(HttpResponse::Ok().json(json!({ "success": true, "data": view })))
}

async fn aircraft_resource_view(
    svc: web::Data<Arc<OntologyService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ontology.read")?;
    let view = svc
        .aircraft_resource_view(&path.into_inner())
        .await
        .map_err(map_ontology_error)?;
    Ok(HttpResponse::Ok().json(json!({ "success": true, "data": view })))
}

fn actor_flags(claims: &JwtAuth) -> (String, Vec<String>, bool) {
    (
        actor_id(claims),
        claims.0.permissions.clone(),
        claims.0.is_admin.unwrap_or(false),
    )
}

async fn allocate_stand(
    svc: web::Data<Arc<OntologyService>>,
    body: web::Json<AllocateStandRequest>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ontology.stand.manage")?;
    let (actor, permissions, is_admin) = actor_flags(&claims);
    let result = svc
        .allocate_stand(body.into_inner(), &actor, &permissions, is_admin)
        .await
        .map_err(map_ontology_error)?;
    Ok(HttpResponse::Created().json(json!({ "success": true, "data": result })))
}

async fn adjust_stand(
    svc: web::Data<Arc<OntologyService>>,
    path: web::Path<String>,
    body: web::Json<AdjustStandRequest>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ontology.stand.manage")?;
    let (actor, permissions, is_admin) = actor_flags(&claims);
    let result = svc
        .adjust_stand(&path.into_inner(), body.into_inner(), &actor, &permissions, is_admin)
        .await
        .map_err(map_ontology_error)?;
    Ok(HttpResponse::Ok().json(json!({ "success": true, "data": result })))
}

async fn release_stand(
    svc: web::Data<Arc<OntologyService>>,
    path: web::Path<String>,
    body: web::Json<ReleaseResourceRequest>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ontology.stand.manage")?;
    let (actor, permissions, is_admin) = actor_flags(&claims);
    let mut request = body.into_inner();
    request.released_by = Some(actor.clone());
    let result = svc
        .release_stand(&path.into_inner(), request, &actor, &permissions, is_admin)
        .await
        .map_err(map_ontology_error)?;
    Ok(HttpResponse::Ok().json(json!({ "success": true, "data": result })))
}

async fn allocate_gate(
    svc: web::Data<Arc<OntologyService>>,
    body: web::Json<AllocateGateRequest>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ontology.gate.manage")?;
    let (actor, permissions, is_admin) = actor_flags(&claims);
    let result = svc
        .allocate_gate(body.into_inner(), &actor, &permissions, is_admin)
        .await
        .map_err(map_ontology_error)?;
    Ok(HttpResponse::Created().json(json!({ "success": true, "data": result })))
}

async fn adjust_gate(
    svc: web::Data<Arc<OntologyService>>,
    path: web::Path<String>,
    body: web::Json<AdjustGateRequest>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ontology.gate.manage")?;
    let (actor, permissions, is_admin) = actor_flags(&claims);
    let result = svc
        .adjust_gate(&path.into_inner(), body.into_inner(), &actor, &permissions, is_admin)
        .await
        .map_err(map_ontology_error)?;
    Ok(HttpResponse::Ok().json(json!({ "success": true, "data": result })))
}

async fn release_gate(
    svc: web::Data<Arc<OntologyService>>,
    path: web::Path<String>,
    body: web::Json<ReleaseResourceRequest>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ontology.gate.manage")?;
    let (actor, permissions, is_admin) = actor_flags(&claims);
    let mut request = body.into_inner();
    request.released_by = Some(actor.clone());
    let result = svc
        .release_gate(&path.into_inner(), request, &actor, &permissions, is_admin)
        .await
        .map_err(map_ontology_error)?;
    Ok(HttpResponse::Ok().json(json!({ "success": true, "data": result })))
}

async fn allocate_carousel(
    svc: web::Data<Arc<OntologyService>>,
    body: web::Json<AllocateCarouselRequest>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ontology.carousel.manage")?;
    let (actor, permissions, is_admin) = actor_flags(&claims);
    let result = svc
        .allocate_carousel(body.into_inner(), &actor, &permissions, is_admin)
        .await
        .map_err(map_ontology_error)?;
    Ok(HttpResponse::Created().json(json!({ "success": true, "data": result })))
}

async fn adjust_carousel(
    svc: web::Data<Arc<OntologyService>>,
    path: web::Path<String>,
    body: web::Json<AdjustCarouselRequest>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ontology.carousel.manage")?;
    let (actor, permissions, is_admin) = actor_flags(&claims);
    let result = svc
        .adjust_carousel(&path.into_inner(), body.into_inner(), &actor, &permissions, is_admin)
        .await
        .map_err(map_ontology_error)?;
    Ok(HttpResponse::Ok().json(json!({ "success": true, "data": result })))
}

async fn release_carousel(
    svc: web::Data<Arc<OntologyService>>,
    path: web::Path<String>,
    body: web::Json<ReleaseResourceRequest>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ontology.carousel.manage")?;
    let (actor, permissions, is_admin) = actor_flags(&claims);
    let mut request = body.into_inner();
    request.released_by = Some(actor.clone());
    let result = svc
        .release_carousel(&path.into_inner(), request, &actor, &permissions, is_admin)
        .await
        .map_err(map_ontology_error)?;
    Ok(HttpResponse::Ok().json(json!({ "success": true, "data": result })))
}

async fn list_carousel_assignments(
    svc: web::Data<Arc<OntologyService>>,
    path: web::Path<String>,
    query: web::Query<std::collections::HashMap<String, String>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    if !(claims.has_permission("ontology.carousel.manage") || claims.has_permission("ontology.read")) {
        return Err(ApiError::Forbidden("missing permission ontology.read".into()));
    }
    let limit: i64 = query.get("limit").and_then(|v| v.parse().ok()).unwrap_or(50);
    let result = svc
        .list_carousel_assignments(&path.into_inner(), limit)
        .await
        .map_err(map_ontology_error)?;
    Ok(HttpResponse::Ok().json(json!({ "success": true, "data": result })))
}

async fn create_turnaround_link(
    svc: web::Data<Arc<OntologyService>>,
    body: web::Json<CreateTurnaroundLinkRequest>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let (actor, permissions, is_admin) = actor_flags(&claims);
    let mut request = body.into_inner();
    request.created_by = Some(actor.clone());
    let result = svc
        .create_turnaround_link(request, &actor, &permissions, is_admin)
        .await
        .map_err(map_ontology_error)?;
    Ok(HttpResponse::Created().json(json!({ "success": true, "data": result })))
}

async fn break_turnaround_link(
    svc: web::Data<Arc<OntologyService>>,
    path: web::Path<String>,
    body: web::Json<BreakTurnaroundLinkRequest>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let (actor, permissions, is_admin) = actor_flags(&claims);
    let mut request = body.into_inner();
    request.broken_by = Some(actor.clone());
    let result = svc
        .break_turnaround_link(&path.into_inner(), request, &actor, &permissions, is_admin)
        .await
        .map_err(map_ontology_error)?;
    Ok(HttpResponse::Ok().json(json!({ "success": true, "data": result })))
}

async fn auto_link_scan(
    svc: web::Data<Arc<OntologyService>>,
    body: web::Json<AutoLinkScanRequest>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ontology.plan.confirm")?;
    let result = svc
        .auto_link_scan(body.into_inner())
        .await
        .map_err(map_ontology_error)?;
    Ok(HttpResponse::Ok().json(json!({ "success": true, "data": result })))
}

async fn list_turnaround_links(
    svc: web::Data<Arc<OntologyService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ontology.read")?;
    let items = svc
        .list_turnaround_links(&path.into_inner())
        .await
        .map_err(map_ontology_error)?;
    Ok(HttpResponse::Ok().json(json!({ "success": true, "data": items })))
}

async fn expire_stale_suggestions(
    svc: web::Data<Arc<OntologyService>>,
    query: web::Query<std::collections::HashMap<String, String>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ontology.plan.confirm")?;
    let limit = query.get("limit").and_then(|v| v.parse::<i64>().ok()).unwrap_or(100);
    let expired = svc.expire_stale_suggestions(limit).await.map_err(map_ontology_error)?;
    Ok(HttpResponse::Ok().json(json!({ "success": true, "data": { "expired": expired } })))
}

async fn create_suggestion(
    svc: web::Data<Arc<OntologyService>>,
    body: web::Json<CreateSuggestionRequest>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    // 细粒度权限由服务层按 kind 判定
    let (actor, permissions, is_admin) = actor_flags(&claims);
    if !is_admin
        && !permissions.iter().any(|p| {
            p == "ontology.stand.manage"
                || p == "ontology.gate.manage"
                || p == "ontology.suggestion.accept_stand"
                || p == "ontology.suggestion.accept_gate"
                || p == "*"
        })
    {
        return Err(ApiError::Forbidden(
            "missing permission to create resource suggestion".into(),
        ));
    }
    let mut request = body.into_inner();
    request.created_by = Some(actor.clone());
    let result = svc
        .create_suggestion(request, &actor, &permissions, is_admin)
        .await
        .map_err(map_ontology_error)?;
    Ok(HttpResponse::Created().json(json!({ "success": true, "data": result })))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/ontology")
            .route("/aircraft/reassign", web::post().to(reassign_aircraft))
            .route("/stands/occupations", web::post().to(allocate_stand))
            .route("/stands/occupations/{id}", web::patch().to(adjust_stand))
            .route("/stands/occupations/{id}/release", web::post().to(release_stand))
            .route("/gates/assignments", web::post().to(allocate_gate))
            .route("/gates/assignments/{id}", web::patch().to(adjust_gate))
            .route("/gates/assignments/{id}/release", web::post().to(release_gate))
            .route("/carousels/assignments", web::post().to(allocate_carousel))
            .route("/carousels/assignments/{id}", web::patch().to(adjust_carousel))
            .route("/carousels/assignments/{id}/release", web::post().to(release_carousel))
            .route(
                "/flights/{flight_id}/carousels",
                web::get().to(list_carousel_assignments),
            )
            .route("/turnaround-links", web::post().to(create_turnaround_link))
            .route("/turnaround-links/{id}/break", web::post().to(break_turnaround_link))
            .route("/turnaround-links/auto-scan", web::post().to(auto_link_scan))
            .route(
                "/flights/{flight_id}/turnaround-links",
                web::get().to(list_turnaround_links),
            )
            .route("/suggestions", web::get().to(list_suggestions))
            .route("/suggestions", web::post().to(create_suggestion))
            .route("/suggestions/expire-stale", web::post().to(expire_stale_suggestions))
            .route("/suggestions/{id}/accept", web::post().to(accept_suggestion))
            .route("/suggestions/{id}/reject", web::post().to(reject_suggestion))
            .route("/flights/confirm-drafts", web::post().to(confirm_drafts))
            .route("/flights/{flight_id}/resources", web::get().to(flight_resource_view))
            .route(
                "/aircraft/{registration}/resources",
                web::get().to(aircraft_resource_view),
            ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use fms_application::schemas::auth_schemas::TokenData;

    fn claims(permissions: Vec<String>, is_admin: bool) -> JwtAuth {
        JwtAuth(TokenData {
            sub: Some("subject-1".into()),
            email: None,
            username: Some("operator-1".into()),
            token_kind: None,
            is_admin: Some(is_admin),
            permissions,
            department: None,
            department_id: None,
            pv: None,
            iat: None,
            exp: None,
            iss: None,
            aud: None,
            ua_hash: None,
            ip_subnet_hash: None,
        })
    }

    #[test]
    fn canonical_accept_request_ignores_client_permissions_and_identity() {
        let request = canonical_accept_request(
            SuggestionAcceptRequest {
                accepted_by: "attacker".into(),
                actor_permissions: vec!["*".into()],
            },
            &claims(vec!["ontology.suggestion.accept_stand".into()], false),
        );

        assert_eq!(request.accepted_by, "operator-1");
        assert_eq!(request.actor_permissions, vec!["ontology.suggestion.accept_stand"]);
    }

    #[test]
    fn canonical_accept_request_adds_wildcard_only_for_jwt_admin() {
        let request = canonical_accept_request(
            SuggestionAcceptRequest {
                accepted_by: String::new(),
                actor_permissions: Vec::new(),
            },
            &claims(Vec::new(), true),
        );

        assert_eq!(request.accepted_by, "operator-1");
        assert_eq!(request.actor_permissions, vec!["*"]);
    }
}
