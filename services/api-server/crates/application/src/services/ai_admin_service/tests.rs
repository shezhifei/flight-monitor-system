use super::*;
use async_trait::async_trait;
use chrono::Utc;
use fms_domain::error::DomainError;
#[cfg(test)]
use fms_domain::models::ai_entity_config::AiEntityConfigRecord;
use fms_domain::ports::ai_entity_config_repository::AiEntityConfigRepository;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

struct InMemoryAiEntityConfigRepository {
    records: Mutex<HashMap<String, serde_json::Value>>,
}

impl InMemoryAiEntityConfigRepository {
    fn new(records: impl IntoIterator<Item = (String, serde_json::Value)>) -> Self {
        Self {
            records: Mutex::new(records.into_iter().collect()),
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
            .map(|(id, config)| AiEntityConfigRecord {
                id: id.clone(),
                config: config.clone(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .collect())
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<AiEntityConfigRecord>, DomainError> {
        let records = self
            .records
            .lock()
            .map_err(|_| DomainError::Internal("repo lock poisoned".to_string()))?;
        Ok(records.get(id).map(|config| AiEntityConfigRecord {
            id: id.to_string(),
            config: config.clone(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }))
    }

    async fn save(&self, _id: &str, _config: &serde_json::Value) -> Result<AiEntityConfigRecord, DomainError> {
        unimplemented!()
    }

    async fn delete(&self, _id: &str) -> Result<bool, DomainError> {
        unimplemented!()
    }
}

#[test]
fn tool_categories_map_reuses_static_catalog() {
    let first = super::catalog::tool_categories_map();
    let second = super::catalog::tool_categories_map();

    assert!(std::ptr::eq(first, second));
    assert_eq!(first.len(), 15);
    assert_eq!(first.get("flight"), Some(&"航班查询"));
    assert_eq!(first.get("media"), Some(&"语音与媒体"));
}

#[tokio::test]
async fn get_entity_status_does_not_leak_api_key() {
    let repo = Arc::new(InMemoryAiEntityConfigRepository::new([(
        "entity-1".to_string(),
        serde_json::json!({
            "api_key": "sk-live-secret-key",
            "base_url": "https://api.example.com",
            "default_model": "gpt-4",
        }),
    )]));
    let service = AiAdminService::new(repo);
    let status = service.get_entity_status("entity-1").await.unwrap();

    assert_eq!(status["id"], "entity-1");
    let config = &status["config"];
    assert!(
        config.get("api_key").is_none(),
        "api_key must not appear in status response"
    );
    assert!(config.get("base_url").is_none(), "connection lives in providers.default");
    assert_eq!(
        config.pointer("/providers/default/base_url"),
        Some(&serde_json::json!("https://api.example.com"))
    );
    assert!(config.pointer("/providers/default/api_key").is_none());
    assert_eq!(
        config.pointer("/model_routing/default"),
        Some(&serde_json::json!("gpt-4"))
    );
    let serialized = serde_json::to_string(&status).unwrap();
    assert!(!serialized.contains("sk-live-secret-key"));
}

fn entity_config_with_nested_credentials() -> serde_json::Value {
    serde_json::json!({
        "config_version": 2,
        "model_routing": { "default": "gpt-4" },
        "providers": {
            "default": {
                "type": "openai_compatible",
                "base_url": "https://api.example.com/v1",
                "api_key": "sk-default-nested-secret"
            },
            "asr": {
                "type": "openai_compatible",
                "base_url": "https://api.example.com/v1",
                "api_key": "sk-asr-nested-secret"
            }
        },
        "security": {
            "mask_sensitive": true,
            "log_prompts": false
        }
    })
}

#[tokio::test]
async fn get_entity_masked_config_masks_nested_provider_api_keys() {
    let repo = Arc::new(InMemoryAiEntityConfigRepository::new([(
        "entity-1".to_string(),
        entity_config_with_nested_credentials(),
    )]));
    let service = AiAdminService::new(repo);

    let masked = service
        .get_entity_masked_config("entity-1")
        .await
        .unwrap()
        .expect("entity should exist");

    let serialized = serde_json::to_string(&masked).unwrap();
    assert!(
        !serialized.contains("sk-default-nested-secret"),
        "providers.default.api_key must be masked, got: {serialized}"
    );
    assert!(
        !serialized.contains("sk-asr-nested-secret"),
        "providers.asr.api_key must be masked, got: {serialized}"
    );

    let provider_key = masked
        .pointer("/providers/default/api_key")
        .and_then(serde_json::Value::as_str)
        .expect("providers.default.api_key must remain present (masked)");
    assert!(
        provider_key.contains("..."),
        "expected a masked preview, got {provider_key}"
    );
    assert!(!provider_key.contains("nested-secret"));
    assert!(
        masked.get("providers").and_then(|p| p.get("default")).is_some(),
        "providers structure must be preserved"
    );
}

#[tokio::test]
async fn get_entity_status_removes_nested_provider_api_keys() {
    let repo = Arc::new(InMemoryAiEntityConfigRepository::new([(
        "entity-1".to_string(),
        entity_config_with_nested_credentials(),
    )]));
    let service = AiAdminService::new(repo);

    let status = service.get_entity_status("entity-1").await.unwrap();
    let config = &status["config"];

    let serialized = serde_json::to_string(&status).unwrap();
    assert!(
        !serialized.contains("sk-default-nested-secret"),
        "providers.default.api_key must be stripped, got: {serialized}"
    );
    assert!(
        !serialized.contains("sk-asr-nested-secret"),
        "providers.asr.api_key must be stripped, got: {serialized}"
    );

    assert!(
        config
            .get("providers")
            .and_then(|p| p.get("default"))
            .and_then(|d| d.get("api_key"))
            .is_none(),
        "providers.default.api_key key must be absent"
    );
    assert_eq!(
        config.pointer("/providers/default/base_url"),
        Some(&serde_json::json!("https://api.example.com/v1")),
        "non-sensitive provider fields must be preserved"
    );
}

#[test]
fn mask_config_is_recursive_and_char_boundary_safe() {
    let value = serde_json::json!({
        "api_key": "sk-very-long-secret-value-1234",
        "provider": {"api_key": "sk-provider-secret-xyz"},
        "providers": {
            "default": {"api_key": "sk-default-secret"},
            "secondary": {"api_key": "短"}
        },
        "authorization": "Bearer super-secret-token",
        "client_secret": "cs-abcdef",
        "password": "hunter2",
        "secret": "topsecret",
        "base_url": "https://api.example.com/v1"
    });

    let masked = super::config::mask_config(value);
    let serialized = serde_json::to_string(&masked).unwrap();

    for needle in [
        "sk-very-long-secret-value-1234",
        "sk-provider-secret-xyz",
        "sk-default-secret",
        "super-secret-token",
        "cs-abcdef",
        "hunter2",
        "topsecret",
    ] {
        assert!(!serialized.contains(needle), "sensitive value leaked: {needle}");
    }
    assert_eq!(masked["base_url"], "https://api.example.com/v1");
    assert!(masked.get("provider").and_then(|p| p.get("api_key")).is_some());
}

#[test]
fn remove_api_key_strips_nested_sensitive_keys() {
    let value = serde_json::json!({
        "api_key": "sk-top",
        "provider": {"api_key": "sk-provider", "base_url": "https://x.example.com"},
        "providers": {"default": {"api_key": "sk-default"}},
        "tools": [{"config": {"api_key": "sk-in-array"}}]
    });

    let redacted = super::config::remove_api_key(value);
    let serialized = serde_json::to_string(&redacted).unwrap();

    for needle in ["sk-top", "sk-provider", "sk-default", "sk-in-array"] {
        assert!(!serialized.contains(needle), "nested secret leaked: {needle}");
    }
    assert!(redacted.get("api_key").is_none());
    assert!(redacted.get("provider").and_then(|p| p.get("api_key")).is_none());
    assert!(redacted
        .get("providers")
        .and_then(|p| p.get("default"))
        .and_then(|d| d.get("api_key"))
        .is_none());
    assert!(
        redacted
            .get("tools")
            .and_then(|t| t.get(0))
            .and_then(|i| i.get("config"))
            .and_then(|c| c.get("api_key"))
            .is_none(),
        "array-embedded sensitive keys must also be removed"
    );
    assert_eq!(
        redacted.get("provider").and_then(|p| p.get("base_url")),
        Some(&serde_json::json!("https://x.example.com"))
    );
}

#[test]
fn canonicalize_lifts_flat_aliases_into_the_document() {
    let mut config = serde_json::json!({
        "base_url": "https://api.example.com/v1",
        "api_key": "sk-test",
        "default_model": "gpt-4.1",
        "allowed_tool_categories": ["flight"],
        "asr_model": "whisper-large-v3",
        "tts_model": "tts-1-hd",
        "tts_voice": "verse"
    })
    .as_object()
    .cloned()
    .unwrap();

    super::config::canonicalize_entity_document(&mut config);
    let document = serde_json::Value::Object(config.clone());

    assert_eq!(
        document.pointer("/providers/default/base_url"),
        Some(&serde_json::json!("https://api.example.com/v1"))
    );
    assert_eq!(
        document.pointer("/providers/default/api_key"),
        Some(&serde_json::json!("sk-test"))
    );
    assert_eq!(
        document.pointer("/model_routing/default"),
        Some(&serde_json::json!("gpt-4.1"))
    );
    assert_eq!(
        document.pointer("/tooling/allowed_tool_categories"),
        Some(&serde_json::json!(["flight"]))
    );
    assert_eq!(
        document.pointer("/media/asr/model"),
        Some(&serde_json::json!("whisper-large-v3"))
    );
    assert_eq!(
        document.pointer("/media/tts/voice"),
        Some(&serde_json::json!("verse"))
    );
    for alias in [
        "base_url",
        "api_key",
        "default_model",
        "allowed_tool_categories",
        "asr_model",
        "tts_model",
        "tts_voice",
    ] {
        assert!(config.get(alias).is_none(), "{alias} must not remain at the top level");
    }
}
