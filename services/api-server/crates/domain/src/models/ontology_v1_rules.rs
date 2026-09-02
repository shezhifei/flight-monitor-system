//! 本体 V1 不变量（ONTOLOGY_V1.md §10）— 纯函数守门
//!
//! 每条不变量对应 §10 编号；能在纯逻辑层判定的一律在此判定，
//! 涉及数据库唯一性/外键的由表约束 + 仓储层 enforce。

use super::flight::Flight;
use super::ontology_v1::{SuggestionKind, TurnaroundLink};
use super::value_objects::FlightStatus;

/// 不变量 6a: 进港已前站起飞 → 禁止再改执行机。
/// 前站起飞 (PrevDeparted=1) 起物理飞机已绑定（§7.1）。
pub fn inbound_aircraft_locked(status: FlightStatus) -> bool {
    status.code() >= FlightStatus::PrevDeparted.code()
}

/// 不变量 6b: 出港已开始登机 → 禁止再改执行机。
/// Boarding(4) 起禁止（§7.1）。
pub fn outbound_aircraft_locked(status: FlightStatus) -> bool {
    status.code() >= FlightStatus::Boarding.code()
}

/// §7.2 步骤 1 闸门。`flight` 按航段方向分别判定：
/// 进港任务看进港侧状态；出港任务看出港侧状态；过站行两侧都判。
pub fn reassign_gate_violation(flight: &Flight) -> Option<&'static str> {
    if flight.is_arrival_flight() && inbound_aircraft_locked(flight.status) {
        return Some("inbound has departed from previous station; aircraft is locked");
    }
    if flight.is_departure_flight() && outbound_aircraft_locked(flight.status) {
        return Some("outbound has started boarding; aircraft is locked");
    }
    None
}

/// 不变量 4: 周转链接健康衔接要求端点任务同机。
/// 同机时方可视为健康衔接；不同机应拆（§4.8）。
pub fn link_is_healthy(inbound_registration: Option<&str>, outbound_registration: Option<&str>) -> bool {
    match (inbound_registration, outbound_registration) {
        (Some(in_reg), Some(out_reg)) => in_reg == out_reg,
        _ => false,
    }
}

/// 不变量 4（链接侧）: 同机才允许 active，异机必须 broken/拆。
pub fn enforce_link_health(
    link: &TurnaroundLink,
    inbound_registration: Option<&str>,
    outbound_registration: Option<&str>,
) -> TurnaroundLink {
    let mut updated = link.clone();
    if link_is_healthy(inbound_registration, outbound_registration) {
        updated.status = super::ontology_v1::TurnaroundLinkStatus::Active;
    } else {
        updated.status = super::ontology_v1::TurnaroundLinkStatus::Broken;
        updated.broken_reason = Some("registration mismatch after ReassignAircraft".to_string());
    }
    updated
}

/// 不变量 5: draft Flight 不可被新的正式占用引用（§3.3）。
pub fn draft_can_be_occupied(is_draft: bool) -> bool {
    !is_draft
}

/// 不变量 9: 同一账号禁止同时具备 AOC 与 TOC 岗（§3.2）。
pub fn dual_post_conflict(has_aoc: bool, has_toc: bool) -> bool {
    has_aoc && has_toc
}

/// 不变量 10: 地服黑名单 — 改机号 / 写正式位 / 写正式口。
pub fn is_ground_blacklisted_action(object_type: &str, action_name: &str) -> bool {
    matches!(
        (object_type, action_name),
        ("Flight", "ReassignAircraft")
            | ("Flight", "reassign_aircraft")
            | ("Aircraft", "ReassignAircraft")
            | ("Aircraft", "reassign_aircraft")
            | ("StandOccupation", "Allocate")
            | ("StandOccupation", "Adjust")
            | ("GateAssignment", "Allocate")
            | ("GateAssignment", "Adjust")
    )
}

/// 不变量 11: AI proposal 不得包含 ReassignAircraft（§6.4）。
pub fn is_reassign_action(action_name: &str) -> bool {
    action_name == "ReassignAircraft" || action_name == "reassign_aircraft"
}

/// 不变量 12: 建议接受权限与资源类型匹配（位 AOC、口 TOC）。
pub fn accept_permission_for(kind: SuggestionKind) -> &'static str {
    match kind {
        SuggestionKind::Stand => "ontology.suggestion.accept_stand",
        SuggestionKind::Gate => "ontology.suggestion.accept_gate",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ontology_v1::{
        ResourceAdjustmentSuggestion, SuggestionStatus, TurnaroundLinkSource, TurnaroundLinkStatus,
    };

    fn flight_with(status: FlightStatus, inbound: bool, outbound: bool) -> Flight {
        let leg = || {
            Some(crate::models::flight_leg::FlightLeg {
                leg_type: crate::models::flight_leg::LegType::Inbound,
                flight_no: "CA1234".to_string(),
                flight_type: crate::models::flight_leg::FlightTypeCode::Domestic,
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
                Some(crate::models::flight_leg::FlightLeg {
                    leg_type: crate::models::flight_leg::LegType::Outbound,
                    ..leg().expect("leg template")
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
    fn inbound_lock_from_prev_departed() {
        assert!(!inbound_aircraft_locked(FlightStatus::Scheduled));
        assert!(inbound_aircraft_locked(FlightStatus::PrevDeparted));
        assert!(inbound_aircraft_locked(FlightStatus::Arrived));
    }

    #[test]
    fn outbound_lock_from_boarding() {
        assert!(!outbound_aircraft_locked(FlightStatus::Scheduled));
        assert!(!outbound_aircraft_locked(FlightStatus::PrevDeparted));
        assert!(outbound_aircraft_locked(FlightStatus::Boarding));
        assert!(outbound_aircraft_locked(FlightStatus::Departed));
    }

    #[test]
    fn reassign_gate_rejects_inbound_committed() {
        assert!(reassign_gate_violation(&flight_with(FlightStatus::PrevDeparted, true, false)).is_some());
        assert!(reassign_gate_violation(&flight_with(FlightStatus::Scheduled, true, false)).is_none());
    }

    #[test]
    fn reassign_gate_rejects_outbound_boarding() {
        assert!(reassign_gate_violation(&flight_with(FlightStatus::Boarding, false, true)).is_some());
        assert!(reassign_gate_violation(&flight_with(FlightStatus::BoardingUrge, false, true)).is_some());
        assert!(reassign_gate_violation(&flight_with(FlightStatus::CheckInEnd, false, true)).is_none());
    }

    #[test]
    fn reassign_gate_turnaround_row_checks_both_sides() {
        assert!(reassign_gate_violation(&flight_with(FlightStatus::Boarding, true, true)).is_some());
        assert!(reassign_gate_violation(&flight_with(FlightStatus::Scheduled, true, true)).is_none());
    }

    #[test]
    fn link_health_requires_same_registration() {
        assert!(link_is_healthy(Some("B-1111"), Some("B-1111")));
        assert!(!link_is_healthy(Some("B-1111"), Some("B-2222")));
        assert!(!link_is_healthy(Some("B-1111"), None));
    }

    #[test]
    fn link_health_enforcement_breaks_mismatch() {
        let link = TurnaroundLink {
            id: "TL1".into(),
            inbound_flight_id: "FL_IN".into(),
            outbound_flight_id: "FL_OUT".into(),
            status: TurnaroundLinkStatus::Active,
            source: TurnaroundLinkSource::Auto,
            broken_reason: None,
            created_by: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let broken = enforce_link_health(&link, Some("B-1111"), Some("B-2222"));
        assert_eq!(broken.status, TurnaroundLinkStatus::Broken);
        let kept = enforce_link_health(&link, Some("B-1111"), Some("B-1111"));
        assert_eq!(kept.status, TurnaroundLinkStatus::Active);
    }

    #[test]
    fn draft_cannot_be_occupied() {
        assert!(!draft_can_be_occupied(true));
        assert!(draft_can_be_occupied(false));
    }

    #[test]
    fn dual_post_conflict_detection() {
        assert!(dual_post_conflict(true, true));
        assert!(!dual_post_conflict(true, false));
        assert!(!dual_post_conflict(false, false));
    }

    #[test]
    fn ground_blacklist_covers_core_writes() {
        assert!(is_ground_blacklisted_action("Flight", "ReassignAircraft"));
        assert!(is_ground_blacklisted_action("StandOccupation", "Allocate"));
        assert!(is_ground_blacklisted_action("GateAssignment", "Adjust"));
        assert!(!is_ground_blacklisted_action("DispatchOrder", "Create"));
        assert!(!is_ground_blacklisted_action("Flight", "add_note"));
    }

    #[test]
    fn ai_reassign_denial() {
        assert!(is_reassign_action("ReassignAircraft"));
        assert!(is_reassign_action("reassign_aircraft"));
        assert!(!is_reassign_action("UpdatePlanStand"));
    }

    #[test]
    fn accept_permission_matches_resource_kind() {
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
    fn suggestion_expiry_check() {
        let now = chrono::Utc::now();
        let expired = ResourceAdjustmentSuggestion {
            id: "SUG1".into(),
            flight_id: "FL_IN".into(),
            kind: SuggestionKind::Stand,
            current_value: None,
            suggested_value: "201".to_string(),
            status: SuggestionStatus::Pending,
            reason: None,
            payload: serde_json::json!({}),
            created_by: "user1".to_string(),
            decided_by: None,
            decided_at: None,
            expires_at: Some(now - chrono::Duration::minutes(1)),
            created_at: now,
            updated_at: now,
        };
        assert!(expired.is_expired());
    }
}
