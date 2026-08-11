use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::middleware::jwt::JwtAuth;

pub(crate) fn current_user_id(claims: &JwtAuth) -> String {
    claims
        .0
        .sub
        .clone()
        .or_else(|| claims.0.username.clone())
        .unwrap_or_else(|| "unknown_user".to_string())
}

pub(crate) fn target_objects_from_request(body: &NLQueryRequest) -> Vec<(String, String)> {
    let mut targets = Vec::new();
    if let Some(context) = &body.context {
        if let Some(flight_id) = context
            .get("selected_flight_id")
            .or_else(|| context.get("flight_id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            push_target(&mut targets, "Flight", flight_id);
        }

        if let Some(target_object) = context.get("target_object").and_then(Value::as_object) {
            if let (Some(object_type), Some(object_id)) = (
                target_object.get("object_type").and_then(Value::as_str),
                target_object.get("object_id").and_then(Value::as_str),
            ) {
                push_target(&mut targets, object_type, object_id);
            }
        }

        if let Some(items) = context.get("target_objects").and_then(Value::as_array) {
            for item in items {
                if let (Some(object_type), Some(object_id)) = (
                    item.get("object_type").and_then(Value::as_str),
                    item.get("object_id").and_then(Value::as_str),
                ) {
                    push_target(&mut targets, object_type, object_id);
                }
            }
        }
    }

    for token in body
        .question
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .map(str::trim)
        .filter(|token| token.len() >= 4 && token.len() <= 12)
    {
        let upper = token.to_ascii_uppercase();
        let has_digit = upper.chars().any(|ch| ch.is_ascii_digit());
        let has_alpha = upper.chars().any(|ch| ch.is_ascii_alphabetic());
        if has_digit && has_alpha && (upper.starts_with("FL") || upper.len() >= 5) {
            push_target(&mut targets, "Flight", &upper);
        }
    }

    targets
}

fn push_target(targets: &mut Vec<(String, String)>, object_type: &str, object_id: &str) {
    let object_type = object_type.trim();
    let object_id = object_id.trim();
    if object_type.is_empty() || object_id.is_empty() {
        return;
    }
    let candidate = (object_type.to_string(), object_id.to_string());
    if !targets.iter().any(|item| item == &candidate) {
        targets.push(candidate);
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct NLQueryRequest {
    pub question: String,
    #[serde(default)]
    pub conversation_id: Option<String>,
    #[serde(default)]
    pub context: Option<Value>,
    #[serde(default)]
    pub streaming: Option<bool>,
    /// When `true`, return 202 Accepted immediately and let the Python
    /// worker process the job asynchronously (ADR-0004 async path).
    /// The client receives the result via SSE or polls GET /api/v2/ai/jobs/{job_id}.
    #[serde(default)]
    pub async_mode: Option<bool>,
}
