//! Dashboard workbench routes.

use std::sync::Arc;

use actix_web::{web, HttpResponse};
use serde_json::json;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use fms_application::services::dashboard_workbench_service::DashboardWorkbenchService;

/// GET /api/v2/dashboard/workbench
async fn workbench(svc: web::Data<Arc<DashboardWorkbenchService>>, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    let response = svc.build_workbench(&claims.0).await;
    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "data": response,
        "message": "dashboard workbench loaded",
    })))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(web::scope("/api/v2/dashboard").route("/workbench", web::get().to(workbench)));
}
