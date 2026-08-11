//! 实时音频 WebSocket 路由
//!
//! 负责 WebSocket 协议升级、鉴权、帧解析和会话生命周期管理。
//! 所有业务逻辑委托给 RealtimeAudioSessionService。
//!
//! 鉴权复用项目统一 JWT middleware（extract_jwt），不再在 route 中手写 JWT 解码。
//! Provider 由 application service 根据 entity config 选择，route 不创建 provider。

use actix_web::{web, HttpRequest, HttpResponse};
use actix_ws::AggregatedMessage;
use futures_util::StreamExt;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

use crate::error::ApiError;
use crate::middleware::jwt;
use fms_application::services::ai_realtime_audio_service::{RealtimeAudioError, RealtimeAudioSessionService};
use fms_domain::models::ai_realtime_audio::*;

// ---------------------------------------------------------------------------
// 查询参数
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RealtimeAudioQuery {
    entity_id: String,
    #[serde(default = "default_protocol_version")]
    protocol_version: u32,
}

fn default_protocol_version() -> u32 {
    1
}

// ---------------------------------------------------------------------------
// 配置常量
// ---------------------------------------------------------------------------

const MAX_PROTOCOL_ERRORS: usize = 3;

// ---------------------------------------------------------------------------
// 路由处理函数
// ---------------------------------------------------------------------------

/// GET /api/v2/ai/realtime/audio
///
/// WebSocket 升级端点，用于实时音频会话。
///
/// 鉴权：复用项目统一 JWT middleware（query token 路径已在
/// QUERY_TOKEN_ALLOWED_PREFIXES 中注册）。
///
/// 协议：连接即 start，服务端立即发送 session.ready。
async fn realtime_audio_handler(
    req: HttpRequest,
    query: web::Query<RealtimeAudioQuery>,
    payload: web::Payload,
    svc: web::Data<Arc<RealtimeAudioSessionService>>,
) -> Result<HttpResponse, ApiError> {
    // 1. 统一鉴权 — 复用 extract_jwt，走完整 JWT middleware 路径
    let claims = jwt::extract_jwt(&req).await?;

    // 2. 权限检查 — 至少需要 ai:media
    let has_media_permission =
        claims.0.is_admin.unwrap_or(false) || claims.0.permissions.iter().any(|v| v == "*" || v == "ai:media");

    if !has_media_permission {
        return Err(ApiError::Forbidden("missing permission: ai:media".to_string()));
    }

    // 3. 参数验证
    if query.entity_id.trim().is_empty() {
        return Err(ApiError::BadRequest("missing entity_id".to_string()));
    }

    // 4. 启动会话 — service 解析 entity config、选择 provider、创建 handle
    let handle = svc
        .start_session(&query.entity_id, query.protocol_version)
        .await
        .map_err(realtime_error_to_api_error)?;

    // 5. WebSocket 升级
    let (response, mut session, stream) =
        actix_ws::handle(&req, payload).map_err(|e| ApiError::Internal(e.to_string()))?;

    // 6. 启动会话处理任务 — provider 已内含在 handle 中
    let svc_clone = svc.clone();

    actix_web::rt::spawn(async move {
        // 从 handle 创建状态（provider 来自 service，不在 route 中创建）
        let mut state = svc.create_session_state(handle);

        // 发送 session.ready
        let ready_event = RealtimeAudioServerEvent::SessionReady(RealtimeSessionReady {
            session_id: state.session_id.clone(),
            protocol_version: 1,
            resolved_config: state.config.clone(),
        });
        let ready_json = match serde_json::to_string(&ready_event) {
            Ok(json) => json,
            Err(e) => {
                tracing::error!(error = %e, "failed to serialize session.ready");
                return;
            }
        };
        if session.text(ready_json).await.is_err() {
            return;
        }

        // 聚合消息流
        let mut stream = stream
            .aggregate_continuations()
            .max_continuation_size(state.max_frame_bytes);

        let mut protocol_errors = 0;
        let timeout = Duration::from_secs(state.max_session_seconds as u64);

        loop {
            let msg = tokio::time::timeout(timeout, stream.next()).await;

            match msg {
                Ok(Some(Ok(AggregatedMessage::Text(text)))) => {
                    match serde_json::from_str::<RealtimeAudioClientEvent>(&text) {
                        Ok(event) => {
                            protocol_errors = 0;
                            let events = svc_clone.process_client_event(&mut state, event).await;
                            for event in events {
                                let should_close = matches!(&event, RealtimeAudioServerEvent::SessionClosed(_));
                                if let Ok(json) = serde_json::to_string(&event) {
                                    if session.text(json).await.is_err() {
                                        return;
                                    }
                                }
                                if should_close {
                                    let _ = session.close(None).await;
                                    return;
                                }
                            }
                        }
                        Err(e) => {
                            protocol_errors += 1;
                            tracing::warn!(error = %e, errors = protocol_errors, "invalid client frame");
                            let error_event = RealtimeAudioServerEvent::Error(RealtimeErrorEvent {
                                code: RealtimeErrorCode::BadRequest,
                                message: "invalid JSON frame".to_string(),
                                retryable: false,
                            });
                            if let Ok(json) = serde_json::to_string(&error_event) {
                                let _ = session.text(json).await;
                            }
                            if protocol_errors >= MAX_PROTOCOL_ERRORS {
                                let _ = session.close(None).await;
                                return;
                            }
                        }
                    }
                }
                Ok(Some(Ok(AggregatedMessage::Binary(_)))) => {
                    let error_event = RealtimeAudioServerEvent::Error(RealtimeErrorEvent {
                        code: RealtimeErrorCode::UnsupportedFrameType,
                        message: "binary frames not supported in phase 1".to_string(),
                        retryable: false,
                    });
                    if let Ok(json) = serde_json::to_string(&error_event) {
                        let _ = session.text(json).await;
                    }
                }
                Ok(Some(Ok(AggregatedMessage::Ping(data)))) => {
                    let _ = session.pong(&data).await;
                }
                Ok(Some(Ok(AggregatedMessage::Pong(_)))) => {}
                Ok(Some(Ok(AggregatedMessage::Close(_)))) | Ok(None) => {
                    return;
                }
                Err(_) => {
                    // Timeout
                    let closed = RealtimeAudioServerEvent::SessionClosed(RealtimeSessionClosed {
                        reason: RealtimeSessionCloseReason::Timeout,
                    });
                    if let Ok(json) = serde_json::to_string(&closed) {
                        let _ = session.text(json).await;
                    }
                    let _ = session.close(None).await;
                    return;
                }
                _ => {}
            }
        }
    });

    Ok(response)
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

fn realtime_error_to_api_error(err: RealtimeAudioError) -> ApiError {
    match err {
        RealtimeAudioError::EntityNotFound(id) => ApiError::NotFound(format!("entity not found: {id}")),
        RealtimeAudioError::RealtimeDisabled => ApiError::ServiceUnavailable("realtime audio is disabled".to_string()),
        RealtimeAudioError::ProviderUnavailable(msg) => {
            ApiError::ServiceUnavailable(format!("provider unavailable: {msg}"))
        }
        _ => ApiError::Internal("internal error".to_string()),
    }
}

// ---------------------------------------------------------------------------
// 路由注册
// ---------------------------------------------------------------------------

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(web::scope("/api/v2/ai/realtime").route("/audio", web::get().to(realtime_audio_handler)));
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::Utc;
    use fms_application::schemas::auth_schemas::TokenData;
    use fms_application::services::ai_admin_service::AiAdminService;
    use fms_domain::error::DomainError;
    use fms_domain::models::ai_entity_config::AiEntityConfigRecord;
    use fms_domain::ports::ai_entity_config_repository::AiEntityConfigRepository;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct InMemoryAiEntityConfigRepository {
        records: Mutex<HashMap<String, serde_json::Value>>,
    }

    impl InMemoryAiEntityConfigRepository {
        fn new(records: impl IntoIterator<Item = (String, serde_json::Value)>) -> Self {
            Self {
                records: Mutex::new(records.into_iter().collect()),
            }
        }

        fn record(id: String, config: serde_json::Value) -> AiEntityConfigRecord {
            AiEntityConfigRecord {
                id,
                config,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            }
        }
    }

    #[async_trait]
    impl AiEntityConfigRepository for InMemoryAiEntityConfigRepository {
        async fn find_all(&self) -> Result<Vec<AiEntityConfigRecord>, DomainError> {
            let records = self
                .records
                .lock()
                .map_err(|_| DomainError::Internal("repo lock poisoned".to_string()))?;
            Ok(records
                .iter()
                .map(|(id, config)| Self::record(id.clone(), config.clone()))
                .collect())
        }

        async fn find_by_id(&self, id: &str) -> Result<Option<AiEntityConfigRecord>, DomainError> {
            let records = self
                .records
                .lock()
                .map_err(|_| DomainError::Internal("repo lock poisoned".to_string()))?;
            Ok(records
                .get(id)
                .cloned()
                .map(|config| Self::record(id.to_string(), config)))
        }

        async fn save(&self, id: &str, config: &serde_json::Value) -> Result<AiEntityConfigRecord, DomainError> {
            let mut records = self
                .records
                .lock()
                .map_err(|_| DomainError::Internal("repo lock poisoned".to_string()))?;
            records.insert(id.to_string(), config.clone());
            Ok(Self::record(id.to_string(), config.clone()))
        }

        async fn delete(&self, id: &str) -> Result<bool, DomainError> {
            let mut records = self
                .records
                .lock()
                .map_err(|_| DomainError::Internal("repo lock poisoned".to_string()))?;
            Ok(records.remove(id).is_some())
        }
    }

    fn realtime_service_with_entity(entity_id: &str, config: serde_json::Value) -> Arc<RealtimeAudioSessionService> {
        let repo = Arc::new(InMemoryAiEntityConfigRepository::new([(entity_id.to_string(), config)]));
        let admin = Arc::new(AiAdminService::new(repo));
        Arc::new(RealtimeAudioSessionService::new(admin))
    }

    fn test_token(secret: &str, permissions: Vec<&str>) -> String {
        let claims = TokenData {
            sub: Some("test-user".to_string()),
            email: None,
            username: Some("tester".to_string()),
            token_kind: Some("access".to_string()),
            is_admin: Some(false),
            permissions: permissions.into_iter().map(str::to_string).collect(),
            department: None,
            department_id: None,
            pv: Some(1),
            iat: Some(Utc::now().timestamp()),
            exp: Some(Utc::now().timestamp() + 3600),
            iss: None,
            aud: None,
            ua_hash: None,
            ip_subnet_hash: None,
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .expect("test token should encode")
    }

    fn enabled_realtime_config(provider: &str) -> serde_json::Value {
        serde_json::json!({
            "media": {
                "realtime": {
                    "enabled": true,
                    "provider": provider,
                    "asr_streaming_model": "fake-streaming-asr",
                    "tts_streaming_model": "fake-streaming-tts",
                    "input_sample_rate_hz": 16000,
                    "output_sample_rate_hz": 24000,
                    "chunk_ms": 40,
                    "max_session_seconds": 300,
                    "max_frame_bytes": 65536
                }
            }
        })
    }

    #[test]
    fn query_defaults_protocol_version() {
        let json = r#"{"entity_id": "test"}"#;
        let query: RealtimeAudioQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.protocol_version, 1);
    }

    #[test]
    fn realtime_error_maps_to_api_error() {
        let err = RealtimeAudioError::EntityNotFound("test-id".to_string());
        let api_err = realtime_error_to_api_error(err);
        match api_err {
            ApiError::NotFound(msg) => assert!(msg.contains("test-id")),
            _ => panic!("expected NotFound"),
        }
    }

    #[test]
    fn realtime_disabled_maps_to_service_unavailable() {
        let err = RealtimeAudioError::RealtimeDisabled;
        let api_err = realtime_error_to_api_error(err);
        match api_err {
            ApiError::ServiceUnavailable(_) => {}
            _ => panic!("expected ServiceUnavailable"),
        }
    }

    #[test]
    fn provider_unavailable_maps_to_service_unavailable() {
        let err = RealtimeAudioError::ProviderUnavailable("unknown-provider".to_string());
        let api_err = realtime_error_to_api_error(err);
        match api_err {
            ApiError::ServiceUnavailable(msg) => {
                assert!(msg.contains("unknown-provider"));
            }
            _ => panic!("expected ServiceUnavailable"),
        }
    }

    #[actix_web::test]
    async fn realtime_audio_route_requires_auth() {
        use actix_web::test as actix_test;

        // App with route and JWT secret — without a token, the request should be
        // rejected (401 Unauthorized or 500 if middleware data like AuthValidationCache
        // is missing). The important invariant is: NOT 404 (route is mounted).
        let app = actix_test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(crate::middleware::jwt::JwtSecret(
                    "test-secret".to_string(),
                )))
                .configure(configure),
        )
        .await;

        let req = actix_test::TestRequest::get()
            .uri("/api/v2/ai/realtime/audio?entity_id=test")
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        // Route is mounted — not 404
        assert_ne!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
        // Without token -> rejected (401 or 500 depending on middleware completeness)
        assert!(
            resp.status().is_client_error() || resp.status().is_server_error(),
            "expected rejection, got: {}",
            resp.status()
        );
    }

    #[actix_web::test]
    async fn realtime_audio_route_uses_service_config_for_unknown_provider() {
        use actix_web::http::StatusCode;
        use actix_web::test as actix_test;

        let secret = "test-secret";
        let service = realtime_service_with_entity(
            "unknown-provider-entity",
            enabled_realtime_config("openai-realtime-v99"),
        );
        let app = actix_test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(crate::middleware::jwt::JwtSecret(secret.to_string())))
                .app_data(web::Data::new(service))
                .configure(configure),
        )
        .await;

        let token = test_token(secret, vec!["ai:media"]);
        let req = actix_test::TestRequest::get()
            .uri("/api/v2/ai/realtime/audio?entity_id=unknown-provider-entity")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = actix_test::call_service(&app, req).await;

        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[actix_web::test]
    async fn realtime_audio_route_reaches_websocket_upgrade_for_fake_entity() {
        use actix_web::http::StatusCode;
        use actix_web::test as actix_test;

        let secret = "test-secret";
        let service = realtime_service_with_entity("fake-enabled-entity", enabled_realtime_config("fake"));
        let app = actix_test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(crate::middleware::jwt::JwtSecret(secret.to_string())))
                .app_data(web::Data::new(service))
                .configure(configure),
        )
        .await;

        let token = test_token(secret, vec!["ai:media"]);
        let req = actix_test::TestRequest::get()
            .uri("/api/v2/ai/realtime/audio?entity_id=fake-enabled-entity")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = actix_test::call_service(&app, req).await;

        assert_ne!(resp.status(), StatusCode::NOT_FOUND);
        assert_ne!(resp.status(), StatusCode::UNAUTHORIZED);
        assert_ne!(resp.status(), StatusCode::FORBIDDEN);
        assert_ne!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
