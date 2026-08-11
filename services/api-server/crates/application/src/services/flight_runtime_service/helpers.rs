use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::schemas::flight_schemas::FlightResponse;
use fms_domain::ports::flight_runtime_projection_repository::FlightRuntimeProjection;

use super::types::FlightRuntimeState;

pub(super) const MAX_RETAINED_FLIGHTS: usize = 500;

fn perf_trace_enabled() -> bool {
    std::env::var("FMS_PERF_TRACE")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

pub(super) fn should_emit_perf_trace(counter: &AtomicU64) -> bool {
    if !perf_trace_enabled() {
        return false;
    }
    let sample_rate = std::env::var("FMS_PERF_TRACE_SAMPLE_RATE")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(1000);
    counter.fetch_add(1, Ordering::Relaxed) % sample_rate == 0
}

pub(super) fn apply_timeline_to_flight(flight: &mut FlightResponse, events: &HashMap<String, DateTime<Utc>>) {
    if let Some(&t) = events.get("on_blocks_time") {
        flight.on_blocks_time = Some(t);
    }
    if let Some(&t) = events.get("cabin_door_open_time") {
        flight.cabin_door_open_time = Some(t);
    }
    if let Some(&t) = events.get("deboarding_complete_time") {
        flight.deboarding_complete_time = Some(t);
    }
    if let Some(&t) = events.get("cleaning_start_time") {
        flight.cleaning_start_time = Some(t);
    }
    if let Some(&t) = events.get("cleaning_end_time") {
        flight.cleaning_end_time = Some(t);
    }
    if let Some(&t) = events.get("boarding_allowed_time") {
        flight.boarding_allowed_time = Some(t);
    }
    if let Some(&t) = events.get("start_boarding_time") {
        flight.start_boarding_time = Some(t);
    }
    if let Some(&t) = events.get("passenger_ready_time") {
        flight.passenger_ready_time = Some(t);
    }
    if let Some(&t) = events.get("end_boarding_time") {
        flight.end_boarding_time = Some(t);
    }
    if let Some(&t) = events.get("cabin_door_close_time") {
        flight.cabin_door_close_time = Some(t);
    }
    if let Some(&t) = events.get("cargo_door_close_time") {
        flight.cargo_door_close_time = Some(t);
    }
    if let Some(&t) = events.get("loading_complete_time") {
        flight.loading_complete_time = Some(t);
    }
    if let Some(&t) = events.get("off_blocks_time") {
        flight.off_blocks_time = Some(t);
    }
}

pub(super) fn apply_projection_to_flight(flight: &mut FlightResponse, projection: &FlightRuntimeProjection) {
    apply_timeline_to_flight(flight, &projection.timeline_snapshot);
    flight.business_cases = projection.business_cases.clone();
}

pub(super) fn timestamp_from_value(value: &Value) -> DateTime<Utc> {
    value
        .get("timestamp")
        .and_then(Value::as_str)
        .or_else(|| value.get("created_at").and_then(Value::as_str))
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        .map(|ts| ts.with_timezone(&Utc))
        .unwrap_or_else(Utc::now)
}

pub(super) fn trim_deque<T>(deque: &mut VecDeque<T>, max_len: usize) {
    while deque.len() > max_len {
        deque.pop_back();
    }
}

/// Evict oldest flight IDs from in-memory state to stay within budget.
/// Oldest by min(occurred_at) per flight in history_by_flight.
pub(super) fn evict_idle_flights(state: &mut FlightRuntimeState) {
    let mut flight_entries: Vec<(String, DateTime<Utc>)> = state
        .history_by_flight
        .iter()
        .map(|(fid, entries)| {
            let oldest = entries.back().map(|e| e.occurred_at).unwrap_or(Utc::now());
            (fid.clone(), oldest)
        })
        .collect();
    flight_entries.sort_by_key(|(_, ts)| *ts);

    let excess = state.history_by_flight.len().saturating_sub(MAX_RETAINED_FLIGHTS);
    for (flight_id, _) in flight_entries.into_iter().take(excess) {
        state.history_by_flight.remove(&flight_id);
        state.timeline_by_flight.remove(&flight_id);
    }
}

pub(super) fn action_label(action: &str) -> &str {
    match action {
        "create" => "创建",
        "delete" => "删除",
        _ => "更新",
    }
}
