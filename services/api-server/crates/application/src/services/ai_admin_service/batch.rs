#[derive(Debug, Clone)]
pub(super) struct BatchExecutionConfig {
    pub(super) base_url: String,
    pub(super) api_key: String,
    pub(super) default_model: String,
    pub(super) api_format: String,
    pub(super) temperature: Option<f64>,
    pub(super) max_tokens: Option<u64>,
    pub(super) timeout_seconds: u64,
    pub(super) max_retries: usize,
    pub(super) retry_delay_seconds: f64,
    pub(super) cost_per_1k_input: f64,
    pub(super) cost_per_1k_output: f64,
}

#[derive(Debug, Clone, Default)]
pub(super) struct BatchUsage {
    pub(super) input_tokens: u64,
    pub(super) output_tokens: u64,
    pub(super) total_tokens: u64,
}

pub(super) fn normalize_api_format(api_format: &str) -> &'static str {
    if api_format.trim().eq_ignore_ascii_case("responses") {
        "responses"
    } else {
        "chat_completions"
    }
}

pub(super) fn build_batch_request_payload(config: &BatchExecutionConfig, user_content: &str) -> serde_json::Value {
    if config.api_format == "responses" {
        let mut payload = serde_json::json!({
            "model": config.default_model,
            "input": [{"role": "user", "content": user_content}],
            "stream": false,
        });
        if let Some(temperature) = config.temperature {
            payload["temperature"] = serde_json::json!(temperature);
        }
        if let Some(max_tokens) = config.max_tokens {
            payload["max_output_tokens"] = serde_json::json!(max_tokens);
        }
        payload
    } else {
        let mut payload = serde_json::json!({
            "model": config.default_model,
            "messages": [
                {"role": "user", "content": user_content}
            ],
            "stream": false,
        });
        if let Some(temperature) = config.temperature {
            payload["temperature"] = serde_json::json!(temperature);
        }
        if let Some(max_tokens) = config.max_tokens {
            payload["max_tokens"] = serde_json::json!(max_tokens);
        }
        payload
    }
}

pub(super) fn extract_batch_response_text(response: &serde_json::Value) -> String {
    if let Some(text) = response
        .get("output_text")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return text.to_string();
    }

    if let Some(items) = response.get("output").and_then(serde_json::Value::as_array) {
        let mut parts = Vec::new();
        for item in items {
            if item.get("type").and_then(serde_json::Value::as_str) != Some("message") {
                continue;
            }
            let Some(content) = item.get("content") else {
                continue;
            };
            if let Some(text) = content.as_str().map(str::trim).filter(|value| !value.is_empty()) {
                parts.push(text.to_string());
                continue;
            }
            let Some(content_parts) = content.as_array() else {
                continue;
            };
            for part in content_parts {
                if let Some(text) = part
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    parts.push(text.to_string());
                    continue;
                }
                if let Some(text) = part.as_str().map(str::trim).filter(|value| !value.is_empty()) {
                    parts.push(text.to_string());
                }
            }
        }
        if !parts.is_empty() {
            return parts.join("\n");
        }
    }

    response
        .get("choices")
        .and_then(serde_json::Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub(super) fn extract_batch_usage(response: &serde_json::Value) -> BatchUsage {
    let Some(usage) = response.get("usage") else {
        return BatchUsage::default();
    };

    let input_tokens = usage
        .get("input_tokens")
        .or_else(|| usage.get("prompt_tokens"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let output_tokens = usage
        .get("output_tokens")
        .or_else(|| usage.get("completion_tokens"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let total_tokens = usage
        .get("total_tokens")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(input_tokens + output_tokens);

    BatchUsage {
        input_tokens,
        output_tokens,
        total_tokens,
    }
}
