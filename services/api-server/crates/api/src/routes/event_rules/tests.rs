use super::*;

#[test]
fn preview_match_requires_event_pattern_and_conditions() {
    let conditions = json!({
        "operator": "AND",
        "children": [
            { "field": "is_vip", "op": "eq", "value": true },
            { "field": "delay_minutes", "op": "gte", "value": 30 }
        ]
    });
    let payload = json!({
        "is_vip": true,
        "delay_minutes": 45
    });

    assert!(rule_matches_preview(
        "flight.status_updated_v2",
        &["flight.status_updated_v2".to_string()],
        &Some(conditions),
        &payload,
    ));
}

#[test]
fn preview_match_rejects_failed_conditions() {
    let conditions = json!({
        "operator": "AND",
        "children": [
            { "field": "is_vip", "op": "eq", "value": true },
            { "field": "delay_minutes", "op": "gte", "value": 30 }
        ]
    });
    let payload = json!({
        "is_vip": true,
        "delay_minutes": 10
    });

    assert!(!rule_matches_preview(
        "flight.status_updated_v2",
        &["flight.status_updated_v2".to_string()],
        &Some(conditions),
        &payload,
    ));
}

#[test]
fn preview_generation_rule_builds_generated_order_preview() {
    let now = chrono::Utc::now();
    let rule = GenerationRuleRecord {
        id: "rule-generation-1".to_string(),
        generator_type: "event_generated".to_string(),
        name: "VIP delay support".to_string(),
        description: None,
        event_patterns: vec!["flight.status_updated_v2".to_string()],
        priority: 10,
        conditions: None,
        config: json!({
            "task_type": "vip_delay_support",
            "fixed_duration_minutes": 45,
            "crew_requirements": [{
                "slot_code": "coordinator",
                "qualification_code": "VIP",
                "required_count": 1
            }]
        }),
        is_enabled: true,
        department_id: Some("dept-service".to_string()),
        department_name: None,
        created_at: now,
        updated_at: now,
        created_by: None,
    };
    let payload = RulePreviewRequest {
        event_type: "flight.status_updated_v2".to_string(),
        flight_id: Some("flight-1".to_string()),
        payload: json!({
            "scheduled_time": "2026-05-12T08:00:00Z",
            "stand_id": "stand-1",
            "terminal": "T1"
        }),
    };

    let preview = build_generation_order_preview(&rule, &payload)
        .expect("generation order preview")
        .expect("matched generation rule should preview an order");

    assert_eq!(preview["flight_id"], json!("flight-1"));
    assert_eq!(preview["task_type"], json!("vip_delay_support"));
    assert_eq!(preview["stand_id"], json!("stand-1"));
    assert_eq!(preview["terminal"], json!("T1"));
    assert_eq!(preview["department_id"], json!("dept-service"));
    assert_eq!(preview["planned_start_time"], json!("2026-05-12T08:00:00Z"));
    assert_eq!(preview["planned_end_time"], json!("2026-05-12T08:45:00Z"));
    assert_eq!(
        preview["crew_requirement_snapshot"][0]["slot_code"],
        json!("coordinator")
    );
}
