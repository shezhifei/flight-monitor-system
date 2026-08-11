use std::collections::{HashMap, HashSet};

use reqwest::StatusCode;
use serde_json::{json, Map, Value};

use super::error::LLMEvalServiceError;
use super::service::LLMEvalService;
use super::types::{
    ArgExpectation, EvalCaseDefinition, RawArgExpectation, RawCaseDefinition, RawCaseFile, RuntimeProfile,
};
use crate::schemas::llm_eval_schemas::{EvalProfileRequest, EvalRunOptionsRequest};

impl LLMEvalService {
    pub(crate) fn normalize_profiles(
        &self,
        profiles: Vec<EvalProfileRequest>,
    ) -> Result<Vec<RuntimeProfile>, LLMEvalServiceError> {
        let mut normalized = Vec::with_capacity(profiles.len());
        let mut seen_ids = HashSet::new();
        for (index, profile) in profiles.into_iter().enumerate() {
            let base_url = required_text(profile.base_url, "base_url cannot be empty")?;
            let api_key = required_text(profile.api_key, "api_key cannot be empty")?;
            let model = required_text(profile.model, "model cannot be empty")?;
            let profile_id =
                optional_text(profile.profile_id).unwrap_or_else(|| format!("profile_{}", ulid::Ulid::new()));
            if !seen_ids.insert(profile_id.clone()) {
                return Err(LLMEvalServiceError::Validation(format!(
                    "duplicate profile_id: {profile_id}"
                )));
            }
            let api_mode = optional_text(profile.api_mode)
                .unwrap_or_else(|| "chat".to_string())
                .to_lowercase();
            if !matches!(api_mode.as_str(), "chat" | "responses") {
                return Err(LLMEvalServiceError::Validation(
                    "api_mode must be one of: chat, responses".to_string(),
                ));
            }
            let reasoning_effort = optional_text(profile.reasoning_effort).map(|value| value.to_lowercase());
            if let Some(value) = reasoning_effort.as_deref() {
                if !matches!(value, "low" | "medium" | "high") {
                    return Err(LLMEvalServiceError::Validation(
                        "reasoning_effort must be one of: low, medium, high".to_string(),
                    ));
                }
            }
            let reasoning_summary = optional_text(profile.reasoning_summary).map(|value| value.to_lowercase());
            if let Some(value) = reasoning_summary.as_deref() {
                if !matches!(value, "auto" | "concise" | "detailed") {
                    return Err(LLMEvalServiceError::Validation(
                        "reasoning_summary must be one of: auto, concise, detailed".to_string(),
                    ));
                }
            }
            if let Some(max_completion_tokens) = profile.max_completion_tokens {
                if !(1..=65_536).contains(&max_completion_tokens) {
                    return Err(LLMEvalServiceError::Validation(
                        "max_completion_tokens must be between 1 and 65536".to_string(),
                    ));
                }
            }

            normalized.push(RuntimeProfile {
                profile_id,
                name: optional_text(profile.name).unwrap_or_else(|| format!("Profile {}", index + 1)),
                base_url,
                api_key,
                model,
                timeout: profile.timeout.clamp(1.0, 300.0),
                max_retries: profile.max_retries.clamp(0, 10),
                retry_delay: profile.retry_delay.clamp(0.0, 20.0),
                reasoning_effort,
                max_completion_tokens: profile.max_completion_tokens,
                api_mode,
                instructions: optional_text(profile.instructions),
                reasoning_summary,
                store: profile.store,
                include: profile.include.filter(|items| !items.is_empty()),
            });
        }
        Ok(normalized)
    }

    pub(crate) fn normalize_options(
        &self,
        options: EvalRunOptionsRequest,
    ) -> Result<EvalRunOptionsRequest, LLMEvalServiceError> {
        let suite = required_text(options.suite, "suite cannot be empty")?.to_lowercase();
        if !matches!(suite.as_str(), "quick" | "standard" | "full" | "reasoning" | "text2sql") {
            return Err(LLMEvalServiceError::Validation(
                "suite must be one of: quick, standard, full, reasoning, text2sql".to_string(),
            ));
        }
        Ok(EvalRunOptionsRequest {
            suite,
            repeat_count: options.repeat_count.clamp(1, 10),
            profile_concurrency: options.profile_concurrency.clamp(1, 10),
            case_concurrency: options.case_concurrency.clamp(1, 16),
            enable_tool_routing: options.enable_tool_routing,
        })
    }

    pub(crate) fn build_suite(&self, suite: &str) -> Result<Vec<EvalCaseDefinition>, LLMEvalServiceError> {
        let library = self.load_case_library()?;
        Ok(library
            .into_iter()
            .filter(|case| case.suites.iter().any(|item| item == suite))
            .collect())
    }

    pub(crate) fn load_case_library(&self) -> Result<Vec<EvalCaseDefinition>, LLMEvalServiceError> {
        let raw = std::fs::read_to_string(&self.cases_file_path).map_err(|error| {
            LLMEvalServiceError::Internal(format!(
                "failed to read cases file {}: {error}",
                self.cases_file_path.display()
            ))
        })?;
        let file: RawCaseFile = serde_yaml::from_str(&raw).map_err(|error| {
            LLMEvalServiceError::Internal(format!(
                "failed to parse cases file {}: {error}",
                self.cases_file_path.display()
            ))
        })?;
        file.cases
            .into_iter()
            .map(|raw_case| self.parse_case_definition(raw_case))
            .collect()
    }

    pub(crate) fn parse_case_definition(
        &self,
        raw_case: RawCaseDefinition,
    ) -> Result<EvalCaseDefinition, LLMEvalServiceError> {
        let case_id = required_text(raw_case.case_id, "case_id cannot be empty")?;
        let prompt = required_text(raw_case.prompt, "prompt cannot be empty")?;
        let expected_behavior = optional_text(raw_case.expected_behavior).unwrap_or_else(|| "tool_call".to_string());
        let expected_tools = raw_case
            .expected_tools
            .into_iter()
            .filter_map(|value| optional_text(Some(value)))
            .collect::<Vec<_>>();
        if expected_tools.is_empty() && expected_behavior != "fallback" {
            return Err(LLMEvalServiceError::Validation(format!(
                "case[{case_id}] expected_tools cannot be empty unless behavior is fallback"
            )));
        }
        let expectations = raw_case
            .expectations
            .into_iter()
            .map(|item| self.parse_expectation(&case_id, item))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(EvalCaseDefinition {
            case_id,
            prompt,
            expected_tools,
            expectations,
            tags: raw_case
                .tags
                .into_iter()
                .filter_map(|value| optional_text(Some(value)))
                .collect(),
            suites: raw_case
                .suites
                .into_iter()
                .filter_map(|value| optional_text(Some(value)))
                .map(|value| value.to_lowercase())
                .collect(),
            eval_type: optional_text(raw_case.eval_type).unwrap_or_else(|| "tool_routing".to_string()),
            expected_behavior,
        })
    }

    pub(crate) fn parse_expectation(
        &self,
        case_id: &str,
        raw_expectation: RawArgExpectation,
    ) -> Result<ArgExpectation, LLMEvalServiceError> {
        let key = required_text(raw_expectation.key, "expectation key cannot be empty")?;
        let one_of = match raw_expectation.one_of {
            Some(Value::Array(values)) => Some(values),
            Some(value) => Some(vec![value]),
            None => None,
        };
        if key.is_empty() {
            return Err(LLMEvalServiceError::Validation(format!(
                "case[{case_id}] expectation missing key"
            )));
        }
        Ok(ArgExpectation {
            key,
            required: raw_expectation.required.unwrap_or(true),
            expected: raw_expectation.expected,
            contains: raw_expectation.contains,
            one_of,
            min_value: raw_expectation.min_value,
        })
    }
}

// ---------------------------------------------------------------------------
// Free helper functions
// ---------------------------------------------------------------------------

pub(crate) fn retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

pub(crate) fn build_reasoning_block(profile: &RuntimeProfile) -> Value {
    let mut reasoning = Map::new();
    if let Some(effort) = profile.reasoning_effort.as_ref() {
        reasoning.insert("effort".to_string(), Value::String(effort.clone()));
    }
    if let Some(summary) = profile.reasoning_summary.as_ref() {
        reasoning.insert("summary".to_string(), Value::String(summary.clone()));
    }
    if reasoning.is_empty() {
        Value::Null
    } else {
        Value::Object(reasoning)
    }
}

pub(crate) fn required_text(value: String, message: &str) -> Result<String, LLMEvalServiceError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(LLMEvalServiceError::Validation(message.to_string()));
    }
    Ok(value)
}

pub(crate) fn optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|item| {
        let trimmed = item.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

pub(crate) fn has_value(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(text) => !text.trim().is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
        _ => true,
    }
}

pub(crate) fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        _ => value.to_string(),
    }
}

pub(crate) fn value_to_upper(value: &Value) -> String {
    value_to_string(value).to_uppercase()
}

pub(crate) fn value_bool(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

pub(crate) fn value_f64(value: &Value, key: &str) -> f64 {
    value.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

pub(crate) fn value_str<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

pub(crate) fn round_f64(value: f64, decimals: i32) -> f64 {
    let factor = 10_f64.powi(decimals.max(0));
    (value * factor).round() / factor
}

pub(crate) fn mean_f64(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

pub(crate) fn mean_i64(values: &[i64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<i64>() as f64 / values.len() as f64
    }
}

pub(crate) fn percentile_i64(values: &[i64], pct: i32) -> i64 {
    if values.is_empty() {
        return 0;
    }
    let mut ordered = values.to_vec();
    ordered.sort_unstable();
    if ordered.len() == 1 {
        return ordered[0];
    }
    let rank = pct.clamp(0, 100) as f64 / 100.0 * (ordered.len() - 1) as f64;
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    if lower == upper {
        return ordered[lower];
    }
    let ratio = rank - lower as f64;
    (ordered[lower] as f64 + ((ordered[upper] - ordered[lower]) as f64 * ratio)).round() as i64
}

pub(crate) fn mask_api_key(value: &str) -> String {
    let raw = value.trim();
    if raw.is_empty() {
        return String::new();
    }
    if raw.len() <= 10 {
        return "*".repeat(raw.len());
    }
    format!("{}{}{}", &raw[..4], "*".repeat(raw.len() - 8), &raw[raw.len() - 4..])
}

pub(crate) fn truncate_text(text: &str, limit: usize) -> String {
    let mut chars = text.chars();
    let truncated = chars.by_ref().take(limit).collect::<String>();
    if chars.next().is_some() {
        truncated
    } else {
        text.to_string()
    }
}

pub(crate) fn parse_json_like_text(content: &str) -> Value {
    let raw = content.trim();
    if raw.is_empty() {
        return json!({});
    }
    if let Ok(parsed) = serde_json::from_str::<Value>(raw) {
        return parsed;
    }
    let Some(start) = raw.find('{') else {
        return json!({});
    };
    let Some(end) = raw.rfind('}') else {
        return json!({});
    };
    if end <= start {
        return json!({});
    }
    serde_json::from_str::<Value>(&raw[start..=end]).unwrap_or_else(|_| json!({}))
}

pub(crate) fn extract_chat_response_content(response: &Value) -> String {
    let content = response
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"));
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

pub(crate) fn extract_chat_tool_response(response: &Value) -> (Option<String>, Value, String) {
    let assistant_content = extract_chat_response_content(response);
    let tool_calls = response
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("tool_calls"))
        .and_then(Value::as_array);
    if let Some(calls) = tool_calls {
        if let Some(first_call) = calls.first() {
            let tool_name = first_call
                .get("function")
                .and_then(|function| function.get("name"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let arguments = first_call
                .get("function")
                .and_then(|function| function.get("arguments"))
                .map(parse_raw_arguments)
                .unwrap_or_else(|| json!({}));
            return (tool_name, arguments, assistant_content);
        }
    }
    let parsed = parse_json_like_text(&assistant_content);
    let tool_name = parsed
        .get("tool")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let arguments = parsed
        .get("arguments")
        .and_then(Value::as_object)
        .cloned()
        .map(Value::Object)
        .unwrap_or_else(|| json!({}));
    (tool_name, arguments, assistant_content)
}

pub(crate) fn extract_chat_usage(response: &Value) -> Value {
    let usage = response.get("usage").cloned().unwrap_or_else(|| json!({}));
    json!({
        "prompt_tokens": usage.get("prompt_tokens").and_then(Value::as_i64).unwrap_or(0),
        "completion_tokens": usage.get("completion_tokens").and_then(Value::as_i64).unwrap_or(0),
        "total_tokens": usage.get("total_tokens").and_then(Value::as_i64).unwrap_or(0),
        "reasoning_tokens": usage.get("reasoning_tokens")
            .and_then(Value::as_i64)
            .or_else(|| usage.get("completion_tokens_details").and_then(|details| details.get("reasoning_tokens")).and_then(Value::as_i64))
            .unwrap_or(0),
    })
}

pub(crate) fn extract_chat_reasoning_content(response: &Value) -> Value {
    response
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("reasoning_content").or_else(|| message.get("reasoning")))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| Value::String(value.to_string()))
        .unwrap_or(Value::Null)
}

pub(crate) fn extract_responses_text(response: &Value) -> String {
    let mut fragments = Vec::new();
    if let Some(output_text) = response.get("output_text").and_then(Value::as_str) {
        fragments.push(output_text.to_string());
    }
    if let Some(outputs) = response.get("output").and_then(Value::as_array) {
        for item in outputs {
            if let Some(text) = item.get("text").and_then(Value::as_str) {
                fragments.push(text.to_string());
            }
            if let Some(content) = item.get("content").and_then(Value::as_array) {
                for part in content {
                    if let Some(text) = part.get("text").and_then(Value::as_str) {
                        fragments.push(text.to_string());
                    }
                }
            }
        }
    }
    fragments.join("\n")
}

pub(crate) fn extract_responses_tool_response(response: &Value) -> (Option<String>, Value, String) {
    let assistant_content = extract_responses_text(response);
    if let Some(outputs) = response.get("output").and_then(Value::as_array) {
        for item in outputs {
            if item.get("type").and_then(Value::as_str) == Some("function_call") {
                let tool_name = item.get("name").and_then(Value::as_str).map(str::to_string);
                let arguments = item
                    .get("arguments")
                    .map(parse_raw_arguments)
                    .unwrap_or_else(|| json!({}));
                return (tool_name, arguments, assistant_content);
            }
        }
    }
    let parsed = parse_json_like_text(&assistant_content);
    let tool_name = parsed
        .get("tool")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let arguments = parsed
        .get("arguments")
        .and_then(Value::as_object)
        .cloned()
        .map(Value::Object)
        .unwrap_or_else(|| json!({}));
    (tool_name, arguments, assistant_content)
}

pub(crate) fn extract_responses_usage(response: &Value) -> Value {
    let usage = response.get("usage").cloned().unwrap_or_else(|| json!({}));
    json!({
        "prompt_tokens": usage.get("input_tokens").and_then(Value::as_i64).unwrap_or(0),
        "completion_tokens": usage.get("output_tokens").and_then(Value::as_i64).unwrap_or(0),
        "total_tokens": usage.get("total_tokens").and_then(Value::as_i64).unwrap_or(0),
        "reasoning_tokens": usage.get("reasoning_tokens")
            .and_then(Value::as_i64)
            .or_else(|| usage.get("output_tokens_details").and_then(|details| details.get("reasoning_tokens")).and_then(Value::as_i64))
            .unwrap_or(0),
    })
}

pub(crate) fn extract_responses_reasoning_content(response: &Value) -> Value {
    if let Some(outputs) = response.get("output").and_then(Value::as_array) {
        let mut fragments = Vec::new();
        for item in outputs {
            if item.get("type").and_then(Value::as_str) == Some("reasoning") {
                if let Some(summary) = item.get("summary").and_then(Value::as_str) {
                    fragments.push(summary.to_string());
                }
                if let Some(text) = item.get("text").and_then(Value::as_str) {
                    fragments.push(text.to_string());
                }
            }
        }
        let joined = fragments.join("\n");
        if !joined.trim().is_empty() {
            return Value::String(joined);
        }
    }
    Value::Null
}

pub(crate) fn parse_raw_arguments(raw: &Value) -> Value {
    match raw {
        Value::Object(_) => raw.clone(),
        Value::String(text) => serde_json::from_str::<Value>(text).unwrap_or_else(|_| json!({})),
        _ => json!({}),
    }
}

pub(crate) fn should_fallback_to_text_mode(error_text: &str) -> bool {
    let message = error_text.to_lowercase();
    [
        "tool",
        "tools",
        "tool_choice",
        "function",
        "functions",
        "function_call",
        "unsupported",
        "not support",
        "not supported",
        "invalid_parameter",
    ]
    .iter()
    .any(|token| message.contains(token))
}

pub(crate) fn error_attempt(latency_ms: i64, error_message: String) -> Value {
    json!({
        "success": false,
        "case_score": 0.0,
        "tool_match": false,
        "arg_required_score": 0.0,
        "arg_value_score": 0.0,
        "observed_tool": Value::Null,
        "observed_arguments": {},
        "signature": "__error__",
        "latency_ms": latency_ms,
        "error_message": error_message,
        "fallback_used": false,
        "fallback_reason": Value::Null,
        "usage": { "prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0, "reasoning_tokens": 0 },
        "assistant_excerpt": "",
        "reasoning_content": Value::Null,
    })
}

pub(crate) fn cancelled_attempt(attempt_index: i32) -> Value {
    json!({
        "success": false,
        "case_score": 0.0,
        "tool_match": false,
        "arg_required_score": 0.0,
        "arg_value_score": 0.0,
        "observed_tool": Value::Null,
        "observed_arguments": {},
        "signature": "__cancelled__",
        "latency_ms": 0,
        "error_message": "cancelled",
        "fallback_used": false,
        "fallback_reason": Value::Null,
        "usage": { "prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0, "reasoning_tokens": 0 },
        "assistant_excerpt": "",
        "reasoning_content": Value::Null,
        "attempt_index": attempt_index,
    })
}

pub(crate) fn strip_nulls(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for entry in map.values_mut() {
                strip_nulls(entry);
            }
            map.retain(|_, entry| {
                !matches!(entry, Value::Null) && !matches!(entry, Value::Array(items) if items.is_empty())
            });
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                strip_nulls(item);
            }
        }
        _ => {}
    }
}
