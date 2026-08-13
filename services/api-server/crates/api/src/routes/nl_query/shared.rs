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

pub(crate) fn bind_conversation_id(
    body: &NLQueryRequest,
    envelope: &mut fms_domain::models::ai_context_envelope::ContextEnvelope,
) -> String {
    let conversation_id = body
        .conversation_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| envelope.correlation_id.clone());
    envelope.correlation_id = conversation_id.clone();
    conversation_id
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

#[cfg(test)]
mod tests {
    use super::*;
    use fms_domain::models::ai_context_envelope::*;

    fn envelope() -> ContextEnvelope {
        ContextEnvelope {
            contract_version: "ai-runtime.v1".into(),
            job_id: "job-1".into(),
            run_id: "run-1".into(),
            correlation_id: "generated-correlation".into(),
            requester: EnvelopeRequester {
                user_id: "user-1".into(),
                roles: vec![],
                department_id: None,
                permission_version: None,
            },
            ontology: EnvelopeOntology {
                version: "flight-ops.v1".into(),
                allowed_object_types: vec![],
                allowed_actions: vec![],
                risk_ceiling: "medium".into(),
            },
            context: EnvelopeContext {
                objects: vec![],
                relations: vec![],
                evidence: vec![],
                limits: EnvelopeLimits {
                    max_objects: 100,
                    max_tokens: 12000,
                    redaction: "standard".into(),
                },
            },
            task: EnvelopeTask {
                task_type: "nl_query".into(),
                user_message: "status".into(),
            },
        }
    }

    #[test]
    fn bind_conversation_id_uses_client_id_as_runtime_cache_key() {
        let request = NLQueryRequest {
            question: "status".into(),
            conversation_id: Some(" conversation-1 ".into()),
            context: None,
            streaming: None,
            async_mode: None,
        };
        let mut envelope = envelope();

        assert_eq!(bind_conversation_id(&request, &mut envelope), "conversation-1");
        assert_eq!(envelope.correlation_id, "conversation-1");
    }

    #[test]
    fn bind_conversation_id_preserves_generated_id_for_first_turn() {
        let request = NLQueryRequest {
            question: "status".into(),
            conversation_id: None,
            context: None,
            streaming: None,
            async_mode: None,
        };
        let mut envelope = envelope();

        assert_eq!(bind_conversation_id(&request, &mut envelope), "generated-correlation");
    }
}
