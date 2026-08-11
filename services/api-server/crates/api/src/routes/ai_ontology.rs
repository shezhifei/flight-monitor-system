use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use actix_web::{web, HttpResponse};
use chrono::Utc;
use fms_application::services::ontology_read_action_service::{
    read_action_permission, OntologyReadActionService, ReadActionError,
};
use fms_domain::ontology::flight_ops_v1::build_flight_ops_v1_schema;
use fms_domain::ontology::schema_export::{build_schema_export, OntologySchemaExport};
use fms_domain::ports::ai_ontology_repository::AiOntologyRepository;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

async fn load_schema(
    repo: Option<web::Data<Arc<dyn AiOntologyRepository + Send + Sync>>>,
) -> fms_domain::models::ai_ontology::OntologySchema {
    if let Some(repo) = repo {
        match repo.load_active_schema().await {
            Ok(Some(schema)) => return schema,
            Ok(None) => {}
            Err(error) => {
                tracing::warn!("failed to load active AI ontology schema from DB: {}", error);
            }
        }
    }
    build_flight_ops_v1_schema()
}

/// 契约 §7：返回稳定导出结构（ontology_version / exported_at / objects / actions /
/// risk_policies / constraints）。
async fn get_schema(
    repo: Option<web::Data<Arc<dyn AiOntologyRepository + Send + Sync>>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    let schema = load_schema(repo).await;
    let export: OntologySchemaExport = build_schema_export(&schema, Utc::now());
    Ok(HttpResponse::Ok().json(export))
}

async fn get_objects(
    repo: Option<web::Data<Arc<dyn AiOntologyRepository + Send + Sync>>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    let schema = load_schema(repo).await;
    let objects: Vec<_> = schema.objects.values().cloned().collect();
    Ok(HttpResponse::Ok().json(objects))
}

async fn get_actions(
    repo: Option<web::Data<Arc<dyn AiOntologyRepository + Send + Sync>>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    let schema = load_schema(repo).await;
    let mut actions = Vec::new();
    for (obj_name, obj_def) in schema.objects {
        for (action_name, action_def) in obj_def.actions {
            actions.push(serde_json::json!({
                "object": obj_name,
                "action": action_name,
                "definition": action_def
            }));
        }
    }
    Ok(HttpResponse::Ok().json(actions))
}

/// 契约 §4.2：只读动作直接执行，不创建 pending action；权限按动作声明校验。
#[derive(Debug, Deserialize)]
struct ReadActionRequest {
    action_name: String,
    #[serde(default = "default_arguments")]
    arguments: Value,
}

fn default_arguments() -> Value {
    serde_json::json!({})
}

async fn execute_read_action(
    claims: JwtAuth,
    service: web::Data<Arc<OntologyReadActionService>>,
    body: web::Json<ReadActionRequest>,
) -> Result<HttpResponse, ApiError> {
    let permission = read_action_permission(&body.action_name)
        .ok_or_else(|| ApiError::BadRequest(format!("unknown read action: {}", body.action_name)))?;
    claims.ensure_permission(permission)?;

    match service.execute(&body.action_name, &body.arguments).await {
        Ok(result) => Ok(HttpResponse::Ok().json(result)),
        Err(ReadActionError::UnknownAction(name)) => Err(ApiError::BadRequest(format!("unknown read action: {name}"))),
        Err(ReadActionError::InvalidArguments(msg)) => Err(ApiError::BadRequest(msg)),
        Err(ReadActionError::NotFound(msg)) => Err(ApiError::NotFound(msg)),
        Err(ReadActionError::Repository(msg)) => Err(ApiError::Internal(msg)),
        Err(ReadActionError::Internal(msg)) => Err(ApiError::Internal(msg)),
    }
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/ai/ontology")
            .route("/schema", web::get().to(get_schema))
            .route("/objects", web::get().to(get_objects))
            .route("/actions", web::get().to(get_actions))
            .route("/actions/read", web::post().to(execute_read_action)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::middleware::jwt::JwtSecret;
    use actix_web::{test, App};
    use fms_infrastructure::repositories::pg_ai_ontology_repository::PgAiOntologyRepository;
    use sqlx::PgPool;

    fn make_jwt(permissions: &[&str]) -> String {
        use chrono::Utc;
        use jsonwebtoken::{encode, EncodingKey, Header};
        let now = Utc::now().timestamp();
        let claims = serde_json::json!({
            "sub": "test_user",
            "username": "tester",
            "permissions": permissions,
            "is_admin": false,
            "iat": now,
            "exp": now + 3600,
            "type": "access",
        });
        encode(&Header::default(), &claims, &EncodingKey::from_secret(b"test-secret")).expect("jwt encoding")
    }

    fn has_pool() -> bool {
        std::env::var("TEST_DATABASE_URL").is_ok()
    }

    async fn create_pool() -> PgPool {
        let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
        PgPool::connect(&url).await.expect("test db")
    }

    #[actix_web::test]
    async fn test_get_ontology_routes_fallback_without_repo() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
                .configure(configure),
        )
        .await;
        let token = make_jwt(&["ai:view"]);

        let unauth_req = test::TestRequest::get().uri("/api/v2/ai/ontology/schema").to_request();
        let unauth_resp = test::call_service(&app, unauth_req).await;
        assert_eq!(unauth_resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);

        let req = test::TestRequest::get()
            .uri("/api/v2/ai/ontology/schema")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

        let req = test::TestRequest::get()
            .uri("/api/v2/ai/ontology/objects")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

        let req = test::TestRequest::get()
            .uri("/api/v2/ai/ontology/actions")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

        // 只读动作执行入口：未认证必须 401（契约 §4.2：权限按动作声明校验）。
        let unauth_read = test::TestRequest::post()
            .uri("/api/v2/ai/ontology/actions/read")
            .set_json(serde_json::json!({"action_name": "flight.search", "arguments": {}}))
            .to_request();
        let resp = test::call_service(&app, unauth_read).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    #[ignore = "requires TEST_DATABASE_URL; run with: cargo test -- --ignored"]
    async fn test_get_ontology_routes_success_with_repo() {
        if !has_pool() {
            return;
        }
        let pool = create_pool().await;
        let repo: Arc<dyn AiOntologyRepository + Send + Sync> = Arc::new(PgAiOntologyRepository::new(pool));

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
                .app_data(web::Data::new(repo))
                .configure(configure),
        )
        .await;
        let token = make_jwt(&["ai:view"]);

        // 1. GET /schema
        let req = test::TestRequest::get()
            .uri("/api/v2/ai/ontology/schema?version=active")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
        let export: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(export["ontology_version"], "flight-ops.v1");
        assert!(export.get("exported_at").is_some(), "exported_at required by contract §7");
        assert!(export["objects"].get("Flight").is_some());
        assert!(export["actions"].get("Flight.change_stand").is_some());
        for level in ["low", "medium", "high", "critical"] {
            assert!(export["risk_policies"].get(level).is_some(), "risk policy for {level}");
        }

        // 2. GET /objects
        let req = test::TestRequest::get()
            .uri("/api/v2/ai/ontology/objects?version=active")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
        let objects: Vec<fms_domain::models::ai_ontology::OntologyObjectDef> = test::read_body_json(resp).await;
        assert!(!objects.is_empty());
        assert!(objects.iter().any(|o| o.name == "Flight"));

        // 3. GET /actions
        let req = test::TestRequest::get()
            .uri("/api/v2/ai/ontology/actions?version=active")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
        let actions: serde_json::Value = test::read_body_json(resp).await;
        assert!(actions.is_array());
        let actions_arr = actions.as_array().unwrap();
        assert!(!actions_arr.is_empty());
        assert!(actions_arr.iter().any(|item| {
            item.get("object").and_then(|v| v.as_str()) == Some("Flight")
                && item.get("action").and_then(|v| v.as_str()) == Some("change_stand")
        }));
    }
}
