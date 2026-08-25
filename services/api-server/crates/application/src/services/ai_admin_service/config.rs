use serde_json::Value;

#[derive(Debug, Clone, Default)]
pub(super) struct EntityRuntimeMetrics {
    pub(super) requests: u64,
    pub(super) errors: u64,
    pub(super) total_tokens: u64,
    pub(super) total_cost: f64,
}

pub(super) fn default_provider_config() -> serde_json::Value {
    serde_json::json!({
        "type": "openai_compatible",
        "base_url": "https://api.openai.com/v1",
        "api_key": "",
        "api_format": "chat_completions",
        "timeout": 30.0,
        "max_retries": 3,
        "retry_delay": 0.5
    })
}

fn is_blank_value(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => true,
        serde_json::Value::String(text) => text.trim().is_empty(),
        _ => false,
    }
}

const CONNECTION_KEYS: &[&str] = &[
    "base_url",
    "api_key",
    "api_format",
    "timeout",
    "max_retries",
    "retry_delay",
];

const DOCUMENT_ALIASES: &[&str] = &[
    "base_url",
    "api_key",
    "api_format",
    "timeout",
    "max_retries",
    "retry_delay",
    "default_model",
    "provider",
    "allowed_tool_categories",
    "allowed_tools",
    "denied_tools",
    "asr_model",
    "tts_model",
    "tts_voice",
    "realtime_audio_enabled",
];

/// Lift inbound / stored aliases into the current document, then drop them.
pub(super) fn canonicalize_entity_document(config: &mut serde_json::Map<String, serde_json::Value>) {
    lift_connection(config);
    lift_model_routing(config);
    lift_tooling(config);
    lift_media(config);
    for key in DOCUMENT_ALIASES {
        config.remove(*key);
    }
}

fn lift_connection(config: &mut serde_json::Map<String, serde_json::Value>) {
    let top_level: Vec<(String, serde_json::Value)> = CONNECTION_KEYS
        .iter()
        .filter_map(|key| {
            config
                .get(*key)
                .filter(|value| !is_blank_value(value))
                .cloned()
                .map(|value| ((*key).to_string(), value))
        })
        .collect();
    let singular = config.get("provider").and_then(Value::as_object).cloned();

    let providers = config.entry("providers").or_insert_with(|| serde_json::json!({}));
    let Some(providers_obj) = providers.as_object_mut() else {
        return;
    };
    if let Some(single) = singular {
        if !providers_obj.contains_key("default") {
            providers_obj.insert("default".to_string(), serde_json::Value::Object(single));
        }
    }
    let default_provider = providers_obj.entry("default").or_insert_with(default_provider_config);
    let Some(default_obj) = default_provider.as_object_mut() else {
        return;
    };
    for (key, value) in top_level {
        default_obj.insert(key, value);
    }
}

fn lift_model_routing(config: &mut serde_json::Map<String, serde_json::Value>) {
    let default_model = config
        .get("default_model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let Some(default_model) = default_model else {
        return;
    };
    let routing = config.entry("model_routing").or_insert_with(|| serde_json::json!({}));
    if let Some(routing_obj) = routing.as_object_mut() {
        routing_obj.insert("default".to_string(), serde_json::Value::String(default_model));
    }
}

fn lift_tooling(config: &mut serde_json::Map<String, serde_json::Value>) {
    const KEYS: &[&str] = &["allowed_tool_categories", "allowed_tools", "denied_tools"];
    let lifted: Vec<(String, serde_json::Value)> = KEYS
        .iter()
        .filter_map(|key| config.get(*key).cloned().map(|value| ((*key).to_string(), value)))
        .collect();
    if lifted.is_empty() {
        return;
    }
    let tooling = config.entry("tooling").or_insert_with(|| serde_json::json!({}));
    if let Some(tooling_obj) = tooling.as_object_mut() {
        for (key, value) in lifted {
            tooling_obj.insert(key, value);
        }
    }
}

fn lift_media(config: &mut serde_json::Map<String, serde_json::Value>) {
    let asr_model = non_blank_string(config.get("asr_model"));
    let tts_model = non_blank_string(config.get("tts_model"));
    let tts_voice = non_blank_string(config.get("tts_voice"));
    let realtime_enabled = config.get("realtime_audio_enabled").and_then(Value::as_bool);

    if asr_model.is_none() && tts_model.is_none() && tts_voice.is_none() && realtime_enabled.is_none() {
        return;
    }

    if let Some(model) = asr_model.as_ref() {
        ensure_nested_object(config, &["media", "asr"])
            .insert("model".to_string(), serde_json::Value::String(model.clone()));
        let routing = config.entry("model_routing").or_insert_with(|| serde_json::json!({}));
        if let Some(routing_obj) = routing.as_object_mut() {
            routing_obj.insert(
                "audio_transcription".to_string(),
                serde_json::Value::String(model.clone()),
            );
        }
    }
    if let Some(model) = tts_model.as_ref() {
        ensure_nested_object(config, &["media", "tts"])
            .insert("model".to_string(), serde_json::Value::String(model.clone()));
        let routing = config.entry("model_routing").or_insert_with(|| serde_json::json!({}));
        if let Some(routing_obj) = routing.as_object_mut() {
            routing_obj.insert("audio_speech".to_string(), serde_json::Value::String(model.clone()));
        }
    }
    if let Some(voice) = tts_voice.as_ref() {
        ensure_nested_object(config, &["media", "tts"])
            .insert("voice".to_string(), serde_json::Value::String(voice.clone()));
    }
    if let Some(enabled) = realtime_enabled {
        ensure_nested_object(config, &["media", "realtime"])
            .insert("enabled".to_string(), serde_json::Value::Bool(enabled));
    }
}

fn ensure_nested_object<'a>(
    config: &'a mut serde_json::Map<String, serde_json::Value>,
    path: &[&str],
) -> &'a mut serde_json::Map<String, serde_json::Value> {
    let mut current = config;
    for key in path {
        let entry = current
            .entry((*key).to_string())
            .or_insert_with(|| serde_json::json!({}));
        if !entry.is_object() {
            *entry = serde_json::json!({});
        }
        current = entry.as_object_mut().expect("object just inserted");
    }
    current
}

fn non_blank_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

pub(super) fn provider_string<'a>(
    config: &'a serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Option<&'a str> {
    config
        .get("providers")
        .and_then(|providers| providers.get("default"))
        .and_then(|provider| provider.get(field))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(super) fn provider_number(config: &serde_json::Map<String, serde_json::Value>, field: &str) -> Option<f64> {
    let as_number = |value: &serde_json::Value| value.as_f64().or_else(|| value.as_i64().map(|n| n as f64));
    config
        .get("providers")
        .and_then(|providers| providers.get("default"))
        .and_then(|provider| provider.get(field))
        .and_then(as_number)
}

pub(super) fn routed_model<'a>(config: &'a serde_json::Map<String, serde_json::Value>, field: &str) -> Option<&'a str> {
    config
        .get("model_routing")
        .and_then(|routing| routing.get(field))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(super) fn has_api_key(config: &serde_json::Value) -> bool {
    config
        .pointer("/providers/default/api_key")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
}

pub(super) fn default_entity_config() -> serde_json::Map<String, serde_json::Value> {
    serde_json::json!({
        "config_version": 2,
        "providers": {
            "default": default_provider_config()
        },
        "model_routing": {
            "default": "gpt-4o",
            "chat": "gpt-4o"
        },
        "models": {},
        "temperature": 0.7,
        "max_tokens": 2000,
        "top_p": 0.95,
        "frequency_penalty": 0.0,
        "presence_penalty": 0.0,
        "cost_per_1k_input": 0.0015,
        "cost_per_1k_output": 0.002,
        "context_window": 128000,
        "tools": {
            "timeout": 30,
            "max_retries": 3,
            "retry_delay": 1.0,
            "auto_execute": true
        },
        "monitoring": {
            "metrics_enabled": true,
            "trace_enabled": false,
            "log_prompts": false,
            "mask_sensitive": true
        },
        "media": {
            "asr": {
                "model": "whisper-1",
                "language": null,
                "response_format": "json"
            },
            "tts": {
                "model": "tts-1",
                "voice": "alloy",
                "response_format": "mp3",
                "speed": 1.0
            },
            "realtime": {
                "enabled": false,
                "provider": null,
                "asr_streaming_model": null,
                "tts_streaming_model": null,
                "input_sample_rate_hz": 16000,
                "output_sample_rate_hz": 24000,
                "chunk_ms": 40,
                "latency_budget_ms": 800,
                "vad_enabled": true,
                "barge_in_enabled": true,
                "max_session_seconds": 300,
                "max_frame_bytes": 65536
            }
        },
        "endpoints": {
            "chat": null,
            "vision": null,
            "asr": null,
            "tts": null
        },
        "tooling": {
            "enabled": true,
            "max_rounds": 5,
            "allow_parallel": false,
            "allowed_tool_sources": ["builtin"],
            "allowed_tool_categories": ["flight", "flight_event", "todo", "business_case", "media"],
            "allowed_tools": null,
            "denied_tools": [],
            "write_action_policy": "proposal_only"
        },
        "mcp": { "enabled": false, "servers": [] },
        "skills": { "enabled": false, "allowlist": [], "bindings": [] },
        "subagents": { "enabled": false, "allowed_entity_ids": [] },
        "context_policy": {
            "strategy": "hybrid",
            "max_context_tokens": 64000,
            "compression_threshold_tokens": 48000,
            "preserve_recent_messages": 12
        },
        "cache_policy": { "enabled": true },
        "security": { "mask_sensitive": true, "log_prompts": false },
        "system_prompt": "你是一个航班监控系统的AI助手，可以帮助用户查询航班信息、管理航班事件和待办事项。",
        "task_template": null
    })
    .as_object()
    .cloned()
    .unwrap_or_default()
}

fn default_entity_metrics() -> serde_json::Value {
    serde_json::json!({
        "requests": 0,
        "errors": 0,
        "total_tokens": 0,
        "total_cost": 0.0,
    })
}

pub(super) fn metrics_to_value(metrics: &EntityRuntimeMetrics) -> serde_json::Value {
    if metrics.requests == 0 && metrics.errors == 0 && metrics.total_tokens == 0 && metrics.total_cost == 0.0 {
        return default_entity_metrics();
    }

    serde_json::json!({
        "requests": metrics.requests,
        "errors": metrics.errors,
        "total_tokens": metrics.total_tokens,
        "total_cost": metrics.total_cost,
    })
}

pub(super) fn merged_entity_config(value: &serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    let mut merged = default_entity_config();
    if let Some(object) = value.as_object() {
        merge_objects(&mut merged, object.clone());
    }
    canonicalize_entity_document(&mut merged);
    merged
}

pub(super) fn merge_objects(
    target: &mut serde_json::Map<String, serde_json::Value>,
    source: serde_json::Map<String, serde_json::Value>,
) {
    for (key, value) in source {
        target.insert(key, value);
    }
}

pub(super) fn mask_config(mut value: serde_json::Value) -> serde_json::Value {
    sanitize_sensitive(value.as_object_mut());
    value
}

pub(super) fn remove_api_key(value: serde_json::Value) -> serde_json::Value {
    let mut value = value;
    redact_sensitive(value.as_object_mut());
    value
}

fn is_sensitive_key(key: &str) -> bool {
    matches!(
        key.trim().to_ascii_lowercase().as_str(),
        "api_key" | "apikey" | "authorization" | "secret" | "client_secret" | "password" | "passwd" | "pwd"
    )
}

fn sanitize_sensitive(value: Option<&mut serde_json::Map<String, serde_json::Value>>) {
    let Some(object) = value else {
        return;
    };
    for (key, entry) in object.iter_mut() {
        if is_sensitive_key(key) {
            mask_sensitive_entry(entry);
        } else {
            sanitize_value_for_mask(entry);
        }
    }
}

fn sanitize_value_for_mask(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => sanitize_sensitive(Some(object)),
        serde_json::Value::Array(items) => {
            for item in items {
                sanitize_value_for_mask(item);
            }
        }
        _ => {}
    }
}

fn mask_sensitive_entry(entry: &mut serde_json::Value) {
    let Some(key) = entry.as_str().map(str::trim) else {
        *entry = serde_json::Value::String("***".to_string());
        return;
    };
    let masked = mask_secret_string(key);
    *entry = serde_json::Value::String(masked);
}

fn mask_secret_string(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    if value.chars().count() > 12 {
        let head: String = value.chars().take(8).collect();
        let tail: String = value
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("{head}...{tail}")
    } else {
        "***".to_string()
    }
}

fn redact_sensitive(value: Option<&mut serde_json::Map<String, serde_json::Value>>) {
    let Some(object) = value else {
        return;
    };
    let sensitive_keys: Vec<String> = object.keys().filter(|key| is_sensitive_key(key)).cloned().collect();
    for key in sensitive_keys {
        object.remove(&key);
    }
    for entry in object.values_mut() {
        redact_value(entry);
    }
}

fn redact_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => redact_sensitive(Some(object)),
        serde_json::Value::Array(items) => {
            for item in items {
                redact_value(item);
            }
        }
        _ => {}
    }
}
