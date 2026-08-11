//! OntologyService 纯规则与错误映射单元测试（不依赖数据库）。

use fms_domain::models::flight::Flight;
use fms_domain::models::ontology_v1_rules::{
    accept_permission_for, draft_can_be_occupied, is_ground_blacklisted_action, is_reassign_action,
    reassign_gate_violation,
};
use fms_domain::models::ontology_v1::SuggestionKind;
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
    let forbidden = OntologyError::from(fms_domain::error::DomainError::PermissionDenied(
        "nope".into(),
    ));
    assert!(matches!(forbidden, OntologyError::Forbidden(_)));

    let conflict = OntologyError::from(fms_domain::error::DomainError::ConcurrencyConflict(
        "ver".into(),
    ));
    assert!(matches!(conflict, OntologyError::Conflict(_)));
}
