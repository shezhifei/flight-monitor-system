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

impl DispatchService {
    pub(super) fn build_conflict(
        conflict_type: &str,
        severity: &str,
        resource_id: Option<String>,
        resource_name: Option<String>,
        related_dispatch_order_ids: Vec<String>,
        message: &str,
        suggested_action: Option<&str>,
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

    pub(super) fn deduplicate_conflicts(items: Vec<Value>) -> Vec<Value> {
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
            let key = (conflict_type, resource_id, related);

            match unique.get(&key) {
                Some(current)
                    if Self::severity_rank(item.get("severity").and_then(Value::as_str).unwrap_or("low"))
                        <= Self::severity_rank(current.get("severity").and_then(Value::as_str).unwrap_or("low")) => {}
                _ => {
                    unique.insert(key, item);
                }
            }
        }

        let mut items = unique.into_values().collect::<Vec<_>>();
        items.sort_by(|left, right| {
            let left_rank = Self::severity_rank(left.get("severity").and_then(Value::as_str).unwrap_or("low"));
            let right_rank = Self::severity_rank(right.get("severity").and_then(Value::as_str).unwrap_or("low"));
            right_rank.cmp(&left_rank).then_with(|| {
                left.get("conflict_type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .cmp(right.get("conflict_type").and_then(Value::as_str).unwrap_or_default())
            })
        });
        items
    }

    pub(super) fn assignment_from_order(order: &DispatchOrder) -> Value {
        let assignee_type = match order.assignee_type {
            AssigneeType::Team => "team",
            AssigneeType::Individual => "individual",
        };
        json!({
            "assignee_type": assignee_type,
            "team_id": order.team_id,
            "team_name": order.team_name,
            "individual_user_id": order.individual_user_id,
            "individual_username": order.individual_username,
            "driver_type": order.driver_type.map(|value| format!("{:?}", value).to_lowercase()),
            "driver_team_id": order.driver_team_id,
            "driver_user_id": order.driver_user_id,
            "equipment_ids": Self::equipment_ids_from_order(order),
            "member_user_ids": Self::order_member_user_ids(order),
            "department_rule_version": order.department_rule_version,
            "crew_requirement_snapshot": order.crew_requirement_snapshot,
            "equipment_requirement_snapshot": order.equipment_requirement_snapshot,
            "equipment_assignment": order.equipment_assignment,
            "qualification_gap": order.qualification_gap,
            "equipment_gap": order.equipment_gap,
            "availability_reason": order.availability_reason,
            "score_breakdown": order.score_breakdown,
            "lock_level": order.lock_level.as_ref(),
            "task_crew": order.task_crew,
        })
    }

    pub(super) fn assignment_string_field(assignment: &Value, field: &str) -> Option<String> {
        assignment
            .get(field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    }

    pub(super) fn assignment_array_field(assignment: &Value, field: &str) -> Vec<Value> {
        assignment
            .get(field)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    }

    pub(super) fn assignment_object_field(assignment: &Value, field: &str) -> Value {
        assignment
            .get(field)
            .cloned()
            .unwrap_or_else(|| Value::Object(Default::default()))
    }

    pub(super) fn assignment_task_crew_members(assignment: &Value) -> Vec<Value> {
        assignment
            .get("task_crew")
            .and_then(Value::as_object)
            .and_then(|task_crew| task_crew.get("members"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    }

    pub(super) fn assignment_equipment_ids(assignment: &Value) -> Vec<String> {
        let mut equipment_ids = assignment
            .get("equipment_assignment")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|item| item.get("equipment_id").and_then(Value::as_str))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        if equipment_ids.is_empty() {
            equipment_ids = assignment
                .get("equipment_ids")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect();
        }
        equipment_ids
    }

    pub(super) fn assignment_driver_binding(
        assignment: &Value,
    ) -> (Option<AssigneeType>, Option<String>, Option<String>) {
        let driver_type = match Self::assignment_string_field(assignment, "driver_type").as_deref() {
            Some("individual") => Some(AssigneeType::Individual),
            Some("team") => Some(AssigneeType::Team),
            _ => assignment
                .get("equipment_assignment")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find_map(|item| {
                    item.get("driver_user_id")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(|_| AssigneeType::Individual)
                }),
        };
        let driver_team_id = Self::assignment_string_field(assignment, "driver_team_id");
        let driver_user_id = Self::assignment_string_field(assignment, "driver_user_id").or_else(|| {
            assignment
                .get("equipment_assignment")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find_map(|item| {
                    item.get("driver_user_id")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                })
        });
        (driver_type, driver_team_id, driver_user_id)
    }

    pub(super) fn assignment_member_user_ids(assignment: &Value) -> Vec<String> {
        let mut user_ids = Self::assignment_task_crew_members(assignment)
            .into_iter()
            .filter_map(|item| {
                item.get("user_id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
            })
            .collect::<Vec<_>>();
        if user_ids.is_empty() {
            user_ids = assignment
                .get("member_user_ids")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect();
        }
        user_ids
    }

    pub(super) fn member_role_from_slot(slot_code: Option<&str>) -> MemberRole {
        match slot_code.map(str::trim).filter(|value| !value.is_empty()) {
            Some("lead") => MemberRole::Leader,
            Some("driver") => MemberRole::Driver,
            _ => MemberRole::Member,
        }
    }

    pub(super) fn build_dispatch_members_from_assignment(
        order: &DispatchOrder,
        assignment: &Value,
    ) -> Vec<DispatchOrderMember> {
        let source_type = match Self::assignment_string_field(assignment, "assignee_type").as_deref() {
            Some("individual") => AssigneeType::Individual,
            _ => AssigneeType::Team,
        };
        let fallback_team_id = Self::assignment_string_field(assignment, "team_id");
        let task_crew_members = Self::assignment_task_crew_members(assignment);
        let mut desired_members = task_crew_members
            .into_iter()
            .filter_map(|member| {
                let user_id = member
                    .get("user_id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())?
                    .to_string();
                let slot_code = member
                    .get("slot_code")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);
                Some(DispatchOrderMember {
                    id: Self::new_dispatch_id(),
                    dispatch_order_id: order.id.clone(),
                    user_id,
                    role: Self::member_role_from_slot(slot_code.as_deref()),
                    source_type,
                    source_team_id: member
                        .get("source_team_id")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                        .or_else(|| fallback_team_id.clone()),
                    slot_code,
                    qualification_code: member
                        .get("qualification_code")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    qualification_level_code: member
                        .get("qualification_level_code")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    assigned_at: Some(Utc::now()),
                    check_in_time: None,
                    check_out_time: None,
                    is_active: true,
                    username: member.get("username").and_then(Value::as_str).map(str::to_string),
                })
            })
            .collect::<Vec<_>>();
        if desired_members.is_empty() {
            let fallback_individual_user_id = Self::assignment_string_field(assignment, "individual_user_id");
            for user_id in Self::assignment_member_user_ids(assignment) {
                desired_members.push(DispatchOrderMember {
                    id: Self::new_dispatch_id(),
                    dispatch_order_id: order.id.clone(),
                    user_id: user_id.clone(),
                    role: MemberRole::Member,
                    source_type,
                    source_team_id: fallback_team_id.clone(),
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
            if desired_members.is_empty() {
                if let Some(user_id) = fallback_individual_user_id {
                    desired_members.push(DispatchOrderMember {
                        id: Self::new_dispatch_id(),
                        dispatch_order_id: order.id.clone(),
                        user_id,
                        role: MemberRole::Member,
                        source_type,
                        source_team_id: fallback_team_id,
                        slot_code: Some("primary".to_string()),
                        qualification_code: None,
                        qualification_level_code: None,
                        assigned_at: Some(Utc::now()),
                        check_in_time: None,
                        check_out_time: None,
                        is_active: true,
                        username: Self::assignment_string_field(assignment, "individual_username"),
                    });
                }
            }
        }
        desired_members
    }

    pub(super) async fn sync_assignment_members(
        &self,
        order: &DispatchOrder,
        assignment: &Value,
    ) -> Result<(), DomainError> {
        let member_repo = self.order.member_repo.as_ref();
        let existing_members = member_repo.find_by_order(&order.id).await?;
        let desired_members = Self::build_dispatch_members_from_assignment(order, assignment);
        let desired_by_user = desired_members
            .into_iter()
            .map(|member| (member.user_id.clone(), member))
            .collect::<HashMap<_, _>>();

        for member in &existing_members {
            if let Some(desired) = desired_by_user.get(&member.user_id) {
                let mut updated = member.clone();
                updated.role = desired.role;
                updated.source_type = desired.source_type;
                updated.source_team_id = desired.source_team_id.clone();
                updated.slot_code = desired.slot_code.clone();
                updated.qualification_code = desired.qualification_code.clone();
                updated.qualification_level_code = desired.qualification_level_code.clone();
                updated.username = desired.username.clone();
                updated.is_active = true;
                member_repo.save(&updated).await?;
            } else {
                let mut deactivated = member.clone();
                deactivated.is_active = false;
                member_repo.save(&deactivated).await?;
            }
        }

        let existing_user_ids: HashSet<&str> = existing_members.iter().map(|m| m.user_id.as_str()).collect();
        for (user_id, desired) in desired_by_user {
            if !existing_user_ids.contains(user_id.as_str()) {
                member_repo.save(&desired).await?;
            }
        }

        Ok(())
    }

    pub(super) fn apply_assignment_json(order: &mut DispatchOrder, assignment: Option<&Value>) {
        let Some(assignment) = assignment else {
            return;
        };

        match assignment
            .get("assignee_type")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some("team") => {
                order.assignee_type = AssigneeType::Team;
                order.team_id = assignment
                    .get("team_id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                order.team_name = assignment
                    .get("team_name")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                order.individual_user_id = None;
                order.individual_username = None;
            }
            Some("individual") => {
                order.assignee_type = AssigneeType::Individual;
                order.individual_user_id = assignment
                    .get("individual_user_id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                order.individual_username = assignment
                    .get("individual_username")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                order.team_id = None;
                order.team_name = None;
            }
            _ => {}
        }

        let (driver_type, driver_team_id, driver_user_id) = Self::assignment_driver_binding(assignment);
        order.driver_type = driver_type;
        order.driver_team_id = driver_team_id;
        order.driver_user_id = driver_user_id;
        order.department_rule_version = Self::assignment_string_field(assignment, "department_rule_version");
        order.crew_requirement_snapshot = Self::assignment_array_field(assignment, "crew_requirement_snapshot");
        order.equipment_requirement_snapshot =
            Self::assignment_array_field(assignment, "equipment_requirement_snapshot");
        order.equipment_assignment = Self::assignment_array_field(assignment, "equipment_assignment");
        order.qualification_gap = Self::assignment_array_field(assignment, "qualification_gap");
        order.equipment_gap = Self::assignment_array_field(assignment, "equipment_gap");
        order.availability_reason = assignment
            .get("availability_reason")
            .and_then(Value::as_str)
            .map(str::to_string);
        order.score_breakdown = Self::assignment_object_field(assignment, "score_breakdown");
        order.task_crew = Self::assignment_object_field(assignment, "task_crew");
        if let Some(lock_level) = Self::assignment_string_field(assignment, "lock_level") {
            order.lock_level = match lock_level.as_str() {
                "active" => DispatchLockLevel::Active,
                "frozen" => DispatchLockLevel::Frozen,
                "manual_lock" => DispatchLockLevel::ManualLock,
                _ => DispatchLockLevel::Optimizable,
            };
        }
    }

    pub(super) async fn build_reassignment_suggestion(
        &self,
        current: &DispatchOrder,
        previous: &DispatchOrder,
        planned_start_time: DateTime<Utc>,
        planned_end_time: DateTime<Utc>,
    ) -> Result<Option<Value>, DomainError> {
        if matches!(
            current.lock_level,
            DispatchLockLevel::Frozen | DispatchLockLevel::ManualLock
        ) || current.task_type.trim().is_empty()
        {
            return Ok(None);
        }

        let team_type_repo = self.resources.team_type_repo.as_ref();
        let team_repo = self.resources.team_repo.as_ref();

        let team_types = team_type_repo.find_by_task_type(&current.task_type).await?;
        if team_types.is_empty() {
            return Ok(None);
        }

        let mut seen_team_ids = HashSet::new();
        for team_type in team_types {
            let teams = team_repo
                .find_available_for_dispatch(Some(&team_type.id), current.terminal.as_deref())
                .await?;
            for team in teams {
                if !seen_team_ids.insert(team.id.clone()) {
                    continue;
                }
                if current.team_id.as_deref() == Some(team.id.as_str()) {
                    continue;
                }
                let overlaps = self
                    .order
                    .order_repo
                    .find_overlapping_orders(
                        planned_start_time,
                        planned_end_time,
                        Some(&team.id),
                        None,
                        None,
                        Some(&current.id),
                    )
                    .await?;
                if !overlaps.is_empty() {
                    continue;
                }

                let mut suggested_assignment = Self::assignment_from_order(current);
                suggested_assignment["assignee_type"] = json!("team");
                suggested_assignment["team_id"] = json!(team.id);
                suggested_assignment["team_name"] = json!(team.name);
                suggested_assignment["individual_user_id"] = Value::Null;
                suggested_assignment["individual_username"] = Value::Null;
                return Ok(Some(json!({
                    "dispatch_order_id": current.id,
                    "reason": "resource_reassignment",
                    "suggestion_type": "assigned_conflict_resolution",
                    "order_class": "assigned_conflict",
                    "original_start_time": current.planned_start_time,
                    "original_end_time": current.planned_end_time,
                    "suggested_start_time": current.planned_start_time,
                    "suggested_end_time": current.planned_end_time,
                    "related_dispatch_order_id": previous.id,
                    "impact_score": 1.0,
                    "current_assignment": Self::assignment_from_order(current),
                    "suggested_assignment": suggested_assignment,
                    "lateness_minutes": 0,
                    "travel_minutes": 0,
                })));
            }
        }

        Ok(None)
    }

    pub(super) fn is_better_replan_candidate(candidate: &Value, existing: Option<&Value>) -> bool {
        let candidate_impact = candidate
            .get("impact_score")
            .and_then(Value::as_f64)
            .unwrap_or(f64::MAX);
        match existing {
            Some(existing) => {
                candidate_impact < existing.get("impact_score").and_then(Value::as_f64).unwrap_or(f64::MAX)
            }
            None => true,
        }
    }

    pub(super) fn build_delay_replan_suggestion(
        current: &DispatchOrder,
        related_order_id: &str,
        current_start: DateTime<Utc>,
        current_end: DateTime<Utc>,
        target_start: DateTime<Utc>,
        min_duration: Duration,
    ) -> Value {
        let duration = std::cmp::max(current_end - current_start, min_duration);
        let suggested_start = target_start;
        let suggested_end = suggested_start + duration;
        let impact_minutes = ((suggested_start - current_start).num_seconds().max(0) as f64) / 60.0;

        json!({
            "dispatch_order_id": current.id,
            "reason": "resource_time_overlap",
            "suggestion_type": "assigned_conflict_resolution",
            "order_class": "assigned_conflict",
            "original_start_time": current.planned_start_time,
            "original_end_time": current.planned_end_time,
            "suggested_start_time": suggested_start,
            "suggested_end_time": suggested_end,
            "related_dispatch_order_id": related_order_id,
            "impact_score": (impact_minutes * 100.0).round() / 100.0,
            "current_assignment": Self::assignment_from_order(current),
            "suggested_assignment": Self::assignment_from_order(current),
            "lateness_minutes": impact_minutes.round() as i64,
            "travel_minutes": 0,
        })
    }

    pub(super) async fn is_active_order_member(&self, order_id: &str, actor_id: &str) -> Result<bool, DomainError> {
        let member_repo = self.order.member_repo.as_ref();
        Ok(member_repo
            .find_by_order_and_user(order_id, actor_id)
            .await?
            .map(|member| member.is_active)
            .unwrap_or(false))
    }

    pub(super) async fn ensure_actor_can_start_order(
        &self,
        order: &DispatchOrder,
        order_id: &str,
        actor_id: &str,
        denied_message: &str,
    ) -> Result<(), DomainError> {
        if order.can_be_started_by(actor_id) || self.is_active_order_member(order_id, actor_id).await? {
            return Ok(());
        }
        Err(DomainError::PermissionDenied(denied_message.to_string()))
    }

    pub(super) async fn ensure_actor_can_complete_order(
        &self,
        order: &DispatchOrder,
        order_id: &str,
        actor_id: &str,
        denied_message: &str,
    ) -> Result<(), DomainError> {
        if order.can_be_completed_by(actor_id) || self.is_active_order_member(order_id, actor_id).await? {
            return Ok(());
        }
        Err(DomainError::PermissionDenied(denied_message.to_string()))
    }

    pub(super) fn build_checklist_status(
        dispatch_order_id: &str,
        task_type: &str,
        template: Option<&serde_json::Value>,
        records: &[serde_json::Value],
    ) -> Result<serde_json::Value, DomainError> {
        let mut record_map = std::collections::HashMap::new();
        for record in records {
            if let Some(item_code) = record.get("item_code").and_then(|v| v.as_str()) {
                record_map.insert(item_code.to_string(), record.clone());
            }
        }

        let Some(template) = template else {
            let mut items = Vec::new();
            for (item_code, record) in record_map {
                let result = record.get("result").and_then(|v| v.as_str()).map(str::to_string);
                items.push(serde_json::json!({
                    "item_code": item_code,
                    "title": item_code,
                    "required": false,
                    "allow_na": true,
                    "order": 0,
                    "level": "routine",
                    "result": result,
                    "checked_by": record.get("checked_by").unwrap_or(&NULL_VALUE),
                    "checked_by_username": record.get("checked_by_username").unwrap_or(&NULL_VALUE),
                    "checked_at": record.get("checked_at").unwrap_or(&NULL_VALUE),
                    "note": record.get("note").unwrap_or(&NULL_VALUE),
                    "status": result.as_deref().unwrap_or("pending"),
                }));
            }
            items.sort_by(|a, b| {
                a.get("item_code")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .cmp(b.get("item_code").and_then(|v| v.as_str()).unwrap_or(""))
            });
            return Ok(serde_json::json!({
                "dispatch_order_id": dispatch_order_id,
                "task_type": task_type,
                "template_id": serde_json::Value::Null,
                "template_version": serde_json::Value::Null,
                "enforced": false,
                "ready": true,
                "required_total": 0,
                "completed_required": 0,
                "pending_required_items": [],
                "failed_required_items": [],
                "blocking_issues": [],
                "soft_missing_count": 0,
                "can_soft_complete": true,
                "routine_total": items.len(),
                "completed_routine": items.iter().filter(|item| {
                    matches!(
                        item.get("status").and_then(|value| value.as_str()),
                        Some("pass") | Some("na")
                    )
                }).count(),
                "pending_routine_items": [],
                "failed_routine_items": [],
                "items": items,
            }));
        };

        let template_items = template
            .get("checklist_items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let mut pending_required_items = Vec::<String>::new();
        let mut failed_required_items = Vec::<String>::new();
        let mut blocking_issues = Vec::<String>::new();
        let mut pending_routine_items = Vec::<String>::new();
        let mut failed_routine_items = Vec::<String>::new();
        let mut items = Vec::<serde_json::Value>::new();
        let mut required_total = 0i64;
        let mut completed_required = 0i64;
        let mut routine_total = 0i64;
        let mut completed_routine = 0i64;

        for (index, item) in template_items.iter().enumerate() {
            let item_code = item
                .get("item_code")
                .and_then(|v| v.as_str())
                .ok_or_else(|| DomainError::Internal("安全检查模板缺少 item_code".to_string()))?;
            let title = item.get("title").and_then(|v| v.as_str()).unwrap_or(item_code);
            let required = item.get("required").and_then(|v| v.as_bool()).unwrap_or(true);
            let allow_na = item.get("allow_na").and_then(|v| v.as_bool()).unwrap_or(false);
            let _order = item.get("order").and_then(|v| v.as_i64()).unwrap_or((index + 1) as i64);
            let level = item.get("level").and_then(|v| v.as_str()).unwrap_or("critical");
            let is_routine = level == "routine";
            let record = record_map.get(item_code);
            let result = record.and_then(|value| value.get("result")).and_then(|v| v.as_str());
            let status = match result {
                Some("pass") => "pass",
                Some("fail") => "fail",
                Some("na") => "na",
                _ => "pending",
            };

            if required {
                if is_routine {
                    routine_total += 1;
                    if result == Some("pass") || (result == Some("na") && allow_na) {
                        completed_routine += 1;
                    } else if result.is_none() {
                        pending_routine_items.push(title.to_string());
                    } else {
                        failed_routine_items.push(title.to_string());
                    }
                } else {
                    required_total += 1;
                    if result == Some("pass") || (result == Some("na") && allow_na) {
                        completed_required += 1;
                    } else if result.is_none() {
                        pending_required_items.push(title.to_string());
                        blocking_issues.push(format!("未检查: {}", title));
                    } else {
                        failed_required_items.push(title.to_string());
                        blocking_issues.push(format!("未通过: {}", title));
                    }
                }
            }

            items.push(serde_json::json!({
                "item_code": item_code,
                "title": title,
                "required": required,
                "allow_na": allow_na,
                "order": _order,
                "level": level,
                "result": result,
                "checked_by": record.and_then(|value| value.get("checked_by")).unwrap_or(&NULL_VALUE),
                "checked_by_username": record.and_then(|value| value.get("checked_by_username")).unwrap_or(&NULL_VALUE),
                "checked_at": record.and_then(|value| value.get("checked_at")).unwrap_or(&NULL_VALUE),
                "note": record.and_then(|value| value.get("note")).unwrap_or(&NULL_VALUE),
                "status": status,
            }));
        }

        let soft_missing_count = (pending_routine_items.len() + failed_routine_items.len()) as i64;
        let can_soft_complete = blocking_issues.is_empty();

        Ok(serde_json::json!({
            "dispatch_order_id": dispatch_order_id,
            "task_type": task_type,
            "template_id": template.get("template_id").unwrap_or(&NULL_VALUE),
            "template_version": template.get("checklist_version").unwrap_or(&NULL_VALUE),
            "enforced": required_total > 0,
            "ready": pending_required_items.is_empty() && failed_required_items.is_empty(),
            "required_total": required_total,
            "completed_required": completed_required,
            "pending_required_items": pending_required_items,
            "failed_required_items": failed_required_items,
            "blocking_issues": blocking_issues,
            "soft_missing_count": soft_missing_count,
            "can_soft_complete": can_soft_complete,
            "routine_total": routine_total,
            "completed_routine": completed_routine,
            "pending_routine_items": pending_routine_items,
            "failed_routine_items": failed_routine_items,
            "items": items,
        }))
    }

    pub(super) fn checklist_completion_blocked_error(gate: &serde_json::Value) -> DomainError {
        let empty_array = serde_json::Value::Array(vec![]);
        let zero = serde_json::Value::from(0);
        let true_val = serde_json::Value::Bool(true);
        DomainError::BusinessRuleViolationWithDetails {
            message: "安全检查清单未完成，无法完工".to_string(),
            details: serde_json::json!({
                "message": "安全检查清单未完成，无法完工",
                "pending_required_items": gate.get("pending_required_items").unwrap_or(&empty_array),
                "failed_required_items": gate.get("failed_required_items").unwrap_or(&empty_array),
                "blocking_issues": gate.get("blocking_issues").unwrap_or(&empty_array),
                "soft_missing_count": gate.get("soft_missing_count").unwrap_or(&zero),
                "can_soft_complete": gate.get("can_soft_complete").unwrap_or(&true_val),
                "required_total": gate.get("required_total").unwrap_or(&zero),
                "completed_required": gate.get("completed_required").unwrap_or(&zero),
                "template_version": gate.get("template_version").unwrap_or(&NULL_VALUE),
                "routine_total": gate.get("routine_total").unwrap_or(&zero),
                "completed_routine": gate.get("completed_routine").unwrap_or(&zero),
            }),
        }
    }

    pub(super) fn mobile_sync_result(
        client_action_id: Option<&str>,
        order_id: &str,
        action_type: &str,
        status: &str,
        message: impl Into<String>,
    ) -> MobileSyncActionResult {
        MobileSyncActionResult {
            client_action_id: client_action_id.unwrap_or_default().to_string(),
            dispatch_order_id: order_id.to_string(),
            action_type: action_type.to_string(),
            status: status.to_string(),
            message: message.into(),
            server_timestamp: Utc::now(),
        }
    }

    pub(super) fn normalize_optional_string(value: Option<String>) -> Option<String> {
        value
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
    }

    pub(super) fn normalize_optional_ref<'a>(value: Option<&'a str>) -> Option<&'a str> {
        value.map(str::trim).filter(|item| !item.is_empty())
    }

    pub(super) fn should_auto_create_checkin_member(
        assignee_type: AssigneeType,
        individual_user_id: Option<&str>,
        actor_id: &str,
    ) -> bool {
        assignee_type == AssigneeType::Individual
            && individual_user_id.as_deref().is_some_and(|user_id| user_id == actor_id)
    }

    pub(super) fn serialize_crew_requirement_snapshot(requirements: &[TaskTypeCrewSlotRequirement]) -> Vec<Value> {
        requirements
            .iter()
            .map(|item| {
                json!({
                    "slot_code": item.slot_code,
                    "qualification_code": item.qualification_code,
                    "min_level_code": item.min_level_code,
                    "required_count": item.required_count,
                    "must_be_distinct": item.must_be_distinct,
                    "exclusive_group": item.exclusive_group,
                    "remarks": item.remarks,
                })
            })
            .collect()
    }

    pub(super) fn serialize_equipment_requirement_snapshot(
        requirements: &[TaskTypeEquipmentRequirement],
    ) -> Vec<Value> {
        requirements
            .iter()
            .map(|item| {
                json!({
                    "slot_code": item.slot_code,
                    "equipment_type_id": item.equipment_type_id,
                    "equipment_type_code": item.equipment_type_code,
                    "required_count": item.required_count,
                    "must_be_distinct": item.must_be_distinct,
                    "requires_driver": item.requires_driver,
                    "driver_qualification_code": item.driver_qualification_code,
                    "driver_min_level_code": item.driver_min_level_code,
                    "remarks": item.remarks,
                })
            })
            .collect()
    }

    pub(super) async fn resolve_order_department_id(
        &self,
        order: &DispatchOrder,
    ) -> Result<Option<String>, DomainError> {
        if let Some(department_id) = order
            .department_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Ok(Some(department_id.to_string()));
        }
        let task_type_repo = self.rules.task_type_repo.as_ref();
        let Some(task_type) = task_type_repo.find_by_code(&order.task_type).await? else {
            return Ok(None);
        };
        let task_type_department = task_type
            .default_department_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if task_type_department.is_some() {
            return Ok(task_type_department);
        }

        let team_type_repo = self.resources.team_type_repo.as_ref();
        let department_ids = team_type_repo
            .find_by_task_type(&order.task_type)
            .await?
            .into_iter()
            .filter_map(|team_type| {
                team_type
                    .department_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
            })
            .collect::<HashSet<_>>();
        if department_ids.len() == 1 {
            return Ok(department_ids.into_iter().next());
        }

        Ok(None)
    }

    pub(super) async fn resolve_order_requirement_snapshots(
        &self,
        order: &DispatchOrder,
        department_id: &str,
    ) -> Result<(Vec<Value>, Vec<Value>, Option<String>), DomainError> {
        let mut crew_requirement_snapshot = order.crew_requirement_snapshot.clone();
        let mut equipment_requirement_snapshot = order.equipment_requirement_snapshot.clone();
        let mut department_rule_version = order
            .department_rule_version
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        if crew_requirement_snapshot.is_empty() || equipment_requirement_snapshot.is_empty() {
            if let Some(requirement_version) = self
                .rules
                .task_type_requirement_repo
                .as_ref()
                .find_published(department_id, &order.task_type)
                .await?
            {
                if crew_requirement_snapshot.is_empty() {
                    crew_requirement_snapshot =
                        Self::serialize_crew_requirement_snapshot(&requirement_version.crew_requirements);
                    if crew_requirement_snapshot.is_empty() {
                        crew_requirement_snapshot =
                            Self::serialize_crew_requirement_snapshot(&requirement_version.requirements);
                    }
                }
                if equipment_requirement_snapshot.is_empty() {
                    equipment_requirement_snapshot =
                        Self::serialize_equipment_requirement_snapshot(&requirement_version.equipment_requirements);
                }
                if department_rule_version.is_none() {
                    department_rule_version = Some(requirement_version.id);
                }
            }
        }

        Ok((
            crew_requirement_snapshot,
            equipment_requirement_snapshot,
            department_rule_version,
        ))
    }

    pub(super) fn order_has_required_assignments(order: &DispatchOrder) -> Result<(), String> {
        let has_member_assignment = order
            .task_crew
            .get("members")
            .and_then(Value::as_array)
            .map(|items| !items.is_empty())
            .unwrap_or(false)
            || order
                .individual_user_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_some()
            || order
                .team_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_some();
        if !has_member_assignment {
            return Err("缺少执行编组".to_string());
        }

        if !order.equipment_requirement_snapshot.is_empty() {
            if !order.equipment_gap.is_empty() {
                return Err("设备需求未完全覆盖".to_string());
            }
            if order.equipment_assignment.is_empty() {
                return Err("缺少设备分配".to_string());
            }

            for requirement in &order.equipment_requirement_snapshot {
                let Some(requirement_obj) = requirement.as_object() else {
                    continue;
                };
                let slot_code = requirement_obj
                    .get("slot_code")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or_default();
                let required_count = requirement_obj
                    .get("required_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(1)
                    .max(1) as usize;
                let matching_assignments = order
                    .equipment_assignment
                    .iter()
                    .filter(|item| {
                        item.get("slot_code")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .unwrap_or_default()
                            == slot_code
                    })
                    .collect::<Vec<_>>();
                if matching_assignments.len() < required_count {
                    return Err("设备需求未完全覆盖".to_string());
                }
                let requires_driver = requirement_obj
                    .get("requires_driver")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if requires_driver {
                    let driver_bound_count = matching_assignments
                        .iter()
                        .filter(|item| {
                            item.get("driver_user_id")
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .is_some()
                        })
                        .count();
                    if driver_bound_count < required_count {
                        return Err("设备司机需求未完全覆盖".to_string());
                    }
                }
            }
        }

        Ok(())
    }

    pub(super) fn mobile_sync_error_result(
        client_action_id: Option<&str>,
        order_id: &str,
        action_type: &str,
        error: DomainError,
    ) -> MobileSyncActionResult {
        Self::mobile_sync_result(
            client_action_id,
            order_id,
            action_type,
            "failed",
            error.user_message().to_string(),
        )
    }
}
