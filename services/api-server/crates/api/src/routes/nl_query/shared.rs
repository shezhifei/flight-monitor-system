use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use fms_application::services::ai_job_service::AiJobServiceError;

/// Map `AiJobServiceError` to an API error. Concurrency-limit rejections
/// surface as 409 Conflict (carrying `concurrency_limit_exceeded`, the
/// scope and current/limit values) instead of a generic 500.
pub(crate) fn map_job_error(err: AiJobServiceError) -> ApiError {
    match err {
        AiJobServiceError::ConcurrencyLimitExceeded { .. } => ApiError::Conflict(err.to_string()),
        other => ApiError::Internal(other.to_string()),
    }
}

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

pub(crate) fn entity_id_from_request(body: &NLQueryRequest) -> Option<&str> {
    body.context
        .as_ref()
        .and_then(|context| {
            context
                .get("entity_id")
                .or_else(|| context.get("entityId"))
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
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
    /// Optional envelope task type (Task I4). Restricted to registered task
    /// templates so page-embedded shells (e.g. the dispatch board assistant)
    /// can run on `dispatch_ops` instead of the default `nl_query` surface.
    #[serde(default)]
    pub task_type: Option<String>,
}

/// Task types a client may pin on an nl-query run. Anything else is a 400:
/// the envelope task type selects the sidecar policy template, so it must
/// never be free-form client input.
pub(crate) const ALLOWED_STREAM_TASK_TYPES: &[&str] = &["nl_query", "query_ops", "anomaly_ops", "dispatch_ops"];

/// Resolve the envelope task type for a stream request. Defaults to
/// `nl_query`; unknown values fail closed with 400.
pub(crate) fn resolve_stream_task_type(body: &NLQueryRequest) -> Result<&'static str, ApiError> {
    let requested = body
        .task_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("nl_query");
    ALLOWED_STREAM_TASK_TYPES
        .iter()
        .find(|allowed| **allowed == requested)
        .copied()
        .ok_or_else(|| {
            ApiError::BadRequest(format!(
                "unsupported task_type '{requested}'; allowed: {}",
                ALLOWED_STREAM_TASK_TYPES.join(", ")
            ))
        })
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

    fn request(task_type: Option<&str>) -> NLQueryRequest {
        NLQueryRequest {
            question: "status".into(),
            conversation_id: None,
            context: None,
            streaming: None,
            async_mode: None,
            task_type: task_type.map(str::to_string),
        }
    }

    #[test]
    fn resolve_stream_task_type_defaults_to_nl_query() {
        assert_eq!(resolve_stream_task_type(&request(None)).unwrap(), "nl_query");
        assert_eq!(resolve_stream_task_type(&request(Some("  "))).unwrap(), "nl_query");
    }

    #[test]
    fn resolve_stream_task_type_accepts_registered_templates() {
        assert_eq!(
            resolve_stream_task_type(&request(Some("dispatch_ops"))).unwrap(),
            "dispatch_ops"
        );
        assert_eq!(
            resolve_stream_task_type(&request(Some(" query_ops "))).unwrap(),
            "query_ops"
        );
        assert_eq!(
            resolve_stream_task_type(&request(Some("anomaly_ops"))).unwrap(),
            "anomaly_ops"
        );
    }

    #[test]
    fn resolve_stream_task_type_rejects_unknown_values() {
        let err = resolve_stream_task_type(&request(Some("god_mode"))).unwrap_err();
        match err {
            ApiError::BadRequest(msg) => assert!(msg.contains("god_mode")),
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[test]
    fn entity_id_from_request_reads_context() {
        let missing = NLQueryRequest {
            question: "status".into(),
            conversation_id: None,
            context: None,
            streaming: None,
            async_mode: None,
            task_type: None,
        };
        assert_eq!(entity_id_from_request(&missing), None);
        let present = NLQueryRequest {
            question: "status".into(),
            conversation_id: None,
            context: Some(serde_json::json!({ "entity_id": " ops-entity " })),
            streaming: None,
            async_mode: None,
            task_type: None,
        };
        assert_eq!(entity_id_from_request(&present), Some("ops-entity"));
    }

    #[test]
    fn bind_conversation_id_uses_client_id_as_runtime_cache_key() {
        let request = NLQueryRequest {
            question: "status".into(),
            conversation_id: Some(" conversation-1 ".into()),
            context: None,
            streaming: None,
            async_mode: None,
            task_type: None,
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
            task_type: None,
        };
        let mut envelope = envelope();

        assert_eq!(bind_conversation_id(&request, &mut envelope), "generated-correlation");
    }
}
