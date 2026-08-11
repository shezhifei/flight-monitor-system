//! Ping 端点 - 用于健康检查和连通性测试
//!
//! 对应 Python 后端的 `/api/ping` 和 `/api/v2/ping`

use actix_web::{web, HttpResponse};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub struct PingResponse {
    status: &'static str,
    version: &'static str,
}

/// Health check endpoint.
#[utoipa::path(
    get,
    path = "/api/v2/ping",
    tag = "health",
    responses(
        (status = 200, description = "Service is healthy", body = PingResponse)
    )
)]
pub async fn ping() -> HttpResponse {
    HttpResponse::Ok().json(PingResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// 配置 ping 路由
pub fn configure(cfg: &mut web::ServiceConfig) {
    // Python 兼容路径: /api/ping
    cfg.route("/api/ping", web::get().to(ping));
    // Rust 原生路径: /api/v2/ping
    cfg.route("/api/v2/ping", web::get().to(ping));
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};

    #[actix_web::test]
    async fn ping_returns_ok_status() {
        let app = test::init_service(App::new().configure(configure)).await;

        let req = test::TestRequest::get().uri("/api/v2/ping").to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["status"], "ok");
        assert!(body["version"].is_string());
    }
}
