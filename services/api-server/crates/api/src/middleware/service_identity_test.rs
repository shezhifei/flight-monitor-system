#[cfg(test)]
mod tests {
    use actix_web::{test, web, App};
    use jsonwebtoken::{encode, EncodingKey, Header};
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::middleware::jwt::JwtSecret;
    use crate::middleware::service_identity::{ServiceIdentity, ServiceIdentityClaims};

    const TEST_SECRET: &str = "test-secret-key-for-testing";
    const SERVICE_IDENTITY_HEADER: &str = "X-Service-Identity";
    const TEST_ISSUER: &str = "fms-rust-api";
    const TEST_SUBJECT: &str = "rust-api-gateway";
    const TEST_AUDIENCE: &str = "python-ai-runtime";

    fn create_service_token(path: &str, expired: bool) -> String {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as usize;

        let claims = ServiceIdentityClaims {
            iss: TEST_ISSUER.to_string(),
            sub: TEST_SUBJECT.to_string(),
            aud: TEST_AUDIENCE.to_string(),
            iat: now,
            exp: if expired { now - 100 } else { now + 60 },
            path: path.to_string(),
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(TEST_SECRET.as_bytes()),
        )
        .unwrap()
    }

    #[actix_web::test]
    async fn test_complete_run_requires_service_identity() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(JwtSecret(TEST_SECRET.to_string())))
                .route(
                    "/internal/ai/v1/runs/{run_id}/complete",
                    web::post().to(|_: ServiceIdentity| async {
                        actix_web::HttpResponse::Ok().json(json!({"success": true}))
                    }),
                ),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/internal/ai/v1/runs/test-run-123/complete")
            .insert_header((
                SERVICE_IDENTITY_HEADER,
                create_service_token("/internal/ai/v1/runs/test-run-123/complete", false),
            ))
            .set_json(json!({"output_raw": {"answer": "test"}}))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_complete_run_rejects_missing_token() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(JwtSecret(TEST_SECRET.to_string())))
                .route(
                    "/internal/ai/v1/runs/{run_id}/complete",
                    web::post().to(|_: ServiceIdentity| async {
                        actix_web::HttpResponse::Ok().json(json!({"success": true}))
                    }),
                ),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/internal/ai/v1/runs/test-run-123/complete")
            .set_json(json!({"output_raw": {"answer": "test"}}))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 401);
    }

    #[actix_web::test]
    async fn test_complete_run_rejects_expired_token() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(JwtSecret(TEST_SECRET.to_string())))
                .route(
                    "/internal/ai/v1/runs/{run_id}/complete",
                    web::post().to(|_: ServiceIdentity| async {
                        actix_web::HttpResponse::Ok().json(json!({"success": true}))
                    }),
                ),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/internal/ai/v1/runs/test-run-123/complete")
            .insert_header((
                SERVICE_IDENTITY_HEADER,
                create_service_token("/internal/ai/v1/runs/test-run-123/complete", true),
            ))
            .set_json(json!({"output_raw": {"answer": "test"}}))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 401);
    }

    #[actix_web::test]
    async fn test_complete_run_rejects_wrong_path() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(JwtSecret(TEST_SECRET.to_string())))
                .route(
                    "/internal/ai/v1/runs/{run_id}/complete",
                    web::post().to(|_: ServiceIdentity| async {
                        actix_web::HttpResponse::Ok().json(json!({"success": true}))
                    }),
                ),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/internal/ai/v1/runs/test-run-123/complete")
            .insert_header((SERVICE_IDENTITY_HEADER, create_service_token("/different/path", false)))
            .set_json(json!({"output_raw": {"answer": "test"}}))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 403);
    }

    #[actix_web::test]
    async fn test_health_endpoint_allows_no_token() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(JwtSecret(TEST_SECRET.to_string())))
                .route(
                    "/internal/ai/v1/health",
                    web::get().to(|| async { actix_web::HttpResponse::Ok().json(json!({"status": "healthy"})) }),
                ),
        )
        .await;

        let req = test::TestRequest::get().uri("/internal/ai/v1/health").to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }
}
