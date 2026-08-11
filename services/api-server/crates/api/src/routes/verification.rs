//! 写路径验证观测路由。
//!
//! 对齐 Python `verification_routes.py` 的只读诊断接口。

use actix_web::{web, HttpRequest, HttpResponse};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::services::python_sidecar_proxy::{forward_json_request, SidecarAuth};
use crate::types::ConcreteRuntimeDiagnosticsService;

fn token_configured() -> bool {
    std::env::var("VERIFICATION_TOKEN")
        .ok()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

fn build_verification_health_payload(redis_connected: bool, token_configured: bool) -> Value {
    json!({
        "success": true,
        "data": {
            "verification_service": "running",
            "redis_connected": redis_connected,
            "token_configured": token_configured,
        },
        "message": "Verification service health check",
    })
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

async fn get_verification_stats(request: HttpRequest, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    ensure_diagnostics_permission(&claims)?;

    let stats = match runtime_diagnostics_service(&request) {
        Some(service) => match service.verification_stats().await {
            Ok(stats) => stats,
            Err(error) => {
                tracing::warn!(error = %error, "failed to read write verification stats from diagnostics table");
                return Ok(HttpResponse::Ok().json(json!({
                    "success": true,
                    "data": {
                        "error": "diagnostic query failed",
                    },
                    "message": "Stats fetch failed",
                })));
            }
        },
        None => {
            return Ok(HttpResponse::Ok().json(json!({
                "success": true,
                "data": {
                    "error": "diagnostic store unavailable",
                },
                "message": "Stats unavailable",
            })));
        }
    };

    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "data": stats,
        "message": "Write verification stats fetched",
    })))
}

async fn get_verification_health(request: HttpRequest, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    ensure_diagnostics_permission(&claims)?;

    let diagnostics_connected = match runtime_diagnostics_service(&request) {
        Some(service) => service.diagnostics_connected().await,
        None => false,
    };

    Ok(HttpResponse::Ok().json(build_verification_health_payload(
        diagnostics_connected,
        token_configured(),
    )))
}

fn verification_compare_url() -> String {
    std::env::var("WRITE_VERIFICATION_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "http://localhost:8088/api/v2/verification/compare".to_string())
}

fn verification_token() -> Option<String> {
    std::env::var("VERIFICATION_TOKEN")
        .ok()
        .filter(|value| !value.trim().is_empty())
}

async fn compare_write_result(
    req: HttpRequest,
    claims: JwtAuth,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    ensure_diagnostics_permission(&claims)?;

    let url = verification_compare_url();
    let auth = verification_token()
        .map(SidecarAuth::VerificationToken)
        .unwrap_or(SidecarAuth::None);

    Ok(forward_json_request(
        &req,
        reqwest::Method::POST,
        &url,
        &body.into_inner(),
        auth,
        Duration::from_secs(10),
    )
    .await)
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/verification")
            .route("/compare", web::post().to(compare_write_result))
            .route("/stats", web::get().to(get_verification_stats))
            .route("/health", web::get().to(get_verification_health)),
    );
}

#[cfg(test)]
mod tests {
    use super::build_verification_health_payload;
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

    #[test]
    fn verification_health_payload_matches_python_when_token_is_unset() {
        let expected = json!({
            "success": true,
            "data": {
                "verification_service": "running",
                "redis_connected": false,
                "token_configured": false,
            },
            "message": "Verification service health check",
        });
        let actual = build_verification_health_payload(false, false);

        assert_eq!(actual, expected);
    }

    #[test]
    fn verification_health_payload_matches_python_when_token_is_set() {
        let expected = json!({
            "success": true,
            "data": {
                "verification_service": "running",
                "redis_connected": false,
                "token_configured": true,
            },
            "message": "Verification service health check",
        });
        let actual = build_verification_health_payload(false, true);

        assert_eq!(actual, expected);
    }

    #[actix_web::test]
    async fn verification_health_requires_authentication() {
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
                .configure(super::configure),
        )
        .await;

        let resp = actix_test::call_service(
            &app,
            actix_test::TestRequest::get()
                .uri("/api/v2/verification/health")
                .to_request(),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn verification_compare_rejects_token_without_diagnostics_permission() {
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
                .configure(super::configure),
        )
        .await;
        let token = make_jwt(&["ai:view"]);

        let resp = actix_test::call_service(
            &app,
            actix_test::TestRequest::post()
                .uri("/api/v2/verification/compare")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .set_json(json!({"probe": true}))
                .to_request(),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }
}
