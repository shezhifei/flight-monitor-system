use std::sync::OnceLock;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use regex::Regex;
use serde_json::{json, Value};

use crate::schemas::flight_schemas::FlightResponse;
use crate::schemas::nl_query_schemas::{NLConversationListItemSchema, NLQueryContextSchema};

use super::types::{ConversationMessage, ConversationRecord, NLQueryServiceError, RuntimeQueryEvent};

const CONVERSATION_TTL_HOURS: i64 = 24;
const MAX_CONVERSATIONS: usize = 256;
const MAX_MESSAGES_PER_CONVERSATION: usize = 32;
const MAX_TOOL_MESSAGE_BYTES: usize = 8 * 1024;
const TOOL_MESSAGE_EXCERPT_BYTES: usize = 2 * 1024;
static FLIGHT_NUMBER_REGEX: OnceLock<Regex> = OnceLock::new();

pub(super) fn normalize_user_id(user_id: &str) -> Result<String, NLQueryServiceError> {
    let normalized = user_id.trim();
    if normalized.is_empty() {
        return Err(NLQueryServiceError::Validation("未认证".to_string()));
    }
    Ok(normalized.to_string())
}

pub(super) fn normalize_order(order: &str) -> Result<&str, NLQueryServiceError> {
    match order.trim().to_lowercase().as_str() {
        "" | "desc" => Ok("desc"),
        "asc" => Ok("asc"),
        _ => Err(NLQueryServiceError::Validation(
            "invalid order, expected 'asc' or 'desc'".to_string(),
        )),
    }
}

pub(super) fn to_conversation_item(record: ConversationRecord) -> NLConversationListItemSchema {
    NLConversationListItemSchema {
        conversation_id: record.conversation_id,
        title: record.title,
        status: record.status,
        model: record.model,
        message_count: record.messages.len(),
        total_tokens: 0,
        total_cost: 0.0,
        tags: record.tags,
        created_at: Some(to_timestamp(record.created_at)),
        updated_at: Some(to_timestamp(record.updated_at)),
        last_activity_at: Some(to_timestamp(record.last_activity_at)),
        ended_at: record.ended_at.map(to_timestamp),
    }
}

pub(super) fn to_timestamp(value: DateTime<Utc>) -> f64 {
    value.timestamp_millis() as f64 / 1000.0
}

pub(super) fn build_title(question: &str) -> String {
    let trimmed = question.trim();
    let mut chars = trimmed.chars();
    let collected: String = chars.by_ref().take(24).collect();
    if chars.next().is_some() {
        format!("{collected}...")
    } else {
        collected
    }
}

pub(super) fn build_tags(context: &NLQueryContextSchema) -> Vec<String> {
    let mut tags = Vec::new();
    if let Some(source_page) = context
        .source_page
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        tags.push(source_page.to_string());
    }
    if context.selected_flight_id.is_some() || context.selected_flight_no.is_some() {
        tags.push("flight_focus".to_string());
    }
    tags
}

pub(super) fn context_to_metadata(context: &NLQueryContextSchema) -> Option<Value> {
    let value = serde_json::to_value(context).ok()?;
    if value == Value::Null || value == json!({}) {
        None
    } else {
        Some(value)
    }
}

pub(super) fn content_to_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| item.as_str().map(ToOwned::to_owned))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => value.to_string(),
    }
}

pub(super) fn extract_flight_number(question: &str) -> Option<String> {
    let captures = FLIGHT_NUMBER_REGEX
        .get_or_init(|| {
            Regex::new(r"(?i)([a-z])[\s_-]*([a-z])[\s_-]*(\d[\s_-]*\d[\s_-]*\d(?:[\s_-]*\d){0,2})")
                .expect("flight number regex must compile")
        })
        .captures(question)?;
    let mut flight_number = String::with_capacity(7);
    flight_number.push_str(&captures[1].to_ascii_uppercase());
    flight_number.push_str(&captures[2].to_ascii_uppercase());
    flight_number.extend(captures[3].chars().filter(|ch| ch.is_ascii_digit()));
    Some(flight_number)
}

pub(super) fn resolve_scene_from_context(context: &NLQueryContextSchema) -> String {
    let source_page = context.source_page.as_deref().map(str::trim).unwrap_or_default();
    if source_page.eq_ignore_ascii_case("flight_monitor") {
        "flight_monitor".to_string()
    } else {
        "nl_query".to_string()
    }
}

pub(super) fn split_summary(summary: &str, chunk_size: usize) -> Vec<String> {
    let chars: Vec<char> = summary.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }
    chars
        .chunks(chunk_size.max(1))
        .map(|chunk| chunk.iter().collect())
        .collect()
}

pub(super) fn contains_any(question: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|keyword| question.contains(keyword))
}

pub(super) fn status_matches(flight: &FlightResponse, expected: &[&str]) -> bool {
    let status = flight
        .status
        .as_deref()
        .map(|value| value.trim().to_uppercase())
        .unwrap_or_default();
    expected.iter().any(|keyword| status.contains(keyword))
}

pub(super) fn flight_label(flight: &FlightResponse) -> String {
    format!(
        "{}({})",
        fallback_text(flight.flight_number.as_deref(), "未知航班"),
        fallback_text(flight.status.as_deref(), "未知状态"),
    )
}

pub(super) fn fallback_text(value: Option<&str>, fallback: &str) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

pub(super) fn attach_runtime_metadata(mut structured_data: Value, runtime_event: Option<&RuntimeQueryEvent>) -> Value {
    if let (Some(event), Some(map)) = (runtime_event, structured_data.as_object_mut()) {
        map.insert(
            "runtime".to_string(),
            json!({
                "execution_id": event.execution_id,
                "tool_call_id": event.tool_call_id,
                "tool_name": event.tool_name,
                "status": event.status,
                "duration_ms": event.duration_ms,
                "arguments": event.arguments,
                "result": event.result,
                "result_preview": build_runtime_result_preview(&event.result),
            }),
        );
    }
    structured_data
}

pub(super) fn build_assistant_metadata(
    interpretation: &str,
    visualization_hint: Option<&str>,
    runtime_event: Option<&RuntimeQueryEvent>,
) -> Value {
    json!({
        "interpretation": interpretation,
        "visualization_hint": visualization_hint,
        "runtime": runtime_event.map(|event| json!({
            "execution_id": event.execution_id,
            "tool_call_id": event.tool_call_id,
            "tool_name": event.tool_name,
            "status": event.status,
            "duration_ms": event.duration_ms,
            "arguments": event.arguments,
            "result_preview": build_runtime_result_preview(&event.result),
        })),
    })
}

pub(super) fn build_runtime_result_preview(result: &Value) -> Value {
    match result {
        Value::Object(map) => {
            let item_count = map
                .get("items")
                .and_then(Value::as_array)
                .map(|items| items.len())
                .or_else(|| map.get("count").and_then(Value::as_u64).map(|value| value as usize))
                .or_else(|| map.get("total").and_then(Value::as_u64).map(|value| value as usize));
            json!({
                "kind": map.get("kind").unwrap_or(&Value::Null),
                "item_count": item_count,
                "keys": map.keys().take(8).cloned().collect::<Vec<_>>(),
            })
        }
        Value::Array(items) => json!({
            "kind": "array",
            "item_count": items.len(),
        }),
        Value::String(text) => json!({
            "kind": "text",
            "preview": text.chars().take(120).collect::<String>(),
        }),
        Value::Null => Value::Null,
        other => json!({
            "kind": "scalar",
            "value": other,
        }),
    }
}

pub(super) fn schedule_suffix(flight: &FlightResponse) -> String {
    let estimated_departure = flight
        .estimated_departure
        .map(|value| value.format("%H:%M").to_string());
    let estimated_arrival = flight.estimated_arrival.map(|value| value.format("%H:%M").to_string());

    match (estimated_departure, estimated_arrival) {
        (Some(dep), Some(arr)) => format!("，预计离港 {dep} / 预计到达 {arr}"),
        (Some(dep), None) => format!("，预计离港 {dep}"),
        (None, Some(arr)) => format!("，预计到达 {arr}"),
        (None, None) => String::new(),
    }
}

pub(super) fn cleanup_conversations(conversations: &DashMap<String, ConversationRecord>, now: DateTime<Utc>) {
    conversations.retain(|_, record| now < record.last_activity_at + chrono::Duration::hours(CONVERSATION_TTL_HOURS));
    enforce_conversation_cap(conversations);
}

pub(super) fn enforce_conversation_cap(conversations: &DashMap<String, ConversationRecord>) {
    while conversations.len() > MAX_CONVERSATIONS {
        let oldest_id = conversations
            .iter()
            .min_by_key(|entry| entry.value().last_activity_at)
            .map(|entry| entry.key().clone());
        if let Some(id) = oldest_id {
            conversations.remove(&id);
        } else {
            break;
        }
    }
}

pub(super) fn trim_conversation_messages(messages: &mut Vec<ConversationMessage>) {
    if messages.len() > MAX_MESSAGES_PER_CONVERSATION {
        let overflow = messages.len() - MAX_MESSAGES_PER_CONVERSATION;
        messages.drain(0..overflow);
    }
}

pub(super) fn sanitize_tool_payload(value: &Value) -> Value {
    let serialized = serde_json::to_vec(value).unwrap_or_default();
    if serialized.len() <= MAX_TOOL_MESSAGE_BYTES {
        return value.clone();
    }

    let excerpt = String::from_utf8_lossy(&serialized[..serialized.len().min(TOOL_MESSAGE_EXCERPT_BYTES)]).to_string();
    json!({
        "truncated": true,
        "reason": "tool_payload_too_large",
        "approx_bytes": serialized.len(),
        "preview": build_runtime_result_preview(value),
        "excerpt": excerpt,
    })
}
