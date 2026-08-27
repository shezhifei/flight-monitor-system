use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use tracing::warn;

use crate::schemas::dispatch_schemas::*;
use crate::services::notification_service::DispatchBatchNotificationCreate;
use fms_domain::error::DomainError;
use fms_domain::models::anomaly::{AnomalySeverity, AnomalyType};
use fms_domain::models::dispatch::*;
use fms_domain::models::dispatch_collaboration::DispatchCollaborationEvent;

use super::{DispatchService, NULL_VALUE};

// ---------------------------------------------------------------------------
// Free functions (module-private helpers)
// ---------------------------------------------------------------------------

pub(super) fn optimal_order_status(order: &DispatchOrder) -> String {
    order.status.as_ref().to_string()
}

pub(super) fn optimal_order_has_assignment(order: &DispatchOrder) -> bool {
    order
        .individual_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some()
        || !order_member_user_ids(order).is_empty()
}

pub(super) fn order_to_response(o: &DispatchOrder) -> DispatchOrderResponse {
    let now = Utc::now();
    let (effective_start_time, effective_end_time) = effective_interval(o, now);
    DispatchOrderResponse {
        id: o.id.clone(),
        flight_id: o.flight_id.clone(),
        task_type: o.task_type.clone(),
        task_type_name: o.task_type_name.clone(),
        stand_id: o.stand_id.clone(),
        stand_code: o.stand_code.clone(),
        terminal: o.terminal.clone(),
        department: o.department.clone(),
        individual_user_id: o.individual_user_id.clone(),
        individual_username: o.individual_username.clone(),
        driver_type: o.driver_type.map(|value| value.as_ref().to_string()),
        driver_user_id: o.driver_user_id.clone(),
        driver_assignment: None,
        planned_start_time: o.planned_start_time,
        planned_end_time: o.planned_end_time,
        actual_start_time: o.actual_start_time,
        actual_end_time: o.actual_end_time,
        estimated_completion_time: o.estimated_completion_time,
        estimated_completion_reported_by: o.estimated_completion_reported_by.clone(),
        estimated_completion_reported_at: o.estimated_completion_reported_at,
        estimated_completion_note: o.estimated_completion_note.clone(),
        effective_start_time: Some(effective_start_time),
        effective_end_time: Some(effective_end_time),
        effective_end_source: None,
        gate: o.gate.clone(),
        status: o.status.as_ref().to_string(),
        dispatch_type: o.dispatch_type.as_ref().to_string(),
        dispatched_at: o.dispatched_at,
        estimated_arrival_minutes: o.estimated_arrival_minutes,
        source: o.source.clone(),
        schedule_source: o.schedule_source.as_ref().to_string(),
        lock_level: o.lock_level.as_ref().to_string(),
        publication_state: o.publication_state.clone(),
        source_type: o.source_type.clone(),
        department_id: o.department_id.clone(),
        leg_scope: o.leg_scope.clone(),
        generation_rule_id: o.generation_rule_id.clone(),
        generation_rule_version: o.generation_rule_version,
        generation_anchor_type: o.generation_anchor_type.clone(),
        generation_anchor_time: o.generation_anchor_time,
        completion_time_mode: o.completion_time_mode.clone(),
        completion_anchor_type: o.completion_anchor_type.clone(),
        completion_anchor_time: o.completion_anchor_time,
        completion_offset_minutes: o.completion_offset_minutes,
        completion_warning_lead_minutes: o.completion_warning_lead_minutes,
        publish_trigger_mode: o.publish_trigger_mode.clone(),
        publish_at: o.publish_at,
        turnaround_pair_key: o.turnaround_pair_key.clone(),
        turnaround_constraint_mode: o.turnaround_constraint_mode.clone(),
        department_rule_version: o.department_rule_version.clone(),
        crew_requirement_snapshot: o.crew_requirement_snapshot.clone(),
        equipment_requirement_snapshot: o.equipment_requirement_snapshot.clone(),
        task_crew: match &o.task_crew {
            serde_json::Value::Object(map) if !map.is_empty() => serde_json::from_value(o.task_crew.clone()).ok(),
            _ => None,
        },
        equipment_assignment: o.equipment_assignment.clone(),
        qualification_gap: o.qualification_gap.clone(),
        equipment_gap: o.equipment_gap.clone(),
        availability_reason: o.availability_reason.clone(),
        score_breakdown: match &o.score_breakdown {
            serde_json::Value::Object(map) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            _ => HashMap::new(),
        },
        conflict_reason: o.conflict_reason.clone(),
        origin_type: if o.source.trim().eq_ignore_ascii_case("workflow") {
            "workflow".to_string()
        } else {
            "manual".to_string()
        },
        origin_label: if o.source.trim().eq_ignore_ascii_case("workflow") {
            "流程".to_string()
        } else {
            "人工".to_string()
        },
        process_instance_id: o.process_instance_id.clone(),
        process_task_id: o.process_task_id.clone(),
        workflow_status: Some(o.workflow_status.clone()),
        workflow_context: match &o.workflow_context {
            serde_json::Value::Object(map) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            _ => HashMap::new(),
        },
        recommended_assignees: Vec::new(),
        recommendation_score: o.recommendation_score,
        supervisor_notified: o.supervisor_notified,
        supervisor_notified_at: o.supervisor_notified_at,
        assignment_deadline: o.assignment_deadline,
        completion_notes: o.completion_notes.clone(),
        created_at: o.created_at,
        members: o
            .members
            .iter()
            .map(|item| DispatchOrderMemberResponse {
                id: item.id.clone(),
                user_id: item.user_id.clone(),
                role: item.role.as_ref().to_string(),
                source_type: item.source_type.as_ref().to_string(),
                source_team_id: item.source_team_id.clone(),
                slot_code: item.slot_code.clone(),
                qualification_code: item.qualification_code.clone(),
                qualification_level_code: item.qualification_level_code.clone(),
                assigned_at: item.assigned_at,
                check_in_time: item.check_in_time,
                check_out_time: item.check_out_time,
                is_active: item.is_active,
                username: item.username.clone(),
            })
            .collect(),
        equipment_codes: o.equipment_list.iter().map(|e| e.code.clone()).collect(),
        notification_receipt_summary: HashMap::new(),
    }
}

pub(super) fn effective_interval(order: &DispatchOrder, fallback_now: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
    let start = order
        .actual_start_time
        .or(order.planned_start_time)
        .or(order.assignment_deadline)
        .or(order.created_at)
        .unwrap_or(fallback_now);
    let mut end = order
        .actual_end_time
        .or(order.estimated_completion_time)
        .or(order.planned_end_time)
        .unwrap_or(start + Duration::minutes(20));
    if end <= start {
        end = start + Duration::minutes(8);
    }
    (start, end)
}

pub(super) fn normalize_duration(duration: Duration) -> Duration {
    if duration <= Duration::zero() {
        Duration::minutes(8)
    } else {
        duration
    }
}

pub(super) fn order_member_user_ids(order: &DispatchOrder) -> Vec<String> {
    let mut user_ids = Vec::new();
    let mut seen = HashSet::new();
    if let Some(user_id) = order
        .individual_user_id
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        if seen.insert(user_id) {
            user_ids.push(user_id.to_string());
        }
    }

    for member in order.members.iter().filter(|member| member.is_active) {
        let user_id = member.user_id.trim();
        if !user_id.is_empty() && seen.insert(user_id) {
            user_ids.push(user_id.to_string());
        }
    }

    user_ids
}

fn overlapping_member_user_ids(left: &DispatchOrder, right: &DispatchOrder) -> Vec<String> {
    let left_user_ids = order_member_user_ids(left);
    let right_user_ids = order_member_user_ids(right).into_iter().collect::<HashSet<_>>();
    let mut overlapping = left_user_ids
        .into_iter()
        .filter(|user_id| right_user_ids.contains(user_id))
        .collect::<Vec<_>>();
    overlapping.sort();
    overlapping.dedup();
    overlapping
}

pub(super) fn eta_conflict_kinds(current: &DispatchOrder, candidate: &DispatchOrder) -> Vec<String> {
    let mut conflict_kinds = Vec::new();

    if current.individual_user_id.is_some() && current.individual_user_id == candidate.individual_user_id {
        conflict_kinds.push("individual_overlap".to_string());
    }
    if !overlapping_member_user_ids(current, candidate).is_empty()
        && !(current.individual_user_id.is_some() && current.individual_user_id == candidate.individual_user_id)
    {
        conflict_kinds.push("person_time_overlap".to_string());
    }
    if current.stand_id.is_some() && current.stand_id == candidate.stand_id {
        conflict_kinds.push("stand_overlap".to_string());
    }

    conflict_kinds.sort();
    conflict_kinds.dedup();
    conflict_kinds
}

pub(super) fn describe_conflict_kinds(conflict_kinds: &[String]) -> String {
    let labels = conflict_kinds
        .iter()
        .map(|kind| match kind.as_str() {
            "team_overlap" => "班组时间冲突",
            "individual_overlap" => "执行人重叠",
            "person_time_overlap" => "成员编组重叠",
            "stand_overlap" => "机位窗口重叠",
            _ => "资源冲突",
        })
        .collect::<Vec<_>>();
    labels.join("/")
}

/// 地球表面两点间距离（米）
pub(super) fn haversine_distance(lat1: f64, lng1: f64, lat2: f64, lng2: f64) -> f64 {
    const R: f64 = 6_371_000.0; // 地球半径（米）
    let d_lat = (lat2 - lat1).to_radians();
    let d_lng = (lng2 - lng1).to_radians();
    let a =
        (d_lat / 2.0).sin().powi(2) + lat1.to_radians().cos() * lat2.to_radians().cos() * (d_lng / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();
    R * c
}

pub(super) fn equipment_distance_sort_key(equipment: &Equipment, stand_position: Option<(f64, f64)>) -> f64 {
    let Some((stand_lat, stand_lng)) = stand_position else {
        return f64::MAX / 2.0;
    };
    let Some(lat) = equipment.current_position_lat else {
        return f64::MAX / 2.0;
    };
    let Some(lng) = equipment.current_position_lng else {
        return f64::MAX / 2.0;
    };
    haversine_distance(lat, lng, stand_lat, stand_lng)
}
