use fms_domain::error::DomainError;
use fms_domain::models::dispatch::DispatchOrder;
use fms_domain::models::dispatch_collaboration::NotificationReceiptSummary;
use fms_domain::ports::dispatch_collaboration_repository::DispatchCollaborationRepository;
use serde_json::{json, Value};

use super::helpers::{
    dispatch_order_member_to_value, dispatch_type_value, driver_assignee_type_value, lock_level_value,
    non_empty_object_string, null_if_blank_with_default, order_status_value, resolve_effective_times,
    resolve_notification_receipt_summary, resolve_order_department, roster_team_projection, schedule_source_value,
};

pub(crate) fn is_workflow_pending_query(status: Option<&str>, source: Option<&str>) -> bool {
    matches!(status, Some(value) if value.eq_ignore_ascii_case("pending"))
        && matches!(source, Some(value) if value.eq_ignore_ascii_case("workflow"))
}

pub fn dispatch_order_to_value(order: &DispatchOrder) -> Value {
    dispatch_order_to_value_with_summary(order, None)
}

pub async fn serialize_orders_with_receipt_summaries(
    collaboration_repo: &(dyn DispatchCollaborationRepository + Send + Sync),
    orders: &[DispatchOrder],
) -> Result<Vec<Value>, DomainError> {
    if orders.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = orders.iter().map(|order| order.id.clone()).collect();
    let summaries = collaboration_repo.summarize_receipts_for_orders(&ids).await?;
    Ok(orders
        .iter()
        .map(|order| dispatch_order_to_value_with_summary(order, summaries.get(&order.id)))
        .collect())
}

pub fn dispatch_order_to_value_with_summary(
    order: &DispatchOrder,
    notification_receipt_summary: Option<&NotificationReceiptSummary>,
) -> Value {
    let (effective_start_time, effective_end_time, effective_end_source) = resolve_effective_times(order);
    let department = resolve_order_department(order);
    let (roster_team_id, roster_team_name) = roster_team_projection(order);
    let driver_bindings = order
        .equipment_assignment
        .iter()
        .filter_map(Value::as_object)
        .filter(|entry| {
            entry
                .get("driver_user_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .map(|value| !value.is_empty())
                .unwrap_or(false)
        })
        .map(|entry| {
            json!({
                "slot_code": entry.get("slot_code").unwrap_or(&Value::Null),
                "equipment_id": entry.get("equipment_id").unwrap_or(&Value::Null),
                "equipment_code": entry.get("equipment_code").unwrap_or(&Value::Null),
                "driver_user_id": entry.get("driver_user_id").unwrap_or(&Value::Null),
                "driver_username": entry.get("driver_username").unwrap_or(&Value::Null),
                "driver_source_team_id": entry.get("driver_source_team_id").unwrap_or(&Value::Null),
                "driver_source_team_name": entry.get("driver_source_team_name").unwrap_or(&Value::Null),
                "driver_qualification_level_code": entry.get("driver_qualification_level_code").unwrap_or(&Value::Null),
                "driver_from_task_crew": entry.get("driver_from_task_crew").unwrap_or(&Value::Null),
            })
        })
        .collect::<Vec<_>>();
    let driver_assignment = if !driver_bindings.is_empty()
        || order
            .driver_user_id
            .as_deref()
            .map(str::trim)
            .map(|value| !value.is_empty())
            .unwrap_or(false)
    {
        Some(json!({
            "driver_type": order
                .driver_type
                .map(driver_assignee_type_value)
                .unwrap_or_default(),
            "driver_user_id": order.driver_user_id,
            "bindings": driver_bindings,
        }))
    } else {
        None
    };
    let recommended_assignees = order
        .recommended_assignees
        .iter()
        .filter_map(|entry| entry.as_object())
        .filter(|entry| non_empty_object_string(entry, "user_id") && non_empty_object_string(entry, "username"))
        .map(|entry| {
            json!({
                "user_id": entry.get("user_id").cloned().unwrap_or_else(|| json!("")),
                "username": entry.get("username").cloned().unwrap_or_else(|| json!("")),
                "status": entry.get("status").cloned().unwrap_or_else(|| json!("offline")),
                "department": entry.get("department").unwrap_or(&Value::Null),
                "job_title": entry.get("job_title").unwrap_or(&Value::Null),
                "score": entry.get("score").cloned().unwrap_or_else(|| json!(0.0)),
                "reason": entry.get("reason").cloned().unwrap_or_else(|| json!("")),
                "workload": entry.get("workload").cloned().unwrap_or_else(|| json!(0)),
            })
        })
        .collect::<Vec<_>>();

    json!({
        "id": order.id,
        "flight_id": order.flight_id,
        "task_type": order.task_type,
        "task_type_name": order.task_type_name,
        "stand_id": order.stand_id,
        "stand_code": order.stand_code,
        "terminal": order.terminal,
        "department": department,
        "team_id": roster_team_id,
        "team_name": roster_team_name,
        "individual_user_id": order.individual_user_id,
        "individual_username": order.individual_username,
        "driver_type": order.driver_type.map(driver_assignee_type_value),
        "driver_user_id": order.driver_user_id,
        "driver_assignment": driver_assignment,
        "planned_start_time": order.planned_start_time,
        "planned_end_time": order.planned_end_time,
        "actual_start_time": order.actual_start_time,
        "actual_end_time": order.actual_end_time,
        "estimated_completion_time": order.estimated_completion_time,
        "estimated_completion_reported_by": order.estimated_completion_reported_by,
        "estimated_completion_reported_at": order.estimated_completion_reported_at,
        "estimated_completion_note": order.estimated_completion_note,
        "effective_start_time": effective_start_time,
        "effective_end_time": effective_end_time,
        "effective_end_source": effective_end_source,
        "gate": order.gate,
        "status": order_status_value(order.status),
        "dispatch_type": dispatch_type_value(order),
        "dispatched_at": order.dispatched_at,
        "estimated_arrival_minutes": order.estimated_arrival_minutes,
        "source": order.source,
        "schedule_source": schedule_source_value(order.schedule_source),
        "lock_level": lock_level_value(order.lock_level),
        "publication_state": null_if_blank_with_default(&order.publication_state, "published"),
        "source_type": null_if_blank_with_default(&order.source_type, "manual"),
        "department_id": order.department_id,
        "leg_scope": null_if_blank_with_default(&order.leg_scope, "none"),
        "generation_rule_id": order.generation_rule_id,
        "generation_rule_version": order.generation_rule_version,
        "generation_anchor_type": order.generation_anchor_type,
        "generation_anchor_time": order.generation_anchor_time,
        "publish_trigger_mode": order.publish_trigger_mode,
        "publish_at": order.publish_at,
        "turnaround_pair_key": order.turnaround_pair_key,
        "turnaround_constraint_mode": order.turnaround_constraint_mode,
        "department_rule_version": order.department_rule_version,
        "crew_requirement_snapshot": order.crew_requirement_snapshot,
        "equipment_requirement_snapshot": order.equipment_requirement_snapshot,
        "task_crew": match &order.task_crew {
            serde_json::Value::Object(map) if map.is_empty() => Value::Null,
            serde_json::Value::Null => Value::Null,
            _ => order.task_crew.clone(),
        },
        "equipment_assignment": order.equipment_assignment,
        "qualification_gap": order.qualification_gap,
        "equipment_gap": order.equipment_gap,
        "availability_reason": order.availability_reason,
        "score_breakdown": order.score_breakdown,
        "conflict_reason": order.conflict_reason,
        "origin_type": if order.source.eq_ignore_ascii_case("workflow") { "workflow" } else { "manual" },
        "origin_label": if order.source.eq_ignore_ascii_case("workflow") { "流程" } else { "人工" },
        "process_instance_id": order.process_instance_id,
        "process_task_id": order.process_task_id,
        "workflow_status": order.workflow_status,
        "workflow_context": order.workflow_context,
        "recommended_assignees": recommended_assignees,
        "recommendation_score": order.recommendation_score,
        "supervisor_notified": order.supervisor_notified,
        "supervisor_notified_at": order.supervisor_notified_at,
        "assignment_deadline": order.assignment_deadline,
        "completion_notes": order.completion_notes,
        "created_at": order.created_at,
        "members": order.members.iter().map(dispatch_order_member_to_value).collect::<Vec<_>>(),
        "equipment_codes": order.equipment_list.iter().map(|item| item.code.clone()).collect::<Vec<_>>(),
        "notification_receipt_summary": resolve_notification_receipt_summary(
            order,
            notification_receipt_summary,
        ),
    })
}
