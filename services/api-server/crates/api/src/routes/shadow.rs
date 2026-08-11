//! Shadow 对比观测路由。
//!
//! 对齐 Python `shadow_routes.py` 的查询接口。

use actix_web::{web, HttpRequest, HttpResponse};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::types::ConcreteRuntimeDiagnosticsService;

#[derive(Debug, Deserialize)]
struct ShadowDiffQuery {
    count: Option<usize>,
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ShadowStatsQuery {
    minutes: Option<i64>,
}

fn ensure_diagnostics_permission(claims: &JwtAuth) -> Result<(), ApiError> {
    if claims.has_permission("system:diagnostics") || claims.has_permission("system:admin") {
        Ok(())
    } else {
        Err(ApiError::Forbidden(
            "缺少权限: system:diagnostics 或 system:admin".into(),
        ))
    }
}

fn runtime_diagnostics_service(request: &HttpRequest) -> Option<Arc<ConcreteRuntimeDiagnosticsService>> {
    request
        .app_data::<web::Data<Arc<ConcreteRuntimeDiagnosticsService>>>()
        .map(|service| service.get_ref().clone())
}

async fn get_shadow_diffs(
    request: HttpRequest,
    claims: JwtAuth,
    query: web::Query<ShadowDiffQuery>,
) -> Result<HttpResponse, ApiError> {
    ensure_diagnostics_permission(&claims)?;

    let count = query.count.unwrap_or(100).clamp(1, 1000);
    let path_filter = query.path.as_deref().map(str::trim).filter(|value| !value.is_empty());

    let diffs = match runtime_diagnostics_service(&request) {
        Some(service) => match service.recent_shadow_diffs(count, path_filter).await {
            Ok(entries) => entries,
            Err(error) => {
                tracing::warn!(error = %error, "failed to read shadow diffs from diagnostics table");
                Vec::new()
            }
        },
        None => Vec::new(),
    };

    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "data": diffs,
        "message": format!("获取到 {} 条差异记录", diffs.len()),
    })))
}

async fn get_shadow_stats(
    request: HttpRequest,
    claims: JwtAuth,
    query: web::Query<ShadowStatsQuery>,
) -> Result<HttpResponse, ApiError> {
    ensure_diagnostics_permission(&claims)?;

    let minutes = query.minutes.unwrap_or(60).clamp(1, 1440);
    let cutoff_ts = (chrono::Utc::now().timestamp() - minutes * 60) as f64;

    let stats = match runtime_diagnostics_service(&request) {
        Some(service) => match service.shadow_stats(minutes).await {
            Ok(stats) => stats,
            Err(error) => {
                tracing::warn!(error = %error, "failed to read shadow stats from diagnostics table");
                ConcreteRuntimeDiagnosticsService::build_shadow_stats(Vec::new(), minutes, cutoff_ts)
            }
        },
        None => ConcreteRuntimeDiagnosticsService::build_shadow_stats(Vec::new(), minutes, cutoff_ts),
    };

    let total = stats["total_comparisons"].as_u64().unwrap_or(0);
    let mismatches = stats["mismatches"].as_u64().unwrap_or(0);
    let match_rate = stats["match_rate_percent"].as_f64().unwrap_or(100.0);

    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "data": stats,
        "message": format!(
            "Shadow 对比统计: {} 次对比, {} 次差异, 匹配率 {:.1}%",
            total,
            mismatches,
            match_rate
        ),
    })))
}

async fn get_shadow_health(request: HttpRequest, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    ensure_diagnostics_permission(&claims)?;

    let Some(service) = runtime_diagnostics_service(&request) else {
        return Ok(HttpResponse::Ok().json(json!({
            "success": true,
            "data": {
                "status": "degraded",
                "reason": "diagnostic store not available",
            },
            "message": "Shadow 比较服务降级",
        })));
    };

    match service.shadow_diff_event_count().await {
        Ok(event_count) => Ok(HttpResponse::Ok().json(json!({
            "success": true,
            "data": {
                "status": "ok",
                "diff_event_count": event_count,
            },
            "message": "Shadow 比较服务正常",
        }))),
        Err(error) => {
            tracing::warn!(error = %error, "failed to query shadow diagnostic health");
            Ok(HttpResponse::Ok().json(json!({
                "success": true,
                "data": {
                    "status": "error",
                    "reason": "diagnostic query failed",
                },
                "message": "Shadow 比较服务异常",
            })))
        }
    }
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/shadow")
            .route("/diffs", web::get().to(get_shadow_diffs))
            .route("/stats", web::get().to(get_shadow_stats))
            .route("/health", web::get().to(get_shadow_health)),
    );
}

#[cfg(test)]
mod tests {
    use actix_web::{http::StatusCode, test as actix_test, web, App};
    use chrono::Utc;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use serde_json::json;

    use crate::middleware::jwt::JwtSecret;

    fn make_jwt(permissions: &[&str]) -> String {
        let now = Utc::now().timestamp();
        let claims = json!({
            "sub": "test_user",
            "username": "tester",
            "permissions": permissions,
            "department_id": null,
            "is_admin": false,
            "iat": now,
            "exp": now + 3600,
            "type": "access",
        });
        encode(&Header::default(), &claims, &EncodingKey::from_secret(b"test-secret")).expect("jwt encoding")
    }

    #[actix_web::test]
    async fn shadow_diffs_requires_authentication() {
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
                .configure(super::configure),
        )
        .await;

        let resp = actix_test::call_service(
            &app,
            actix_test::TestRequest::get().uri("/api/v2/shadow/diffs").to_request(),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn shadow_stats_rejects_token_without_diagnostics_permission() {
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
                .configure(super::configure),
        )
        .await;
        let token = make_jwt(&["ai:view"]);

        let resp = actix_test::call_service(
            &app,
            actix_test::TestRequest::get()
                .uri("/api/v2/shadow/stats")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request(),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }
}
