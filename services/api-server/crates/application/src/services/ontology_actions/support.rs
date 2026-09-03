use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};

use fms_domain::ontology::schema_export::FLIGHT_OPS_ONTOLOGY_VERSION;

use super::error::OntologyActionError;

pub const CANDIDATE_STANDS_SCANNED: i64 = 20;
pub const SEARCH_LIMIT_MAX: i64 = 200;
pub const SEARCH_LIMIT_DEFAULT: i64 = 50;
pub const ANOMALY_LIMIT_DEFAULT: i64 = 50;
pub const ALTERNATIVE_STAND_SUGGESTIONS_MAX: usize = 5;
pub const ALTERNATIVE_STAND_CANDIDATES_SCANNED: i64 = 20;
pub const BRIEFING_UPCOMING_TASKS_MAX: usize = 10;
const SUGGESTION_TTL_MINUTES: i64 = 30;

pub fn arg_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

pub fn required_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, OntologyActionError> {
    arg_str(args, key)
        .ok_or_else(|| OntologyActionError::InvalidArguments(format!("missing required argument `{key}`")))
}

pub fn arg_datetime(args: &Value, key: &str) -> Result<Option<DateTime<Utc>>, OntologyActionError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(raw)) => raw
            .parse::<DateTime<Utc>>()
            .map(Some)
            .map_err(|_| OntologyActionError::InvalidArguments(format!("`{key}` is not an RFC3339 datetime"))),
        Some(_) => Err(OntologyActionError::InvalidArguments(format!(
            "`{key}` must be an RFC3339 datetime string"
        ))),
    }
}

pub fn constraint(name: &str, passed: bool, severity: &str, message: Option<&str>) -> Value {
    json!({
        "constraint_name": name,
        "constraint_type": "Precondition",
        "passed": passed,
        "severity": severity,
        "message": message,
    })
}

pub fn evidence(query_params: Option<Value>) -> Value {
    let mut evidence = serde_json::Map::new();
    evidence.insert("retrieved_at".to_string(), json!(Utc::now()));
    evidence.insert("ontology_version".to_string(), json!(FLIGHT_OPS_ONTOLOGY_VERSION));
    if let Some(params) = query_params {
        evidence.insert("query_params".to_string(), params);
    }
    Value::Object(evidence)
}

fn suggestion_evidence() -> Value {
    json!({
        "retrieved_at": Utc::now(),
        "ontology_version": FLIGHT_OPS_ONTOLOGY_VERSION,
        "context": {},
    })
}

/// Proposal payload for an advisory action. Nothing is persisted or executed here.
#[allow(clippy::too_many_arguments)]
pub fn suggestion_envelope(
    object_type: &str,
    object_id: &str,
    action_name: &str,
    arguments: Value,
    risk_level: &str,
    constraint_results: Vec<Value>,
    before_snapshot: Value,
    after_preview: Value,
    confidence: f64,
    reasoning: &str,
    extra: Value,
) -> Value {
    let now = Utc::now();
    let mut payload = json!({
        "suggestion": {
            "ontology_version": FLIGHT_OPS_ONTOLOGY_VERSION,
            "object_type": object_type,
            "object_id": object_id,
            "action_name": action_name,
            "arguments": arguments,
            "risk_level": risk_level,
            "approval_policy": "require_approval",
            "constraint_results": constraint_results,
            "before_snapshot": before_snapshot,
            "after_preview": after_preview,
            "confidence": confidence,
            "reasoning": reasoning,
            "expires_at": now + Duration::minutes(SUGGESTION_TTL_MINUTES),
        },
        "evidence": suggestion_evidence(),
    });
    if let (Some(root), Some(extra)) = (payload.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            root.insert(key.clone(), value.clone());
        }
    }
    payload
}
