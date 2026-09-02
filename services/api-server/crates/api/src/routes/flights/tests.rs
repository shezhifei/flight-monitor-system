use super::cache::{
    cached_flight_list_response, invalidate_flight_list_response_cache_now, store_flight_list_response_cache,
};
use super::shared::{dispatch_timeline_flight_updated_payload, dispatch_timeline_patch_payload};
use super::sse::{sse_payload_bytes, websocket_payload};
use crate::sse::hub::SseMessage;
use actix_web::web;
use chrono::{TimeZone, Utc};
use fms_application::schemas::flight_schemas::{DispatchTimelineEventResponse, FlightAnomalySummary, FlightResponse};
use serde_json::json;
use std::sync::{Arc, Barrier};
use std::thread;

#[test]
fn flight_response_contract_fields_present() {
    let flight = base_flight();
    let json = serde_json::to_value(&flight).unwrap();
    assert!(json.get("flight_id").is_some());
    assert!(json.get("flight_number").is_some());
    assert!(json.get("status").is_some());
    assert!(json.get("scheduled_departure").is_some());
    assert!(json.get("scheduled_arrival").is_some());
    assert!(json.get("inbound_leg").is_some());
    assert!(json.get("outbound_leg").is_some());
    assert!(json.get("anomaly_summary").is_some());
    assert!(json.get("labels").is_some());
    assert!(json.get("version").is_some());
    assert!(json.get("business_cases").is_some());
    assert!(json.get("created_at").is_some());
    assert!(json.get("updated_at").is_some());
    assert!(json.get("flight_remarks").is_some());
}

#[test]
fn flight_response_serialization_roundtrip() {
    let flight = base_flight();
    let json_str = serde_json::to_string(&flight).unwrap();
    let deserialized: FlightResponse = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.flight_id, flight.flight_id);
    assert_eq!(deserialized.flight_number, flight.flight_number);
    assert_eq!(deserialized.status, flight.status);
}

#[test]
fn flight_list_response_cache_supports_sync_concurrent_reads_and_invalidation() {
    invalidate_flight_list_response_cache_now();
    store_flight_list_response_cache(web::Bytes::from_static(b"cached"));

    let barrier = Arc::new(Barrier::new(9));
    let mut readers = Vec::new();
    for _ in 0..8 {
        let barrier = Arc::clone(&barrier);
        readers.push(thread::spawn(move || {
            barrier.wait();
            for _ in 0..1000 {
                let body = cached_flight_list_response();
                assert!(match body.as_deref() {
                    None => true,
                    Some(bytes) => bytes == b"cached" || bytes == b"refreshed",
                });
            }
        }));
    }

    barrier.wait();
    invalidate_flight_list_response_cache_now();
    assert!(cached_flight_list_response().is_none());
    store_flight_list_response_cache(web::Bytes::from_static(b"refreshed"));

    for reader in readers {
        reader.join().expect("cache reader thread should finish");
    }

    assert_eq!(cached_flight_list_response().as_deref(), Some(&b"refreshed"[..]));
    invalidate_flight_list_response_cache_now();
}

#[test]
fn websocket_payload_preserves_object_messages_and_wraps_non_objects() {
    let object_message = SseMessage {
        topic: "flights".to_string(),
        event: Some(" flight_updated ".to_string()),
        serialized_data: Arc::new(r#"{"flight_id":"flight-001","timestamp":"2026-04-27T08:30:00Z"}"#.to_string()),
    };
    let (event_type, payload, payload_json) = websocket_payload(&object_message, "fallback").unwrap();
    assert_eq!(event_type, "flight_updated");
    assert_eq!(payload["type"], json!("flight_updated"));
    assert_eq!(payload["flight_id"], json!("flight-001"));
    assert!(payload["timestamp"].is_number());
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&payload_json).unwrap(),
        payload
    );

    let scalar_message = SseMessage {
        topic: "flights".to_string(),
        event: None,
        serialized_data: Arc::new(r#""boarding""#.to_string()),
    };
    let (event_type, payload, _) = websocket_payload(&scalar_message, "fallback").unwrap();
    assert_eq!(event_type, "fallback");
    assert_eq!(payload["type"], json!("fallback"));
    assert_eq!(payload["data"], json!("boarding"));
    assert!(payload["timestamp"].is_number());

    let text_message = SseMessage {
        topic: "flights".to_string(),
        event: None,
        serialized_data: Arc::new("not-json".to_string()),
    };
    let (_, payload, _) = websocket_payload(&text_message, "fallback").unwrap();
    assert_eq!(payload["data"], json!("not-json"));
}

#[test]
fn sse_payload_bytes_matches_event_stream_format_exactly() {
    let payload = sse_payload_bytes("flight_updated", r#"{"flight_id":"flight-001"}"#);
    assert_eq!(
        payload.as_ref(),
        b"event: flight_updated\ndata: {\"flight_id\":\"flight-001\"}\n\n"
    );
}

fn base_flight() -> FlightResponse {
    FlightResponse {
        flight_id: Some("flight-001".to_string()),
        row_id: None,
        link_id: None,
        kind: None,
        inbound_flight_id: None,
        outbound_flight_id: None,
        flight_number: Some("MU1234".to_string()),
        airline_code: None,
        registration: None,
        aircraft_type_detail: None,
        status: Some("boarding".to_string()),
        scheduled_departure: None,
        scheduled_arrival: None,
        estimated_departure: None,
        estimated_arrival: None,
        actual_departure: None,
        actual_arrival: None,
        cobt_time: None,
        codt: None,
        on_blocks_time: None,
        cabin_door_open_time: None,
        deboarding_complete_time: None,
        cleaning_start_time: None,
        cleaning_end_time: None,
        boarding_allowed_time: Some(Utc.with_ymd_and_hms(2026, 4, 27, 8, 30, 0).unwrap()),
        start_boarding_time: None,
        passenger_ready_time: None,
        end_boarding_time: None,
        cabin_door_close_time: None,
        cargo_door_close_time: None,
        loading_complete_time: None,
        off_blocks_time: None,
        stand: None,
        gate: None,
        terminal: None,
        position: None,
        baggage_carousel: None,
        has_boarding_restriction: false,
        is_quick_turnaround: false,
        is_commercial_signed: true,
        inbound_leg: None,
        outbound_leg: None,
        anomaly_summary: FlightAnomalySummary::default(),
        business_cases: Vec::new(),
        created_at: None,
        updated_at: Some(Utc.with_ymd_and_hms(2026, 4, 27, 8, 0, 0).unwrap()),
        version: 7,
        labels: Vec::new(),
        flight_remarks: None,
        load_planning_remarks: None,
        aircraft_maintenance_remarks: None,
        aircraft_check_remarks: None,
        direction: None,
        flight_kind: None,
        is_draft: None,
        divert: None,
        created_by: None,
        updated_by: None,
        risk_score: None,
        risk_level: None,
        risk_reasons: None,
        next_primary_action: None,
        data_freshness: None,
    }
}

fn timeline_event(milestone_code: &str) -> DispatchTimelineEventResponse {
    DispatchTimelineEventResponse {
        timeline_id: "timeline-001".to_string(),
        flight_id: "flight-001".to_string(),
        milestone_code: milestone_code.to_string(),
        occurred_at: Utc.with_ymd_and_hms(2026, 4, 27, 8, 30, 0).unwrap(),
        leg_type: Some("outbound".to_string()),
        recorded_by: Some("tester".to_string()),
        client_action_id: Some("action-001".to_string()),
        source: "manual".to_string(),
        payload: json!({}),
        created_at: Utc.with_ymd_and_hms(2026, 4, 27, 8, 31, 0).unwrap(),
    }
}

#[test]
fn dispatch_timeline_patch_includes_milestone_and_version_fields() {
    let flight = base_flight();
    let event = timeline_event("boarding_allowed_time");

    let patch = dispatch_timeline_patch_payload(Some(&flight), &event);

    assert_eq!(patch["flight_id"], json!("flight-001"));
    assert_eq!(patch["version"], json!(7));
    assert_eq!(patch["updated_at"], json!("2026-04-27T08:00:00Z"));
    assert_eq!(patch["boarding_allowed_time"], json!("2026-04-27T08:30:00Z"));
    assert!(patch.get("flight_number").is_none());
    assert!(patch.get("status").is_none());
}

#[test]
fn dispatch_timeline_flight_updated_payload_uses_patch_not_full_snapshot() {
    let flight = base_flight();
    let event = timeline_event("boarding_allowed_time");
    let patch = dispatch_timeline_patch_payload(Some(&flight), &event);

    let payload = dispatch_timeline_flight_updated_payload("flight-001", patch, &event);

    assert_eq!(payload["type"], json!("flight_updated"));
    assert_eq!(payload["flight_id"], json!("flight-001"));
    assert_eq!(payload["changed_fields"], json!(["boarding_allowed_time"]));
    assert_eq!(payload["flight"], payload["patch"]);
    assert_eq!(payload["patch"]["boarding_allowed_time"], json!("2026-04-27T08:30:00Z"));
    assert!(payload["patch"].get("flight_number").is_none());
    assert!(payload["patch"].get("business_cases").is_none());
    assert!(payload.get("flights").is_none());
}
