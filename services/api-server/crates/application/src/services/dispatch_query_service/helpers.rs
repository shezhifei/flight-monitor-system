use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use fms_domain::models::dispatch::{DispatchOrder, DispatchOrderStatus};
use fms_domain::models::dispatch_collaboration::NotificationReceiptSummary;
use serde_json::{json, Value};

use super::types::{NormalizedTimelineOrder, TimelineItem, TimelineLane};

pub(crate) fn dispatch_order_member_to_value(member: &fms_domain::models::dispatch::DispatchOrderMember) -> Value {
    json!({
        "id": member.id,
        "user_id": member.user_id,
        "role": match member.role {
            fms_domain::models::dispatch::MemberRole::Leader => "leader",
            fms_domain::models::dispatch::MemberRole::Member => "member",
            fms_domain::models::dispatch::MemberRole::Driver => "driver",
        },
        "source_type": match member.source_type {
            fms_domain::models::dispatch::AssigneeType::Team => "team",
            fms_domain::models::dispatch::AssigneeType::Individual => "individual",
        },
        "source_team_id": member.source_team_id,
        "slot_code": member.slot_code,
        "qualification_code": member.qualification_code,
        "qualification_level_code": member.qualification_level_code,
        "assigned_at": member.assigned_at,
        "check_in_time": member.check_in_time,
        "check_out_time": member.check_out_time,
        "is_active": member.is_active,
        "username": member.username,
    })
}

/// 工单列表只读班组投影：来自名册 `source_team_*`，不是 order.team_id。
pub(crate) fn roster_team_projection(order: &DispatchOrder) -> (Option<String>, Option<String>) {
    let mut ids = Vec::new();
    let mut names = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut seen_names = HashSet::new();

    for member in &order.members {
        if let Some(id) = member
            .source_team_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if seen_ids.insert(id.to_string()) {
                ids.push(id.to_string());
            }
        }
    }

    if let Some(obj) = order.task_crew.as_object() {
        if let Some(arr) = obj.get("source_team_ids").and_then(Value::as_array) {
            for value in arr {
                if let Some(id) = value.as_str().map(str::trim).filter(|value| !value.is_empty()) {
                    if seen_ids.insert(id.to_string()) {
                        ids.push(id.to_string());
                    }
                }
            }
        }
        if let Some(arr) = obj.get("source_team_names").and_then(Value::as_array) {
            for value in arr {
                if let Some(name) = value.as_str().map(str::trim).filter(|value| !value.is_empty()) {
                    if seen_names.insert(name.to_string()) {
                        names.push(name.to_string());
                    }
                }
            }
        }
        if let Some(arr) = obj.get("members").and_then(Value::as_array) {
            for member in arr {
                if let Some(id) = member
                    .get("source_team_id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    if seen_ids.insert(id.to_string()) {
                        ids.push(id.to_string());
                    }
                }
                if let Some(name) = member
                    .get("source_team_name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    if seen_names.insert(name.to_string()) {
                        names.push(name.to_string());
                    }
                }
            }
        }
    }

    (
        ids.first().cloned(),
        if names.is_empty() {
            None
        } else {
            Some(names.join(" / "))
        },
    )
}

fn source_team_name_for_user(order: &DispatchOrder, user_id: &str) -> Option<String> {
    order
        .task_crew
        .as_object()
        .and_then(|obj| obj.get("members"))
        .and_then(Value::as_array)
        .and_then(|members| {
            members.iter().find(|member| {
                member
                    .get("user_id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    == Some(user_id)
            })
        })
        .and_then(|member| {
            member
                .get("source_team_name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
}

pub(crate) fn non_empty_object_string(entry: &serde_json::Map<String, Value>, key: &str) -> bool {
    entry
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .map(|value| !value.is_empty())
        .unwrap_or(false)
}

pub(crate) fn null_if_blank_with_default(value: &str, default: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

pub(crate) fn resolve_notification_receipt_summary(
    _order: &DispatchOrder,
    summary: Option<&NotificationReceiptSummary>,
) -> Value {
    if let Some(summary) = summary {
        return notification_receipt_summary_to_value(summary);
    }

    json!({})
}

pub(crate) fn notification_receipt_summary_to_value(summary: &NotificationReceiptSummary) -> Value {
    json!({
        "total_count": summary.total_count,
        "pending_count": summary.pending_count,
        "acknowledged_count": summary.acknowledged_count,
        "rejected_count": summary.rejected_count,
        "latest_updated_at": summary.latest_updated_at.clone(),
        "receipt_group_ids": summary.receipt_group_ids.clone(),
    })
}

pub(crate) fn schedule_source_value(value: fms_domain::models::dispatch::ScheduleSource) -> &'static str {
    match value {
        fms_domain::models::dispatch::ScheduleSource::ShiftInstance => "shift_instance",
        fms_domain::models::dispatch::ScheduleSource::CurrentStatusFallback => "current_status_fallback",
    }
}

pub(crate) fn lock_level_value(value: fms_domain::models::dispatch::DispatchLockLevel) -> &'static str {
    match value {
        fms_domain::models::dispatch::DispatchLockLevel::Active => "active",
        fms_domain::models::dispatch::DispatchLockLevel::Frozen => "frozen",
        fms_domain::models::dispatch::DispatchLockLevel::ManualLock => "manual_lock",
        fms_domain::models::dispatch::DispatchLockLevel::Optimizable => "optimizable",
    }
}

pub(crate) fn resolve_window(
    window_start: Option<DateTime<Utc>>,
    window_end: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> (DateTime<Utc>, DateTime<Utc>) {
    match (window_start, window_end) {
        (None, None) => (now - Duration::minutes(60), now + Duration::minutes(360)),
        (Some(start), None) => (start, start + Duration::minutes(360)),
        (None, Some(end)) => (end - Duration::minutes(420), end),
        (Some(start), Some(end)) if end <= start => (start, start + Duration::minutes(360)),
        (Some(start), Some(end)) => (start, end),
    }
}

pub(crate) fn effective_interval(order: &DispatchOrder, fallback_now: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
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

pub(crate) fn order_member_user_ids(order: &DispatchOrder) -> Vec<String> {
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
        let normalized_user_id = member.user_id.trim();
        if !normalized_user_id.is_empty() && seen.insert(normalized_user_id) {
            user_ids.push(normalized_user_id.to_string());
        }
    }

    user_ids
}

pub(crate) fn intersecting_member_user_ids(left: &DispatchOrder, right: &DispatchOrder) -> Vec<String> {
    let left_user_ids = order_member_user_ids(left);
    let right_user_ids = order_member_user_ids(right).into_iter().collect::<HashSet<_>>();
    let mut overlapping_user_ids = left_user_ids
        .into_iter()
        .filter(|user_id| right_user_ids.contains(user_id))
        .collect::<Vec<_>>();
    overlapping_user_ids.sort();
    overlapping_user_ids.dedup();
    overlapping_user_ids
}

pub(crate) fn resolve_effective_times(
    order: &DispatchOrder,
) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>, Option<String>) {
    let effective_start = order
        .actual_start_time
        .or(order.planned_start_time)
        .or(order.assignment_deadline)
        .or(order.created_at);

    if order.actual_end_time.is_some() {
        return (
            effective_start,
            order.actual_end_time,
            Some("actual_end_time".to_string()),
        );
    }
    if order.estimated_completion_time.is_some() {
        return (
            effective_start,
            order.estimated_completion_time,
            Some("estimated_completion_time".to_string()),
        );
    }
    if order.planned_end_time.is_some() {
        return (
            effective_start,
            order.planned_end_time,
            Some("planned_end_time".to_string()),
        );
    }
    (
        effective_start,
        effective_start,
        effective_start.map(|_| "effective_start_time".to_string()),
    )
}

pub(crate) fn resolve_order_department(order: &DispatchOrder) -> Option<String> {
    order
        .workflow_context
        .get("target_department")
        .and_then(Value::as_str)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| order.department.clone().filter(|value| !value.trim().is_empty()))
}

pub(crate) fn order_status_value(status: DispatchOrderStatus) -> &'static str {
    match status {
        DispatchOrderStatus::Pending => "pending",
        DispatchOrderStatus::Assigned => "assigned",
        DispatchOrderStatus::InProgress => "in_progress",
        DispatchOrderStatus::Completed => "completed",
        DispatchOrderStatus::Cancelled => "cancelled",
    }
}

pub(crate) fn dispatch_type_value(order: &DispatchOrder) -> &'static str {
    match order.dispatch_type {
        fms_domain::models::dispatch::DispatchType::Auto => "auto",
        fms_domain::models::dispatch::DispatchType::Manual => "manual",
    }
}

pub(crate) fn driver_assignee_type_value(assignee_type: fms_domain::models::dispatch::AssigneeType) -> &'static str {
    match assignee_type {
        fms_domain::models::dispatch::AssigneeType::Team => "team",
        fms_domain::models::dispatch::AssigneeType::Individual => "individual",
    }
}

pub(crate) fn normalize_order_for_timeline(order: &DispatchOrder) -> NormalizedTimelineOrder {
    let now = Utc::now();
    let (start_time, end_time) = effective_interval(order, now);
    let (_, _, effective_end_source) = resolve_effective_times(order);
    let members = order
        .members
        .iter()
        .map(|member| {
            json!({
                "id": member.user_id,
                "user_id": member.user_id,
                "name": member.username,
                "username": member.username,
                "source_team_id": member.source_team_id,
                "source_team_name": source_team_name_for_user(order, &member.user_id),
                "slot_code": member.slot_code,
            })
        })
        .collect::<Vec<_>>();
    let equipments = order
        .equipment_list
        .iter()
        .map(|equipment| {
            json!({
                "id": equipment.id,
                "name": equipment.code,
            })
        })
        .collect::<Vec<_>>();
    let member_names = order
        .members
        .iter()
        .filter_map(|member| member.username.clone())
        .collect::<Vec<_>>();
    let equipment_codes = order
        .equipment_list
        .iter()
        .map(|equipment| equipment.code.clone())
        .collect::<Vec<_>>();

    let assignee_text = order
        .individual_username
        .clone()
        .or_else(|| member_names.first().cloned())
        .unwrap_or_else(|| "未分配".to_string());
    let equipment_text = if equipment_codes.is_empty() {
        "无设备".to_string()
    } else {
        equipment_codes.join("/")
    };

    NormalizedTimelineOrder {
        order_id: order.id.clone(),
        flight_id: order.flight_id.clone(),
        flight_no: order.flight_id.clone(),
        task_type: order.task_type.clone(),
        task_type_name: order.task_type_name.clone().unwrap_or_else(|| order.task_type.clone()),
        status: order_status_value(order.status).to_string(),
        start_time,
        end_time,
        planned_start_time: order.planned_start_time,
        planned_end_time: order.planned_end_time,
        actual_start_time: order.actual_start_time,
        actual_end_time: order.actual_end_time,
        estimated_completion_time: order.estimated_completion_time,
        estimated_completion_reported_by: order.estimated_completion_reported_by.clone(),
        estimated_completion_reported_at: order.estimated_completion_reported_at,
        estimated_completion_note: order.estimated_completion_note.clone(),
        effective_end_source: effective_end_source.unwrap_or_else(|| "planned_end_time".to_string()),
        individual_user_id: order.individual_user_id.clone(),
        individual_username: order.individual_username.clone(),
        stand_id: order.stand_id.clone(),
        stand_code: order.stand_code.clone(),
        gate: order.gate.clone(),
        terminal: order.terminal.clone(),
        source: order.source.clone(),
        dispatch_type: dispatch_type_value(order).to_string(),
        members,
        equipments,
        member_names,
        equipment_codes,
        display_label: format!(
            "{} | {} | {} | {}",
            order.flight_id,
            order.task_type_name.clone().unwrap_or_else(|| order.task_type.clone()),
            assignee_text,
            equipment_text
        ),
    }
}

pub(crate) fn build_flight_items(orders: &[NormalizedTimelineOrder]) -> Vec<TimelineItem> {
    orders
        .iter()
        .map(|order| TimelineItem {
            id: order.order_id.clone(),
            order_id: Some(order.order_id.clone()),
            flight_id: order.flight_id.clone(),
            flight_no: order.flight_no.clone(),
            task_type: Some(order.task_type.clone()),
            task_type_name: order.task_type_name.clone(),
            status: order.status.clone(),
            start_time: order.start_time,
            end_time: order.end_time,
            planned_start_time: order.planned_start_time,
            planned_end_time: order.planned_end_time,
            actual_start_time: order.actual_start_time,
            actual_end_time: order.actual_end_time,
            estimated_completion_time: order.estimated_completion_time,
            estimated_completion_reported_by: order.estimated_completion_reported_by.clone(),
            estimated_completion_reported_at: order.estimated_completion_reported_at,
            estimated_completion_note: order.estimated_completion_note.clone(),
            effective_end_source: order.effective_end_source.clone(),
            lane_key: String::new(),
            lane_label: String::new(),
            lane_index: 0,
            lane_subtrack: 0,
            lane_subtrack_count: 1,
            individual_user_id: order.individual_user_id.clone(),
            individual_username: order.individual_username.clone(),
            stand_id: order.stand_id.clone(),
            stand_code: order.stand_code.clone(),
            gate: order.gate.clone(),
            terminal: order.terminal.clone(),
            source: order.source.clone(),
            dispatch_type: order.dispatch_type.clone(),
            members: order.members.clone(),
            equipments: order.equipments.clone(),
            member_names: order.member_names.clone(),
            equipment_codes: order.equipment_codes.clone(),
            label: order.display_label.clone(),
            is_flight_summary: false,
            related_order_ids: vec![order.order_id.clone()],
            related_orders: Vec::new(),
            focus_user_id: None,
            focus_user_name: None,
            focus_equipment_id: None,
            focus_equipment_code: None,
        })
        .collect()
}

pub(crate) fn build_flight_summary_items(orders: &[NormalizedTimelineOrder]) -> Vec<TimelineItem> {
    let mut grouped: HashMap<String, Vec<&NormalizedTimelineOrder>> = HashMap::new();
    for order in orders {
        grouped.entry(order.flight_id.clone()).or_default().push(order);
    }

    let mut result = Vec::new();
    for (flight_id, items) in grouped {
        let start_time = items.iter().map(|item| item.start_time).min().unwrap_or_else(Utc::now);
        let end_time = items.iter().map(|item| item.end_time).max().unwrap_or(start_time);
        let first = items[0];
        result.push(TimelineItem {
            id: format!("flight-summary:{flight_id}"),
            order_id: None,
            flight_id: flight_id.clone(),
            flight_no: first.flight_no.clone(),
            task_type: None,
            task_type_name: format!("{}项保障任务", items.len()),
            status: derive_group_status(items.iter().map(|item| item.status.as_str()).collect()),
            start_time,
            end_time,
            planned_start_time: None,
            planned_end_time: None,
            actual_start_time: None,
            actual_end_time: None,
            estimated_completion_time: None,
            estimated_completion_reported_by: None,
            estimated_completion_reported_at: None,
            estimated_completion_note: None,
            effective_end_source: "planned_end_time".to_string(),
            lane_key: String::new(),
            lane_label: String::new(),
            lane_index: 0,
            lane_subtrack: 0,
            lane_subtrack_count: 1,
            individual_user_id: None,
            individual_username: None,
            stand_id: first.stand_id.clone(),
            stand_code: first.stand_code.clone(),
            gate: first.gate.clone(),
            terminal: first.terminal.clone(),
            source: first.source.clone(),
            dispatch_type: first.dispatch_type.clone(),
            members: Vec::new(),
            equipments: Vec::new(),
            member_names: Vec::new(),
            equipment_codes: Vec::new(),
            label: first.flight_no.clone(),
            is_flight_summary: true,
            related_order_ids: items.iter().map(|item| item.order_id.clone()).collect(),
            related_orders: items
                .iter()
                .map(|item| {
                    json!({
                        "order_id": item.order_id,
                        "task_type_name": item.task_type_name,
                        "status": item.status,
                        "start_time": item.start_time.to_rfc3339(),
                        "end_time": item.end_time.to_rfc3339(),
                    })
                })
                .collect(),
            focus_user_id: None,
            focus_user_name: None,
            focus_equipment_id: None,
            focus_equipment_code: None,
        });
    }
    result.sort_by_key(|item| (item.start_time, item.flight_no.clone()));
    result
}

pub(crate) fn build_employee_view_items(orders: &[NormalizedTimelineOrder]) -> Vec<TimelineItem> {
    let mut items = Vec::new();
    for order in orders {
        let mut candidates = order
            .members
            .iter()
            .filter_map(|item| {
                Some((
                    item.get("id")?.as_str()?.to_string(),
                    item.get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("未分配人员")
                        .to_string(),
                ))
            })
            .collect::<Vec<_>>();
        if let Some(user_id) = &order.individual_user_id {
            if !candidates.iter().any(|(candidate_id, _)| candidate_id == user_id) {
                candidates.push((
                    user_id.clone(),
                    order
                        .individual_username
                        .clone()
                        .unwrap_or_else(|| "未分配人员".to_string()),
                ));
            }
        }
        if candidates.is_empty() {
            candidates.push(("__unassigned__".to_string(), "未分配人员".to_string()));
        }

        for (user_id, user_name) in candidates {
            let Some(mut item) = build_flight_items(std::slice::from_ref(order)).into_iter().next() else {
                tracing::warn!(
                    order_id = %order.order_id,
                    "employee timeline view skipped order because no flight timeline item was produced"
                );
                continue;
            };
            item.id = format!("{}:employee:{user_id}", order.order_id);
            item.lane_key = format!("employee:{user_id}");
            item.lane_label = user_name.clone();
            item.focus_user_id = if user_id == "__unassigned__" {
                None
            } else {
                Some(user_id)
            };
            item.focus_user_name = Some(user_name);
            items.push(item);
        }
    }
    items
}

pub(crate) fn build_equipment_view_items(orders: &[NormalizedTimelineOrder]) -> Vec<TimelineItem> {
    let mut items = Vec::new();
    for order in orders {
        let mut equipments = order
            .equipments
            .iter()
            .filter_map(|item| {
                Some((
                    item.get("id")?.as_str()?.to_string(),
                    item.get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("未分配设备")
                        .to_string(),
                ))
            })
            .collect::<Vec<_>>();
        if equipments.is_empty() {
            equipments.push(("__unassigned__".to_string(), "未分配设备".to_string()));
        }

        for (equipment_id, equipment_name) in equipments {
            let Some(mut item) = build_flight_items(std::slice::from_ref(order)).into_iter().next() else {
                tracing::warn!(
                    order_id = %order.order_id,
                    "equipment timeline view skipped order because no flight timeline item was produced"
                );
                continue;
            };
            item.id = format!("{}:equipment:{equipment_id}", order.order_id);
            item.lane_key = format!("equipment:{equipment_id}");
            item.lane_label = equipment_name.clone();
            item.focus_equipment_id = if equipment_id == "__unassigned__" {
                None
            } else {
                Some(equipment_id)
            };
            item.focus_equipment_code = Some(equipment_name);
            items.push(item);
        }
    }
    items
}

pub(crate) fn layout_dynamic_tracks(mut items: Vec<TimelineItem>) -> (Vec<TimelineLane>, Vec<TimelineItem>) {
    items.sort_by_key(|item| (item.start_time, item.end_time, item.id.clone()));
    let mut lane_end_times: Vec<DateTime<Utc>> = Vec::new();
    let mut lane_item_counts: HashMap<usize, usize> = HashMap::new();

    for item in &mut items {
        let mut lane_index = 0usize;
        while lane_index < lane_end_times.len() && item.start_time < lane_end_times[lane_index] {
            lane_index += 1;
        }

        if lane_index == lane_end_times.len() {
            lane_end_times.push(item.end_time);
        } else {
            lane_end_times[lane_index] = lane_end_times[lane_index].max(item.end_time);
        }

        *lane_item_counts.entry(lane_index).or_insert(0) += 1;
        item.lane_key = format!("flight-track-{}", lane_index + 1);
        item.lane_label = format!("时间轨道 {}", lane_index + 1);
        item.lane_index = lane_index;
        item.lane_subtrack = 0;
        item.lane_subtrack_count = 1;
    }

    let lanes = lane_end_times
        .iter()
        .enumerate()
        .map(|(index, _)| TimelineLane {
            id: format!("flight-track-{}", index + 1),
            label: format!("时间轨道 {}", index + 1),
            index,
            subtrack_count: 1,
            item_count: *lane_item_counts.get(&index).unwrap_or(&0),
        })
        .collect::<Vec<_>>();

    (lanes, items)
}

pub(crate) fn layout_fixed_lanes(mut items: Vec<TimelineItem>) -> (Vec<TimelineLane>, Vec<TimelineItem>) {
    let mut grouped: HashMap<String, Vec<TimelineItem>> = HashMap::new();
    let mut lane_labels: HashMap<String, String> = HashMap::new();

    for item in items.drain(..) {
        lane_labels
            .entry(item.lane_key.clone())
            .or_insert_with(|| item.lane_label.clone());
        grouped.entry(item.lane_key.clone()).or_default().push(item);
    }

    let mut lane_keys = grouped.keys().cloned().collect::<Vec<_>>();
    lane_keys.sort_by_key(|key| {
        (
            key.contains("__unassigned__"),
            lane_labels.get(key).cloned().unwrap_or_default(),
            key.clone(),
        )
    });

    let mut lanes = Vec::new();
    let mut layout_items = Vec::new();
    for (lane_index, lane_key) in lane_keys.into_iter().enumerate() {
        let mut lane_items = grouped.remove(&lane_key).unwrap_or_default();
        lane_items.sort_by_key(|item| (item.start_time, item.end_time, item.id.clone()));

        let mut subtrack_end_times: Vec<DateTime<Utc>> = Vec::new();
        for item in &mut lane_items {
            let mut subtrack_index = 0usize;
            while subtrack_index < subtrack_end_times.len() && item.start_time < subtrack_end_times[subtrack_index] {
                subtrack_index += 1;
            }

            if subtrack_index == subtrack_end_times.len() {
                subtrack_end_times.push(item.end_time);
            } else {
                subtrack_end_times[subtrack_index] = subtrack_end_times[subtrack_index].max(item.end_time);
            }

            item.lane_index = lane_index;
            item.lane_subtrack = subtrack_index;
            item.lane_subtrack_count = subtrack_end_times.len().max(1);
        }

        let subtrack_count = subtrack_end_times.len().max(1);
        for item in &mut lane_items {
            item.lane_subtrack_count = subtrack_count;
        }
        lanes.push(TimelineLane {
            id: lane_key.clone(),
            label: lane_labels.get(&lane_key).cloned().unwrap_or_else(|| lane_key.clone()),
            index: lane_index,
            subtrack_count,
            item_count: lane_items.len(),
        });
        layout_items.extend(lane_items);
    }

    layout_items.sort_by_key(|item| (item.lane_index, item.start_time, item.id.clone()));
    (lanes, layout_items)
}

pub(crate) fn derive_group_status(statuses: Vec<&str>) -> String {
    let priority = |status: &str| match status {
        "in_progress" => 5,
        "pending" => 4,
        "assigned" => 3,
        "completed" => 2,
        "cancelled" => 1,
        _ => 0,
    };
    statuses
        .into_iter()
        .max_by_key(|status| priority(status))
        .unwrap_or("pending")
        .to_string()
}

pub(crate) fn build_status_counts(orders: &[NormalizedTimelineOrder]) -> HashMap<String, usize> {
    let mut counts = HashMap::from([
        ("pending".to_string(), 0usize),
        ("assigned".to_string(), 0usize),
        ("in_progress".to_string(), 0usize),
        ("completed".to_string(), 0usize),
        ("cancelled".to_string(), 0usize),
    ]);
    for order in orders {
        *counts.entry(order.status.clone()).or_insert(0) += 1;
    }
    counts
}

pub(crate) fn build_status_orders(
    orders: &[NormalizedTimelineOrder],
    order_focus_map: &HashMap<String, String>,
    flight_focus_map: &HashMap<String, String>,
) -> HashMap<String, Vec<Value>> {
    let mut result: HashMap<String, Vec<Value>> = HashMap::new();
    for order in orders {
        let focus_item_id = order_focus_map
            .get(&order.order_id)
            .cloned()
            .or_else(|| flight_focus_map.get(&order.flight_id).cloned());
        result.entry(order.status.clone()).or_default().push(json!({
            "order_id": order.order_id,
            "flight_id": order.flight_id,
            "flight_no": order.flight_no,
            "task_type_name": order.task_type_name,
            "status": order.status,
            "label": format!("{} | {}", order.flight_no, order.task_type_name),
            "start_time": order.start_time.to_rfc3339(),
            "end_time": order.end_time.to_rfc3339(),
            "effective_end_source": order.effective_end_source,
            "focus_item_id": focus_item_id,
        }));
    }
    result
}

pub(crate) fn serialize_lane(lane: &TimelineLane) -> Value {
    json!({
        "id": lane.id,
        "label": lane.label,
        "index": lane.index,
        "subtrack_count": lane.subtrack_count,
        "item_count": lane.item_count,
    })
}

pub(crate) fn serialize_timeline_item(item: &TimelineItem) -> Value {
    json!({
        "id": item.id,
        "order_id": item.order_id,
        "flight_id": item.flight_id,
        "flight_no": item.flight_no,
        "task_type": item.task_type,
        "task_type_name": item.task_type_name,
        "status": item.status,
        "start_time": item.start_time.to_rfc3339(),
        "end_time": item.end_time.to_rfc3339(),
        "planned_start_time": item.planned_start_time,
        "planned_end_time": item.planned_end_time,
        "actual_start_time": item.actual_start_time,
        "actual_end_time": item.actual_end_time,
        "estimated_completion_time": item.estimated_completion_time,
        "estimated_completion_reported_by": item.estimated_completion_reported_by,
        "estimated_completion_reported_at": item.estimated_completion_reported_at,
        "estimated_completion_note": item.estimated_completion_note,
        "effective_start_time": item.start_time,
        "effective_end_time": item.end_time,
        "effective_end_source": item.effective_end_source,
        "lane_id": item.lane_key,
        "lane_label": item.lane_label,
        "lane_index": item.lane_index,
        "lane_subtrack": item.lane_subtrack,
        "lane_subtrack_count": item.lane_subtrack_count,
        "individual_user_id": item.individual_user_id,
        "individual_username": item.individual_username,
        "stand_id": item.stand_id,
        "stand_code": item.stand_code,
        "gate": item.gate,
        "terminal": item.terminal,
        "source": item.source,
        "dispatch_type": item.dispatch_type,
        "members": item.members,
        "equipments": item.equipments,
        "member_names": item.member_names,
        "equipment_codes": item.equipment_codes,
        "label": item.label,
        "is_flight_summary": item.is_flight_summary,
        "related_order_ids": item.related_order_ids,
        "related_orders": item.related_orders,
        "focus_user_id": item.focus_user_id,
        "focus_user_name": item.focus_user_name,
        "focus_equipment_id": item.focus_equipment_id,
        "focus_equipment_code": item.focus_equipment_code,
        "team_id": timeline_roster_team_id(&item.members),
        "team_name": timeline_roster_team_name(&item.members),
    })
}

fn timeline_roster_team_id(members: &[Value]) -> Option<String> {
    let mut ids = Vec::new();
    let mut seen = HashSet::new();
    for member in members {
        if let Some(id) = member
            .get("source_team_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if seen.insert(id.to_string()) {
                ids.push(id.to_string());
            }
        }
    }
    ids.first().cloned()
}

fn timeline_roster_team_name(members: &[Value]) -> Option<String> {
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    for member in members {
        if let Some(name) = member
            .get("source_team_name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if seen.insert(name.to_string()) {
                names.push(name.to_string());
            }
        }
    }
    if names.is_empty() {
        None
    } else {
        Some(names.join(" / "))
    }
}

pub(crate) fn build_conflict(
    conflict_type: &str,
    severity: &str,
    resource_id: Option<String>,
    resource_name: Option<String>,
    related_dispatch_order_ids: Vec<String>,
    message: &str,
    suggested_action: Option<String>,
    context: Value,
) -> Value {
    json!({
        "conflict_type": conflict_type,
        "severity": severity,
        "resource_id": resource_id,
        "resource_name": resource_name,
        "related_dispatch_order_ids": related_dispatch_order_ids,
        "message": message,
        "suggested_action": suggested_action,
        "context": context,
    })
}

pub(crate) fn deduplicate_conflicts(items: Vec<Value>, limit: usize) -> Vec<Value> {
    let mut unique: HashMap<(String, String, String), Value> = HashMap::new();
    for item in items {
        let conflict_type = item
            .get("conflict_type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let resource_id = item
            .get("resource_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let related = item
            .get("related_dispatch_order_ids")
            .and_then(Value::as_array)
            .map(|values| {
                let mut ids = values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToString::to_string)
                    .collect::<Vec<_>>();
                ids.sort();
                ids.join(",")
            })
            .unwrap_or_default();
        unique.insert((conflict_type, resource_id, related), item);
    }

    let mut items = unique.into_values().collect::<Vec<_>>();
    items.sort_by_key(|item| {
        std::cmp::Reverse(match item.get("severity").and_then(Value::as_str).unwrap_or("low") {
            "critical" => 4,
            "high" => 3,
            "medium" => 2,
            _ => 1,
        })
    });
    items.truncate(limit);
    items
}

pub(crate) fn round_to_2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}
