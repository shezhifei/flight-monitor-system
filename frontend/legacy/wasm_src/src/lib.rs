use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

#[derive(Debug, Deserialize)]
struct ClusterRequest {
    cluster_id: String,
    #[serde(default)]
    solver_version: Option<String>,
    #[serde(default)]
    model_version: Option<String>,
    #[serde(default)]
    objective_config: ObjectiveConfig,
    #[serde(default)]
    optimizable_orders: Vec<OrderInput>,
    #[serde(default)]
    fixed_anchor_orders: Vec<OrderInput>,
    #[serde(default)]
    employee_anchor_states: Vec<AnchorState>,
    #[serde(default)]
    equipment_anchor_states: Vec<AnchorState>,
    #[serde(default)]
    employee_free_windows: Vec<FreeWindow>,
    #[serde(default)]
    equipment_free_windows: Vec<FreeWindow>,
    #[serde(default)]
    resource_travel_edges: Vec<TravelEdge>,
    #[serde(default)]
    turnaround_pairs: Vec<TurnaroundPair>,
}

#[derive(Debug, Deserialize, Default)]
struct ObjectiveConfig {
    #[serde(default)]
    timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct OrderInput {
    order_id: String,
    #[serde(default)]
    flight_id: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    conflict_state: Option<String>,
    #[serde(default)]
    order_class: Option<String>,
    #[serde(default)]
    planned_start_time: Option<String>,
    #[serde(default)]
    planned_end_time: Option<String>,
    #[serde(default)]
    required_start_time: Option<String>,
    #[serde(default)]
    effective_start_time: Option<String>,
    #[serde(default)]
    effective_end_time: Option<String>,
    #[serde(default)]
    stand_id: Option<String>,
    #[serde(default)]
    baseline_assignment: AssignmentSummary,
    #[serde(default)]
    current_assignment: Option<AssignmentSummary>,
    #[serde(default)]
    personnel_slots: Vec<PersonnelSlot>,
    #[serde(default)]
    equipment_slots: Vec<EquipmentSlot>,
    #[serde(default)]
    is_locked: bool,
}

#[derive(Debug, Deserialize, Clone, Default, Serialize)]
struct AssignmentSummary {
    #[serde(default)]
    assignee_type: Option<String>,
    #[serde(default)]
    team_id: Option<String>,
    #[serde(default)]
    individual_user_id: Option<String>,
    #[serde(default)]
    equipment_ids: Vec<String>,
    #[serde(default)]
    member_user_ids: Vec<String>,
    #[serde(default)]
    department_rule_version: Option<String>,
    #[serde(default)]
    crew_requirement_snapshot: Vec<serde_json::Value>,
    #[serde(default)]
    equipment_requirement_snapshot: Vec<serde_json::Value>,
    #[serde(default)]
    qualification_gap: Vec<serde_json::Value>,
    #[serde(default)]
    task_crew: TaskCrewSummary,
    #[serde(default)]
    personnel_slot_assignments: Vec<PersonnelSlotAssignment>,
    #[serde(default)]
    equipment_slot_assignments: Vec<EquipmentSlotAssignment>,
}

#[derive(Debug, Deserialize, Clone, Default, Serialize)]
struct TaskCrewSummary {
    #[serde(default)]
    members: Vec<TaskCrewMember>,
}

#[derive(Debug, Deserialize, Clone, Default, Serialize)]
struct TaskCrewMember {
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    slot_code: Option<String>,
    #[serde(default)]
    qualification_code: Option<String>,
    #[serde(default)]
    qualification_level_code: Option<String>,
    #[serde(default)]
    source_team_id: Option<String>,
    #[serde(default)]
    source_team_name: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Default, Serialize)]
struct PersonnelSlotAssignment {
    slot_code: String,
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    qualification_code: Option<String>,
    #[serde(default)]
    qualification_level_code: Option<String>,
    #[serde(default)]
    source_team_id: Option<String>,
    #[serde(default)]
    source_team_name: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Default, Serialize)]
struct EquipmentSlotAssignment {
    slot_code: String,
    #[serde(default)]
    equipment_id: Option<String>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    equipment_type_id: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct PersonnelSlot {
    slot_code: String,
    #[serde(default)]
    qualification_code: Option<String>,
    #[serde(default)]
    qualification_level_code: Option<String>,
    #[serde(default)]
    candidate_user_ids: Vec<String>,
    #[serde(default)]
    baseline_user_id: Option<String>,
    #[serde(default)]
    scarcity_cost: f64,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct EquipmentSlot {
    slot_code: String,
    #[serde(default)]
    equipment_type_id: Option<String>,
    #[serde(default)]
    candidate_equipment_ids: Vec<String>,
    #[serde(default)]
    baseline_equipment_id: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct AnchorState {
    resource_type: String,
    resource_id: String,
    #[serde(default)]
    location_stand_id: Option<String>,
    #[serde(default)]
    free_windows: Vec<FreeWindow>,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct FreeWindow {
    resource_type: String,
    resource_id: String,
    #[serde(default)]
    window_start: Option<String>,
    #[serde(default)]
    window_end: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct TravelEdge {
    resource_type: String,
    resource_id: String,
    from_node: String,
    to_node: String,
    #[serde(default)]
    travel_minutes: i64,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct TurnaroundPair {
    pair_key: String,
    inbound_order_id: String,
    outbound_order_id: String,
    inbound_slot_code: String,
    outbound_slot_code: String,
    #[serde(default)]
    hard_continuity_required: bool,
    #[serde(default)]
    continuity_penalty_weight: f64,
}

#[derive(Debug, Serialize, Default, Clone)]
struct OrderObjectiveBreakdown {
    slot_gap: i64,
    lateness_minutes: i64,
    continuity_penalty: f64,
    baseline_change: i64,
    travel_cost: i64,
    scarcity_cost: f64,
    load_cost: f64,
}

#[derive(Debug, Serialize, Default, Clone)]
struct ObjectiveBreakdown {
    slot_gap: i64,
    total_lateness_minutes: i64,
    continuity_break: i64,
    continuity_penalty: f64,
    baseline_change: i64,
    travel_cost: i64,
    scarcity_cost: f64,
    load_deviation: f64,
}

#[derive(Debug, Serialize, Default, Clone)]
struct MemberChangeSummary {
    changed_member_count: i64,
}

#[derive(Debug, Serialize, Default, Clone)]
struct StartTimesSummary {
    planned_start_time: Option<String>,
    planned_end_time: Option<String>,
    required_start_time: Option<String>,
    suggested_start_time: Option<String>,
    suggested_end_time: Option<String>,
}

#[derive(Debug, Serialize, Default, Clone)]
struct LatenessSummary {
    minutes: i64,
    starts_after_required_time: bool,
}

#[derive(Debug, Serialize, Default, Clone)]
struct GapSummary {
    personnel_slot_gap: i64,
    equipment_slot_gap: i64,
    total_slot_gap: i64,
}

#[derive(Debug, Serialize, Default, Clone)]
struct ContinuitySummary {
    relevant_pair_count: i64,
    satisfied_pair_count: i64,
    broken_pair_count: i64,
    hard_broken_pair_count: i64,
    penalty_applied: f64,
}

#[derive(Debug, Serialize, Default, Clone)]
struct ChangeSummary {
    baseline_change_count: i64,
    changed_personnel_slot_count: i64,
    changed_equipment_slot_count: i64,
}

#[derive(Debug, Serialize, Default, Clone)]
struct TravelSummary {
    travel_minutes: i64,
}

#[derive(Debug, Serialize, Default, Clone)]
struct PersonnelSlotAssignmentResult {
    dispatch_order_id: String,
    slot_code: String,
    user_id: Option<String>,
    username: Option<String>,
    source_team_id: Option<String>,
    source_team_name: Option<String>,
    qualification_code: Option<String>,
    qualification_level_code: Option<String>,
    baseline_user_id: Option<String>,
    changed: bool,
}

#[derive(Debug, Serialize, Default, Clone)]
struct EquipmentSlotAssignmentResult {
    dispatch_order_id: String,
    slot_code: String,
    equipment_id: Option<String>,
    code: Option<String>,
    equipment_type_id: Option<String>,
    baseline_equipment_id: Option<String>,
    changed: bool,
}

#[derive(Debug, Serialize, Default, Clone)]
struct ContinuityDecision {
    pair_key: String,
    inbound_order_id: String,
    outbound_order_id: String,
    inbound_slot_code: String,
    outbound_slot_code: String,
    satisfied: bool,
    hard_continuity_required: bool,
    penalty_applied: f64,
}

#[derive(Debug, Serialize, Default, Clone)]
struct OrderResult {
    dispatch_order_id: String,
    reason: String,
    suggestion_type: String,
    order_class: String,
    original_start_time: Option<String>,
    original_end_time: Option<String>,
    suggested_start_time: Option<String>,
    suggested_end_time: Option<String>,
    lateness_minutes: i64,
    gap_count: i64,
    travel_minutes: i64,
    baseline_change_count: i64,
    impact_score: f64,
    current_assignment: AssignmentSummary,
    suggested_assignment: AssignmentSummary,
    task_crew: TaskCrewSummary,
    crew_requirement_snapshot: Vec<serde_json::Value>,
    qualification_gap: Vec<serde_json::Value>,
    member_change_summary: MemberChangeSummary,
    requires_manual_confirmation: bool,
    start_times: StartTimesSummary,
    lateness: LatenessSummary,
    gap_summary: GapSummary,
    continuity_summary: ContinuitySummary,
    change_summary: ChangeSummary,
    travel_summary: TravelSummary,
    personnel_slot_assignments: Vec<PersonnelSlotAssignmentResult>,
    equipment_slot_assignments: Vec<EquipmentSlotAssignmentResult>,
    continuity_decisions: Vec<ContinuityDecision>,
    objective_breakdown: OrderObjectiveBreakdown,
}

#[derive(Debug, Serialize, Default)]
struct SolverRunMetadata {
    solver: String,
    solver_mode: String,
    solver_version: String,
    model_version: String,
    feasible: bool,
    timed_out: bool,
    timeout_ms: u64,
    total_lateness_minutes: i64,
    unresolved_assigned_conflict_order_ids: Vec<String>,
    unassigned_unplanned_order_ids: Vec<String>,
    objective_values: ObjectiveBreakdown,
}

#[derive(Debug, Serialize, Default)]
struct ClusterResponse {
    cluster_id: String,
    order_results: Vec<OrderResult>,
    personnel_slot_assignments: Vec<PersonnelSlotAssignmentResult>,
    equipment_slot_assignments: Vec<EquipmentSlotAssignmentResult>,
    continuity_decisions: Vec<ContinuityDecision>,
    objective_breakdown: ObjectiveBreakdown,
    solver_run_metadata: SolverRunMetadata,
}

#[derive(Debug, Clone)]
struct Reservation {
    order_id: String,
    start_ms: i64,
    end_ms: i64,
}

type Calendars = HashMap<String, Vec<Reservation>>;
type WindowsByResource = HashMap<String, Vec<FreeWindow>>;
type TravelLookup = HashMap<String, i64>;

#[wasm_bindgen]
pub fn solve_cluster(input_json: &str) -> Result<String, JsValue> {
    console_error_panic_hook::set_once();
    let payload: ClusterRequest = serde_json::from_str(input_json)
        .map_err(|err| JsValue::from_str(&format!("invalid cluster input: {err}")))?;
    let timeout_ms = payload.objective_config.timeout_ms.unwrap_or(4000);
    let model_version = payload.model_version.unwrap_or_else(|| "dispatch_wasm_full_model_v1".to_string());
    let solver_version = payload.solver_version.unwrap_or_else(|| "dispatch_solver_wasm_full_model_v1".to_string());
    let travel_lookup = build_travel_lookup(&payload.resource_travel_edges);
    let windows = build_windows_lookup(&payload.employee_anchor_states, &payload.equipment_anchor_states, &payload.employee_free_windows, &payload.equipment_free_windows);
    let mut calendars = Calendars::new();
    for order in &payload.fixed_anchor_orders {
        if let Some((start_ms, end_ms)) = order_interval(order) {
            reserve_fixed_order(&mut calendars, order, start_ms, end_ms);
        }
    }
    let mut order_results = payload.optimizable_orders.iter().map(|order| plan_order(order, &mut calendars, &windows, &travel_lookup)).collect::<Vec<_>>();
    order_results.sort_by(|left, right| String::cmp(&left.dispatch_order_id, &right.dispatch_order_id));
    let continuity_decisions = build_continuity(&payload.turnaround_pairs, &order_results);
    order_results.iter_mut().for_each(|item| {
        let relevant = continuity_decisions.iter().filter(|pair| {
            pair.inbound_order_id == item.dispatch_order_id || pair.outbound_order_id == item.dispatch_order_id
        }).cloned().collect::<Vec<_>>();
        let broken_pair_count = relevant.iter().filter(|pair| !pair.satisfied).count() as i64;
        let hard_broken_pair_count = relevant.iter().filter(|pair| !pair.satisfied && pair.hard_continuity_required).count() as i64;
        let satisfied_pair_count = relevant.iter().filter(|pair| pair.satisfied).count() as i64;
        let penalty_applied = relevant.iter().map(|pair| pair.penalty_applied).sum::<f64>();
        item.continuity_summary = ContinuitySummary {
            relevant_pair_count: relevant.len() as i64,
            satisfied_pair_count,
            broken_pair_count,
            hard_broken_pair_count,
            penalty_applied,
        };
        item.objective_breakdown.continuity_penalty = penalty_applied;
        item.impact_score += penalty_applied;
        item.continuity_decisions = relevant;
    });
    let objective_breakdown = summarize_objective(&order_results, &continuity_decisions);
    let personnel_slot_assignments = order_results.iter().flat_map(|item| item.personnel_slot_assignments.clone()).collect::<Vec<_>>();
    let equipment_slot_assignments = order_results.iter().flat_map(|item| item.equipment_slot_assignments.clone()).collect::<Vec<_>>();
    let response = ClusterResponse {
        cluster_id: payload.cluster_id,
        personnel_slot_assignments,
        equipment_slot_assignments,
        continuity_decisions: continuity_decisions.clone(),
        objective_breakdown: objective_breakdown.clone(),
        solver_run_metadata: SolverRunMetadata {
            solver: "dispatch_solver_wasm_full_model_v1".to_string(),
            solver_mode: "frontend_wasm".to_string(),
            solver_version,
            model_version,
            feasible: true,
            timed_out: false,
            timeout_ms,
            total_lateness_minutes: objective_breakdown.total_lateness_minutes,
            unresolved_assigned_conflict_order_ids: order_results.iter().filter(|item| item.order_class == "assigned_conflict" && item.gap_count > 0).map(|item| item.dispatch_order_id.clone()).collect(),
            unassigned_unplanned_order_ids: order_results.iter().filter(|item| item.order_class == "unassigned" && item.gap_count > 0).map(|item| item.dispatch_order_id.clone()).collect(),
            objective_values: objective_breakdown,
        },
        order_results,
    };
    serde_json::to_string(&response).map_err(|err| JsValue::from_str(&format!("serialize result failed: {err}")))
}

fn plan_order(order: &OrderInput, calendars: &mut Calendars, windows: &WindowsByResource, travel_lookup: &TravelLookup) -> OrderResult {
    let (planned_start_ms, planned_end_ms) = order_interval(order).unwrap_or((0, 15 * 60_000));
    let duration_ms = (planned_end_ms - planned_start_ms).max(5 * 60_000);
    let order_class = order.order_class.clone().unwrap_or_else(|| match order.conflict_state.as_deref().unwrap_or("none") {
        "resource_conflict" => "assigned_conflict".to_string(),
        "gap" => "unassigned".to_string(),
        _ if order.is_locked => "locked".to_string(),
        _ => "unassigned".to_string(),
    });
    let current_assignment = order.current_assignment.clone().unwrap_or_else(|| order.baseline_assignment.clone());
    let (personnel_slot_assignments, equipment_slot_assignments, start_ms, travel_minutes, gap_count, scarcity_cost, load_cost) =
        choose_assignment(order, planned_start_ms, duration_ms, calendars, windows, travel_lookup);
    let suggested_assignment = build_assignment(order, &personnel_slot_assignments, &equipment_slot_assignments);
    reserve_assignment(calendars, &personnel_slot_assignments, &equipment_slot_assignments, start_ms, start_ms + duration_ms, &order.order_id);
    let lateness_minutes = ((start_ms - planned_start_ms).max(0)) / 60_000;
    let changed_personnel_slot_count = personnel_slot_assignments.iter().filter(|item| item.changed).count() as i64;
    let changed_equipment_slot_count = equipment_slot_assignments.iter().filter(|item| item.changed).count() as i64;
    let baseline_change_count = changed_personnel_slot_count + changed_equipment_slot_count;
    let personnel_slot_gap = order.personnel_slots.iter().filter(|slot| {
        !personnel_slot_assignments.iter().any(|item| item.slot_code == slot.slot_code && item.user_id.is_some())
    }).count() as i64;
    let equipment_slot_gap = order.equipment_slots.iter().filter(|slot| {
        !equipment_slot_assignments.iter().any(|item| item.slot_code == slot.slot_code && item.equipment_id.is_some())
    }).count() as i64;
    let suggestion_type = match order_class.as_str() {
        "assigned_conflict" => "assigned_conflict_resolution".to_string(),
        "unassigned" if lateness_minutes > 0 => "unassigned_late_assignment".to_string(),
        _ => "unassigned_new_assignment".to_string(),
    };
    let qualification_gap = if gap_count > 0 {
        order.personnel_slots.iter().filter_map(|slot| {
            let assigned = personnel_slot_assignments.iter().any(|item| item.slot_code == slot.slot_code && item.user_id.is_some());
            if assigned { None } else {
                Some(serde_json::json!({
                    "slot_code": slot.slot_code,
                    "qualification_code": slot.qualification_code,
                    "qualification_level_code": slot.qualification_level_code,
                }))
            }
        }).collect()
    } else {
        Vec::new()
    };
    let task_crew = TaskCrewSummary {
        members: personnel_slot_assignments.iter().filter_map(|item| item.user_id.clone().map(|user_id| TaskCrewMember {
            user_id: Some(user_id.clone()),
            username: item.username.clone().or_else(|| Some(user_id)),
            slot_code: Some(item.slot_code.clone()),
            qualification_code: item.qualification_code.clone(),
            qualification_level_code: item.qualification_level_code.clone(),
            source_team_id: item.source_team_id.clone(),
            source_team_name: item.source_team_name.clone(),
        })).collect(),
    };
    OrderResult {
        dispatch_order_id: order.order_id.clone(),
        reason: "solver_assignment".to_string(),
        suggestion_type,
        order_class,
        original_start_time: order.planned_start_time.clone(),
        original_end_time: order.planned_end_time.clone(),
        suggested_start_time: iso_from_ms(start_ms),
        suggested_end_time: iso_from_ms(start_ms + duration_ms),
        lateness_minutes,
        gap_count,
        travel_minutes,
        baseline_change_count,
        impact_score: (gap_count * 100 + lateness_minutes * 10 + baseline_change_count * 5 + travel_minutes) as f64,
        current_assignment,
        suggested_assignment,
        task_crew,
        crew_requirement_snapshot: order.baseline_assignment.crew_requirement_snapshot.clone(),
        qualification_gap,
        member_change_summary: MemberChangeSummary { changed_member_count: baseline_change_count },
        requires_manual_confirmation: gap_count > 0 || baseline_change_count >= 2,
        start_times: StartTimesSummary {
            planned_start_time: order.planned_start_time.clone(),
            planned_end_time: order.planned_end_time.clone(),
            required_start_time: order.required_start_time.clone(),
            suggested_start_time: iso_from_ms(start_ms),
            suggested_end_time: iso_from_ms(start_ms + duration_ms),
        },
        lateness: LatenessSummary {
            minutes: lateness_minutes,
            starts_after_required_time: parse_ms(order.required_start_time.as_ref())
                .is_some_and(|required_start_ms| start_ms > required_start_ms),
        },
        gap_summary: GapSummary {
            personnel_slot_gap,
            equipment_slot_gap,
            total_slot_gap: gap_count,
        },
        continuity_summary: ContinuitySummary::default(),
        change_summary: ChangeSummary {
            baseline_change_count,
            changed_personnel_slot_count,
            changed_equipment_slot_count,
        },
        travel_summary: TravelSummary {
            travel_minutes,
        },
        personnel_slot_assignments,
        equipment_slot_assignments,
        continuity_decisions: Vec::new(),
        objective_breakdown: OrderObjectiveBreakdown {
            slot_gap: gap_count,
            lateness_minutes,
            continuity_penalty: 0.0,
            baseline_change: baseline_change_count,
            travel_cost: travel_minutes,
            scarcity_cost,
            load_cost,
        },
    }
}

fn choose_assignment(
    order: &OrderInput,
    planned_start_ms: i64,
    duration_ms: i64,
    calendars: &Calendars,
    windows: &WindowsByResource,
    travel_lookup: &TravelLookup,
) -> (
    Vec<PersonnelSlotAssignmentResult>,
    Vec<EquipmentSlotAssignmentResult>,
    i64,
    i64,
    i64,
    f64,
    f64,
) {
    let mut personnel = Vec::new();
    let mut equipment = Vec::new();
    let mut selected_resources = Vec::new();
    let mut gap_count = 0_i64;
    let mut scarcity_cost = 0.0_f64;
    let mut load_cost = 0.0_f64;
    for slot in &order.personnel_slots {
        let user_id = slot.baseline_user_id.clone().or_else(|| slot.candidate_user_ids.first().cloned());
        if let Some(value) = user_id.clone() {
            selected_resources.push(("employee".to_string(), value.clone(), slot.slot_code.clone()));
            scarcity_cost += slot.scarcity_cost;
            load_cost += calendars.get(&resource_key("employee", &value)).map(|items| items.len() as f64).unwrap_or(0.0);
        } else {
            gap_count += 1;
        }
        personnel.push(PersonnelSlotAssignmentResult {
            dispatch_order_id: order.order_id.clone(),
            slot_code: slot.slot_code.clone(),
            user_id: user_id.clone(),
            username: user_id.clone(),
            source_team_id: order.baseline_assignment.team_id.clone(),
            source_team_name: None,
            qualification_code: slot.qualification_code.clone(),
            qualification_level_code: slot.qualification_level_code.clone(),
            baseline_user_id: slot.baseline_user_id.clone(),
            changed: user_id != slot.baseline_user_id,
        });
    }
    for slot in &order.equipment_slots {
        let equipment_id = slot.baseline_equipment_id.clone().or_else(|| slot.candidate_equipment_ids.first().cloned());
        if let Some(value) = equipment_id.clone() {
            selected_resources.push(("equipment".to_string(), value.clone(), slot.slot_code.clone()));
            load_cost += calendars.get(&resource_key("equipment", &value)).map(|items| items.len() as f64).unwrap_or(0.0);
        } else {
            gap_count += 1;
        }
        equipment.push(EquipmentSlotAssignmentResult {
            dispatch_order_id: order.order_id.clone(),
            slot_code: slot.slot_code.clone(),
            equipment_id: equipment_id.clone(),
            code: equipment_id.clone(),
            equipment_type_id: slot.equipment_type_id.clone(),
            baseline_equipment_id: slot.baseline_equipment_id.clone(),
            changed: equipment_id != slot.baseline_equipment_id,
        });
    }
    let mut start_ms = planned_start_ms;
    let mut travel_minutes_total = 0_i64;
    for _ in 0..4 {
        let mut changed = false;
        for (resource_type, resource_id, _slot_code) in &selected_resources {
            if let Some((resource_start, resource_travel)) = earliest_resource_start(resource_type, resource_id, &order.order_id, planned_start_ms, duration_ms, calendars, windows, travel_lookup) {
                travel_minutes_total += resource_travel;
                if resource_start > start_ms {
                    start_ms = resource_start;
                    changed = true;
                }
            } else {
                gap_count += 1;
            }
        }
        if !changed {
            break;
        }
    }
    (personnel, equipment, start_ms, travel_minutes_total, gap_count, scarcity_cost, load_cost)
}

fn earliest_resource_start(
    resource_type: &str,
    resource_id: &str,
    order_id: &str,
    planned_start_ms: i64,
    duration_ms: i64,
    calendars: &Calendars,
    windows: &WindowsByResource,
    travel_lookup: &TravelLookup,
) -> Option<(i64, i64)> {
    let windows_for_resource = windows.get(&resource_key(resource_type, resource_id))?;
    let reservations = calendars.get(&resource_key(resource_type, resource_id)).cloned().unwrap_or_default();
    let order_node = format!("order:{order_id}");
    for window in windows_for_resource {
        let window_start_ms = parse_ms(window.window_start.as_ref()).unwrap_or(planned_start_ms);
        let window_end_ms = parse_ms(window.window_end.as_ref()).unwrap_or(planned_start_ms + 24 * 60 * 60_000);
        let mut candidate_start_ms = planned_start_ms.max(window_start_ms);
        let mut previous_node = format!("anchor:{resource_type}:{resource_id}");
        let mut previous_end_ms = window_start_ms;
        for reservation in reservations.iter().filter(|item| item.end_ms > window_start_ms && item.start_ms < window_end_ms) {
            let arrival_travel = travel_minutes(travel_lookup, resource_type, resource_id, &previous_node, &order_node);
            candidate_start_ms = candidate_start_ms.max(previous_end_ms + arrival_travel * 60_000);
            let travel_to_next = travel_minutes(travel_lookup, resource_type, resource_id, &order_node, &format!("order:{}", reservation.order_id));
            if candidate_start_ms + duration_ms + travel_to_next * 60_000 <= reservation.start_ms && candidate_start_ms + duration_ms <= window_end_ms {
                return Some((candidate_start_ms, arrival_travel));
            }
            previous_end_ms = reservation.end_ms;
            previous_node = format!("order:{}", reservation.order_id);
        }
        let arrival_travel = travel_minutes(travel_lookup, resource_type, resource_id, &previous_node, &order_node);
        candidate_start_ms = candidate_start_ms.max(previous_end_ms + arrival_travel * 60_000);
        if candidate_start_ms + duration_ms <= window_end_ms {
            return Some((candidate_start_ms, arrival_travel));
        }
    }
    None
}

fn build_assignment(
    order: &OrderInput,
    personnel: &[PersonnelSlotAssignmentResult],
    equipment: &[EquipmentSlotAssignmentResult],
) -> AssignmentSummary {
    let personnel_slot_assignments = personnel.iter().map(|item| PersonnelSlotAssignment {
        slot_code: item.slot_code.clone(),
        user_id: item.user_id.clone(),
        username: item.username.clone(),
        qualification_code: item.qualification_code.clone(),
        qualification_level_code: item.qualification_level_code.clone(),
        source_team_id: item.source_team_id.clone(),
        source_team_name: item.source_team_name.clone(),
    }).collect::<Vec<_>>();
    let equipment_slot_assignments = equipment.iter().map(|item| EquipmentSlotAssignment {
        slot_code: item.slot_code.clone(),
        equipment_id: item.equipment_id.clone(),
        code: item.code.clone(),
        equipment_type_id: item.equipment_type_id.clone(),
    }).collect::<Vec<_>>();
    AssignmentSummary {
        assignee_type: if personnel_slot_assignments.len() <= 1 { Some("individual".to_string()) } else { Some("team".to_string()) },
        team_id: order.baseline_assignment.team_id.clone(),
        individual_user_id: personnel_slot_assignments.iter().find_map(|item| item.user_id.clone()),
        equipment_ids: equipment_slot_assignments.iter().filter_map(|item| item.equipment_id.clone()).collect(),
        member_user_ids: personnel_slot_assignments.iter().filter_map(|item| item.user_id.clone()).collect(),
        department_rule_version: order.baseline_assignment.department_rule_version.clone(),
        crew_requirement_snapshot: order.baseline_assignment.crew_requirement_snapshot.clone(),
        equipment_requirement_snapshot: order.baseline_assignment.equipment_requirement_snapshot.clone(),
        qualification_gap: Vec::new(),
        task_crew: TaskCrewSummary {
            members: personnel_slot_assignments.iter().filter_map(|item| item.user_id.clone().map(|user_id| TaskCrewMember {
                user_id: Some(user_id.clone()),
                username: item.username.clone().or_else(|| Some(user_id)),
                slot_code: Some(item.slot_code.clone()),
                qualification_code: item.qualification_code.clone(),
                qualification_level_code: item.qualification_level_code.clone(),
                source_team_id: item.source_team_id.clone(),
                source_team_name: item.source_team_name.clone(),
            })).collect(),
        },
        personnel_slot_assignments,
        equipment_slot_assignments,
    }
}

fn reserve_fixed_order(calendars: &mut Calendars, order: &OrderInput, start_ms: i64, end_ms: i64) {
    let personnel = order.baseline_assignment.personnel_slot_assignments.iter().map(|item| PersonnelSlotAssignmentResult {
        dispatch_order_id: order.order_id.clone(),
        slot_code: item.slot_code.clone(),
        user_id: item.user_id.clone(),
        username: item.username.clone(),
        source_team_id: item.source_team_id.clone(),
        source_team_name: item.source_team_name.clone(),
        qualification_code: item.qualification_code.clone(),
        qualification_level_code: item.qualification_level_code.clone(),
        baseline_user_id: item.user_id.clone(),
        changed: false,
    }).collect::<Vec<_>>();
    let equipment = order.baseline_assignment.equipment_slot_assignments.iter().map(|item| EquipmentSlotAssignmentResult {
        dispatch_order_id: order.order_id.clone(),
        slot_code: item.slot_code.clone(),
        equipment_id: item.equipment_id.clone(),
        code: item.code.clone(),
        equipment_type_id: item.equipment_type_id.clone(),
        baseline_equipment_id: item.equipment_id.clone(),
        changed: false,
    }).collect::<Vec<_>>();
    reserve_assignment(calendars, &personnel, &equipment, start_ms, end_ms, &order.order_id);
}

fn reserve_assignment(
    calendars: &mut Calendars,
    personnel: &[PersonnelSlotAssignmentResult],
    equipment: &[EquipmentSlotAssignmentResult],
    start_ms: i64,
    end_ms: i64,
    order_id: &str,
) {
    for item in personnel.iter().filter_map(|slot| slot.user_id.clone()) {
        calendars.entry(resource_key("employee", &item)).or_default().push(Reservation { order_id: order_id.to_string(), start_ms, end_ms });
    }
    for item in equipment.iter().filter_map(|slot| slot.equipment_id.clone()) {
        calendars.entry(resource_key("equipment", &item)).or_default().push(Reservation { order_id: order_id.to_string(), start_ms, end_ms });
    }
    calendars.values_mut().for_each(|items| items.sort_by(|left, right| left.start_ms.cmp(&right.start_ms)));
}

fn build_continuity(turnaround_pairs: &[TurnaroundPair], order_results: &[OrderResult]) -> Vec<ContinuityDecision> {
    turnaround_pairs.iter().map(|pair| {
        let inbound_user = order_results.iter().find(|item| item.dispatch_order_id == pair.inbound_order_id)
            .and_then(|result| result.personnel_slot_assignments.iter().find(|item| item.slot_code == pair.inbound_slot_code))
            .and_then(|item| item.user_id.clone());
        let outbound_user = order_results.iter().find(|item| item.dispatch_order_id == pair.outbound_order_id)
            .and_then(|result| result.personnel_slot_assignments.iter().find(|item| item.slot_code == pair.outbound_slot_code))
            .and_then(|item| item.user_id.clone());
        let satisfied = inbound_user.is_some() && inbound_user == outbound_user;
        ContinuityDecision {
            pair_key: pair.pair_key.clone(),
            inbound_order_id: pair.inbound_order_id.clone(),
            outbound_order_id: pair.outbound_order_id.clone(),
            inbound_slot_code: pair.inbound_slot_code.clone(),
            outbound_slot_code: pair.outbound_slot_code.clone(),
            satisfied,
            hard_continuity_required: pair.hard_continuity_required,
            penalty_applied: if satisfied { 0.0 } else { pair.continuity_penalty_weight.max(if pair.hard_continuity_required { 1000.0 } else { 100.0 }) },
        }
    }).collect()
}

fn summarize_objective(order_results: &[OrderResult], continuity: &[ContinuityDecision]) -> ObjectiveBreakdown {
    ObjectiveBreakdown {
        slot_gap: order_results.iter().map(|item| item.gap_count).sum(),
        total_lateness_minutes: order_results.iter().map(|item| item.lateness_minutes).sum(),
        continuity_break: continuity.iter().filter(|item| !item.satisfied).count() as i64,
        continuity_penalty: continuity.iter().map(|item| item.penalty_applied).sum(),
        baseline_change: order_results.iter().map(|item| item.baseline_change_count).sum(),
        travel_cost: order_results.iter().map(|item| item.travel_minutes).sum(),
        scarcity_cost: order_results.iter().map(|item| item.objective_breakdown.scarcity_cost).sum(),
        load_deviation: order_results.iter().map(|item| item.objective_breakdown.load_cost).sum(),
    }
}

fn build_windows_lookup(
    employee_anchor_states: &[AnchorState],
    equipment_anchor_states: &[AnchorState],
    employee_free_windows: &[FreeWindow],
    equipment_free_windows: &[FreeWindow],
) -> WindowsByResource {
    let mut windows = WindowsByResource::new();
    for window in employee_free_windows.iter().chain(equipment_free_windows.iter()) {
        windows.entry(resource_key(&window.resource_type, &window.resource_id)).or_default().push(window.clone());
    }
    for anchor in employee_anchor_states.iter().chain(equipment_anchor_states.iter()) {
        let key = resource_key(&anchor.resource_type, &anchor.resource_id);
        if !windows.contains_key(&key) && !anchor.free_windows.is_empty() {
            windows.insert(key, anchor.free_windows.clone());
        }
    }
    windows.values_mut().for_each(|items| items.sort_by(|left, right| parse_ms(left.window_start.as_ref()).cmp(&parse_ms(right.window_start.as_ref()))));
    windows
}

fn build_travel_lookup(edges: &[TravelEdge]) -> TravelLookup {
    let mut lookup = TravelLookup::new();
    for edge in edges {
        lookup.insert(travel_key(&edge.resource_type, &edge.resource_id, &edge.from_node, &edge.to_node), edge.travel_minutes.max(0));
    }
    lookup
}

fn order_interval(order: &OrderInput) -> Option<(i64, i64)> {
    let start_ms = parse_ms(order.required_start_time.as_ref().or(order.planned_start_time.as_ref()).or(order.effective_start_time.as_ref()))?;
    let end_ms = parse_ms(order.planned_end_time.as_ref().or(order.effective_end_time.as_ref())).unwrap_or(start_ms + 15 * 60_000);
    Some((start_ms, end_ms.max(start_ms + 5 * 60_000)))
}

fn parse_ms(value: Option<&String>) -> Option<i64> {
    let text = value?.trim();
    if text.is_empty() {
        return None;
    }
    DateTime::parse_from_rfc3339(text).ok().map(|dt| dt.with_timezone(&Utc).timestamp_millis())
}

fn iso_from_ms(value: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp_millis(value).map(|dt| dt.to_rfc3339())
}

fn resource_key(resource_type: &str, resource_id: &str) -> String {
    format!("{resource_type}:{resource_id}")
}

fn travel_key(resource_type: &str, resource_id: &str, from_node: &str, to_node: &str) -> String {
    format!("{resource_type}|{resource_id}|{from_node}|{to_node}")
}

fn travel_minutes(lookup: &TravelLookup, resource_type: &str, resource_id: &str, from_node: &str, to_node: &str) -> i64 {
    if from_node == to_node {
        return 0;
    }
    lookup.get(&travel_key(resource_type, resource_id, from_node, to_node)).copied().unwrap_or(0)
}
