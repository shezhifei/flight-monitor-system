use super::test_support::{
    anomaly_fixture, flight_fixture, occupation_fixture, order_fixture, stand_fixture, FakeAnomalyRepo,
    FakeBusinessCaseRepo, FakeDispatchRepo, FakeEquipmentRepo, FakeFlightRepo, FakeOccupationRepo,
    FakePersonnelRuntimeRepo, FakeQualificationRepo, FakeStandRepo, FakeTeamRepo, FakeUserRepo,
};
use super::*;
use chrono::{Duration, Utc};
use fms_domain::models::anomaly::{Anomaly, AnomalySeverity, AnomalyStatus};
use fms_domain::models::dispatch::{DispatchOrder, Stand, Team};
use fms_domain::models::flight::Flight;
use fms_domain::models::ontology_v1::StandOccupation;
use fms_domain::models::value_objects::FlightStatus;
use fms_domain::ontology::schema_export::FLIGHT_OPS_ONTOLOGY_VERSION;
use serde_json::json;
use std::sync::Arc;

fn actions(
    flights: Vec<Flight>,
    orders: Vec<DispatchOrder>,
    anomalies: Vec<Anomaly>,
    teams: Vec<Team>,
    stands: Vec<Stand>,
    occupations: Vec<StandOccupation>,
) -> OntologyActionServices {
    OntologyActionServices::new(
        Arc::new(FakeFlightRepo {
            flights: std::sync::Mutex::new(flights),
        }),
        Arc::new(FakeDispatchRepo {
            orders: std::sync::Mutex::new(orders),
        }),
        Arc::new(FakeAnomalyRepo {
            anomalies: std::sync::Mutex::new(anomalies),
        }),
        Arc::new(FakeTeamRepo {
            teams: std::sync::Mutex::new(teams),
        }),
        Arc::new(FakeStandRepo {
            stands: std::sync::Mutex::new(stands),
        }),
        Arc::new(FakeOccupationRepo {
            occupations: std::sync::Mutex::new(occupations),
        }),
        Arc::new(FakeBusinessCaseRepo),
        Arc::new(FakeUserRepo::default()),
        Arc::new(FakePersonnelRuntimeRepo::default()),
        Arc::new(FakeQualificationRepo::default()),
        Arc::new(FakeEquipmentRepo::default()),
    )
}

fn empty() -> OntologyActionServices {
    actions(vec![], vec![], vec![], vec![], vec![], vec![])
}

fn team_fixture(id: &str, name: &str) -> Team {
    Team {
        id: id.to_string(),
        name: name.to_string(),
        department_id: None,
        team_type_id: None,
        code: None,
        leader_id: None,
        current_status: fms_domain::models::dispatch::TeamStatus::OnDuty,
        current_position_lat: None,
        current_position_lng: None,
        current_stand_id: None,
        last_position_update: None,
        created_at: None,
        updated_at: None,
        is_active: true,
        team_type: None,
        members: vec![],
    }
}

#[test]
fn permission_mapping_covers_read_and_advisory_actions() {
    assert_eq!(read_action_permission("flight.get_context"), Some("flight:read"));
    assert_eq!(read_action_permission("flight.search"), Some("flight:read"));
    assert_eq!(read_action_permission("dispatch.get_status"), Some("dispatch:read"));
    assert_eq!(read_action_permission("anomaly.list_open"), Some("anomaly:read"));
    assert_eq!(read_action_permission("stand.check_availability"), Some("flight:read"));
    assert_eq!(read_action_permission("report.generate_briefing"), Some("flight:read"));
    assert_eq!(read_action_permission("personnel.get_context"), Some("dispatch:read"));
    assert_eq!(read_action_permission("team.get_context"), Some("dispatch:read"));
    assert_eq!(read_action_permission("equipment.get_context"), Some("dispatch:read"));
    assert_eq!(read_action_permission("Flight.change_stand"), None);

    assert_eq!(
        advisory_action_permission("flight.suggest_stand_adjustment"),
        Some("flight:read")
    );
    assert_eq!(
        advisory_action_permission("dispatch.suggest_replan"),
        Some("dispatch:read")
    );
    assert_eq!(
        advisory_action_permission("anomaly.suggest_escalation"),
        Some("anomaly:read")
    );
    assert_eq!(
        advisory_action_permission("flight.suggest_delay_action"),
        Some("flight:read")
    );
    assert_eq!(
        advisory_action_permission("notification.suggest_broadcast"),
        Some("notification:send")
    );
    assert_eq!(advisory_action_permission("flight.suggest_nothing"), None);
}

#[tokio::test]
async fn flight_get_context_returns_relations_and_evidence() {
    let svc = actions(
        vec![flight_fixture("FL1", FlightStatus::Scheduled, true, true)],
        vec![order_fixture("ORD1", "FL1", "pending", None)],
        vec![anomaly_fixture(
            "AN1",
            "FL1",
            AnomalySeverity::High,
            AnomalyStatus::Open,
            5,
        )],
        vec![],
        vec![],
        vec![],
    );
    let result = svc
        .flight_context
        .get(&json!({"flight_id": "FL1"}))
        .await
        .expect("get_context");
    assert_eq!(result["flight"]["flight_id"], "FL1");
    assert_eq!(result["dispatch_orders"][0]["id"], "ORD1");
    assert_eq!(result["anomalies"][0]["anomaly_id"], "AN1");
    assert_eq!(result["labels"][0], "vip");
    assert_eq!(result["evidence"]["ontology_version"], FLIGHT_OPS_ONTOLOGY_VERSION);
    assert!(result["evidence"]["retrieved_at"].is_string());
}

#[tokio::test]
async fn flight_get_context_missing_flight_is_not_found() {
    let err = empty()
        .flight_context
        .get(&json!({"flight_id": "MISSING"}))
        .await
        .expect_err("missing flight");
    assert!(matches!(err, OntologyActionError::NotFound(_)));
}

#[tokio::test]
async fn flight_get_context_requires_flight_id() {
    let err = empty()
        .flight_context
        .get(&json!({}))
        .await
        .expect_err("missing argument");
    assert!(matches!(err, OntologyActionError::InvalidArguments(_)));
}

#[tokio::test]
async fn flight_search_filters_by_date_and_status() {
    let svc = actions(
        vec![
            flight_fixture("FL1", FlightStatus::Delayed, true, true),
            flight_fixture("FL2", FlightStatus::Scheduled, false, true),
        ],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
    );
    let result = svc
        .flight_search
        .search(&json!({"date": Utc::now().format("%Y-%m-%d").to_string(), "status": "delayed"}))
        .await
        .expect("search");
    assert_eq!(result["total"], 1);
    assert_eq!(result["flights"][0]["flight_id"], "FL1");
    assert!(result["evidence"]["query_params"]["status"] == "delayed");
}

#[tokio::test]
async fn flight_search_invalid_date_is_rejected() {
    let err = empty()
        .flight_search
        .search(&json!({"date": "not-a-date"}))
        .await
        .expect_err("bad date");
    assert!(matches!(err, OntologyActionError::InvalidArguments(_)));
}

#[tokio::test]
async fn dispatch_get_status_returns_conflicts() {
    let mut order = order_fixture("ORD1", "FL1", "in_progress", Some("TEAM1"));
    order.conflict_reason = Some("equip conflict".to_string());
    let svc = actions(
        vec![],
        vec![order],
        vec![],
        vec![team_fixture("TEAM1", "Alpha")],
        vec![],
        vec![],
    );
    let result = svc
        .dispatch_status
        .get(&json!({"dispatch_order_id": "ORD1"}))
        .await
        .expect("get_status");
    assert_eq!(result["dispatch_order"]["status"], "in_progress");
    assert_eq!(
        result["dispatch_order"]["members"][0]["source_team_id"],
        "TEAM1"
    );
    assert!(result["team"].is_null());
    assert_eq!(result["conflicts"][0]["description"], "equip conflict");
    assert!(result["evidence"].is_object());
}

#[tokio::test]
async fn anomaly_list_open_merges_open_and_acknowledged() {
    let svc = actions(
        vec![],
        vec![],
        vec![
            anomaly_fixture("AN1", "FL1", AnomalySeverity::Critical, AnomalyStatus::Open, 5),
            anomaly_fixture("AN2", "FL2", AnomalySeverity::Low, AnomalyStatus::Acknowledged, 60),
            anomaly_fixture("AN3", "FL1", AnomalySeverity::Medium, AnomalyStatus::Resolved, 120),
        ],
        vec![],
        vec![],
        vec![],
    );
    let result = svc.anomaly_open_list.list(&json!({})).await.expect("list_open");
    assert_eq!(result["total"], 2, "resolved anomalies excluded");
    assert_eq!(result["summary"]["critical"], 1);
    assert_eq!(result["summary"]["low"], 1);
    assert_eq!(result["anomalies"][0]["anomaly_id"], "AN1", "newest first");

    let filtered = svc
        .anomaly_open_list
        .list(&json!({"severity": "low", "flight_id": "FL2"}))
        .await
        .expect("filtered");
    assert_eq!(filtered["total"], 1);
    assert_eq!(filtered["anomalies"][0]["anomaly_id"], "AN2");
}

#[tokio::test]
async fn stand_check_availability_detects_conflict_and_suggests_alternatives() {
    let now = Utc::now();
    let svc = actions(
        vec![],
        vec![],
        vec![],
        vec![],
        vec![stand_fixture("S1", "101", true), stand_fixture("S2", "102", true)],
        vec![occupation_fixture("101", "B-9999", 0, 60)],
    );
    let result = svc
        .stand_availability
        .check(&json!({
            "stand_id": "101",
            "time_window": {
                "start": (now + Duration::minutes(10)).to_rfc3339(),
                "end": (now + Duration::minutes(30)).to_rfc3339(),
            }
        }))
        .await
        .expect("check_availability");
    assert_eq!(result["is_available"], false);
    assert_eq!(result["conflicts"][0]["registration"], "B-9999");
    assert_eq!(result["alternative_suggestions"][0]["stand_id"], "102");
}

#[tokio::test]
async fn stand_check_availability_rejects_invalid_window() {
    let now = Utc::now();
    let svc = actions(
        vec![],
        vec![],
        vec![],
        vec![],
        vec![stand_fixture("S1", "101", true)],
        vec![],
    );
    let err = svc
        .stand_availability
        .check(&json!({
            "stand_id": "101",
            "time_window": {
                "start": (now + Duration::minutes(30)).to_rfc3339(),
                "end": (now + Duration::minutes(10)).to_rfc3339(),
            }
        }))
        .await
        .expect_err("inverted window");
    assert!(matches!(err, OntologyActionError::InvalidArguments(_)));
}

#[tokio::test]
async fn report_generate_briefing_aggregates_and_declares_limitations() {
    let now = Utc::now();
    let svc = actions(
        vec![
            flight_fixture("FL1", FlightStatus::Delayed, true, true),
            flight_fixture("FL2", FlightStatus::Cancelled, false, true),
        ],
        vec![order_fixture("ORD1", "FL1", "pending", None)],
        vec![anomaly_fixture(
            "AN1",
            "FL1",
            AnomalySeverity::Critical,
            AnomalyStatus::Open,
            5,
        )],
        vec![],
        vec![],
        vec![],
    );
    let result = svc
        .briefing
        .generate(&json!({
            "shift_start": now.to_rfc3339(),
            "shift_end": (now + Duration::hours(8)).to_rfc3339(),
        }))
        .await
        .expect("briefing");
    assert_eq!(result["briefing"]["flights_summary"]["total"], 2);
    assert_eq!(result["briefing"]["flights_summary"]["delayed"], 1);
    assert_eq!(result["briefing"]["flights_summary"]["cancelled"], 1);
    assert_eq!(result["briefing"]["dispatch_summary"]["pending"], 1);
    assert_eq!(result["briefing"]["anomaly_summary"]["critical"], 1);
    assert!(result["limitations"].as_array().is_some_and(|items| !items.is_empty()));
    assert!(result["confidence"].as_f64().is_some());
    assert_eq!(result["evidence"]["ontology_version"], FLIGHT_OPS_ONTOLOGY_VERSION);
}

#[tokio::test]
async fn report_generate_briefing_rejects_bad_scope() {
    let err = empty()
        .briefing
        .generate(&json!({"scope": "sideways"}))
        .await
        .expect_err("bad scope");
    assert!(matches!(err, OntologyActionError::InvalidArguments(_)));
}

#[tokio::test]
async fn suggest_stand_adjustment_picks_conflict_free_stand() {
    let mut flight = flight_fixture("FL1", FlightStatus::Arrived, true, true);
    flight.stand = Some("S1".into());
    let svc = actions(
        vec![flight],
        vec![],
        vec![],
        vec![],
        vec![stand_fixture("ST1", "S1", true), stand_fixture("ST2", "S2", true)],
        vec![occupation_fixture("S2", "B-9999", 300, 420)],
    );
    let result = svc
        .stand_recommendation
        .suggest(&json!({"flight_id": "FL1"}))
        .await
        .expect("stand suggestion");
    let suggestion = &result["suggestion"];
    assert_eq!(suggestion["action_name"], "change_stand");
    assert_eq!(suggestion["object_type"], "Flight");
    assert_eq!(suggestion["object_id"], "FL1");
    assert_eq!(suggestion["arguments"]["new_stand_id"], "S2");
    assert_eq!(suggestion["risk_level"], "medium");
    assert_eq!(suggestion["approval_policy"], "require_approval");
    assert_eq!(suggestion["before_snapshot"]["stand"], "S1");
    assert_eq!(suggestion["after_preview"]["stand"], "S2");
    assert!(suggestion["expires_at"].is_string());
    let constraints = suggestion["constraint_results"].as_array().expect("constraints");
    assert!(constraints.iter().all(|c| c["constraint_name"].is_string()));
    assert!(constraints
        .iter()
        .any(|c| c["constraint_name"] == "no_occupation_overlap" && c["passed"].as_bool() == Some(true)));
    assert!(result["conflicts"].as_array().unwrap().is_empty());
    assert!(result["evidence"]["retrieved_at"].is_string());
}

#[tokio::test]
async fn suggest_stand_adjustment_reports_overlap_warning_not_block() {
    let flight = flight_fixture("FL1", FlightStatus::Arrived, true, true);
    let svc = actions(
        vec![flight],
        vec![],
        vec![],
        vec![],
        vec![stand_fixture("ST2", "S2", true)],
        vec![occupation_fixture("S2", "B-9999", -60, 240)],
    );
    let result = svc
        .stand_recommendation
        .suggest(&json!({"flight_id": "FL1", "new_stand_id": "S2"}))
        .await
        .expect("overlap must be warning, not hard reject");
    let suggestion = &result["suggestion"];
    let overlap = suggestion["constraint_results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["constraint_name"] == "no_occupation_overlap")
        .cloned()
        .expect("overlap constraint");
    assert_eq!(overlap["passed"], false);
    assert_eq!(overlap["severity"], "warning");
    assert_eq!(result["conflicts"][0]["registration"], "B-9999");
}

#[tokio::test]
async fn suggest_stand_adjustment_flight_not_found() {
    let err = empty()
        .stand_recommendation
        .suggest(&json!({"flight_id": "MISSING"}))
        .await
        .unwrap_err();
    assert!(matches!(err, OntologyActionError::NotFound(_)));
}

#[tokio::test]
async fn suggest_replan_generates_reassign_proposal_with_scores() {
    let order = order_fixture("ORD1", "FL1", "pending", Some("TEAM_A"));
    let svc = actions(
        vec![],
        vec![order],
        vec![],
        vec![team_fixture("TEAM_A", "Alpha"), team_fixture("TEAM_B", "Bravo")],
        vec![],
        vec![],
    );
    let result = svc
        .dispatch_replan
        .suggest(&json!({"dispatch_order_id": "ORD1", "reason": "team unavailable"}))
        .await
        .expect("replan suggestion");
    let suggestion = &result["suggestion"];
    assert_eq!(suggestion["action_name"], "suggest_replan");
    assert_eq!(suggestion["risk_level"], "high");
    assert_eq!(suggestion["arguments"]["reason"], "team unavailable");
    assert_eq!(suggestion["arguments"]["dispatch_order_id"], "ORD1");
    assert_eq!(result["score_before"], 0.5);
    assert!(result["score_after"].as_f64().unwrap() > 0.5);
    assert_eq!(result["resource_changes"][0]["kind"], "crew_slots");
}

#[tokio::test]
async fn suggest_replan_order_not_found() {
    let svc = actions(vec![], vec![], vec![], vec![team_fixture("T1", "A")], vec![], vec![]);
    let err = svc
        .dispatch_replan
        .suggest(&json!({"dispatch_order_id": "MISSING", "reason": "x"}))
        .await
        .unwrap_err();
    assert!(matches!(err, OntologyActionError::NotFound(_)));
}

#[tokio::test]
async fn suggest_escalation_severity_for_critical_open_anomaly() {
    let anomaly = anomaly_fixture("AN1", "FL1", AnomalySeverity::Critical, AnomalyStatus::Open, 10);
    let svc = actions(vec![], vec![], vec![anomaly], vec![], vec![], vec![]);
    let result = svc
        .anomaly_escalation
        .suggest(&json!({"anomaly_id": "AN1"}))
        .await
        .expect("escalation suggestion");
    let suggestion = &result["suggestion"];
    assert_eq!(suggestion["action_name"], "escalate");
    assert_eq!(suggestion["after_preview"]["escalation_level"], 1);
    assert_eq!(result["escalation_type"], "severity_escalation");
    assert!(result["targets"]["notification"]["title"].is_string());
}

#[tokio::test]
async fn suggest_escalation_rejects_resolved_anomaly() {
    let anomaly = anomaly_fixture("AN2", "FL1", AnomalySeverity::High, AnomalyStatus::Resolved, 10);
    let svc = actions(vec![], vec![], vec![anomaly], vec![], vec![], vec![]);
    let err = svc
        .anomaly_escalation
        .suggest(&json!({"anomaly_id": "AN2"}))
        .await
        .unwrap_err();
    assert!(matches!(err, OntologyActionError::InvalidArguments(_)));
}

#[tokio::test]
async fn suggest_delay_action_lists_impacted_dispatch_orders() {
    let flight = flight_fixture("FL1", FlightStatus::Delayed, true, true);
    let mut impacted = order_fixture("ORD1", "FL1", "pending", Some("TEAM_A"));
    impacted.planned_start_time = Some(Utc::now());
    let svc = actions(vec![flight], vec![impacted], vec![], vec![], vec![], vec![]);
    let result = svc
        .delay
        .suggest(&json!({"flight_id": "FL1"}))
        .await
        .expect("delay suggestion");
    let suggestion = &result["suggestion"];
    assert_eq!(suggestion["action_name"], "update_delay");
    assert!(suggestion["arguments"]["new_estimated_departure"].is_string());
    assert_eq!(result["related_dispatch_actions"][0]["dispatch_order_id"], "ORD1");
    assert_eq!(
        result["related_dispatch_actions"][0]["suggested_action"],
        "reschedule_after_new_departure"
    );
}

#[tokio::test]
async fn suggest_broadcast_has_no_side_effects_and_validates_scope() {
    let svc = empty();
    let result = svc
        .notification_broadcast
        .suggest(&json!({"title": "weather", "body": "snow", "scope": "on_duty_teams"}))
        .await
        .expect("broadcast suggestion");
    let suggestion = &result["suggestion"];
    assert_eq!(suggestion["action_name"], "send");
    assert_eq!(suggestion["object_id"], "broadcast");
    assert_eq!(suggestion["arguments"]["recipients"]["kind"], "team_status");
    assert!(suggestion["before_snapshot"].is_null(), "建议动作不得产生 before 状态");
    assert_eq!(result["side_effects"], "none until approval");

    let err = svc
        .notification_broadcast
        .suggest(&json!({"title": "a", "body": "b", "scope": "vip"}))
        .await
        .unwrap_err();
    assert!(matches!(err, OntologyActionError::InvalidArguments(_)));
    let err = svc
        .notification_broadcast
        .suggest(&json!({"title": "a", "body": "b", "scope": "department"}))
        .await
        .unwrap_err();
    assert!(matches!(err, OntologyActionError::InvalidArguments(_)));
}

fn user_fixture(id: &str, department_id: Option<&str>) -> fms_domain::models::user::User {
    let now = Utc::now();
    fms_domain::models::user::User {
        id: id.to_string(),
        email: format!("{id}@test"),
        password_hash: "secret-hash".to_string(),
        username: id.to_string(),
        display_name: Some(format!("{id} name")),
        roles: vec![],
        created_at: now,
        updated_at: now,
        last_login_at: None,
        is_active: true,
        is_verified: true,
        is_admin: false,
        verification_token: None,
        verification_token_expires: None,
        verified_at: None,
        password_reset_token: None,
        password_reset_token_expires: None,
        password_changed_at: None,
        department: None,
        department_id: department_id.map(|s| s.to_string()),
        job_level: None,
        job_title: Some("handler".to_string()),
        permission_version: 1,
        account_type: "personal".to_string(),
        login_enabled: true,
        current_occupant_user_id: None,
    }
}

fn personnel_runtime_fixture(user_id: &str) -> fms_domain::models::dispatch::PersonnelRuntime {
    fms_domain::models::dispatch::PersonnelRuntime {
        user_id: user_id.to_string(),
        current_status: fms_domain::models::dispatch::PersonnelStatus::OnDuty,
        current_stand_id: Some("S1".to_string()),
        current_position_lat: Some(1.0),
        current_position_lng: Some(2.0),
        last_position_update: Some(Utc::now()),
        updated_at: Some(Utc::now()),
        updated_by: None,
    }
}

#[tokio::test]
async fn personnel_get_context_returns_sanitized_profile_with_runtime_and_default_grants() {
    let svc = OntologyActionServices::new(
        Arc::new(FakeFlightRepo::default()),
        Arc::new(FakeDispatchRepo::default()),
        Arc::new(FakeAnomalyRepo::default()),
        Arc::new(FakeTeamRepo::default()),
        Arc::new(FakeStandRepo::default()),
        Arc::new(FakeOccupationRepo::default()),
        Arc::new(FakeBusinessCaseRepo),
        Arc::new(FakeUserRepo {
            users: std::sync::Mutex::new(vec![user_fixture("P1", Some("dept-1"))]),
        }),
        Arc::new(FakePersonnelRuntimeRepo {
            runtimes: std::sync::Mutex::new(vec![personnel_runtime_fixture("P1")]),
        }),
        Arc::new(FakeQualificationRepo::default()),
        Arc::new(FakeEquipmentRepo::default()),
    );

    let result = svc
        .personnel_context
        .get(&json!({"user_id": "P1"}))
        .await
        .expect("get_context");
    // 脱敏：绝不泄露密码哈希/令牌。
    assert_eq!(result["person"]["user_id"], "P1");
    assert!(result["person"].get("password_hash").is_none());
    assert_eq!(result["runtime"]["current_status"], "on_duty");
    assert_eq!(result["runtime"]["current_stand_id"], "S1");
    assert_eq!(result["qualification_grants"], json!([]));
    assert_eq!(result["evidence"]["ontology_version"], FLIGHT_OPS_ONTOLOGY_VERSION);
}

#[tokio::test]
async fn personnel_get_context_missing_person_is_not_found() {
    let svc = empty();
    let err = svc
        .personnel_context
        .get(&json!({"user_id": "MISSING"}))
        .await
        .unwrap_err();
    assert!(matches!(err, OntologyActionError::NotFound(_)));
}

fn team_with_member_fixture(id: &str, name: &str) -> Team {
    let mut team = team_fixture(id, name);
    team.members = vec![fms_domain::models::dispatch::TeamMember {
        id: format!("{id}-m1"),
        team_id: id.to_string(),
        user_id: "P1".to_string(),
        role: fms_domain::models::dispatch::MemberRole::Leader,
        can_drive: false,
        joined_at: Some(Utc::now()),
        left_at: None,
        is_active: true,
        username: Some("P1".to_string()),
        user_display_name: Some("P1 name".to_string()),
    }];
    team
}

#[tokio::test]
async fn team_get_context_returns_profile_with_active_members() {
    let svc = actions(
        vec![],
        vec![],
        vec![],
        vec![team_with_member_fixture("TEAM1", "Alpha")],
        vec![],
        vec![],
    );

    let result = svc
        .team_context
        .get(&json!({"team_id": "TEAM1"}))
        .await
        .expect("get_context");
    assert_eq!(result["team"]["team_id"], "TEAM1");
    assert_eq!(result["team"]["name"], "Alpha");
    assert_eq!(result["active_member_count"], 1);
    assert_eq!(result["members"][0]["user_id"], "P1");
    assert_eq!(result["evidence"]["ontology_version"], FLIGHT_OPS_ONTOLOGY_VERSION);
}

#[tokio::test]
async fn team_get_context_missing_team_is_not_found() {
    let svc = empty();
    let err = svc
        .team_context
        .get(&json!({"team_id": "MISSING"}))
        .await
        .unwrap_err();
    assert!(matches!(err, OntologyActionError::NotFound(_)));
}

fn equipment_fixture(id: &str, code: &str) -> fms_domain::models::dispatch::Equipment {
    fms_domain::models::dispatch::Equipment {
        id: id.to_string(),
        code: code.to_string(),
        equipment_type_id: Some("ET1".to_string()),
        department_id: None,
        name: Some("Tug".to_string()),
        license_plate: None,
        status: fms_domain::models::dispatch::EquipmentStatus::Available,
        current_position_lat: None,
        current_position_lng: None,
        current_stand_id: None,
        last_position_update: None,
        current_dispatch_id: None,
        last_maintenance_date: None,
        next_maintenance_date: None,
        metadata: None,
        created_at: None,
        updated_at: None,
        is_active: true,
        equipment_type: None,
    }
}

#[tokio::test]
async fn equipment_get_context_returns_profile_with_type() {
    let svc = OntologyActionServices::new(
        Arc::new(FakeFlightRepo::default()),
        Arc::new(FakeDispatchRepo::default()),
        Arc::new(FakeAnomalyRepo::default()),
        Arc::new(FakeTeamRepo::default()),
        Arc::new(FakeStandRepo::default()),
        Arc::new(FakeOccupationRepo::default()),
        Arc::new(FakeBusinessCaseRepo),
        Arc::new(FakeUserRepo::default()),
        Arc::new(FakePersonnelRuntimeRepo::default()),
        Arc::new(FakeQualificationRepo::default()),
        Arc::new(FakeEquipmentRepo {
            equipment: std::sync::Mutex::new(vec![equipment_fixture("EQ1", "TUG-01")]),
        }),
    );

    let result = svc
        .equipment_context
        .get(&json!({"equipment_id": "EQ1"}))
        .await
        .expect("get_context");
    assert_eq!(result["equipment"]["equipment_id"], "EQ1");
    assert_eq!(result["equipment"]["code"], "TUG-01");
    assert_eq!(result["equipment"]["equipment_type_id"], "ET1");
    // 未加载设备类型时返回 null（不强造默认）。
    assert!(result["equipment_type"].is_null());
    assert_eq!(result["evidence"]["ontology_version"], FLIGHT_OPS_ONTOLOGY_VERSION);
}

#[tokio::test]
async fn equipment_get_context_missing_equipment_is_not_found() {
    let svc = empty();
    let err = svc
        .equipment_context
        .get(&json!({"equipment_id": "MISSING"}))
        .await
        .unwrap_err();
    assert!(matches!(err, OntologyActionError::NotFound(_)));
}
