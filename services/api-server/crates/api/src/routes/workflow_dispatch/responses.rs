use serde_json::{json, Value};

use fms_application::schemas::dispatch_schemas::DispatchRecommendationItem;
use fms_application::services::dispatch_query_service::dispatch_order_to_value;
use fms_domain::models::dispatch::DispatchOrder;

pub(crate) fn stored_recommendations(values: &[serde_json::Value]) -> Vec<DispatchRecommendationItem> {
    values
        .iter()
        .filter_map(|value| value.as_object())
        .filter_map(|item| {
            let user_id = item.get("user_id")?.as_str()?.trim();
            let username = item.get("username")?.as_str()?.trim();
            if user_id.is_empty() || username.is_empty() {
                return None;
            }

            Some(DispatchRecommendationItem {
                user_id: user_id.to_string(),
                username: username.to_string(),
                status: item
                    .get("status")
                    .and_then(|value| value.as_str())
                    .unwrap_or("offline")
                    .to_string(),
                department: item
                    .get("department")
                    .and_then(|value| value.as_str())
                    .map(ToOwned::to_owned),
                job_title: item
                    .get("job_title")
                    .and_then(|value| value.as_str())
                    .map(ToOwned::to_owned),
                score: item.get("score").and_then(|value| value.as_f64()).unwrap_or(0.0),
                reason: item
                    .get("reason")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_string(),
                workload: item.get("workload").and_then(|value| value.as_i64()).unwrap_or(0) as i32,
            })
        })
        .collect()
}

pub(crate) fn workflow_dispatch_order_to_value(order: &DispatchOrder) -> Value {
    let mut value = dispatch_order_to_value(order);
    let Some(payload) = value.as_object_mut() else {
        return value;
    };

    for key in [
        "driver_type",
        "driver_team_id",
        "driver_user_id",
        "driver_assignment",
        "estimated_completion_time",
        "estimated_completion_reported_by",
        "estimated_completion_reported_at",
        "estimated_completion_note",
        "effective_start_time",
        "effective_end_time",
        "effective_end_source",
        "gate",
        "department_id",
        "generation_rule_id",
        "generation_rule_version",
        "generation_anchor_type",
        "generation_anchor_time",
        "publish_trigger_mode",
        "publish_at",
        "turnaround_pair_key",
        "turnaround_constraint_mode",
        "department_rule_version",
        "availability_reason",
        "conflict_reason",
    ] {
        payload.insert(key.to_string(), Value::Null);
    }

    for (key, default) in [
        ("schedule_source", json!("current_status_fallback")),
        ("lock_level", json!("optimizable")),
        ("publication_state", json!("published")),
        ("source_type", json!("manual")),
        ("leg_scope", json!("none")),
    ] {
        payload.insert(key.to_string(), default);
    }

    for key in [
        "crew_requirement_snapshot",
        "equipment_requirement_snapshot",
        "equipment_assignment",
        "qualification_gap",
        "equipment_gap",
    ] {
        payload.insert(key.to_string(), json!([]));
    }
    payload.insert("task_crew".to_string(), Value::Null);
    payload.insert("score_breakdown".to_string(), json!({}));

    if let Some(members) = payload.get_mut("members").and_then(Value::as_array_mut) {
        for member in members {
            let Some(member_payload) = member.as_object_mut() else {
                continue;
            };
            member_payload.insert("slot_code".to_string(), Value::Null);
            member_payload.insert("qualification_code".to_string(), Value::Null);
            member_payload.insert("qualification_level_code".to_string(), Value::Null);
        }
    }

    value
}
