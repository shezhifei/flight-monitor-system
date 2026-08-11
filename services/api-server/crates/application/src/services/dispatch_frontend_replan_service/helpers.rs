use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde_json::Value;

use crate::schemas::dispatch_schemas::{
    DispatchReplanAssignment, DispatchReplanSnapshotOrder, DispatchReplanSuggestion, TaskCrewMemberResponse,
    TurnaroundSlotPairSchema,
};
use fms_domain::models::dispatch::{
    AssigneeType, DepartmentRuleStatus, DispatchLockLevel, DispatchOrder, DispatchOrderMember, DispatchOrderStatus,
    FlightGenerationRule, LegScope, MemberRole,
};
pub(crate) fn effective_start_time(order: &DispatchOrder) -> Option<DateTime<Utc>> {
    order
        .actual_start_time
        .or(order.planned_start_time)
        .or(order.created_at)
}

pub(crate) fn effective_end_time(order: &DispatchOrder) -> Option<DateTime<Utc>> {
    order
        .actual_end_time
        .or(order.estimated_completion_time)
        .or(order.planned_end_time)
        .or(order.planned_start_time)
        .or(order.created_at)
}

pub(crate) fn order_status_text(status: DispatchOrderStatus) -> &'static str {
    match status {
        DispatchOrderStatus::Pending => "pending",
        DispatchOrderStatus::Assigned => "assigned",
        DispatchOrderStatus::InProgress => "in_progress",
        DispatchOrderStatus::Completed => "completed",
        DispatchOrderStatus::Cancelled => "cancelled",
    }
}

pub(crate) fn assignee_type_text(assignee_type: AssigneeType) -> &'static str {
    match assignee_type {
        AssigneeType::Team => "team",
        AssigneeType::Individual => "individual",
    }
}

pub(crate) fn lock_level_text(lock_level: DispatchLockLevel) -> &'static str {
    match lock_level {
        DispatchLockLevel::Active => "active",
        DispatchLockLevel::Frozen => "frozen",
        DispatchLockLevel::ManualLock => "manual_lock",
        DispatchLockLevel::Optimizable => "optimizable",
    }
}

pub(crate) fn schedule_source_text(order: &DispatchOrder) -> String {
    serde_json::to_string(&order.schedule_source)
        .unwrap_or_else(|_| "\"current_status_fallback\"".to_string())
        .trim_matches('"')
        .to_string()
}

/// Fallback movement window for an optimizable order's planned start.
///
/// This is only a default. The real value belongs to the department that owns
/// the task: `department_flight_generation_rules.start_flex_minutes`. A pushback
/// pressed against its departure slot and a cabin clean with turnaround room to
/// spare should not get the same slack, and only the department knows which is
/// which. This constant applies when that column is still NULL.
pub(crate) const REPLAN_START_FLEX_MINUTES: i64 = 5;

/// Per-order replan parameters owned by the department rules that generated the
/// orders.
///
/// Indexed by rule id first because a generated order records the exact rule
/// version it came from, so that lookup stays correct even after the department
/// publishes a newer version. Orders created outside the generation path (manual
/// and temporary work) have no rule id and fall back to the best rule for their
/// department × task type × leg scope — see [`status_rank`], then version.
///
/// Both parameters resolve through the same precedence, so they share one index
/// and one load. A rule that configures neither contributes nothing.
#[derive(Debug, Default)]
pub(crate) struct GenerationRuleIndex {
    by_rule_id: HashMap<String, RuleReplanParams>,
    by_rule_key: HashMap<(String, String, LegScope), (u8, i32, RuleReplanParams)>,
}

/// The replan-relevant slice of a generation rule. Every field is `Option`
/// because a department may configure one parameter and not the other.
#[derive(Clone, Debug, Default)]
pub(crate) struct RuleReplanParams {
    start_flex_minutes: Option<i32>,
    duration_by_crew_size: Option<BTreeMap<u32, i32>>,
}

impl RuleReplanParams {
    fn is_empty(&self) -> bool {
        self.start_flex_minutes.is_none() && self.duration_by_crew_size.is_none()
    }
}

/// Higher wins when two rule versions cover the same task. Archived rules still
/// count so a department that has only ever archived does not silently lose its
/// configured slack, but they never outrank a live one.
fn status_rank(status: DepartmentRuleStatus) -> u8 {
    match status {
        DepartmentRuleStatus::Published => 2,
        DepartmentRuleStatus::Draft => 1,
        DepartmentRuleStatus::Archived => 0,
    }
}

/// Reads the stored `{"1":45,"2":30}` map, discarding entries that are not
/// positive-integer to positive-integer.
///
/// Defensive on purpose: the column is JSONB and rows may predate the write-side
/// normalization in `dispatch_rule_service`. A malformed entry is dropped with a
/// warning rather than failing the snapshot — losing one crew size degrades the
/// plan, losing the snapshot leaves the dispatcher with nothing.
fn parse_duration_by_crew_size(rule: &FlightGenerationRule) -> Option<BTreeMap<u32, i32>> {
    let entries = rule.duration_by_crew_size.as_ref()?.as_object()?;
    let mut parsed = BTreeMap::new();
    for (crew_size, minutes) in entries {
        let crew_size = crew_size.trim().parse::<u32>().ok().filter(|value| *value > 0);
        let minutes = minutes
            .as_i64()
            .or_else(|| minutes.as_str().and_then(|text| text.trim().parse::<i64>().ok()))
            .filter(|value| *value > 0 && *value <= i64::from(i32::MAX));
        match (crew_size, minutes) {
            (Some(crew_size), Some(minutes)) => {
                parsed.insert(crew_size, minutes as i32);
            }
            _ => {
                tracing::warn!(
                    rule_id = %rule.id,
                    department_id = %rule.department_id,
                    "部门作业时长表存在非法条目(人数与分钟数都必须是正整数),已忽略该条目"
                );
            }
        }
    }
    (!parsed.is_empty()).then_some(parsed)
}

impl GenerationRuleIndex {
    pub(crate) fn from_rules<'a>(rules: impl IntoIterator<Item = &'a FlightGenerationRule>) -> Self {
        let mut index = Self::default();
        for rule in rules {
            let params = RuleReplanParams {
                start_flex_minutes: rule.start_flex_minutes,
                duration_by_crew_size: parse_duration_by_crew_size(rule),
            };
            if params.is_empty() {
                continue;
            }
            index.by_rule_id.insert(rule.id.clone(), params.clone());
            let rank = status_rank(rule.status);
            let key = (rule.department_id.clone(), rule.task_type.clone(), rule.leg_scope);
            match index.by_rule_key.get(&key) {
                Some((existing_rank, existing_version, _))
                    if (*existing_rank, *existing_version) >= (rank, rule.version_no) => {}
                _ => {
                    index.by_rule_key.insert(key, (rank, rule.version_no, params));
                }
            }
        }
        index
    }

    /// Test-only: distinguishes "no rule carried a value" from "a value was
    /// indexed but this order missed the lookup".
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.by_rule_id.is_empty() && self.by_rule_key.is_empty()
    }

    fn params_for(&self, order: &DispatchOrder) -> Option<&RuleReplanParams> {
        if let Some(rule_id) = order.generation_rule_id.as_deref() {
            if let Some(params) = self.by_rule_id.get(rule_id) {
                return Some(params);
            }
        }
        let department_id = order.department_id.as_deref()?;
        let key = (
            department_id.to_string(),
            order.task_type.clone(),
            parse_leg_scope(&order.leg_scope),
        );
        self.by_rule_key.get(&key).map(|(_, _, params)| params)
    }

    /// `None` means the owning department has not configured this task, which
    /// [`resolve_start_window`] reads as "use the system default".
    pub(crate) fn flex_for(&self, order: &DispatchOrder) -> Option<i32> {
        self.params_for(order)?.start_flex_minutes
    }

    /// `None` means the owning department has not configured this task, which
    /// [`resolve_duration_table`] reads as "keep the constant duration".
    pub(crate) fn duration_by_crew_size_for(&self, order: &DispatchOrder) -> Option<&BTreeMap<u32, i32>> {
        self.params_for(order)?.duration_by_crew_size.as_ref()
    }
}

/// Expands the department's sparse `crew size -> minutes` map into the dense
/// table the solver indexes by how many slots it managed to fill.
///
/// `table[k]` is the duration when `k` people are assigned, for `k` in
/// `0..=slot_count`. A crew size the department did not configure takes the
/// nearest configured entry at or below it — more people never inherit the
/// duration of a smaller crew — and sizes below the smallest configured entry
/// take that smallest entry's value, since `k = 0` (nobody assigned) cannot be
/// quicker than doing it alone.
///
/// Returns `None` when the department configured nothing, which keeps the
/// solver on today's constant duration.
pub(crate) fn resolve_duration_table(
    configured: Option<&BTreeMap<u32, i32>>,
    slot_count: usize,
    duration_minutes: Option<i32>,
) -> Option<Vec<i32>> {
    let configured = configured?;
    let smallest = *configured.values().next()?;
    let fallback = duration_minutes.unwrap_or(15).max(1);
    let table = (0..=slot_count)
        .map(|crew_size| {
            configured
                .range(..=(crew_size as u32))
                .next_back()
                .map(|(_, minutes)| *minutes)
                .unwrap_or(smallest)
                .max(1)
        })
        .collect::<Vec<_>>();
    // A table that says nothing beyond the constant is not worth shipping: it
    // would turn duration into a decision variable with a single value.
    if table.iter().all(|value| *value == fallback) {
        return None;
    }
    Some(table)
}

/// The lower bound the solver sees for an order's start.
pub(crate) fn snapshot_earliest_start_time(order: &DispatchOrder) -> Option<DateTime<Utc>> {
    order
        .planned_start_time
        .or_else(|| effective_start_time(order))
        .or(order.assignment_deadline)
        .or(order.created_at)
}

fn parse_leg_scope(value: &str) -> LegScope {
    match value.trim().to_ascii_lowercase().as_str() {
        "inbound" => LegScope::Inbound,
        "outbound" => LegScope::Outbound,
        _ => LegScope::None,
    }
}

/// Latest permissible start (hard bound). Planned completion is a forecast. The solver may
/// move an optimizable order only inside the department-owned start flex.
///
/// Locked and in-progress orders are genuinely immovable, so they collapse back
/// to a point window. That guard lives here rather than in the caller so no
/// future call site can accidentally hand the solver freedom to move work that
/// is already under way.
///
/// `flex_minutes` is the owning department's configured slack, `None` falling
/// back to [`REPLAN_START_FLEX_MINUTES`].
///
/// Start order across tasks on one flight is deliberately not constrained here.
/// In practice a task that is not ready simply waits, so the ordering is a
/// non-problem and narrowing windows to enforce it only shrinks the solver's
/// feasible region — which is the opposite of what a replan needs.
pub(crate) fn resolve_start_window(
    order: &DispatchOrder,
    earliest_start_time: Option<DateTime<Utc>>,
    flex_minutes: Option<i32>,
) -> Option<DateTime<Utc>> {
    let earliest = match earliest_start_time {
        Some(value) => value,
        None => return None,
    };
    if is_locked_order(order) {
        return Some(earliest);
    }
    let flex = flex_minutes
        .map(|value| i64::from(value.max(0)))
        .unwrap_or(REPLAN_START_FLEX_MINUTES);
    Some(earliest + ChronoDuration::minutes(flex))
}

pub(crate) fn resolve_duration_minutes(
    order: &DispatchOrder,
    effective_start_time: Option<DateTime<Utc>>,
    effective_end_time: Option<DateTime<Utc>>,
) -> Option<i32> {
    let fallback = effective_start_time
        .zip(effective_end_time)
        .map(|(start, end)| ((end - start).num_minutes().max(1)) as i32)
        .or(Some(15));
    json_i64_field(order.score_breakdown.get("duration_minutes"))
        .map(|value| value.max(1) as i32)
        .or(fallback)
}

pub(crate) fn order_leg_scope(order: &DispatchReplanSnapshotOrder) -> Option<&str> {
    order
        .leg_scope
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(crate) fn turnaround_slot_pairs(
    inbound: &DispatchReplanSnapshotOrder,
    outbound: &DispatchReplanSnapshotOrder,
) -> Vec<TurnaroundSlotPairSchema> {
    let mut outbound_by_code: HashSet<&str> = HashSet::new();
    for slot in &outbound.personnel_slots {
        let slot_code = slot.slot_code.trim();
        if !slot_code.is_empty() {
            outbound_by_code.insert(slot_code);
        }
    }

    let mut pairs = inbound
        .personnel_slots
        .iter()
        .filter_map(|slot| {
            let slot_code = slot.slot_code.trim();
            if slot_code.is_empty() || !outbound_by_code.contains(slot_code) {
                None
            } else {
                Some(TurnaroundSlotPairSchema {
                    inbound_slot_code: slot_code.to_string(),
                    outbound_slot_code: slot_code.to_string(),
                })
            }
        })
        .collect::<Vec<_>>();

    if pairs.is_empty() {
        if let (Some(inbound_slot), Some(outbound_slot)) = (
            inbound.personnel_slots.first().map(|slot| slot.slot_code.trim()),
            outbound.personnel_slots.first().map(|slot| slot.slot_code.trim()),
        ) {
            if !inbound_slot.is_empty() && !outbound_slot.is_empty() {
                pairs.push(TurnaroundSlotPairSchema {
                    inbound_slot_code: inbound_slot.to_string(),
                    outbound_slot_code: outbound_slot.to_string(),
                });
            }
        }
    }

    pairs
}

pub(crate) fn json_string_field(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
}

pub(crate) fn json_i64_field(value: Option<&Value>) -> Option<i64> {
    value.and_then(|item| {
        item.as_i64()
            .or_else(|| item.as_f64().map(|number| number.round() as i64))
    })
}

pub(crate) fn json_f64_field(value: Option<&Value>) -> Option<f64> {
    value.and_then(|item| item.as_f64().or_else(|| item.as_i64().map(|number| number as f64)))
}

pub(crate) fn is_locked_order(order: &DispatchOrder) -> bool {
    matches!(
        order.status,
        DispatchOrderStatus::InProgress | DispatchOrderStatus::Completed
    ) || !matches!(order.lock_level, DispatchLockLevel::Optimizable)
}

pub(crate) fn pending_assigned_compatible(snapshot_status: &str, live_status: &str) -> bool {
    matches!(snapshot_status, "pending" | "assigned") && matches!(live_status, "pending" | "assigned")
}

pub(crate) fn shared_resource_keys(left: &DispatchOrder, right: &DispatchOrder) -> Vec<String> {
    let left_keys = order_resource_keys(left);
    let right_keys = order_resource_keys(right);
    left_keys.intersection(&right_keys).cloned().collect()
}

pub(crate) fn order_resource_keys(order: &DispatchOrder) -> HashSet<String> {
    let mut keys = HashSet::new();
    if let Some(team_id) = order.team_id.as_deref() {
        if !team_id.is_empty() {
            keys.insert(format!("team:{team_id}"));
        }
    }
    if let Some(user_id) = order.individual_user_id.as_deref() {
        if !user_id.is_empty() {
            keys.insert(format!("user:{user_id}"));
        }
    }
    for member in &order.members {
        if member.is_active && !member.user_id.trim().is_empty() {
            keys.insert(format!("user:{}", member.user_id));
        }
    }
    for equipment in &order.equipment_list {
        keys.insert(format!("equipment:{}", equipment.id));
    }
    keys
}

pub(crate) fn has_primary_assignment(assignment: &DispatchReplanAssignment) -> bool {
    assignment.individual_user_id.is_some()
        || assignment.team_id.is_some()
        || !assignment.member_user_ids.is_empty()
        || !assignment.task_crew.members.is_empty()
}

pub(crate) fn is_high_risk_suggestion(suggestion: &DispatchReplanSuggestion) -> bool {
    matches!(suggestion.risk_level.as_deref(), Some("critical" | "high"))
        || suggestion.requires_manual_confirmation
        || !suggestion.qualification_gap.is_empty()
        || suggestion.impact_score >= 15.0
        || matches!(
            suggestion.suggestion_type.as_deref(),
            Some("assigned_conflict_resolution" | "unassigned_late_assignment")
        )
}

pub(crate) fn suggestion_risk_level(suggestion: &DispatchReplanSuggestion) -> &'static str {
    if !suggestion.qualification_gap.is_empty() || suggestion.impact_score >= 30.0 {
        "critical"
    } else if suggestion.requires_manual_confirmation
        || suggestion.impact_score >= 15.0
        || matches!(
            suggestion.suggestion_type.as_deref(),
            Some("assigned_conflict_resolution" | "unassigned_late_assignment")
        )
    {
        "high"
    } else if suggestion.impact_score > 0.0 || suggestion.current_assignment != suggestion.suggested_assignment {
        "medium"
    } else {
        "low"
    }
}

pub(crate) fn dedupe_strings(values: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .iter()
        .filter_map(|item| {
            let normalized = item.trim();
            if normalized.is_empty() || !seen.insert(normalized.to_string()) {
                None
            } else {
                Some(normalized.to_string())
            }
        })
        .collect()
}

pub(crate) fn push_candidate_assignment(
    assignments: &mut Vec<DispatchReplanAssignment>,
    seen: &mut HashSet<String>,
    assignment: DispatchReplanAssignment,
) {
    let key = format!(
        "{}|{}|{}|{}",
        assignment.assignee_type.as_deref().unwrap_or_default(),
        assignment.team_id.as_deref().unwrap_or_default(),
        assignment.individual_user_id.as_deref().unwrap_or_default(),
        assignment.equipment_ids.join(",")
    );
    if seen.insert(key) {
        assignments.push(assignment);
    }
}

pub(crate) fn task_crew_members(assignment: &DispatchReplanAssignment) -> Vec<TaskCrewMemberResponse> {
    if !assignment.task_crew.members.is_empty() {
        return assignment.task_crew.members.clone();
    }
    assignment
        .member_user_ids
        .iter()
        .map(|user_id| TaskCrewMemberResponse {
            user_id: user_id.clone(),
            ..TaskCrewMemberResponse::default()
        })
        .collect()
}

pub(crate) fn resolve_baseline_user_for_slot(
    current_assignment: &DispatchReplanAssignment,
    slot_code: &str,
) -> Option<String> {
    for member in &current_assignment.task_crew.members {
        if member.slot_code.as_deref() == Some(slot_code) {
            return Some(member.user_id.clone());
        }
    }
    if slot_code == "primary" {
        return current_assignment.individual_user_id.clone();
    }
    None
}

pub(crate) fn resolve_candidate_user_for_slot(
    assignment: &DispatchReplanAssignment,
    slot_code: &str,
) -> Option<String> {
    for member in &assignment.task_crew.members {
        if member.slot_code.as_deref() == Some(slot_code) {
            return Some(member.user_id.clone());
        }
    }
    if slot_code == "primary" {
        return assignment.individual_user_id.clone();
    }
    None
}

pub(crate) fn assignment_member_ids(assignment: &DispatchReplanAssignment) -> Vec<String> {
    let mut values: Vec<String> = task_crew_members(assignment)
        .into_iter()
        .map(|item| item.user_id)
        .collect();
    if let Some(user_id) = assignment.individual_user_id.clone() {
        values.push(user_id);
    }
    dedupe_strings(&values)
}

pub(crate) fn assignment_resource_keys(assignment: &DispatchReplanAssignment) -> Vec<String> {
    let mut values = Vec::new();
    if let Some(team_id) = assignment.team_id.as_deref() {
        if !team_id.is_empty() {
            values.push(format!("team:{team_id}"));
        }
    }
    if let Some(user_id) = assignment.individual_user_id.as_deref() {
        if !user_id.is_empty() {
            values.push(format!("user:{user_id}"));
        }
    }
    for user_id in assignment_member_ids(assignment) {
        values.push(format!("user:{user_id}"));
    }
    for equipment_id in &assignment.equipment_ids {
        values.push(format!("equipment:{equipment_id}"));
    }
    dedupe_strings(&values)
}

pub(crate) fn parse_resource_key(resource_key: &str) -> (String, String) {
    let mut parts = resource_key.splitn(2, ':');
    let resource_type = parts.next().unwrap_or_default().trim().to_string();
    let resource_id = parts.next().unwrap_or_default().trim().to_string();
    (resource_type, resource_id)
}

pub(crate) fn recipient_user_ids(
    current_assignment: &DispatchReplanAssignment,
    suggested_assignment: &DispatchReplanAssignment,
) -> Vec<String> {
    let mut users = assignment_member_ids(current_assignment);
    users.extend(assignment_member_ids(suggested_assignment));
    dedupe_strings(&users)
}

pub(crate) fn build_dispatch_members(
    order: &DispatchOrder,
    assignment: &DispatchReplanAssignment,
) -> Vec<DispatchOrderMember> {
    let source_type = match assignment.assignee_type.as_deref() {
        Some("individual") => AssigneeType::Individual,
        _ => AssigneeType::Team,
    };
    let task_members = task_crew_members(assignment);
    let mut result = Vec::new();
    for member in task_members {
        if member.user_id.trim().is_empty() {
            continue;
        }
        result.push(DispatchOrderMember {
            id: ulid::Ulid::new().to_string(),
            dispatch_order_id: order.id.clone(),
            user_id: member.user_id.clone(),
            role: match member.slot_code.as_deref() {
                Some("lead") => MemberRole::Leader,
                Some("driver") => MemberRole::Driver,
                _ => MemberRole::Member,
            },
            source_type,
            source_team_id: assignment.team_id.clone(),
            slot_code: member.slot_code.clone(),
            qualification_code: member.qualification_code.clone(),
            qualification_level_code: member.qualification_level_code.clone(),
            assigned_at: Some(Utc::now()),
            check_in_time: None,
            check_out_time: None,
            is_active: true,
            username: member.username.clone(),
        });
    }
    if result.is_empty() {
        for user_id in assignment_member_ids(assignment) {
            result.push(DispatchOrderMember {
                id: ulid::Ulid::new().to_string(),
                dispatch_order_id: order.id.clone(),
                user_id,
                role: MemberRole::Member,
                source_type,
                source_team_id: assignment.team_id.clone(),
                slot_code: None,
                qualification_code: None,
                qualification_level_code: None,
                assigned_at: Some(Utc::now()),
                check_in_time: None,
                check_out_time: None,
                is_active: true,
                username: None,
            });
        }
    }
    result
}
