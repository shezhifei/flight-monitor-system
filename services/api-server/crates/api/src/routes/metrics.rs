//! Prometheus `/metrics` 端点
//!
//! 由 server 组合根安装 recorder 后，将 `PrometheusHandle` 通过 `web::Data` 注入，
//! 此路由渲染当前全部已注册指标。

use actix_web::{web, HttpResponse};
use metrics_exporter_prometheus::PrometheusHandle;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/api/v2/metrics", web::get().to(render_metrics))
        .route("/metrics", web::get().to(render_metrics));
}

async fn render_metrics(handle: web::Data<PrometheusHandle>) -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body(handle.render())
}
