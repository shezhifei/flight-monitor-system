//! AI config management routes
//!
//! Proxies /api/v2/ai/entities/{id}/capabilities, /api/v2/ai/entities/{id}/mcp/*,
//! /api/v2/ai/skills, /api/v2/ai/cache/* to Python sidecar internal endpoints.

use actix_web::{web, HttpRequest, HttpResponse};

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::services::python_sidecar_proxy::{
    ai_sidecar_auth_for_path, ai_sidecar_timeout, ai_sidecar_url, forward_json_request, forward_request,
};

const AI_CONFIG_PREFIX: &str = "/api/v2/ai";
const AI_CONFIG_INTERNAL_PREFIX: &str = "/internal/ai/v1";

fn config_target_url(req: &HttpRequest) -> String {
    let suffix = req.path().strip_prefix(AI_CONFIG_PREFIX).unwrap_or_default();
    let base = ai_sidecar_url();
    let path = format!("{AI_CONFIG_INTERNAL_PREFIX}{suffix}");
    if req.query_string().trim().is_empty() {
        format!("{base}{path}")
    } else {
        format!("{base}{path}?{}", req.query_string())
    }
}

fn extract_internal_path(target: &str) -> &str {
    target.strip_prefix(&ai_sidecar_url()).unwrap_or(target)
}

async fn proxy_get(req: HttpRequest, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    let target = config_target_url(&req);
    let internal_path = extract_internal_path(&target);
    Ok(forward_request(
        &req,
        reqwest::Method::GET,
        &target,
        ai_sidecar_auth_for_path(internal_path),
        ai_sidecar_timeout(),
    )
    .await)
}

async fn proxy_post(
    req: HttpRequest,
    claims: JwtAuth,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:config")?;
    let target = config_target_url(&req);
    let internal_path = extract_internal_path(&target);
    Ok(forward_json_request(
        &req,
        reqwest::Method::POST,
        &target,
        &body.into_inner(),
        ai_sidecar_auth_for_path(internal_path),
        ai_sidecar_timeout(),
    )
    .await)
}

async fn proxy_put(
    req: HttpRequest,
    claims: JwtAuth,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:config")?;
    let target = config_target_url(&req);
    let internal_path = extract_internal_path(&target);
    Ok(forward_json_request(
        &req,
        reqwest::Method::PUT,
        &target,
        &body.into_inner(),
        ai_sidecar_auth_for_path(internal_path),
        ai_sidecar_timeout(),
    )
    .await)
}

async fn proxy_delete(req: HttpRequest, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:config")?;
    let target = config_target_url(&req);
    let internal_path = extract_internal_path(&target);
    Ok(forward_request(
        &req,
        reqwest::Method::DELETE,
        &target,
        ai_sidecar_auth_for_path(internal_path),
        ai_sidecar_timeout(),
    )
    .await)
}

/// Register the config proxy routes RELATIVE to a parent `/api/v2/ai` scope.
///
/// These routes share the `/api/v2/ai` prefix with [`crate::routes::ai`] and
/// [`crate::routes::ai_execution_readiness`]. Actix dispatches a request into the
/// first registered `web::scope` whose prefix matches and does NOT fall through to
/// a later same-prefix scope, so three separate `web::scope("/api/v2/ai")` services
/// would shadow each other. To avoid that, the single `/api/v2/ai` scope is owned by
/// `routes::ai`, which composes this function via `Scope::configure`.
pub fn register_scoped_routes(cfg: &mut web::ServiceConfig) {
    cfg
            // Capabilities
            .route(
                "/entities/{entity_id}/capabilities",
                web::get().to(proxy_get),
            )
            .route(
                "/entities/{entity_id}/capabilities/validate",
                web::post().to(proxy_post),
            )
            // MCP Servers
            .route(
                "/entities/{entity_id}/mcp/servers",
                web::get().to(proxy_get),
            )
            .route(
                "/entities/{entity_id}/mcp/servers",
                web::post().to(proxy_post),
            )
            .route(
                "/entities/{entity_id}/mcp/servers/{server_id}",
                web::put().to(proxy_put),
            )
            .route(
                "/entities/{entity_id}/mcp/servers/{server_id}",
                web::delete().to(proxy_delete),
            )
            .route(
                "/entities/{entity_id}/mcp/servers/{server_id}/probe",
                web::post().to(proxy_post),
            )
            // MCP Resource read (cache-first; enforces enabled binding +
            // allowed_resources + command allowlist on the sidecar side)
            .route(
                "/entities/{entity_id}/mcp/servers/{server_id}/resource",
                web::post().to(proxy_post),
            )
            // MCP Bindings
            .route(
                "/entities/{entity_id}/mcp/bindings",
                web::get().to(proxy_get),
            )
            .route(
                "/entities/{entity_id}/mcp/bindings",
                web::post().to(proxy_post),
            )
            // Skills
            .route("/skills", web::get().to(proxy_get))
            .route(
                "/entities/{entity_id}/skills",
                web::get().to(proxy_get),
            )
            .route(
                "/entities/{entity_id}/skills/{skill_slug}/probe",
                web::post().to(proxy_post),
            )
            .route(
                "/entities/{entity_id}/skills/bindings",
                web::post().to(proxy_post),
            )
            .route(
                "/entities/{entity_id}/skills/bindings/{binding_id}",
                web::delete().to(proxy_delete),
            )
            // Cache
            .route("/cache/metrics", web::get().to(proxy_get))
            .route("/cache/invalidate", web::post().to(proxy_post));
}

/// Standalone mount used by this module's isolation tests. Production composes the
/// routes into the shared `/api/v2/ai` scope via [`register_scoped_routes`]; do not
/// register this alongside `routes::ai` or the two scopes will shadow each other.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(web::scope("/api/v2/ai").configure(register_scoped_routes));
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;

    #[test]
    fn config_target_url_maps_capabilities() {
        let req = TestRequest::with_uri("/api/v2/ai/entities/default/capabilities").to_http_request();
        let url = config_target_url(&req);
        assert_eq!(
            url,
            "http://localhost:9000/internal/ai/v1/entities/default/capabilities"
        );
    }

    #[test]
    fn config_target_url_maps_mcp_servers() {
        let req = TestRequest::with_uri("/api/v2/ai/entities/default/mcp/servers").to_http_request();
        let url = config_target_url(&req);
        assert_eq!(url, "http://localhost:9000/internal/ai/v1/entities/default/mcp/servers");
    }

    #[test]
    fn config_target_url_maps_mcp_server_probe() {
        let req = TestRequest::with_uri("/api/v2/ai/entities/default/mcp/servers/srv-1/probe").to_http_request();
        let url = config_target_url(&req);
        assert_eq!(
            url,
            "http://localhost:9000/internal/ai/v1/entities/default/mcp/servers/srv-1/probe"
        );
    }

    #[test]
    fn config_target_url_maps_mcp_resource_read() {
        let req = TestRequest::with_uri("/api/v2/ai/entities/default/mcp/servers/srv-1/resource").to_http_request();
        let url = config_target_url(&req);
        assert_eq!(
            url,
            "http://localhost:9000/internal/ai/v1/entities/default/mcp/servers/srv-1/resource"
        );
    }

    #[test]
    fn config_target_url_maps_skills() {
        let req = TestRequest::with_uri("/api/v2/ai/skills").to_http_request();
        let url = config_target_url(&req);
        assert_eq!(url, "http://localhost:9000/internal/ai/v1/skills");
    }

    #[test]
    fn config_target_url_maps_cache_metrics() {
        let req = TestRequest::with_uri("/api/v2/ai/cache/metrics?hours=24").to_http_request();
        let url = config_target_url(&req);
        assert_eq!(url, "http://localhost:9000/internal/ai/v1/cache/metrics?hours=24");
    }

    #[test]
    fn config_target_url_maps_skill_bindings_delete() {
        let req = TestRequest::with_uri("/api/v2/ai/entities/ent-1/skills/bindings/bind-123").to_http_request();
        let url = config_target_url(&req);
        assert_eq!(
            url,
            "http://localhost:9000/internal/ai/v1/entities/ent-1/skills/bindings/bind-123"
        );
    }

    #[test]
    fn config_target_url_preserves_query_string() {
        let req = TestRequest::with_uri("/api/v2/ai/cache/metrics?entity_id=default&hours=48").to_http_request();
        let url = config_target_url(&req);
        assert!(url.contains("entity_id=default"));
        assert!(url.contains("hours=48"));
    }
}

#[cfg(test)]
mod route_behavior_tests {
    use actix_web::{test, web, App};
    use serde_json::json;

    use crate::middleware::jwt::JwtSecret;

    fn build_test_app() -> App<
        impl actix_web::dev::ServiceFactory<
            actix_web::dev::ServiceRequest,
            Config = (),
            Response = actix_web::dev::ServiceResponse,
            Error = actix_web::Error,
            InitError = (),
        >,
    > {
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .configure(super::configure)
    }

    #[actix_web::test]
    async fn config_routes_are_mounted_not_found_without_sidecar() {
        let app = test::init_service(build_test_app()).await;

        // GET paths that ai_config_proxy owns exclusively (no conflict with ai.rs)
        let get_paths = [
            "/api/v2/ai/entities/default/capabilities",
            "/api/v2/ai/entities/default/mcp/servers",
            "/api/v2/ai/entities/default/mcp/bindings",
            "/api/v2/ai/skills",
            "/api/v2/ai/entities/default/skills",
            "/api/v2/ai/cache/metrics",
        ];

        for path in get_paths {
            let resp = test::call_service(&app, test::TestRequest::get().uri(path).to_request()).await;
            assert!(
                resp.status() != actix_web::http::StatusCode::NOT_FOUND,
                "route should be mounted: {path}"
            );
        }
    }

    #[actix_web::test]
    async fn capabilities_route_does_not_match_entity_get() {
        let app = test::init_service(build_test_app()).await;

        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v2/ai/entities/default/capabilities")
                .to_request(),
        )
        .await;
        assert_ne!(
            resp.status(),
            actix_web::http::StatusCode::NOT_FOUND,
            "/entities/{{id}}/capabilities must be handled by ai_config_proxy, not ai"
        );
    }

    #[actix_web::test]
    async fn mcp_servers_post_route_mounted() {
        let app = test::init_service(build_test_app()).await;

        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v2/ai/entities/default/mcp/servers")
                .set_json(json!({"id": "test-srv", "display_name": "Test"}))
                .to_request(),
        )
        .await;
        assert_ne!(
            resp.status(),
            actix_web::http::StatusCode::NOT_FOUND,
            "POST /entities/{{id}}/mcp/servers must be mounted"
        );
    }

    #[actix_web::test]
    async fn mcp_resource_read_post_route_mounted() {
        let app = test::init_service(build_test_app()).await;

        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v2/ai/entities/default/mcp/servers/srv-1/resource")
                .set_json(json!({"resource_uri": "file://ok"}))
                .to_request(),
        )
        .await;
        assert_ne!(
            resp.status(),
            actix_web::http::StatusCode::NOT_FOUND,
            "POST /entities/{{id}}/mcp/servers/{{sid}}/resource must be mounted"
        );
    }

    #[actix_web::test]
    async fn skills_bindings_delete_route_mounted() {
        let app = test::init_service(build_test_app()).await;

        let resp = test::call_service(
            &app,
            test::TestRequest::delete()
                .uri("/api/v2/ai/entities/default/skills/bindings/bind-1")
                .to_request(),
        )
        .await;
        assert_ne!(
            resp.status(),
            actix_web::http::StatusCode::NOT_FOUND,
            "DELETE /entities/{{id}}/skills/bindings/{{bid}} must be mounted"
        );
    }

    #[actix_web::test]
    async fn cache_metrics_get_preserves_query() {
        let app = test::init_service(build_test_app()).await;

        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v2/ai/cache/metrics?entity_id=default&hours=24")
                .to_request(),
        )
        .await;
        assert_ne!(
            resp.status(),
            actix_web::http::StatusCode::NOT_FOUND,
            "GET /cache/metrics with query must be mounted"
        );
    }

    #[actix_web::test]
    async fn cache_invalidate_post_route_mounted() {
        let app = test::init_service(build_test_app()).await;

        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v2/ai/cache/invalidate")
                .set_json(json!({"entity_id": "default"}))
                .to_request(),
        )
        .await;
        assert_ne!(
            resp.status(),
            actix_web::http::StatusCode::NOT_FOUND,
            "POST /cache/invalidate must be mounted"
        );
    }
}

#[cfg(test)]
mod authorization_tests {
    use actix_web::{http::StatusCode, test, web, App};
    use serde_json::json;

    use crate::middleware::jwt::JwtSecret;

    fn make_jwt(permissions: &[&str]) -> String {
        use chrono::Utc;
        use jsonwebtoken::{encode, EncodingKey, Header};
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

    fn build_app() -> App<
        impl actix_web::dev::ServiceFactory<
            actix_web::dev::ServiceRequest,
            Config = (),
            Response = actix_web::dev::ServiceResponse,
            Error = actix_web::Error,
            InitError = (),
        >,
    > {
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .configure(super::configure)
    }

    #[actix_web::test]
    async fn get_requires_ai_view_permission() {
        let app = test::init_service(build_app()).await;
        let token = make_jwt(&["other:perm"]);
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v2/ai/entities/default/capabilities")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request(),
        )
        .await;
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "GET must require ai:view permission"
        );
    }

    #[actix_web::test]
    async fn post_mcp_servers_requires_ai_config_permission() {
        let app = test::init_service(build_app()).await;
        let token = make_jwt(&["ai:view"]);
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v2/ai/entities/default/mcp/servers")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .set_json(json!({"id": "test-srv", "display_name": "Test"}))
                .to_request(),
        )
        .await;
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "POST /mcp/servers must require ai:config permission"
        );
    }

    #[actix_web::test]
    async fn delete_mcp_server_requires_ai_config_permission() {
        let app = test::init_service(build_app()).await;
        let token = make_jwt(&["ai:view"]);
        let resp = test::call_service(
            &app,
            test::TestRequest::delete()
                .uri("/api/v2/ai/entities/default/mcp/servers/srv-1")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request(),
        )
        .await;
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "DELETE /mcp/servers/{{id}} must require ai:config permission"
        );
    }

    #[actix_web::test]
    async fn cache_invalidate_requires_ai_config_permission() {
        let app = test::init_service(build_app()).await;
        let token = make_jwt(&["ai:view"]);
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v2/ai/cache/invalidate")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .set_json(json!({"entity_id": "default"}))
                .to_request(),
        )
        .await;
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "POST /cache/invalidate must require ai:config permission"
        );
    }
}
