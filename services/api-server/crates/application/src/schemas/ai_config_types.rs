//! AI 配置 v2 强类型 DTO
//!
//! 提供结构化的配置类型，替代 Option<serde_json::Value> 的松散模式。

use serde::{Deserialize, Serialize};

/// API 格式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ApiFormat {
    ChatCompletions,
    Responses,
}

impl Default for ApiFormat {
    fn default() -> Self {
        Self::ChatCompletions
    }
}

/// 模态类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ModalityType {
    Text,
    Image,
    Audio,
    File,
}

/// 模型模态声明
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelModalities {
    #[serde(default = "default_text_modalities")]
    pub input: Vec<ModalityType>,
    #[serde(default = "default_text_modalities")]
    pub output: Vec<ModalityType>,
}

fn default_text_modalities() -> Vec<ModalityType> {
    vec![ModalityType::Text]
}

/// 模型能力声明
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
    #[serde(default = "default_true")]
    pub tool_calling: bool,
    #[serde(default)]
    pub parallel_tool_calls: bool,
    #[serde(default = "default_true")]
    pub streaming: bool,
    #[serde(default)]
    pub structured_output: bool,
    #[serde(default)]
    pub prompt_cache: bool,
}

impl Default for ModelCapabilities {
    fn default() -> Self {
        Self {
            tool_calling: true,
            parallel_tool_calls: false,
            streaming: true,
            structured_output: false,
            prompt_cache: false,
        }
    }
}

/// 模型成本元数据
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelCost {
    #[serde(default = "default_currency")]
    pub currency: String,
    #[serde(default)]
    pub input_per_1k: f64,
    #[serde(default)]
    pub output_per_1k: f64,
    #[serde(default)]
    pub cached_input_per_1k: f64,
}

fn default_currency() -> String {
    "USD".to_string()
}

/// 模型配置 v2
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfigV2 {
    pub provider_model: String,
    /// 指向 providers 映射的键（如 'default'/'asr'/'tts'）；缺失或旧 config 默认 'default'。
    #[serde(default = "default_provider_ref")]
    pub provider_ref: String,
    #[serde(default)]
    pub api_format: ApiFormat,
    #[serde(default = "default_context_window")]
    pub context_window: i64,
    #[serde(default = "default_max_output_tokens")]
    pub max_output_tokens: i64,
    #[serde(default)]
    pub modalities: ModelModalities,
    #[serde(default)]
    pub capabilities: ModelCapabilities,
    #[serde(default)]
    pub cost: ModelCost,
}

fn default_provider_ref() -> String {
    "default".to_string()
}

fn default_context_window() -> i64 {
    128000
}

fn default_max_output_tokens() -> i64 {
    4096
}

/// Provider 配置 v2
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfigV2 {
    #[serde(default = "default_provider_type", alias = "type")]
    pub provider_type: String,
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub api_format: ApiFormat,
    #[serde(default = "default_timeout")]
    pub timeout: f64,
    #[serde(default = "default_max_retries")]
    pub max_retries: i64,
    #[serde(default = "default_retry_delay")]
    pub retry_delay: f64,
}

fn default_provider_type() -> String {
    "openai_compatible".to_string()
}

fn default_timeout() -> f64 {
    30.0
}

fn default_max_retries() -> i64 {
    3
}

fn default_retry_delay() -> f64 {
    0.5
}

/// 模型路由
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRouting {
    #[serde(default = "default_model", alias = "default")]
    pub default_model: String,
    pub chat: Option<String>,
    pub summary: Option<String>,
    pub vision: Option<String>,
    pub audio_transcription: Option<String>,
    pub audio_speech: Option<String>,
    pub embedding: Option<String>,
}

fn default_model() -> String {
    "gpt-4o".to_string()
}

impl Default for ModelRouting {
    fn default() -> Self {
        Self {
            default_model: default_model(),
            chat: None,
            summary: None,
            vision: None,
            audio_transcription: None,
            audio_speech: None,
            embedding: None,
        }
    }
}

/// 写动作策略
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WriteActionPolicy {
    ProposalOnly,
    DirectExecute,
}

impl Default for WriteActionPolicy {
    fn default() -> Self {
        Self::ProposalOnly
    }
}

/// 工具配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolingConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_max_rounds")]
    pub max_rounds: i64,
    #[serde(default)]
    pub allow_parallel: bool,
    #[serde(default = "default_tool_sources")]
    pub allowed_tool_sources: Vec<String>,
    #[serde(default)]
    pub allowed_tool_categories: Vec<String>,
    pub allowed_tools: Option<Vec<String>>,
    #[serde(default)]
    pub denied_tools: Vec<String>,
    #[serde(default)]
    pub write_action_policy: WriteActionPolicy,
}

fn default_max_rounds() -> i64 {
    5
}

fn default_tool_sources() -> Vec<String> {
    vec!["builtin".to_string()]
}

impl Default for ToolingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_rounds: default_max_rounds(),
            allow_parallel: false,
            allowed_tool_sources: default_tool_sources(),
            allowed_tool_categories: Vec::new(),
            allowed_tools: None,
            denied_tools: Vec::new(),
            write_action_policy: WriteActionPolicy::default(),
        }
    }
}

/// MCP 服务器引用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerRef {
    pub server_id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// MCP 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub servers: Vec<McpServerRef>,
    #[serde(default = "default_mcp_prefix")]
    pub tool_name_prefix: String,
    #[serde(default = "default_discovery_ttl")]
    pub discovery_cache_ttl_seconds: i64,
    #[serde(default)]
    pub fail_closed: bool,
}

fn default_mcp_prefix() -> String {
    "mcp".to_string()
}

fn default_discovery_ttl() -> i64 {
    300
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            servers: Vec::new(),
            tool_name_prefix: default_mcp_prefix(),
            discovery_cache_ttl_seconds: default_discovery_ttl(),
            fail_closed: false,
        }
    }
}

/// Skill 绑定引用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillBindingRef {
    pub skill_slug: String,
    pub version: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Skills 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub allowlist: Vec<String>,
    #[serde(default)]
    pub bindings: Vec<SkillBindingRef>,
    #[serde(default)]
    pub fail_closed: bool,
}

impl Default for SkillsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            allowlist: Vec::new(),
            bindings: Vec::new(),
            fail_closed: false,
        }
    }
}

/// 子代理模式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SubagentsMode {
    EntityHandoff,
}

impl Default for SubagentsMode {
    fn default() -> Self {
        Self::EntityHandoff
    }
}

/// 子代理配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub mode: SubagentsMode,
    #[serde(default)]
    pub allowed_entity_ids: Vec<String>,
    #[serde(default = "default_max_depth")]
    pub max_depth: i64,
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: i64,
    #[serde(default = "default_true")]
    pub inherit_parent_context: bool,
    #[serde(default = "default_true")]
    pub require_tool_calling_capability: bool,
    pub handoff_prompt: Option<String>,
}

fn default_max_depth() -> i64 {
    1
}

fn default_max_concurrency() -> i64 {
    2
}

impl Default for SubagentsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: SubagentsMode::default(),
            allowed_entity_ids: Vec::new(),
            max_depth: default_max_depth(),
            max_concurrency: default_max_concurrency(),
            inherit_parent_context: true,
            require_tool_calling_capability: true,
            handoff_prompt: None,
        }
    }
}

/// 上下文策略
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ContextStrategy {
    SlidingWindow,
    SummaryCompression,
    Hybrid,
}

impl Default for ContextStrategy {
    fn default() -> Self {
        Self::Hybrid
    }
}

/// 上下文策略配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextPolicyConfig {
    #[serde(default)]
    pub strategy: ContextStrategy,
    #[serde(default = "default_max_context_tokens")]
    pub max_context_tokens: i64,
    #[serde(default = "default_compression_threshold")]
    pub compression_threshold_tokens: i64,
    #[serde(default = "default_preserve_recent")]
    pub preserve_recent_messages: i64,
    pub summary_model: Option<String>,
    #[serde(default = "default_summary_max_tokens")]
    pub summary_max_tokens: i64,
    #[serde(default = "default_true")]
    pub persist_summaries: bool,
    /// 信封 risk_ceiling。默认 medium；overlay 调高的动作会被校验器砍掉。
    #[serde(default = "default_risk_ceiling")]
    pub risk_ceiling: String,
}

fn default_max_context_tokens() -> i64 {
    64000
}

fn default_compression_threshold() -> i64 {
    48000
}

fn default_preserve_recent() -> i64 {
    12
}

fn default_summary_max_tokens() -> i64 {
    1200
}

fn default_risk_ceiling() -> String {
    "medium".to_string()
}

impl Default for ContextPolicyConfig {
    fn default() -> Self {
        Self {
            strategy: ContextStrategy::default(),
            max_context_tokens: default_max_context_tokens(),
            compression_threshold_tokens: default_compression_threshold(),
            preserve_recent_messages: default_preserve_recent(),
            summary_model: None,
            summary_max_tokens: default_summary_max_tokens(),
            persist_summaries: true,
            risk_ceiling: default_risk_ceiling(),
        }
    }
}

/// Provider prompt cache 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderPromptCacheConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_cache_retention")]
    pub retention: String,
    #[serde(default = "default_cache_namespace")]
    pub key_namespace: String,
}

fn default_cache_retention() -> String {
    "24h".to_string()
}

fn default_cache_namespace() -> String {
    "flight_monitor".to_string()
}

impl Default for ProviderPromptCacheConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            retention: default_cache_retention(),
            key_namespace: default_cache_namespace(),
        }
    }
}

/// 上下文缓存配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextCacheConfig {
    #[serde(default = "default_cache_backend")]
    pub backend: String,
    #[serde(default = "default_context_cache_ttl")]
    pub ttl_seconds: i64,
}

fn default_cache_backend() -> String {
    "redis".to_string()
}

fn default_context_cache_ttl() -> i64 {
    86400
}

impl Default for ContextCacheConfig {
    fn default() -> Self {
        Self {
            backend: default_cache_backend(),
            ttl_seconds: default_context_cache_ttl(),
        }
    }
}

/// 工具结果缓存配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultCacheConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_tool_cache_ttl")]
    pub ttl_seconds: i64,
    #[serde(default)]
    pub cacheable_tools: Vec<String>,
}

fn default_tool_cache_ttl() -> i64 {
    60
}

impl Default for ToolResultCacheConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            ttl_seconds: default_tool_cache_ttl(),
            cacheable_tools: Vec::new(),
        }
    }
}

/// MCP 资源缓存配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResourceCacheConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_mcp_cache_ttl")]
    pub ttl_seconds: i64,
}

fn default_mcp_cache_ttl() -> i64 {
    300
}

impl Default for McpResourceCacheConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            ttl_seconds: default_mcp_cache_ttl(),
        }
    }
}

/// 缓存策略配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachePolicyConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub provider_prompt_cache: ProviderPromptCacheConfig,
    #[serde(default)]
    pub context_cache: ContextCacheConfig,
    #[serde(default)]
    pub tool_result_cache: ToolResultCacheConfig,
    #[serde(default)]
    pub mcp_resource_cache: McpResourceCacheConfig,
}

impl Default for CachePolicyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            provider_prompt_cache: ProviderPromptCacheConfig::default(),
            context_cache: ContextCacheConfig::default(),
            tool_result_cache: ToolResultCacheConfig::default(),
            mcp_resource_cache: McpResourceCacheConfig::default(),
        }
    }
}

/// 安全配置 v2
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfigV2 {
    #[serde(default = "default_true")]
    pub mask_sensitive: bool,
    #[serde(default)]
    pub log_prompts: bool,
    #[serde(default = "default_max_input_bytes")]
    pub max_input_bytes: i64,
    #[serde(default = "default_allowed_mime_types")]
    pub allowed_input_mime_types: Vec<String>,
}

fn default_max_input_bytes() -> i64 {
    26214400 // 25MB
}

fn default_allowed_mime_types() -> Vec<String> {
    vec![
        "text/plain".to_string(),
        "image/png".to_string(),
        "image/jpeg".to_string(),
        "audio/wav".to_string(),
    ]
}

impl Default for SecurityConfigV2 {
    fn default() -> Self {
        Self {
            mask_sensitive: true,
            log_prompts: false,
            max_input_bytes: default_max_input_bytes(),
            allowed_input_mime_types: default_allowed_mime_types(),
        }
    }
}

/// AI 实体配置 v2 - 顶层结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiEntityConfigV2 {
    #[serde(default = "default_config_version")]
    pub config_version: i64,
    /// 多 provider 映射（键如 'default'/'asr'/'tts'）。旧 config 升级时把原 provider 放入 providers['default']。
    #[serde(default)]
    pub providers: std::collections::HashMap<String, ProviderConfigV2>,
    /// 原单-provider 字段，保留作为 providers['default'] 的镜像/别名以保证向后兼容。
    #[serde(default)]
    pub provider: Option<ProviderConfigV2>,
    #[serde(default)]
    pub model_routing: ModelRouting,
    #[serde(default)]
    pub models: std::collections::HashMap<String, ModelConfigV2>,
    #[serde(default)]
    pub tooling: ToolingConfig,
    #[serde(default)]
    pub mcp: McpConfig,
    #[serde(default)]
    pub skills: SkillsConfig,
    #[serde(default)]
    pub subagents: SubagentsConfig,
    #[serde(default)]
    pub context_policy: ContextPolicyConfig,
    #[serde(default)]
    pub cache_policy: CachePolicyConfig,
    #[serde(default)]
    pub security: SecurityConfigV2,
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
    pub task_template: Option<String>,
}

fn default_config_version() -> i64 {
    2
}

fn default_system_prompt() -> String {
    "你是一个航班监控系统的 AI 助手。".to_string()
}

impl Default for AiEntityConfigV2 {
    fn default() -> Self {
        Self {
            config_version: 2,
            providers: std::collections::HashMap::new(),
            provider: None,
            model_routing: ModelRouting::default(),
            models: std::collections::HashMap::new(),
            tooling: ToolingConfig::default(),
            mcp: McpConfig::default(),
            skills: SkillsConfig::default(),
            subagents: SubagentsConfig::default(),
            context_policy: ContextPolicyConfig::default(),
            cache_policy: CachePolicyConfig::default(),
            security: SecurityConfigV2::default(),
            system_prompt: default_system_prompt(),
            task_template: None,
        }
    }
}

/// 运行时解析后的配置快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedCapabilitySnapshot {
    pub entity_id: String,
    pub config_version: i64,
    pub config_revision: i64,
    pub model_id: String,
    pub provider_model: String,
    pub api_format: ApiFormat,
    pub input_modalities: Vec<ModalityType>,
    pub output_modalities: Vec<ModalityType>,
    pub tool_count: i64,
    pub mcp_enabled: bool,
    pub skills_enabled: bool,
    pub subagents_enabled: bool,
    pub cache_enabled: bool,
    pub context_strategy: ContextStrategy,
    pub snapshot_hash: String,
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_v2() {
        let config = AiEntityConfigV2::default();
        assert_eq!(config.config_version, 2);
        assert_eq!(config.tooling.write_action_policy, WriteActionPolicy::ProposalOnly);
        assert!(!config.mcp.enabled);
        assert!(!config.skills.enabled);
        assert!(!config.subagents.enabled);
        assert!(config.cache_policy.enabled);
    }

    #[test]
    fn test_deserialize_v2_config() {
        let json = r#"{
            "config_version": 2,
            "provider": {
                "base_url": "https://api.openai.com/v1",
                "api_key": "sk-test"
            },
            "model_routing": {
                "default": "gpt-4.1",
                "summary": "gpt-4.1-mini"
            },
            "tooling": {
                "enabled": true,
                "write_action_policy": "proposal_only"
            },
            "mcp": {
                "enabled": false
            }
        }"#;

        let config: AiEntityConfigV2 = serde_json::from_str(json).unwrap();
        assert_eq!(config.provider.unwrap().base_url, "https://api.openai.com/v1");
        assert_eq!(config.model_routing.summary, Some("gpt-4.1-mini".to_string()));
    }

    #[test]
    fn test_model_capabilities_default() {
        let caps = ModelCapabilities::default();
        assert!(caps.tool_calling);
        assert!(caps.streaming);
        assert!(!caps.prompt_cache);
    }

    #[test]
    fn test_provider_type_alias() {
        // 前端发送 "type" 字段，Rust 应能反序列化为 provider_type
        let json = r#"{
            "type": "openai_compatible",
            "base_url": "https://api.openai.com/v1"
        }"#;
        let provider: ProviderConfigV2 = serde_json::from_str(json).unwrap();
        assert_eq!(provider.provider_type, "openai_compatible");

        // 也支持 "provider_type" 字段
        let json2 = r#"{
            "provider_type": "azure",
            "base_url": "https://azure.openai.com/v1"
        }"#;
        let provider2: ProviderConfigV2 = serde_json::from_str(json2).unwrap();
        assert_eq!(provider2.provider_type, "azure");
    }

    #[test]
    fn test_legacy_single_provider_backward_compat() {
        // 旧 JSON 无 providers / provider_ref，必须零行为变化地反序列化。
        let json = r#"{
            "config_version": 2,
            "provider": {
                "base_url": "https://api.openai.com/v1",
                "api_key": "sk-test"
            },
            "models": {
                "gpt-4o": { "provider_model": "gpt-4o" }
            }
        }"#;

        let config: AiEntityConfigV2 = serde_json::from_str(json).unwrap();
        // providers 默认空，原 provider 字段保留
        assert!(config.providers.is_empty());
        assert_eq!(config.provider.as_ref().unwrap().base_url, "https://api.openai.com/v1");
        // 缺失 provider_ref 默认回退 'default'
        assert_eq!(config.models.get("gpt-4o").unwrap().provider_ref, "default");
    }

    #[test]
    fn test_multi_provider_config() {
        // 新 JSON 携带 providers 映射与 model.provider_ref。
        let json = r#"{
            "config_version": 2,
            "providers": {
                "default": {
                    "base_url": "https://api.openai.com/v1",
                    "api_key": "sk-chat"
                },
                "asr": {
                    "base_url": "https://api.asr.example/v1",
                    "api_key": "sk-asr"
                }
            },
            "models": {
                "gpt-4o": { "provider_model": "gpt-4o", "provider_ref": "default" },
                "whisper": { "provider_model": "whisper-1", "provider_ref": "asr" }
            }
        }"#;

        let config: AiEntityConfigV2 = serde_json::from_str(json).unwrap();
        assert_eq!(config.providers.len(), 2);
        assert_eq!(
            config.providers.get("asr").unwrap().base_url,
            "https://api.asr.example/v1"
        );
        assert_eq!(config.models.get("gpt-4o").unwrap().provider_ref, "default");
        assert_eq!(config.models.get("whisper").unwrap().provider_ref, "asr");
    }

    #[test]
    fn test_model_routing_default_alias() {
        // 前端发送 "default" 字段，Rust 应能反序列化为 default_model
        let json = r#"{
            "default": "gpt-4.1",
            "summary": "gpt-4.1-mini"
        }"#;
        let routing: ModelRouting = serde_json::from_str(json).unwrap();
        assert_eq!(routing.default_model, "gpt-4.1");
        assert_eq!(routing.summary, Some("gpt-4.1-mini".to_string()));

        // 也支持 "default_model" 字段
        let json2 = r#"{
            "default_model": "gpt-4o",
            "chat": "gpt-4o"
        }"#;
        let routing2: ModelRouting = serde_json::from_str(json2).unwrap();
        assert_eq!(routing2.default_model, "gpt-4o");
    }
}
