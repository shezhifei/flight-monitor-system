use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use actix_web::{web, HttpResponse};
use chrono::Utc;
use fms_application::services::ontology_actions::{
    advisory_action_permission, read_action_permission, OntologyActionError, OntologyActionServices,
};
use fms_domain::ontology::governed::load_governed_schema;
use fms_domain::ontology::schema_export::{build_schema_export, OntologySchemaExport};
use fms_domain::ports::ai_ontology_repository::AiOntologyRepository;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

async fn load_schema(
    repo: Option<web::Data<Arc<dyn AiOntologyRepository + Send + Sync>>>,
) -> fms_domain::models::ai_ontology::OntologySchema {
    if let Some(repo) = repo {
        match repo.load_action_overlays().await {
            Ok(overlays) => return load_governed_schema(&overlays),
            Err(error) => {
                tracing::warn!("failed to load AI ontology overlays from DB: {}", error);
            }
        }
    }
    load_governed_schema(&[])
}

/// 返回稳定 schema export 结构（ontology_version / exported_at / objects / actions /
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

/// 只读动作直接执行，不创建 pending action；权限按动作声明校验。
#[derive(Debug, Deserialize)]
struct ReadActionRequest {
    action_name: String,
    #[serde(default = "default_arguments")]
    arguments: Value,
}

fn default_arguments() -> Value {
    serde_json::json!({})
}

fn map_action_error(error: OntologyActionError) -> ApiError {
    match error {
        OntologyActionError::InvalidArguments(msg) => ApiError::BadRequest(msg),
        OntologyActionError::NotFound(msg) => ApiError::NotFound(msg),
        OntologyActionError::Repository(msg) | OntologyActionError::Internal(msg) => ApiError::Internal(msg),
    }
}

/// Shared dispatcher for read actions. Both the public user-facing route
/// (`/api/v2/ai/ontology/actions/read`) and the internal agent route
/// (`/internal/ai/v1/ontology/actions/read`) call through here so there is a
/// single source of truth for action-name → service mapping. Callers MUST
/// validate `action_name` against [`read_action_permission`] first.
pub(crate) async fn dispatch_read_action(
    actions: &OntologyActionServices,
    action_name: &str,
    arguments: &Value,
) -> Result<Value, OntologyActionError> {
    match action_name {
        "flight.get_context" => actions.flight_context.get(arguments).await,
        "flight.search" => actions.flight_search.search(arguments).await,
        "dispatch.get_status" => actions.dispatch_status.get(arguments).await,
        "anomaly.list_open" => actions.anomaly_open_list.list(arguments).await,
        "stand.check_availability" => actions.stand_availability.check(arguments).await,
        "report.generate_briefing" => actions.briefing.generate(arguments).await,
        "personnel.get_context" => actions.personnel_context.get(arguments).await,
        "team.get_context" => actions.team_context.get(arguments).await,
        "equipment.get_context" => actions.equipment_context.get(arguments).await,
        other => Err(OntologyActionError::InvalidArguments(format!(
            "unknown read action: {other}"
        ))),
    }
}

/// Shared dispatcher for advisory actions. See [`dispatch_read_action`] for the
/// same contract applied to the advisory surface.
pub(crate) async fn dispatch_advisory_action(
    actions: &OntologyActionServices,
    action_name: &str,
    arguments: &Value,
) -> Result<Value, OntologyActionError> {
    match action_name {
        "flight.suggest_stand_adjustment" => actions.stand_recommendation.suggest(arguments).await,
        "dispatch.suggest_replan" => actions.dispatch_replan.suggest(arguments).await,
        "anomaly.suggest_escalation" => actions.anomaly_escalation.suggest(arguments).await,
        "flight.suggest_delay_action" => actions.delay.suggest(arguments).await,
        "notification.suggest_broadcast" => actions.notification_broadcast.suggest(arguments).await,
        other => Err(OntologyActionError::InvalidArguments(format!(
            "unknown advisory action: {other}"
        ))),
    }
}

async fn execute_read_action(
    claims: JwtAuth,
    actions: web::Data<Arc<OntologyActionServices>>,
    body: web::Json<ReadActionRequest>,
) -> Result<HttpResponse, ApiError> {
    let permission = read_action_permission(&body.action_name)
        .ok_or_else(|| ApiError::BadRequest(format!("unknown read action: {}", body.action_name)))?;
    claims.ensure_permission(permission)?;

    dispatch_read_action(&actions, &body.action_name, &body.arguments)
        .await
        .map(|value| HttpResponse::Ok().json(value))
        .map_err(map_action_error)
}

async fn execute_advisory_action(
    claims: JwtAuth,
    actions: web::Data<Arc<OntologyActionServices>>,
    body: web::Json<ReadActionRequest>,
) -> Result<HttpResponse, ApiError> {
    let permission = advisory_action_permission(&body.action_name)
        .ok_or_else(|| ApiError::BadRequest(format!("unknown advisory action: {}", body.action_name)))?;
    claims.ensure_permission(permission)?;

    dispatch_advisory_action(&actions, &body.action_name, &body.arguments)
        .await
        .map(|value| HttpResponse::Ok().json(value))
        .map_err(map_action_error)
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/ai/ontology")
            .route("/schema", web::get().to(get_schema))
            .route("/objects", web::get().to(get_objects))
            .route("/actions", web::get().to(get_actions))
            .route("/actions/read", web::post().to(execute_read_action))
            .route("/actions/advisory", web::post().to(execute_advisory_action)),
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

        // 只读动作执行入口：未认证必须 401（权限按动作声明校验）。
        let unauth_read = test::TestRequest::post()
            .uri("/api/v2/ai/ontology/actions/read")
            .set_json(serde_json::json!({"action_name": "flight.search", "arguments": {}}))
            .to_request();
        let resp = test::call_service(&app, unauth_read).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);

        // 建议动作执行入口：未认证必须 401。
        let unauth_advisory = test::TestRequest::post()
            .uri("/api/v2/ai/ontology/actions/advisory")
            .set_json(serde_json::json!({"action_name": "flight.suggest_stand_adjustment", "arguments": {}}))
            .to_request();
        let resp = test::call_service(&app, unauth_advisory).await;
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
        assert!(
            export.get("exported_at").is_some(),
            "exported_at is required by the schema export"
        );
        assert!(export["objects"].get("Flight").is_some());
        // Flight.change_stand 已废止（PR #本体两层改造）——合同不得再含该动作
        assert!(export["actions"].get("Flight.change_stand").is_none());
        assert!(export["actions"].get("Flight.update_status").is_some());
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
                && item.get("action").and_then(|v| v.as_str()) == Some("update_status")
        }));
        // Flight.change_stand 已废止 -> 不得出现在动作列表
        assert!(!actions_arr.iter().any(|item| {
            item.get("object").and_then(|v| v.as_str()) == Some("Flight")
                && item.get("action").and_then(|v| v.as_str()) == Some("change_stand")
        }));
    }
}
