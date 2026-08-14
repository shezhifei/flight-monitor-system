//! AI 配置中心 DTO

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntityConfigUpdate {
    pub config_version: Option<i64>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub endpoints: Option<serde_json::Value>,
    pub default_model: Option<String>,
    pub asr_model: Option<String>,
    pub tts_model: Option<String>,
    pub tts_voice: Option<String>,
    pub media: Option<serde_json::Value>,
    pub realtime_audio_enabled: Option<bool>,
    pub api_format: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<i64>,
    pub top_p: Option<f64>,
    pub denied_tools: Option<Vec<String>>,
    pub timeout: Option<f64>,
    pub max_retries: Option<i64>,
    pub retry_delay: Option<f64>,
    pub cost_per_1k_input: Option<f64>,
    pub cost_per_1k_output: Option<f64>,
    pub context_window: Option<i64>,
    pub system_prompt: Option<String>,
    pub task_template: Option<String>,
    pub allowed_tool_categories: Option<Vec<String>>,
    pub allowed_tools: Option<serde_json::Value>,
    pub providers: Option<serde_json::Value>,
    pub model_routing: Option<serde_json::Value>,
    pub models: Option<serde_json::Value>,
    pub tooling: Option<serde_json::Value>,
    pub mcp: Option<serde_json::Value>,
    pub skills: Option<serde_json::Value>,
    pub subagents: Option<serde_json::Value>,
    pub context_policy: Option<serde_json::Value>,
    pub cache_policy: Option<serde_json::Value>,
    pub security: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemPromptUpdate {
    pub prompt: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionProbeRequest {
    pub entity_id: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    #[serde(default = "default_true")]
    pub include_models: bool,
    #[serde(default)]
    pub include_capabilities: bool,
    #[serde(default = "default_connection_timeout")]
    pub timeout: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntityToolsUpdateRequest {
    pub allowed_tool_categories: Option<Vec<String>>,
    pub allowed_tools: Option<Vec<String>>,
    pub denied_tools: Option<Vec<String>>,
}

fn default_true() -> bool {
    true
}

fn default_connection_timeout() -> f64 {
    10.0
}

#[cfg(test)]
mod tests {
    use super::{ConnectionProbeRequest, EntityConfigUpdate};

    #[test]
    fn entity_config_update_accepts_media_model_fields() {
        let payload = r#"{
            "asr_model": "whisper-large-v3",
            "tts_model": "gpt-4o-mini-tts",
            "tts_voice": "verse"
        }"#;

        let update: EntityConfigUpdate = serde_json::from_str(payload).unwrap();

        assert_eq!(update.asr_model.as_deref(), Some("whisper-large-v3"));
        assert_eq!(update.tts_model.as_deref(), Some("gpt-4o-mini-tts"));
        assert_eq!(update.tts_voice.as_deref(), Some("verse"));
    }

    #[test]
    fn entity_config_update_accepts_capability_policy_fields() {
        let payload = r#"{
            "config_version": 2,
            "model_routing": {
                "default": "gpt-4.1",
                "summary": "gpt-4.1-mini"
            },
            "models": {
                "gpt-4.1": {
                    "modalities": {
                        "input": ["text", "image"],
                        "output": ["text"]
                    },
                    "capabilities": {
                        "tool_calling": true,
                        "prompt_cache": true
                    }
                }
            },
            "tooling": {
                "enabled": true,
                "allowed_tool_sources": ["builtin", "mcp"],
                "write_action_policy": "proposal_only"
            },
            "mcp": {
                "enabled": true,
                "binding_ids": ["default:ops-docs"]
            },
            "skills": {
                "enabled": true,
                "allowlist": ["Code"]
            },
            "subagents": {
                "enabled": true,
                "allowed_entity_ids": ["dispatcher"]
            },
            "context_policy": {
                "strategy": "hybrid"
            },
            "cache_policy": {
                "enabled": true
            },
            "security": {
                "max_input_bytes": 26214400
            }
        }"#;

        let update: EntityConfigUpdate = serde_json::from_str(payload).unwrap();

        assert_eq!(update.config_version, Some(2));
        assert_eq!(
            update
                .model_routing
                .as_ref()
                .and_then(|value| value.get("summary"))
                .and_then(serde_json::Value::as_str),
            Some("gpt-4.1-mini")
        );
        assert_eq!(
            update
                .tooling
                .as_ref()
                .and_then(|value| value.get("write_action_policy"))
                .and_then(serde_json::Value::as_str),
            Some("proposal_only")
        );
        assert_eq!(
            update
                .subagents
                .as_ref()
                .and_then(|value| value.get("enabled"))
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert!(update.cache_policy.is_some());
        assert!(update.security.is_some());
    }

    #[test]
    fn entity_config_update_accepts_providers() {
        let payload = r#"{
            "providers": {
                "default": {
                    "type": "openai_compatible",
                    "base_url": "https://api.example.com/v1",
                    "api_key": "sk-test"
                },
                "asr": {
                    "type": "openai_compatible",
                    "base_url": "https://asr.example.com/v1"
                }
            }
        }"#;

        let update: EntityConfigUpdate = serde_json::from_str(payload).unwrap();
        let default_url = update
            .providers
            .as_ref()
            .and_then(|value| value.get("default"))
            .and_then(|value| value.get("base_url"))
            .and_then(serde_json::Value::as_str);
        assert_eq!(default_url, Some("https://api.example.com/v1"));
        assert!(update
            .providers
            .as_ref()
            .and_then(|value| value.get("asr"))
            .is_some());
    }

    #[test]
    fn connection_probe_request_defaults_include_capabilities_false() {
        let payload = r#"{"entity_id": "default"}"#;
        let req: ConnectionProbeRequest = serde_json::from_str(payload).unwrap();
        assert_eq!(req.entity_id.as_deref(), Some("default"));
        assert!(!req.include_capabilities);
        assert!(req.include_models);
    }

    #[test]
    fn connection_probe_request_accepts_include_capabilities_true() {
        let payload = r#"{
            "entity_id": "default",
            "include_models": true,
            "include_capabilities": true
        }"#;
        let req: ConnectionProbeRequest = serde_json::from_str(payload).unwrap();
        assert!(req.include_capabilities);
        assert!(req.include_models);
    }

    #[test]
    fn connection_probe_request_include_capabilities_false_explicit() {
        let payload = r#"{
            "entity_id": "default",
            "include_capabilities": false
        }"#;
        let req: ConnectionProbeRequest = serde_json::from_str(payload).unwrap();
        assert!(!req.include_capabilities);
    }
}
