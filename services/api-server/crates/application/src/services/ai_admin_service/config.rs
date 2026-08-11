use serde_json::Value;

#[derive(Debug, Clone, Default)]
pub(super) struct EntityRuntimeMetrics {
    pub(super) requests: u64,
    pub(super) errors: u64,
    pub(super) total_tokens: u64,
    pub(super) total_cost: f64,
}

pub(super) fn default_entity_config() -> serde_json::Map<String, serde_json::Value> {
    serde_json::json!({
        "api_key": "",
        "base_url": "https://api.openai.com/v1",
        "default_model": "gpt-3.5-turbo",
        "asr_model": "whisper-1",
        "tts_model": "tts-1",
        "tts_voice": "alloy",
        "api_format": "chat_completions",
        "temperature": 0.7,
        "max_tokens": 2000,
        "top_p": 0.95,
        "frequency_penalty": 0.0,
        "presence_penalty": 0.0,
        "timeout": 30.0,
        "max_retries": 3,
        "retry_delay": 0.5,
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
        "allowed_tool_categories": ["flight", "flight_event", "todo", "business_case", "media"],
        "allowed_tools": null,
        "denied_tools": [],
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
