use chrono::{DateTime, Utc};
use metrics::{counter, histogram};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use ulid::Ulid;

use super::service::{
    AiRuntimeError, AiToolExecutionSpec, AI_QUERY_MISROUTE_TOTAL_METRIC, AI_QUERY_MISSELECTION_TOTAL_METRIC,
    AI_QUERY_ROUTE_TOTAL_METRIC, AI_QUERY_SELECTION_TOTAL_METRIC, AI_REPORT_SCHEMA_VALIDATION_ERROR_COUNT_METRIC,
    AI_REPORT_SCHEMA_VALIDATION_TOTAL_METRIC, CHAIN_RETENTION_HOURS, DEFAULT_TODO_GRAPH_PILOT_ENTITY_ID,
    EXECUTION_RETENTION_HOURS, MAX_CHAINS, MAX_EXECUTIONS, MAX_PENDING_ACTIONS, METRIC_SAMPLE_WINDOW,
    PENDING_ACTION_RETENTION_MINUTES, READY_COMPLETION_RATE_MIN, READY_DUPLICATE_TOOL_EXECUTION_BLOCKED_TOTAL_MAX,
    READY_DUPLICATE_TOOL_EXECUTION_TOTAL_MAX, READY_GRAPH_FALLBACK_RATE_MAX, READY_GRAPH_REQUESTED_TOTAL_MIN,
    READY_GRAPH_RESUME_SUCCESS_RATE_MIN, READY_GRAPH_RESUME_TOTAL_MIN, READY_STALE_PENDING_TOTAL_MAX,
    ROLLBACK_GRAPH_FALLBACK_RATE_GT, ROLLBACK_GRAPH_REQUESTED_TOTAL_MIN, ROLLBACK_GRAPH_RESUME_SUCCESS_RATE_LT,
    ROLLBACK_GRAPH_RESUME_TOTAL_MIN,
};
use super::types::{ChainRecord, ExecutionRecord, PendingActionRecord};

pub(super) fn ensure_action_open(action: &mut PendingActionRecord, now: DateTime<Utc>) -> Result<(), AiRuntimeError> {
    if action.status == "expired" {
        return Err(AiRuntimeError::conflict(
            "PENDING_ACTION_EXPIRED",
            "pending action expired",
            Some("expired".to_string()),
        ));
    }
    if action.status != "pending" {
        return Err(AiRuntimeError::conflict(
            "PENDING_ACTION_STATE_CONFLICT",
            format!("pending action state invalid: {}", action.status),
            None,
        ));
    }
    if action.expires_at.map(|value| now >= value).unwrap_or(false) {
        action.mark_expired(now);
        return Err(AiRuntimeError::conflict(
            "PENDING_ACTION_EXPIRED",
            "pending action expired",
            Some("expired".to_string()),
        ));
    }
    Ok(())
}

pub(super) fn merge_json_objects(base: Value, patch: Option<Value>) -> Value {
    let mut merged = match base {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    if let Some(Value::Object(patch_map)) = patch {
        for (key, value) in patch_map {
            merged.insert(key, value);
        }
    }
    Value::Object(merged)
}

pub(super) fn build_read_tool_output(
    spec: &AiToolExecutionSpec,
    tool_args: &Value,
    execution_id: &str,
    tool_call_id: &str,
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
) -> Value {
    let query_type = infer_query_type(&spec.tool_name, tool_args);
    let arguments = normalize_object(tool_args.clone());
    let filters = extract_filters(&arguments);
    let matches_estimate = estimate_matches(&arguments, &query_type);
    let summary = format!(
        "{} completed with {} estimated match(es)",
        spec.tool_name, matches_estimate
    );
    json!({
        "tool_call_id": tool_call_id,
        "tool_name": spec.tool_name,
        "category": spec.category,
        "operation_level": spec.operation_level,
        "side_effect": false,
        "invocation_mode": "user_requested",
        "arguments": arguments,
        "output": {
            "query_type": query_type,
            "filters": filters,
            "matches_estimate": matches_estimate,
            "preview": build_result_preview(spec, tool_args),
        },
        "summary": summary,
        "execution": {
            "execution_id": execution_id,
            "tool_call_id": tool_call_id,
            "started_at": started_at.to_rfc3339(),
            "finished_at": finished_at.to_rfc3339(),
            "duration_ms": millis_between(started_at, finished_at),
            "status": "success",
        }
    })
}

pub(super) fn build_write_tool_output(
    tool_name: &str,
    operation_level: &str,
    final_arguments: &Value,
    execution_id: Option<&str>,
    tool_call_id: &str,
    approved_by: &str,
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
) -> Value {
    let arguments = normalize_object(final_arguments.clone());
    json!({
        "tool_call_id": tool_call_id,
        "tool_name": tool_name,
        "operation_level": operation_level,
        "side_effect": true,
        "approved_by": approved_by,
        "arguments": arguments,
        "output": {
            "status": "applied",
            "receipt": "change accepted and execution recorded",
            "changed_fields": object_keys(final_arguments),
        },
        "execution": {
            "execution_id": execution_id,
            "status": "success",
            "started_at": started_at.to_rfc3339(),
            "finished_at": finished_at.to_rfc3339(),
            "duration_ms": millis_between(started_at, finished_at),
        }
    })
}

pub(super) fn build_execution_receipt(
    execution_id: Option<&str>,
    tool_call_id: &str,
    status: &str,
    actor_id: Option<&str>,
    decided_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
) -> Value {
    json!({
        "execution_id": execution_id,
        "tool_call_id": tool_call_id,
        "status": status,
        "actor_id": actor_id,
        "decided_at": decided_at.to_rfc3339(),
        "finished_at": finished_at.to_rfc3339(),
        "duration_ms": millis_between(decided_at, finished_at),
    })
}

pub(super) fn build_result_preview(spec: &AiToolExecutionSpec, tool_args: &Value) -> Value {
    let query_type = infer_query_type(&spec.tool_name, tool_args);
    json!({
        "query_type": query_type,
        "focus": extract_filters(tool_args),
        "summary": format!("{} prepared runtime payload", spec.tool_name),
    })
}

pub(super) fn resolve_query_route_labels(spec: &AiToolExecutionSpec, tool_args: &Value) -> Option<(String, String)> {
    if spec.tool_name.eq_ignore_ascii_case("sql_query_readonly") {
        return None;
    }

    let intent = spec
        .query_intent
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| query_route_label_from_args(tool_args, "intent"))
        .or_else(|| infer_query_intent_label(&spec.tool_name, tool_args));
    let dataset = spec
        .query_dataset
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| query_route_label_from_args(tool_args, "dataset"))
        .or_else(|| infer_query_dataset_label(&spec.tool_name));

    if let (Some(intent), Some(dataset)) = (&intent, &dataset) {
        return Some((intent.clone(), dataset.clone()));
    }

    if spec.category.eq_ignore_ascii_case("query") {
        return Some((
            intent.unwrap_or_else(|| "search".to_string()),
            dataset.unwrap_or_else(|| "flights".to_string()),
        ));
    }

    None
}

pub(super) struct QueryRouteMetricDecision {
    pub(super) intent: String,
    pub(super) dataset: String,
    pub(super) adapter: String,
    pub(super) status: &'static str,
    pub(super) misroute: bool,
    pub(super) reason: &'static str,
}

impl QueryRouteMetricDecision {
    fn success(intent: String, dataset: String, adapter: String) -> Self {
        Self {
            intent,
            dataset,
            adapter,
            status: "success",
            misroute: false,
            reason: "none",
        }
    }

    fn validation_error(intent: String, dataset: String, adapter: String, reason: &'static str) -> Self {
        Self {
            intent,
            dataset,
            adapter,
            status: "validation_error",
            misroute: true,
            reason,
        }
    }
}

pub(super) fn resolve_query_route_metric(
    spec: &AiToolExecutionSpec,
    tool_args: &Value,
) -> Option<QueryRouteMetricDecision> {
    let (intent, dataset) = resolve_query_route_labels(spec, tool_args)?;
    let normalized_intent = intent.trim().to_ascii_lowercase();
    let normalized_dataset = dataset.trim().to_ascii_lowercase();

    if !matches!(normalized_dataset.as_str(), "flights" | "alerts" | "tasks" | "ops") {
        return Some(QueryRouteMetricDecision::validation_error(
            normalized_intent,
            normalized_dataset,
            "none".to_string(),
            "unsupported_dataset",
        ));
    }

    if !is_supported_query_intent(&normalized_dataset, &normalized_intent) {
        return Some(QueryRouteMetricDecision::validation_error(
            normalized_intent,
            normalized_dataset,
            "none".to_string(),
            "unsupported_intent",
        ));
    }

    if !is_supported_query_adapter(&normalized_dataset, &normalized_intent, &spec.tool_name) {
        return Some(QueryRouteMetricDecision::validation_error(
            normalized_intent,
            normalized_dataset,
            spec.tool_name.clone(),
            "dataset_specific_routing_failure",
        ));
    }

    Some(QueryRouteMetricDecision::success(
        normalized_intent,
        normalized_dataset,
        spec.tool_name.clone(),
    ))
}

pub(super) fn is_supported_query_intent(dataset: &str, intent: &str) -> bool {
    match dataset {
        "flights" | "alerts" | "tasks" | "ops" => {
            matches!(intent, "search" | "detail" | "timeseries" | "aggregate" | "compare")
        }
        _ => false,
    }
}

pub(super) fn is_supported_query_adapter(dataset: &str, intent: &str, adapter: &str) -> bool {
    let normalized_adapter = adapter.trim().to_ascii_lowercase();
    match (dataset, intent) {
        ("flights", "search") | ("flights", "detail") => matches!(
            normalized_adapter.as_str(),
            "search_flights_advanced"
                | "get_delayed_flights"
                | "get_flights_by_time_range"
                | "get_abnormal_flights"
                | "get_flight_detail"
                | "get_flight_overview"
                | "get_arrival_flights"
                | "get_departure_flights"
        ),
        ("flights", "timeseries") => normalized_adapter == "get_flights_by_time_range",
        ("flights", "aggregate") | ("flights", "compare") => matches!(
            normalized_adapter.as_str(),
            "count_flights_by_status" | "get_turnaround_stats" | "get_flight_status_summary"
        ),
        ("alerts", "search") | ("alerts", "detail") => {
            matches!(normalized_adapter.as_str(), "get_anomaly_detail" | "list_anomalies")
        }
        ("alerts", "timeseries") => normalized_adapter == "alerts_timeseries",
        ("alerts", "aggregate") | ("alerts", "compare") => normalized_adapter == "get_anomaly_stats",
        ("tasks", "search") | ("tasks", "detail") => {
            matches!(normalized_adapter.as_str(), "get_todo" | "search_todos" | "list_todos")
        }
        ("tasks", "timeseries") => normalized_adapter == "tasks_timeseries",
        ("tasks", "aggregate") | ("tasks", "compare") => normalized_adapter == "get_todo_stats",
        ("ops", "search") | ("ops", "detail") | ("ops", "aggregate") | ("ops", "compare") => {
            normalized_adapter == "ops_snapshot"
        }
        ("ops", "timeseries") => normalized_adapter == "ops_timeseries",
        _ => false,
    }
}

pub(super) fn query_route_label_from_args(tool_args: &Value, key: &str) -> Option<String> {
    tool_args
        .as_object()
        .and_then(|map| map.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn infer_query_intent_label(tool_name: &str, tool_args: &Value) -> Option<String> {
    let normalized = tool_name.to_ascii_lowercase();
    let intent = if normalized.contains("detail") {
        "detail"
    } else if normalized.contains("timeseries") {
        "timeseries"
    } else if normalized.contains("compare") {
        "compare"
    } else if normalized.contains("stats")
        || normalized.contains("summary")
        || normalized.contains("count")
        || has_any_key(tool_args, &["group_by", "metrics"])
    {
        "aggregate"
    } else if normalized.contains("query") {
        return None;
    } else {
        "search"
    };
    Some(intent.to_string())
}

pub(super) fn infer_query_dataset_label(tool_name: &str) -> Option<String> {
    let normalized = tool_name.to_ascii_lowercase();
    let dataset = if normalized.contains("anomaly") || normalized.contains("alert") {
        "alerts"
    } else if normalized.contains("todo") || normalized.contains("task") {
        "tasks"
    } else if normalized.contains("ops") {
        "ops"
    } else if normalized.contains("flight") || normalized.contains("arrival") || normalized.contains("departure") {
        "flights"
    } else {
        return None;
    };
    Some(dataset.to_string())
}

pub(super) fn infer_query_type(tool_name: &str, tool_args: &Value) -> &'static str {
    let normalized = tool_name.to_ascii_lowercase();
    if normalized.contains("detail")
        || has_any_key(
            tool_args,
            &["flight_id", "flight_no", "selected_flight_id", "selected_flight_no"],
        )
    {
        "flight_detail"
    } else if normalized.contains("delay") {
        "delayed_flights"
    } else if normalized.contains("status") || has_any_key(tool_args, &["status", "group_by"]) {
        "status_summary"
    } else if normalized.contains("arrival") {
        "arrival_list"
    } else if normalized.contains("departure") {
        "departure_list"
    } else {
        "flight_overview"
    }
}

pub(super) fn extract_filters(tool_args: &Value) -> Value {
    match tool_args {
        Value::Object(map) => {
            let filtered = map
                .iter()
                .filter(|(_, value)| !value.is_null())
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<Map<String, Value>>();
            Value::Object(filtered)
        }
        _ => Value::Object(Map::new()),
    }
}

pub(super) fn estimate_matches(tool_args: &Value, query_type: &str) -> usize {
    if has_any_key(
        tool_args,
        &["flight_id", "flight_no", "selected_flight_id", "selected_flight_no"],
    ) {
        1
    } else {
        match query_type {
            "status_summary" => 6,
            "delayed_flights" => 8,
            "arrival_list" | "departure_list" => 10,
            _ => 12,
        }
    }
}

pub(super) fn normalize_object(value: Value) -> Value {
    match value {
        Value::Object(_) => value,
        Value::Null => Value::Object(Map::new()),
        other => json!({ "value": other }),
    }
}

pub(super) fn object_keys(value: &Value) -> Vec<String> {
    value
        .as_object()
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default()
}

pub(super) fn has_any_key(value: &Value, keys: &[&str]) -> bool {
    value
        .as_object()
        .map(|map| keys.iter().any(|key| map.contains_key(*key)))
        .unwrap_or(false)
}

pub(super) fn millis_between(start: DateTime<Utc>, end: DateTime<Utc>) -> u64 {
    let delta = end
        .signed_duration_since(start)
        .to_std()
        .unwrap_or(Duration::from_millis(0));
    delta.as_millis() as u64
}

pub(super) fn metric_summary_base(samples: &[f64]) -> Value {
    let avg = if samples.is_empty() {
        0.0
    } else {
        samples.iter().sum::<f64>() / samples.len() as f64
    };
    let p95 = percentile(samples, 95.0);
    json!({
        "count": samples.len(),
        "avg": avg,
        "p95": p95,
    })
}

pub(super) fn first_progress_metric_summary(samples: &[f64], target: f64, violation_total: usize) -> Value {
    let mut summary = metric_summary_base(samples);
    if let Some(map) = summary.as_object_mut() {
        let p95 = map.get("p95").and_then(Value::as_f64).unwrap_or(0.0);
        map.insert("target_p95_lt_ms".to_string(), json!(target));
        map.insert("violation_total".to_string(), json!(violation_total));
        map.insert("target_met".to_string(), json!(!samples.is_empty() && p95 < target));
    }
    summary
}

pub(super) fn event_interval_metric_summary(samples: &[f64], target: f64, violation_total: usize) -> Value {
    let mut summary = metric_summary_base(samples);
    if let Some(map) = summary.as_object_mut() {
        map.insert("target_lte_ms".to_string(), json!(target));
        map.insert("violation_total".to_string(), json!(violation_total));
        map.insert(
            "target_met".to_string(),
            json!(!samples.is_empty() && violation_total == 0),
        );
    }
    summary
}

pub(super) fn percentile(values: &[f64], pct: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut ordered = values.to_vec();
    ordered.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rank = ((pct / 100.0) * (ordered.len().saturating_sub(1) as f64)).round() as usize;
    ordered[rank.min(ordered.len() - 1)]
}

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct GuardrailMetrics {
    pub(super) duplicate_tool_execution_total: i64,
    pub(super) duplicate_tool_execution_runs: i64,
    pub(super) duplicate_tool_execution_blocked_total: i64,
    pub(super) duplicate_tool_execution_blocked_runs: i64,
    pub(super) duplicate_tool_execution_backstop_total: i64,
    pub(super) duplicate_tool_execution_backstop_runs: i64,
}

impl GuardrailMetrics {
    pub(super) fn merge(&mut self, other: Self) {
        self.duplicate_tool_execution_total += other.duplicate_tool_execution_total;
        self.duplicate_tool_execution_runs += other.duplicate_tool_execution_runs;
        self.duplicate_tool_execution_blocked_total += other.duplicate_tool_execution_blocked_total;
        self.duplicate_tool_execution_blocked_runs += other.duplicate_tool_execution_blocked_runs;
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct ToolCallDuplicateMetrics {
    pub(super) total: i64,
    pub(super) runs: i64,
}

pub(super) fn execution_matches_scope(
    execution: &ExecutionRecord,
    entity_id: Option<&str>,
    scoped_todo_ids: &HashSet<String>,
) -> bool {
    let Some(entity_id) = entity_id else {
        return true;
    };
    execution.entity_id.as_deref() == Some(entity_id)
        || execution
            .todo_id
            .as_ref()
            .map(|todo_id| scoped_todo_ids.contains(todo_id))
            .unwrap_or(false)
        || extract_todo_id(&execution.input)
            .map(|todo_id| scoped_todo_ids.contains(todo_id))
            .unwrap_or(false)
        || extract_entity_id(&execution.input).as_deref() == Some(entity_id)
}

pub(super) fn pending_action_matches_scope(
    action: &PendingActionRecord,
    entity_id: Option<&str>,
    scoped_todo_ids: &HashSet<String>,
    execution_index: &HashMap<String, ExecutionRecord>,
) -> bool {
    let Some(entity_id) = entity_id else {
        return true;
    };

    if extract_entity_id(&action.arguments).as_deref() == Some(entity_id) {
        return true;
    }
    if extract_todo_id(&action.arguments)
        .map(|todo_id| scoped_todo_ids.contains(todo_id))
        .unwrap_or(false)
    {
        return true;
    }
    action
        .correlation_id
        .as_ref()
        .and_then(|run_id| execution_index.get(run_id))
        .map(|execution| execution_matches_scope(execution, Some(entity_id), scoped_todo_ids))
        .unwrap_or(false)
}

pub(super) fn extract_todo_id(value: &Value) -> Option<&str> {
    value
        .as_object()
        .and_then(|map| map.get("todo_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(super) fn extract_entity_id(value: &Value) -> Option<String> {
    value
        .as_object()
        .and_then(|map| map.get("entity_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn is_completed_status(status: &str) -> bool {
    matches!(status, "success" | "completed")
}

pub(super) fn is_failed_status(status: &str) -> bool {
    matches!(status, "failed" | "rejected" | "error")
}

pub(super) fn is_pending_status(status: &str) -> bool {
    matches!(status, "pending" | "pending_approval" | "in_progress" | "running")
}

pub(super) fn normalized_runtime_text(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("")
        .to_ascii_lowercase()
}

pub(super) fn ratio(numerator: i64, denominator: i64) -> f64 {
    if denominator <= 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

pub(super) fn execution_duration_ms(execution: &ExecutionRecord) -> Option<f64> {
    execution
        .output
        .pointer("/runtime/duration_ms")
        .and_then(json_number_to_f64)
        .or_else(|| {
            execution
                .output
                .pointer("/metadata/duration_ms")
                .and_then(json_number_to_f64)
        })
        .or_else(|| {
            execution
                .output
                .pointer("/execution/duration_ms")
                .and_then(json_number_to_f64)
        })
        .or_else(|| {
            execution
                .finished_at
                .map(|finished_at| millis_between(execution.started_at, finished_at) as f64)
        })
}

pub(super) fn approval_response_time_ms(action: &PendingActionRecord) -> Option<f64> {
    let decided_at = action.approved_at.or(action.rejected_at).unwrap_or(action.updated_at);
    Some(millis_between(action.created_at, decided_at) as f64)
}

pub(super) fn execution_receipt_resume_mode(action: &PendingActionRecord) -> Option<&str> {
    action
        .execution_receipt
        .as_ref()
        .and_then(|receipt| receipt.get("resume_mode"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(super) fn execution_receipt_status(action: &PendingActionRecord) -> Option<&str> {
    action
        .execution_receipt
        .as_ref()
        .and_then(|receipt| receipt.get("status"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(super) fn extract_guardrail_metrics(execution: &ExecutionRecord) -> GuardrailMetrics {
    let mut metrics = GuardrailMetrics::default();
    let Some(guardrails) = find_first_object_field(&execution.output, "graph_runtime_guardrails") else {
        return metrics;
    };

    metrics.duplicate_tool_execution_total = guardrails
        .get("duplicate_tool_execution_total")
        .and_then(json_number_to_i64)
        .unwrap_or(0);
    metrics.duplicate_tool_execution_blocked_total = guardrails
        .get("duplicate_tool_execution_blocked_total")
        .and_then(json_number_to_i64)
        .unwrap_or(0);
    metrics.duplicate_tool_execution_runs = (metrics.duplicate_tool_execution_total > 0) as i64;
    metrics.duplicate_tool_execution_blocked_runs = (metrics.duplicate_tool_execution_blocked_total > 0) as i64;
    metrics
}

pub(super) fn scan_tool_call_duplicates(value: &Value) -> ToolCallDuplicateMetrics {
    let mut counts = HashMap::<String, i64>::new();
    collect_tool_call_ids(value, &mut counts);
    let total = counts
        .values()
        .filter(|count| **count > 1)
        .map(|count| count - 1)
        .sum::<i64>();
    ToolCallDuplicateMetrics {
        total,
        runs: (total > 0) as i64,
    }
}

pub(super) fn collect_tool_call_ids(value: &Value, counts: &mut HashMap<String, i64>) {
    match value {
        Value::Object(map) => {
            if let Some(Value::Array(tool_calls)) = map.get("tool_calls") {
                for item in tool_calls {
                    if let Some(tool_call_id) = item
                        .get("tool_call_id")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                    {
                        *counts.entry(tool_call_id.to_string()).or_insert(0) += 1;
                    }
                }
            }
            for child in map.values() {
                collect_tool_call_ids(child, counts);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_tool_call_ids(item, counts);
            }
        }
        _ => {}
    }
}

pub(super) fn find_first_object_field<'a>(value: &'a Value, field_name: &str) -> Option<&'a Map<String, Value>> {
    match value {
        Value::Object(map) => {
            if let Some(Value::Object(found)) = map.get(field_name) {
                return Some(found);
            }
            for child in map.values() {
                if let Some(found) = find_first_object_field(child, field_name) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(items) => items.iter().find_map(|item| find_first_object_field(item, field_name)),
        _ => None,
    }
}

pub(super) fn json_number_to_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().map(|number| number as i64))
        .or_else(|| value.as_f64().map(|number| number.round() as i64))
}

pub(super) fn json_number_to_f64(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_i64().map(|number| number as f64))
}

pub(super) fn percentile_summary(samples: &[f64]) -> Value {
    json!({
        "sample_size": samples.len(),
        "p50": percentile_linear(samples, 50.0),
        "p95": percentile_linear(samples, 95.0),
    })
}

pub(super) fn percentile_linear(values: &[f64], pct: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut ordered = values.to_vec();
    ordered.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if ordered.len() == 1 {
        return ordered[0];
    }
    let rank = (pct / 100.0) * ((ordered.len() - 1) as f64);
    let lower_index = rank.floor() as usize;
    let upper_index = rank.ceil() as usize;
    if lower_index == upper_index {
        return ordered[lower_index];
    }
    let weight = rank - lower_index as f64;
    ordered[lower_index] + (ordered[upper_index] - ordered[lower_index]) * weight
}

pub(super) fn todo_graph_pilot_thresholds() -> Value {
    json!({
        "ready_graph_requested_total_min": READY_GRAPH_REQUESTED_TOTAL_MIN,
        "ready_completion_rate_min": READY_COMPLETION_RATE_MIN,
        "ready_graph_fallback_rate_max": READY_GRAPH_FALLBACK_RATE_MAX,
        "ready_graph_resume_total_min": READY_GRAPH_RESUME_TOTAL_MIN,
        "ready_graph_resume_success_rate_min": READY_GRAPH_RESUME_SUCCESS_RATE_MIN,
        "ready_duplicate_tool_execution_total_max": READY_DUPLICATE_TOOL_EXECUTION_TOTAL_MAX,
        "ready_duplicate_tool_execution_blocked_total_max": READY_DUPLICATE_TOOL_EXECUTION_BLOCKED_TOTAL_MAX,
        "ready_stale_pending_total_max": READY_STALE_PENDING_TOTAL_MAX,
        "rollback_graph_requested_total_min": ROLLBACK_GRAPH_REQUESTED_TOTAL_MIN,
        "rollback_graph_fallback_rate_gt": ROLLBACK_GRAPH_FALLBACK_RATE_GT,
        "rollback_graph_resume_total_min": ROLLBACK_GRAPH_RESUME_TOTAL_MIN,
        "rollback_graph_resume_success_rate_lt": ROLLBACK_GRAPH_RESUME_SUCCESS_RATE_LT,
    })
}

pub(super) fn build_todo_graph_pilot_verdict(
    entity_id: Option<&str>,
    graph_requested_total: i64,
    graph_fallback_rate: f64,
    graph_resume_total: i64,
    graph_resume_success_rate: f64,
    completion_rate: f64,
    stale_pending_total: i64,
    duplicate_total: i64,
    duplicate_blocked_total: i64,
) -> Value {
    if entity_id.map(str::trim).filter(|value| !value.is_empty()).is_none() {
        return Value::Null;
    }

    let mut rollback_reasons = Vec::<String>::new();
    if duplicate_total > 0 {
        rollback_reasons.push("duplicate tool execution detected".to_string());
    }
    if graph_requested_total >= ROLLBACK_GRAPH_REQUESTED_TOTAL_MIN
        && graph_fallback_rate > ROLLBACK_GRAPH_FALLBACK_RATE_GT
    {
        rollback_reasons.push("graph fallback rate exceeded rollback threshold".to_string());
    }
    if graph_resume_total >= ROLLBACK_GRAPH_RESUME_TOTAL_MIN
        && graph_resume_success_rate < ROLLBACK_GRAPH_RESUME_SUCCESS_RATE_LT
    {
        rollback_reasons.push("graph resume success rate fell below rollback threshold".to_string());
    }
    if !rollback_reasons.is_empty() {
        return json!({ "status": "rollback_recommended", "reasons": rollback_reasons });
    }

    let mut insufficient_reasons = Vec::<String>::new();
    if graph_requested_total < READY_GRAPH_REQUESTED_TOTAL_MIN {
        insufficient_reasons.push("graph requested sample size below readiness threshold".to_string());
    }
    if graph_resume_total < READY_GRAPH_RESUME_TOTAL_MIN {
        insufficient_reasons.push("graph resume sample size below readiness threshold".to_string());
    }
    if !insufficient_reasons.is_empty() {
        return json!({ "status": "insufficient_data", "reasons": insufficient_reasons });
    }

    let mut hold_reasons = Vec::<String>::new();
    if completion_rate < READY_COMPLETION_RATE_MIN {
        hold_reasons.push("completion rate below readiness threshold".to_string());
    }
    if graph_fallback_rate > READY_GRAPH_FALLBACK_RATE_MAX {
        hold_reasons.push("graph fallback rate above readiness threshold".to_string());
    }
    if graph_resume_success_rate < READY_GRAPH_RESUME_SUCCESS_RATE_MIN {
        hold_reasons.push("graph resume success rate below readiness threshold".to_string());
    }
    if stale_pending_total > READY_STALE_PENDING_TOTAL_MAX {
        hold_reasons.push("stale pending approvals detected".to_string());
    }
    if duplicate_total > READY_DUPLICATE_TOOL_EXECUTION_TOTAL_MAX {
        hold_reasons.push("duplicate tool execution detected".to_string());
    }
    if duplicate_blocked_total > READY_DUPLICATE_TOOL_EXECUTION_BLOCKED_TOTAL_MAX {
        hold_reasons.push("duplicate execution attempts were blocked".to_string());
    }
    if !hold_reasons.is_empty() {
        return json!({ "status": "hold", "reasons": hold_reasons });
    }

    json!({ "status": "ready_to_expand", "reasons": Vec::<String>::new() })
}

pub(super) fn matches_optional(actual: &Option<String>, expected: Option<&str>) -> bool {
    expected.map(|value| actual.as_deref() == Some(value)).unwrap_or(true)
}

pub(super) fn rate(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

pub(super) fn percentage(used: i64, limit: i64) -> f64 {
    if limit <= 0 {
        0.0
    } else {
        (used as f64 / limit as f64) * 100.0
    }
}

pub(super) fn normalize_metric_label(value: &str, fallback: &str) -> String {
    let normalized = value.trim();
    if normalized.is_empty() {
        fallback.to_string()
    } else {
        normalized.to_string()
    }
}

pub(super) fn bool_metric_label(value: bool) -> String {
    if value {
        "true".to_string()
    } else {
        "false".to_string()
    }
}

pub(super) fn trim_sample_window(samples: &mut Vec<f64>, max_len: usize) {
    if samples.len() > max_len {
        let overflow = samples.len() - max_len;
        samples.drain(0..overflow);
    }
}

pub(super) fn next_id(prefix: &str) -> String {
    format!("{prefix}_{}", Ulid::new())
}
