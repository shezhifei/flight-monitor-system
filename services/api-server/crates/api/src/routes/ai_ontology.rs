use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use actix_web::{web, HttpResponse};
use chrono::Utc;
use fms_application::services::ontology_actions::{
    advisory_action_permission, read_action_permission, OntologyActionError, OntologyActionServices,
};
use fms_domain::ontology::governed::{load_governed_schema, load_governed_schema_with_fields};
use fms_application::types::ConcreteFieldOverlayService;
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

async fn load_action_overlays(
    repo: Option<web::Data<Arc<dyn AiOntologyRepository + Send + Sync>>>,
) -> Vec<fms_domain::ontology::governed::ActionOverlay> {
    match repo {
        Some(repo) => repo.load_action_overlays().await.unwrap_or_default(),
        None => Vec::new(),
    }
}

/// 返回稳定 schema export 结构（ontology_version / exported_at / objects / actions /
/// risk_policies / constraints）。
async fn get_schema(
    repo: Option<web::Data<Arc<dyn AiOntologyRepository + Send + Sync>>>,
    field_repo: Option<web::Data<Arc<ConcreteFieldOverlayService>>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    let mut schema = load_schema(repo.clone()).await;
    if let Some(field_repo) = field_repo {
        if let Ok(fields) = field_repo.list(None, false).await {
            let actions = load_action_overlays(repo.clone()).await;
            schema = load_governed_schema_with_fields(&actions, &fields);
        }
    }
    let export: OntologySchemaExport = build_schema_export(&schema, Utc::now());
    Ok(HttpResponse::Ok().json(export))
}

async fn get_objects(
    repo: Option<web::Data<Arc<dyn AiOntologyRepository + Send + Sync>>>,
    field_repo: Option<web::Data<Arc<ConcreteFieldOverlayService>>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    let mut schema = load_schema(repo.clone()).await;
    if let Some(field_repo) = field_repo {
        if let Ok(fields) = field_repo.list(None, false).await {
            let actions = load_action_overlays(repo.clone()).await;
            schema = load_governed_schema_with_fields(&actions, &fields);
        }
    }
    let objects: Vec<_> = schema.objects.values().cloned().collect();
    Ok(HttpResponse::Ok().json(objects))
}

async fn get_actions(
    repo: Option<web::Data<Arc<dyn AiOntologyRepository + Send + Sync>>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;
    // 定义页要能看见并重新启用被 overlay 停掉的动作，所以这里从代码底 schema 列全量，
    // 再叠 overlay 的启用/风险/审批；运行时信封/导出仍走 load_governed_schema（停用即消失）。
    let mut schema = fms_domain::ontology::flight_ops_v1::build_flight_ops_v1_schema();
    let overlays = if let Some(repo) = repo {
        match repo.load_action_overlays().await {
            Ok(overlays) => overlays,
            Err(error) => {
                tracing::warn!("failed to load AI ontology overlays for action catalog: {}", error);
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    let overlay_map: std::collections::HashMap<(String, String), fms_domain::ontology::governed::ActionOverlay> =
        overlays
            .into_iter()
            .map(|overlay| ((overlay.object.clone(), overlay.action.clone()), overlay))
            .collect();
    let mut actions = Vec::new();
    for (obj_name, obj_def) in schema.objects.iter_mut() {
        for (action_name, action_def) in obj_def.actions.iter_mut() {
            let mut is_active = true;
            if let Some(overlay) = overlay_map.get(&(obj_name.clone(), action_name.clone())) {
                if overlay.is_active == Some(false) {
                    is_active = false;
                }
                if let Some(risk) = overlay.risk {
                    action_def.risk_level = risk.label().to_string();
                }
                if let Some(requires_approval) = overlay.requires_approval {
                    let policy = if requires_approval {
                        "require_approval"
                    } else {
                        "auto_execute"
                    };
                    action_def.approval_strategy = policy.to_string();
                    action_def.approval_policy = policy.to_string();
                }
            }
            actions.push(serde_json::json!({
                "object": obj_name,
                "action": action_name,
                "is_active": is_active,
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

// ---------------------------------------------------------------------------
// Overlay 写（治理 G4 / PR6：定义页配置中心改启用/风险/审批）
// ---------------------------------------------------------------------------

/// 覆盖请求：只能针对代码 schema 已知的 `(object, action)` 键设置治理字段。
/// 一个覆盖会整体改写该键的启用/风险/审批（upsert）；删除覆盖恢复代码默认。
#[derive(Debug, Deserialize)]
struct ActionOverlayRequest {
    object: String,
    action: String,
    #[serde(default = "default_true")]
    is_active: bool,
    risk_level: Option<String>,
    #[serde(default = "default_true")]
    requires_approval: bool,
}

fn default_true() -> bool {
    true
}

fn resolve_overlay_repo(
    repo: Option<web::Data<Arc<dyn AiOntologyRepository + Send + Sync>>>,
    action: &str,
) -> Result<Arc<dyn AiOntologyRepository + Send + Sync>, ApiError> {
    repo.map(|data| data.get_ref().clone())
        .ok_or_else(|| ApiError::Internal(format!("ontology overlay write ({action}) requires a repository"))
    )
}

async fn put_action_overlay(
    claims: JwtAuth,
    repo: Option<web::Data<Arc<dyn AiOntologyRepository + Send + Sync>>>,
    body: web::Json<ActionOverlayRequest>,
) -> Result<HttpResponse, ApiError> {
    // 配置中心写操作：管理员或有 `ai:manage` 授予的用户。
    claims.ensure_permission("ai:manage")?;

    let object = body.object.trim();
    let action = body.action.trim();
    if object.is_empty() || action.is_empty() {
        return Err(ApiError::BadRequest("object 与 action 必填".into()));
    }

    // fail-closed：只允许覆盖代码 schema 已知键，不能凭空新增对象/动作。
    let schema = load_governed_schema(&[]);
    let obj_def = schema.objects.get(object).ok_or_else(|| {
        ApiError::NotFound(format!("object type '{object}' not in flight-ops.v1 base schema"))
    })?;
    if !obj_def.actions.contains_key(action) {
        return Err(ApiError::NotFound(format!(
            "action '{object}.{action}' not in flight-ops.v1 base schema"
        )));
    }

    let risk = match &body.risk_level {
        Some(raw) => fms_domain::models::ai_proposal::RiskLevel::from_str_loose(raw).ok_or_else(|| {
            ApiError::BadRequest(format!("unknown risk_level: {raw}"))
        })?,
        None => fms_domain::models::ai_proposal::RiskLevel::from_str_loose(
            &obj_def.actions[action].risk_level,
        )
        .unwrap_or_default(),
    };

    let overlay = fms_domain::ontology::governed::ActionOverlay {
        object: object.to_string(),
        action: action.to_string(),
        is_active: Some(body.is_active),
        risk: Some(risk),
        requires_approval: Some(body.requires_approval),
    };

    let repo = resolve_overlay_repo(repo, "save")?;
    repo.save_action_overlay(&overlay)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "object": overlay.object,
        "action": overlay.action,
        "is_active": overlay.is_active,
        "risk_level": overlay.risk.map(fms_domain::models::ai_proposal::RiskLevel::label),
        "requires_approval": overlay.requires_approval,
    })))
}

async fn delete_action_overlay(
    claims: JwtAuth,
    repo: Option<web::Data<Arc<dyn AiOntologyRepository + Send + Sync>>>,
    body: web::Json<ActionOverlayRequest>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:manage")?;

    let object = body.object.trim();
    let action = body.action.trim();
    if object.is_empty() || action.is_empty() {
        return Err(ApiError::BadRequest("object 与 action 必填".into()));
    }

    let repo = resolve_overlay_repo(repo, "delete")?;
    repo.delete_action_overlay(object, action)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "object": object,
        "action": action,
        "deleted": true,
    })))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/ai/ontology")
            .route("/schema", web::get().to(get_schema))
            .route("/objects", web::get().to(get_objects))
            .route("/actions", web::get().to(get_actions))
            .route("/actions/read", web::post().to(execute_read_action))
            .route("/actions/advisory", web::post().to(execute_advisory_action))
            .route("/actions/overlay", web::put().to(put_action_overlay))
            .route("/actions/overlay", web::delete().to(delete_action_overlay)),
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

    /// Overlay 写（治理 G4 / PR6）：未认证 401；缺 `ai:manage` 403；无仓储 fail-closed 500。
    #[actix_web::test]
    async fn test_overlay_write_fallback_without_repo() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
                .configure(configure),
        )
        .await;
        let body = serde_json::json!({"object": "Flight", "action": "add_note"});

        let unauth = test::TestRequest::put()
            .uri("/api/v2/ai/ontology/actions/overlay")
            .set_json(&body)
            .to_request();
        let resp = test::call_service(&app, unauth).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);

        // 只有读权限没有 `ai:manage`：写 overlay 必须被拒。
        let viewer = make_jwt(&["ai:view"]);
        let req = test::TestRequest::put()
            .uri("/api/v2/ai/ontology/actions/overlay")
            .insert_header(("Authorization", format!("Bearer {viewer}")))
            .set_json(&body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);

        // 有 `ai:manage` 但没有仓储：既不能凭空新增对象/动作，也不能假装写成功 → 500。
        let manager = make_jwt(&["ai:manage"]);
        let req = test::TestRequest::put()
            .uri("/api/v2/ai/ontology/actions/overlay")
            .insert_header(("Authorization", format!("Bearer {manager}")))
            .set_json(&body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::INTERNAL_SERVER_ERROR);
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
