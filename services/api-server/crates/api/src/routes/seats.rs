//! 运行台占席（OccupySeat）路由
//!
//! 第一性原理：键盘前的人就是写入的人。运行台常驻「换人」，输入个人用户名 + 密码
//! → 把岗位 `users.current_occupant_user_id` 切到该个人，并签发该个人 token（JWT sub
//! 永远个人）。`proof.kind` 本期仅 `password`；`face` / `ext` 预留 501，其它 400。
//!
//! `POST /api/v2/seats/{position_id}/occupy`

pub(crate) use actix_web::{web, HttpRequest, HttpResponse};
pub(crate) use fms_application::schemas::auth_schemas::SeatOccupyRequest;
pub(crate) use fms_application::services::auth_service::AuthService;
pub(crate) use std::sync::Arc;

pub(crate) use crate::error::ApiError;
pub(crate) use crate::middleware::jwt::JwtAuth;
pub(crate) use crate::request_context::{
    build_ip_subnet_hash, build_user_agent_hash, extract_client_ip, extract_user_agent,
};
pub(crate) use crate::routes::auth::shared::{attach_token_response_cookies, parse_client_surface};

pub(crate) async fn occupy(
    svc: web::Data<Arc<AuthService>>,
    claims: JwtAuth,
    path: web::Path<String>,
    body: web::Json<SeatOccupyRequest>,
    req: HttpRequest,
) -> Result<HttpResponse, ApiError> {
    // 登录拒岗位，故此处的 JWT sub 恒为个人。要求已登录（未占席时运行台可读不可写，
    // 但「换人/上岗」是获得写权限的唯一入口，登录个人皆可发起）。
    let _person = claims
        .0
        .sub
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ApiError::Unauthorized("missing authenticated user".into()))?;

    let position_id = path.into_inner();

    match body.proof.kind.as_str() {
        "password" => {
            let password = body
                .proof
                .password
                .as_deref()
                .ok_or_else(|| ApiError::BadRequest("password proof 缺少 password".into()))?;
            let client_ip = extract_client_ip(&req);
            let user_agent_hash = build_user_agent_hash(extract_user_agent(&req).as_deref());
            let ip_subnet_hash = build_ip_subnet_hash(client_ip.as_deref());
            let surface = parse_client_surface(&req);
            let token = svc
                .occupy_seat(
                    &position_id,
                    &body.personal_username,
                    password,
                    client_ip.as_deref(),
                    Some(user_agent_hash.as_str()),
                    Some(ip_subnet_hash.as_str()),
                )
                .await
                .map_err(ApiError::from)?;
            attach_token_response_cookies(&token, surface)
        }
        "face" => Err(ApiError::NotImplemented("人脸占席未实现".into())),
        "ext" => Err(ApiError::NotImplemented("扩展占席证明未实现".into())),
        other => Err(ApiError::BadRequest(format!("未知占席证明 kind: {other}"))),
    }
}

pub(crate) async fn list_seats(svc: web::Data<Arc<AuthService>>, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    let _person = claims
        .0
        .sub
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ApiError::Unauthorized("missing authenticated user".into()))?;
    let seats = svc.list_seats().await.map_err(ApiError::from)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "items": seats, "total": seats.len() })))
}

/// 配置占席路由
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("/api/v2/seats").route(web::get().to(list_seats)));
    cfg.service(web::scope("/api/v2/seats").route("/{position_id}/occupy", web::post().to(occupy)));
}
