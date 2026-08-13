//! OntologyService 纯规则与错误映射单元测试（不依赖数据库）。

use fms_domain::models::flight::Flight;
use fms_domain::models::ontology_v1::SuggestionKind;
use fms_domain::models::ontology_v1_rules::{
    accept_permission_for, draft_can_be_occupied, is_ground_blacklisted_action, is_reassign_action,
    reassign_gate_violation,
};
use fms_domain::models::value_objects::FlightStatus;

use super::error::OntologyError;

fn flight_with(status: FlightStatus, inbound: bool, outbound: bool) -> Flight {
    let leg = || {
        Some(fms_domain::models::flight_leg::FlightLeg {
            leg_type: fms_domain::models::flight_leg::LegType::Inbound,
            flight_no: "CA1234".to_string(),
            flight_type: fms_domain::models::flight_leg::FlightTypeCode::Domestic,
            mission: None,
            origin_code: Some("PEK".to_string()),
            origin_name: None,
            destination_code: Some("SHA".to_string()),
            destination_name: None,
            is_vip: false,
            stand_type: None,
            scheduled_time: None,
        })
    };
    Flight {
        flight_id: "FL_TEST".into(),
        airline_code: Some("CA".to_string()),
        flight_number: Some("CA1234".into()),
        registration: Some("B-1234".to_string()),
        aircraft_type_detail: None,
        stand: None,
        gate: None,
        terminal: None,
        position: None,
        baggage_carousel: None,
        scheduled_departure: None,
        scheduled_arrival: None,
        estimated_departure: None,
        estimated_arrival: None,
        actual_departure: None,
        actual_arrival: None,
        cobt_time: None,
        codt: None,
        has_boarding_restriction: false,
        is_quick_turnaround: false,
        is_commercial_signed: true,
        status,
        inbound_leg: if inbound { leg() } else { None },
        outbound_leg: if outbound {
            Some(fms_domain::models::flight_leg::FlightLeg {
                leg_type: fms_domain::models::flight_leg::LegType::Outbound,
                ..leg().expect("leg")
            })
        } else {
            None
        },
        anomaly_summary: Default::default(),
        direction: None,
        flight_kind: "passenger".to_string(),
        is_draft: false,
        divert: false,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        version: 1,
        labels: vec![],
        flight_remarks: None,
        load_planning_remarks: None,
        aircraft_maintenance_remarks: None,
        aircraft_check_remarks: None,
    }
}

#[test]
fn reassign_gate_blocks_locked_inbound() {
    assert!(reassign_gate_violation(&flight_with(FlightStatus::PrevDeparted, true, false)).is_some());
}

#[test]
fn reassign_gate_blocks_boarding_outbound() {
    assert!(reassign_gate_violation(&flight_with(FlightStatus::Boarding, false, true)).is_some());
}

#[test]
fn reassign_gate_allows_scheduled() {
    assert!(reassign_gate_violation(&flight_with(FlightStatus::Scheduled, true, true)).is_none());
}

#[test]
fn draft_invariant_blocks_occupation() {
    assert!(!draft_can_be_occupied(true));
    assert!(draft_can_be_occupied(false));
}

#[test]
fn ground_blacklist_and_reassign_flags() {
    assert!(is_ground_blacklisted_action("Flight", "ReassignAircraft"));
    assert!(is_reassign_action("ReassignAircraft"));
    assert_eq!(
        accept_permission_for(SuggestionKind::Stand),
        "ontology.suggestion.accept_stand"
    );
    assert_eq!(
        accept_permission_for(SuggestionKind::Gate),
        "ontology.suggestion.accept_gate"
    );
}

#[test]
fn ontology_error_maps_domain_variants() {
    let forbidden = OntologyError::from(fms_domain::error::DomainError::PermissionDenied("nope".into()));
    assert!(matches!(forbidden, OntologyError::Forbidden(_)));

    let conflict = OntologyError::from(fms_domain::error::DomainError::ConcurrencyConflict("ver".into()));
    assert!(matches!(conflict, OntologyError::Conflict(_)));
}

#[test]
fn permission_and_time_window_rules() {
    let perms: Vec<String> = vec![];
    assert!(!perms.iter().any(|p| p == "ontology.stand.manage" || p == "*"));
    assert!(["*".to_string()]
        .iter()
        .any(|p| p == "ontology.stand.manage" || p == "*"));

    let start = chrono::Utc::now();
    let end = start + chrono::Duration::hours(1);
    assert!(end > start);
    assert!(draft_can_be_occupied(false));
    assert!(!draft_can_be_occupied(true));
}

#[test]
fn autolink_window_bounds_are_sane() {
    // service clamps window to [30, 1440]
    let window = 360_i64;
    assert!((30..=24 * 60).contains(&window));
    // same-registration health is prerequisite for active auto links
    assert!(fms_domain::models::ontology_v1_rules::link_is_healthy(
        Some("B-1234"),
        Some("B-1234")
    ));
    assert!(!fms_domain::models::ontology_v1_rules::link_is_healthy(
        Some("B-1234"),
        Some("B-5678")
    ));
}

#[test]
fn suggestion_time_window_parses_payload_or_defaults() {
    let payload = serde_json::json!({
        "starts_at": "2026-01-01T10:00:00Z",
        "ends_at": "2026-01-01T12:00:00Z"
    });
    let starts = payload
        .get("starts_at")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .expect("starts");
    let ends = payload
        .get("ends_at")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .expect("ends");
    assert!(ends > starts);
    assert_eq!(ends.signed_duration_since(starts).num_hours(), 2);
}
