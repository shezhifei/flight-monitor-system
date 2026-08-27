use std::collections::{HashMap, HashSet};

use chrono::Utc;
use serde_json::{json, Value};
use tracing::warn;

use crate::schemas::dispatch_schemas::{
    DispatchReplanApplyRequest, DispatchReplanApplyResponse, DispatchReplanAssignment, DispatchReplanImpactSummary,
    DispatchReplanImpactWarning, DispatchReplanNotificationSummary, DispatchReplanNotificationSummaryItem,
    DispatchReplanSnapshotOrder, DispatchReplanSuggestion,
};
use crate::services::notification_service::DispatchBatchNotificationCreate;
use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{DispatchOrder, DispatchOrderStatus};
use fms_domain::models::dispatch_collaboration::DispatchCollaborationEvent;
use fms_domain::ports::dispatch_repository::CreateDispatchOrderCommand;

use super::super::helpers::*;
use super::{ApplyValidationContext, DispatchFrontendReplanService};

fn slot_assignment_index<'a>(
    assignments: &'a [Value],
    resource_field: &str,
    resource_label: &str,
) -> Result<HashMap<(&'a str, &'a str), &'a str>, DomainError> {
    let mut index = HashMap::new();
    for assignment in assignments {
        let Some(object) = assignment.as_object() else {
            return Err(DomainError::ValidationError(format!(
                "{resource_label}槽位赋值必须是 JSON 对象"
            )));
        };
        let order_id = object
            .get("dispatch_order_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| DomainError::ValidationError("dispatch_order_id 不能为空".to_string()))?;
        let slot_code = object
            .get("slot_code")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| DomainError::ValidationError("slot_code 不能为空".to_string()))?;
        let resource_id = object
            .get(resource_field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| DomainError::ValidationError(format!("{resource_field} 不能为空")))?;
        if index.insert((order_id, slot_code), resource_id).is_some() {
            return Err(DomainError::BusinessRuleViolation(format!(
                "派工单 {order_id} 的{resource_label}槽位 {slot_code} 被重复赋值"
            )));
        }
    }
    Ok(index)
}

fn validate_completion_anchor_suggestion(
    order: &DispatchOrder,
    suggested_end_time: Option<chrono::DateTime<Utc>>,
) -> Result<(), DomainError> {
    if order.completion_time_mode.as_deref() != Some("completion_anchor_offset") {
        return Ok(());
    }
    let expected_end = order
        .completion_anchor_time
        .zip(order.completion_offset_minutes)
        .map(|(anchor, offset)| anchor + chrono::Duration::minutes(i64::from(offset)))
        .or(order.planned_end_time)
        .ok_or_else(|| DomainError::BusinessRuleViolation(format!("完成锚点工单 {} 缺少固定完成目标", order.id)))?;
    if suggested_end_time != Some(expected_end) {
        return Err(DomainError::BusinessRuleViolation(format!(
            "完成锚点工单 {} 的重排结束时间必须保持为 {}",
            order.id,
            expected_end.to_rfc3339()
        )));
    }
    Ok(())
}

fn incomplete_slot_error(order_id: &str, slot_code: &str, resource_label: &str) -> DomainError {
    DomainError::BusinessRuleViolation(format!(
        "派工单 {order_id} 的{resource_label}槽位 {slot_code} 未填满，不能应用不完整方案"
    ))
}

impl DispatchFrontendReplanService {
    pub async fn apply_snapshot(
        &self,
        request: DispatchReplanApplyRequest,
        actor_id: Option<String>,
        actor_name: Option<String>,
    ) -> Result<DispatchReplanApplyResponse, DomainError> {
        let effective_suggestions = if request.order_results.is_empty() {
            request.suggestions.clone()
        } else {
            request
                .order_results
                .iter()
                .map(Self::order_result_to_suggestion)
                .collect()
        };
        let effective_order_results = if request.order_results.is_empty() {
            effective_suggestions
                .iter()
                .cloned()
                .map(Self::suggestion_to_order_result)
                .collect::<Vec<_>>()
        } else {
            request.order_results.clone()
        };
        let effective_solver_metadata = if request.solver_run_metadata.is_empty() {
            request.solver_metadata.clone()
        } else {
            request.solver_run_metadata.clone()
        };
        let snapshot = self.get_snapshot(&request.snapshot_id)?;
        if snapshot.solver_version != request.solver_version {
            return Err(DomainError::BusinessRuleViolation(
                "求解器版本不匹配，请重新预览".to_string(),
            ));
        }

        if effective_suggestions.is_empty() {
            if !request.personnel_slot_assignments.is_empty()
                || !request.equipment_slot_assignments.is_empty()
                || !request.continuity_decisions.is_empty()
            {
                return Err(DomainError::ValidationError("order_results 不能为空".to_string()));
            }
            return Ok(DispatchReplanApplyResponse {
                snapshot_id: request.snapshot_id,
                applied: false,
                suggestions: Vec::new(),
                order_results: Vec::new(),
                personnel_slot_assignments: request.personnel_slot_assignments,
                equipment_slot_assignments: request.equipment_slot_assignments,
                continuity_decisions: request.continuity_decisions,
                objective_breakdown: request.objective_breakdown,
                solver_metadata: effective_solver_metadata.clone(),
                solver_run_metadata: effective_solver_metadata,
                notification_summary: DispatchReplanNotificationSummary::default(),
                impact_summary: DispatchReplanImpactSummary::default(),
                changed_orders: Vec::new(),
                risk_level: "low".to_string(),
                requires_manual_confirmation: false,
                message: "无可应用的重排建议".to_string(),
            });
        }

        Self::validate_complete_solver_output(&request, &snapshot.orders, &effective_solver_metadata)?;

        let snapshot_orders: HashMap<String, DispatchReplanSnapshotOrder> = snapshot
            .orders
            .iter()
            .cloned()
            .map(|item| (item.order_id.clone(), item))
            .collect();
        let validations = self
            .validate_apply_request(&request, &snapshot_orders, &effective_suggestions)
            .await?;
        let affected_flight_ids = validations
            .iter()
            .map(|context| context.live_order.flight_id.trim().to_string())
            .filter(|flight_id| !flight_id.is_empty())
            .collect::<HashSet<_>>();

        let mut applied_suggestions = Vec::new();
        let mut notification_summary = DispatchReplanNotificationSummary::default();
        for context in validations {
            let live_order = context.live_order.clone();
            let original_assignment = self.assignment_from_order(&context.live_order);
            let applied_suggestion = self
                .apply_single_suggestion(
                    context,
                    actor_id.as_deref().unwrap_or("dispatch_system"),
                    actor_name.as_deref(),
                    &mut notification_summary,
                )
                .await?;
            self.record_collaboration_event(
                &live_order,
                &applied_suggestion,
                &original_assignment,
                actor_id.as_deref(),
                actor_name.as_deref(),
            )
            .await;
            applied_suggestions.push(applied_suggestion);
        }

        let impact_summary = self.build_apply_impact_summary(&applied_suggestions, &snapshot_orders);
        self.sync_dispatch_chat_for_flights(&affected_flight_ids).await;
        let changed_orders = applied_suggestions
            .iter()
            .map(|item| item.dispatch_order_id.clone())
            .filter(|item| !item.trim().is_empty())
            .collect::<Vec<_>>();
        let risk_level = self.build_apply_risk_level(&applied_suggestions);
        let requires_manual_confirmation = applied_suggestions.iter().any(|item| {
            item.requires_manual_confirmation
                || matches!(
                    item.suggestion_type.as_deref(),
                    Some("assigned_conflict_resolution" | "unassigned_late_assignment")
                )
        });
        let order_results = Self::merge_order_results(&effective_order_results, &applied_suggestions);

        Ok(DispatchReplanApplyResponse {
            snapshot_id: request.snapshot_id,
            applied: true,
            suggestions: applied_suggestions.clone(),
            order_results,
            personnel_slot_assignments: request.personnel_slot_assignments,
            equipment_slot_assignments: request.equipment_slot_assignments,
            continuity_decisions: request.continuity_decisions,
            objective_breakdown: request.objective_breakdown,
            solver_metadata: effective_solver_metadata.clone(),
            solver_run_metadata: effective_solver_metadata,
            notification_summary,
            impact_summary,
            changed_orders,
            risk_level,
            requires_manual_confirmation,
            message: format!("已应用重排（{}条）", applied_suggestions.len()),
        })
    }

    async fn validate_apply_request(
        &self,
        _request: &DispatchReplanApplyRequest,
        snapshot_orders: &HashMap<String, DispatchReplanSnapshotOrder>,
        effective_suggestions: &[DispatchReplanSuggestion],
    ) -> Result<Vec<ApplyValidationContext>, DomainError> {
        let mut contexts = Vec::new();
        for suggestion in effective_suggestions {
            let order_id = suggestion.dispatch_order_id.trim();
            if order_id.is_empty() {
                return Err(DomainError::ValidationError(
                    "dispatch_order_id is required".to_string(),
                ));
            }
            let Some(snapshot_order) = snapshot_orders.get(order_id).cloned() else {
                return Err(DomainError::BusinessRuleViolation(format!(
                    "派工单 {order_id} 不在当前快照中，请重新预览"
                )));
            };
            if snapshot_order.is_locked || snapshot_order.order_class == "locked" {
                return Err(DomainError::BusinessRuleViolation(format!(
                    "派工单 {order_id} 已锁定，无法应用重排"
                )));
            }
            let Some(live_order) = self.order_repo.find_by_id(order_id, true, None).await? else {
                return Err(DomainError::NotFound {
                    entity_type: "dispatch_order",
                    id: order_id.to_string(),
                });
            };
            self.validate_suggestion_type(&snapshot_order, suggestion)?;

            let live_status = order_status_text(live_order.status);
            if pending_assigned_compatible(&snapshot_order.status, live_status) {
                // compatible
            } else if live_status != snapshot_order.status {
                return Err(DomainError::BusinessRuleViolation(format!(
                    "派工单 {order_id} 状态已变化，请重新预览"
                )));
            }
            if is_locked_order(&live_order) {
                return Err(DomainError::BusinessRuleViolation(format!(
                    "派工单 {order_id} 已锁定，无法应用重排"
                )));
            }

            let current_assignment = suggestion
                .current_assignment
                .as_ref()
                .or(snapshot_order.current_assignment.as_ref())
                .cloned()
                .unwrap_or_default();
            let suggested_assignment = suggestion.suggested_assignment.as_ref().cloned().unwrap_or_default();
            contexts.push(ApplyValidationContext {
                suggestion: suggestion.clone(),
                snapshot_order,
                live_order,
                current_assignment,
                suggested_assignment,
            });
        }
        Ok(contexts)
    }

    pub(super) fn validate_complete_solver_output(
        request: &DispatchReplanApplyRequest,
        snapshot_orders: &[DispatchReplanSnapshotOrder],
        solver_metadata: &HashMap<String, Value>,
    ) -> Result<(), DomainError> {
        if solver_metadata.get("feasible").and_then(Value::as_bool) != Some(true)
            || solver_metadata.get("plan_complete").and_then(Value::as_bool) != Some(true)
        {
            return Err(DomainError::BusinessRuleViolation(
                "重排方案未完整填满全部人员和设备槽位，不能通过普通应用流程提交".to_string(),
            ));
        }

        let personnel = slot_assignment_index(&request.personnel_slot_assignments, "user_id", "人员")?;
        let equipment = slot_assignment_index(&request.equipment_slot_assignments, "equipment_id", "设备")?;

        for order in snapshot_orders.iter().filter(|order| order.is_optimizable) {
            for slot in &order.personnel_slots {
                let key = (order.order_id.as_str(), slot.slot_code.as_str());
                let Some(user_id) = personnel.get(&key) else {
                    return Err(incomplete_slot_error(&order.order_id, &slot.slot_code, "人员"));
                };
                if !slot.candidate_user_ids.iter().any(|candidate| candidate == user_id) {
                    return Err(DomainError::BusinessRuleViolation(format!(
                        "派工单 {} 的人员槽位 {} 选择了快照候选集之外的人员",
                        order.order_id, slot.slot_code
                    )));
                }
            }
            for slot in &order.equipment_slots {
                let key = (order.order_id.as_str(), slot.slot_code.as_str());
                let Some(equipment_id) = equipment.get(&key) else {
                    return Err(incomplete_slot_error(&order.order_id, &slot.slot_code, "设备"));
                };
                if !slot
                    .candidate_equipment_ids
                    .iter()
                    .any(|candidate| candidate == equipment_id)
                {
                    return Err(DomainError::BusinessRuleViolation(format!(
                        "派工单 {} 的设备槽位 {} 选择了快照候选集之外的设备",
                        order.order_id, slot.slot_code
                    )));
                }
            }
        }
        Ok(())
    }

    async fn apply_single_suggestion(
        &self,
        context: ApplyValidationContext,
        actor_id: &str,
        actor_name: Option<&str>,
        notification_summary: &mut DispatchReplanNotificationSummary,
    ) -> Result<DispatchReplanSuggestion, DomainError> {
        let mut order = context.live_order.clone();
        validate_completion_anchor_suggestion(&order, context.suggestion.suggested_end_time)?;
        if let Some(start) = context.suggestion.suggested_start_time {
            order.planned_start_time = Some(start);
        }
        if let Some(end) = context.suggestion.suggested_end_time {
            order.planned_end_time = Some(end);
        }

        let normalized_assignment = self.normalize_assignment(&context.suggested_assignment);
        self.apply_assignment_to_order(&mut order, &normalized_assignment);
        if context.snapshot_order.order_class == "unassigned"
            && matches!(order.status, DispatchOrderStatus::Pending)
            && has_primary_assignment(&normalized_assignment)
        {
            order.status = DispatchOrderStatus::Assigned;
        }
        order.updated_at = Some(Utc::now());
        self.order_repo
            .create_order_atomic(CreateDispatchOrderCommand {
                members: build_dispatch_members(&order, &normalized_assignment),
                persist_equipment_assignments: true,
                equipment_ids: normalized_assignment.equipment_ids.clone(),
                log_action: "replanned".to_string(),
                log_actor_id: Some(actor_id.to_string()),
                log_details: Some(json!({
                    "strategy": context.suggestion.suggestion_type,
                    "suggested_start_time": context.suggestion.suggested_start_time,
                    "suggested_end_time": context.suggestion.suggested_end_time,
                    "actor_name": actor_name,
                })),
                order: order.clone(),
            })
            .await?;

        let mut applied = context.suggestion.clone();
        applied.order_id = Some(order.id.clone());
        applied.order_ids = vec![order.id.clone()];
        applied.flight_id = Some(order.flight_id.clone()).filter(|value| !value.trim().is_empty());
        applied.current_assignment = Some(context.current_assignment.clone());
        applied.suggested_assignment = Some(normalized_assignment.clone());
        applied.order_class = Some(context.snapshot_order.order_class.clone());
        applied.crew_requirement_snapshot = normalized_assignment.crew_requirement_snapshot.clone();
        applied.qualification_gap = normalized_assignment.qualification_gap.clone();
        applied.department_rule_version = normalized_assignment.department_rule_version.clone();
        applied.task_crew = Some(normalized_assignment.task_crew.clone());
        let member_change_summary = self.member_change_summary(&context.current_assignment, &normalized_assignment);
        let changed_member_count = member_change_summary
            .get("changed_member_count")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        applied.member_change_summary = member_change_summary;
        applied.requires_manual_confirmation = applied.requires_manual_confirmation
            || !applied.qualification_gap.is_empty()
            || changed_member_count >= 2
            || applied
                .suggestion_type
                .as_deref()
                .map(|value| value == "unassigned_late_assignment")
                .unwrap_or(false);
        applied.risk_level = Some(suggestion_risk_level(&applied).to_string());
        applied.safety_gate_state = Some(if applied.requires_manual_confirmation {
            "manual_review_required".to_string()
        } else {
            "pass".to_string()
        });

        self.send_notifications(
            &order,
            &context.current_assignment,
            &normalized_assignment,
            &applied,
            notification_summary,
        )
        .await;

        Ok(applied)
    }

    async fn sync_dispatch_chat_for_flights(&self, flight_ids: &HashSet<String>) {
        let Some(dispatch_chat_service) = self.dispatch_chat_service.as_ref() else {
            return;
        };
        for flight_id in flight_ids {
            if let Err(error) = dispatch_chat_service.sync_group_for_flight_id(flight_id).await {
                warn!(flight_id, error = %error, "failed to sync replan dispatch chat group");
            }
        }
    }

    async fn send_notifications(
        &self,
        order: &DispatchOrder,
        current_assignment: &DispatchReplanAssignment,
        suggested_assignment: &DispatchReplanAssignment,
        suggestion: &DispatchReplanSuggestion,
        summary: &mut DispatchReplanNotificationSummary,
    ) {
        let Some(notification_service) = self.notification_service.as_ref() else {
            return;
        };

        let recipient_user_ids = recipient_user_ids(current_assignment, suggested_assignment);
        if recipient_user_ids.is_empty() {
            return;
        }

        let receipt_required = suggestion.requires_manual_confirmation
            || suggestion.lateness_minutes > 0
            || suggestion
                .suggestion_type
                .as_deref()
                .map(|value| value == "unassigned_late_assignment")
                .unwrap_or(false);
        let title = format!("派工重排通知 · {}", order.task_type);
        let body = format!(
            "航班 {} 的派工单 {} 已重排，开始时间 {}。",
            order.flight_id,
            order.id,
            suggestion
                .suggested_start_time
                .or(order.planned_start_time)
                .map(|value| value.to_rfc3339())
                .unwrap_or_else(|| "未变更".to_string())
        );
        match notification_service
            .send_batch(DispatchBatchNotificationCreate {
                user_ids: recipient_user_ids.clone(),
                title,
                body,
                category: "dispatch".to_string(),
                severity: if receipt_required { "warning" } else { "info" }.to_string(),
                flight_id: Some(order.flight_id.clone()),
                related_entity_type: Some("dispatch_order".to_string()),
                related_entity_id: Some(order.id.clone()),
                dispatch_order_id: Some(order.id.clone()),
                group_id: None,
                sender_user_id: None,
                sender_username_snapshot: None,
                origin_type: "manual".to_string(),
                receipt_required,
            })
            .await
        {
            Ok(result) => {
                let receipt_group_id = result
                    .get("receipt_group_id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let sent_count = result
                    .get("items")
                    .and_then(Value::as_array)
                    .map(|items| items.len() as i64)
                    .unwrap_or(0);
                summary.total_sent_count += sent_count;
                if receipt_required {
                    summary.receipt_required_count += 1;
                }
                summary.items.push(DispatchReplanNotificationSummaryItem {
                    dispatch_order_id: order.id.clone(),
                    suggestion_type: suggestion
                        .suggestion_type
                        .clone()
                        .unwrap_or_else(|| "assigned_conflict_resolution".to_string()),
                    recipient_user_ids,
                    sent_count,
                    failed_count: 0,
                    receipt_group_id,
                });
            }
            Err(_) => {
                let failed_count = recipient_user_ids.len() as i64;
                summary.total_failed_count += failed_count;
                summary.items.push(DispatchReplanNotificationSummaryItem {
                    dispatch_order_id: order.id.clone(),
                    suggestion_type: suggestion
                        .suggestion_type
                        .clone()
                        .unwrap_or_else(|| "assigned_conflict_resolution".to_string()),
                    recipient_user_ids,
                    sent_count: 0,
                    failed_count,
                    receipt_group_id: None,
                });
            }
        }
    }

    async fn record_collaboration_event(
        &self,
        order: &DispatchOrder,
        suggestion: &DispatchReplanSuggestion,
        current_assignment: &DispatchReplanAssignment,
        actor_id: Option<&str>,
        actor_name: Option<&str>,
    ) {
        let Some(collaboration_repo) = self.collaboration_repo.as_ref() else {
            return;
        };

        let event = DispatchCollaborationEvent {
            event_id: ulid::Ulid::new().to_string(),
            flight_id: order.flight_id.clone(),
            dispatch_order_id: Some(suggestion.dispatch_order_id.clone()),
            group_id: None,
            event_type: "order_replanned".to_string(),
            actor_user_id: actor_id.map(str::to_string),
            actor_username: actor_name.map(str::to_string),
            correlation_id: None,
            payload: json!({
                "suggestion_type": suggestion.suggestion_type,
                "current_assignment": current_assignment,
                "suggested_assignment": suggestion.suggested_assignment,
                "requires_manual_confirmation": suggestion.requires_manual_confirmation,
            }),
            occurred_at: Utc::now(),
            source_table: Some("dispatch_order_logs".to_string()),
            source_record_id: None,
        };
        let _ = collaboration_repo.create_event(&event).await;
    }

    fn validate_suggestion_type(
        &self,
        snapshot_order: &DispatchReplanSnapshotOrder,
        suggestion: &DispatchReplanSuggestion,
    ) -> Result<(), DomainError> {
        let suggestion_type = suggestion.suggestion_type.as_deref().unwrap_or("").trim();
        match snapshot_order.order_class.as_str() {
            "assigned_conflict" if !suggestion_type.is_empty() && suggestion_type != "assigned_conflict_resolution" => {
                Err(DomainError::BusinessRuleViolation(
                    "冲突修复工单只能应用 assigned_conflict_resolution 建议".to_string(),
                ))
            }
            "unassigned"
                if !matches!(
                    suggestion_type,
                    "" | "unassigned_new_assignment" | "unassigned_late_assignment"
                ) =>
            {
                Err(DomainError::BusinessRuleViolation("未指派工单建议类型非法".to_string()))
            }
            _ => Ok(()),
        }
    }

    fn build_apply_impact_summary(
        &self,
        suggestions: &[DispatchReplanSuggestion],
        snapshot_orders: &HashMap<String, DispatchReplanSnapshotOrder>,
    ) -> DispatchReplanImpactSummary {
        let mut affected_flights = HashSet::new();
        let mut affected_order_ids = HashSet::new();
        let mut delayed_orders = 0i64;
        let mut reassigned_orders = 0i64;
        let mut conflicts_fixed_count = 0i64;
        let mut new_assignment_count = 0i64;
        let mut locked_item_count = 0i64;
        let mut high_risk_change_count = 0i64;
        let mut added_delay_minutes = 0.0;
        let mut replaced_member_count = 0i64;
        let mut qualification_gap_count = 0i64;
        let mut warnings = Vec::new();

        for item in suggestions {
            affected_order_ids.insert(item.dispatch_order_id.clone());
            if let Some(snapshot_order) = snapshot_orders.get(&item.dispatch_order_id) {
                if !snapshot_order.flight_id.trim().is_empty() {
                    affected_flights.insert(snapshot_order.flight_id.clone());
                }
                if snapshot_order.is_locked || snapshot_order.is_fixed_anchor || snapshot_order.order_class == "locked"
                {
                    locked_item_count += 1;
                    warnings.push(DispatchReplanImpactWarning {
                        code: "locked_item_changed".to_string(),
                        label: "建议涉及锁定任务，请人工确认".to_string(),
                        order_id: Some(snapshot_order.order_id.clone()),
                        flight_id: Some(snapshot_order.flight_id.clone()),
                    });
                }
            }

            if item.current_assignment != item.suggested_assignment {
                reassigned_orders += 1;
            }
            if matches!(item.suggestion_type.as_deref(), Some("assigned_conflict_resolution")) {
                conflicts_fixed_count += 1;
            }
            if matches!(
                item.suggestion_type.as_deref(),
                Some("unassigned_assignment" | "unassigned_late_assignment")
            ) || item
                .current_assignment
                .as_ref()
                .map(|assignment| !has_primary_assignment(assignment))
                .unwrap_or(false)
                && item
                    .suggested_assignment
                    .as_ref()
                    .map(has_primary_assignment)
                    .unwrap_or(false)
            {
                new_assignment_count += 1;
            }

            replaced_member_count += item
                .member_change_summary
                .get("changed_member_count")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            qualification_gap_count += item.qualification_gap.len() as i64;
            if !item.qualification_gap.is_empty() {
                warnings.push(DispatchReplanImpactWarning {
                    code: "qualification_gap".to_string(),
                    label: "建议存在资质缺口，需要人工复核".to_string(),
                    order_id: Some(item.dispatch_order_id.clone()),
                    flight_id: item.flight_id.clone(),
                });
            }

            if let (Some(original_start), Some(suggested_start)) = (item.original_start_time, item.suggested_start_time)
            {
                if suggested_start > original_start {
                    delayed_orders += 1;
                }
            }

            added_delay_minutes += item.lateness_minutes as f64;
            if item.lateness_minutes > 0
                || matches!(item.suggestion_type.as_deref(), Some("unassigned_late_assignment"))
            {
                delayed_orders += if item.original_start_time.zip(item.suggested_start_time).is_none() {
                    1
                } else {
                    0
                };
            }
            if is_high_risk_suggestion(item) {
                high_risk_change_count += 1;
                warnings.push(DispatchReplanImpactWarning {
                    code: "high_risk_change".to_string(),
                    label: "高风险变更，需要人工确认".to_string(),
                    order_id: Some(item.dispatch_order_id.clone()),
                    flight_id: item.flight_id.clone(),
                });
            }
        }

        DispatchReplanImpactSummary {
            affected_order_count: affected_order_ids.len() as i64,
            affected_flight_count: affected_flights.len() as i64,
            conflicts_fixed_count,
            new_assignment_count,
            late_assignment_count: delayed_orders,
            locked_item_count,
            high_risk_change_count,
            warnings,
            affected_flights: affected_flights.len() as i64,
            changed_orders: suggestions.len() as i64,
            reassigned_orders,
            delayed_orders,
            added_delay_minutes: (added_delay_minutes * 100.0).round() / 100.0,
            replaced_member_count,
            qualification_gap_count,
        }
    }

    fn build_apply_risk_level(&self, suggestions: &[DispatchReplanSuggestion]) -> String {
        if suggestions.iter().any(|item| !item.qualification_gap.is_empty()) {
            return "critical".to_string();
        }
        if suggestions.iter().any(|item| item.requires_manual_confirmation) {
            return "high".to_string();
        }
        if suggestions.iter().any(|item| item.impact_score >= 30.0) {
            return "critical".to_string();
        }
        if suggestions.iter().any(|item| item.impact_score >= 15.0) {
            return "high".to_string();
        }
        if suggestions.is_empty() {
            "low".to_string()
        } else {
            "medium".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{slot_assignment_index, validate_completion_anchor_suggestion};
    use chrono::{Duration, TimeZone, Utc};
    use fms_domain::error::DomainError;
    use fms_domain::models::dispatch::{
        AssigneeType, DispatchLockLevel, DispatchOrder, DispatchOrderStatus, DispatchType, ScheduleSource,
    };
    use serde_json::json;

    #[test]
    fn duplicate_slot_assignments_are_rejected_instead_of_overwritten() {
        let assignments = vec![
            json!({
                "dispatch_order_id": "order-1",
                "slot_code": "loader#1",
                "user_id": "user-a",
            }),
            json!({
                "dispatch_order_id": "order-1",
                "slot_code": "loader#1",
                "user_id": "user-b",
            }),
        ];

        let error =
            slot_assignment_index(&assignments, "user_id", "人员").expect_err("duplicate slot must be rejected");

        assert!(matches!(error, DomainError::BusinessRuleViolation(message) if message.contains("重复赋值")));
    }

    #[test]
    fn malformed_slot_assignments_are_rejected_instead_of_skipped() {
        let assignments = vec![json!({
            "dispatch_order_id": "order-1",
            "slot_code": "loader#1",
        })];

        let error =
            slot_assignment_index(&assignments, "user_id", "人员").expect_err("missing resource id must be rejected");

        assert!(matches!(error, DomainError::ValidationError(message) if message.contains("user_id")));
    }

    #[test]
    fn completion_anchor_rejects_a_shifted_solver_end() {
        let anchor = Utc.with_ymd_and_hms(2026, 8, 8, 10, 0, 0).unwrap();
        let expected_end = anchor - Duration::minutes(10);
        let order = DispatchOrder {
            id: "order-anchored".to_string(),
            flight_id: "flight-1".to_string(),
            task_type: "boarding".to_string(),
            task_type_name: None,
            stand_id: None,
            stand_code: None,
            terminal: None,
            department: None,
            individual_user_id: None,
            individual_username: None,
            driver_type: None,
            driver_user_id: None,
            planned_start_time: Some(expected_end - Duration::minutes(30)),
            planned_end_time: Some(expected_end),
            actual_start_time: None,
            actual_end_time: None,
            estimated_completion_time: None,
            estimated_completion_reported_by: None,
            estimated_completion_reported_at: None,
            estimated_completion_note: None,
            status: DispatchOrderStatus::Pending,
            dispatch_type: DispatchType::Auto,
            dispatched_at: None,
            dispatched_by: None,
            snapshot_assignee_position: None,
            snapshot_equipment_positions: None,
            estimated_arrival_minutes: None,
            process_instance_id: None,
            process_task_id: None,
            workflow_context: json!({}),
            workflow_status: "pending_assignment".to_string(),
            source: "system".to_string(),
            schedule_source: ScheduleSource::CurrentStatusFallback,
            lock_level: DispatchLockLevel::Optimizable,
            publication_state: "prepublished".to_string(),
            source_type: "flight_rule".to_string(),
            department_id: Some("dept-1".to_string()),
            leg_scope: "outbound".to_string(),
            generation_rule_id: Some("rule-1".to_string()),
            generation_rule_version: Some(1),
            generation_anchor_type: Some("estimated_departure".to_string()),
            generation_anchor_time: Some(anchor),
            completion_time_mode: Some("completion_anchor_offset".to_string()),
            completion_anchor_type: Some("estimated_departure".to_string()),
            completion_anchor_time: Some(anchor),
            completion_offset_minutes: Some(-10),
            completion_warning_lead_minutes: None,
            publish_trigger_mode: None,
            publish_at: None,
            turnaround_pair_key: None,
            turnaround_constraint_mode: None,
            department_rule_version: None,
            crew_requirement_snapshot: vec![],
            equipment_requirement_snapshot: vec![],
            task_crew: json!({}),
            equipment_assignment: vec![],
            qualification_gap: vec![],
            equipment_gap: vec![],
            availability_reason: None,
            score_breakdown: json!({}),
            conflict_reason: None,
            recommended_assignees: vec![],
            recommendation_score: None,
            supervisor_notified: false,
            supervisor_notified_at: None,
            assignment_deadline: None,
            completed_by: None,
            completion_notes: None,
            gate: None,
            created_at: None,
            updated_at: None,
            members: vec![],
            equipment_list: vec![],
        };

        validate_completion_anchor_suggestion(&order, Some(expected_end)).expect("fixed target must pass");
        let error = validate_completion_anchor_suggestion(&order, Some(expected_end + Duration::minutes(1)))
            .expect_err("shifted completion target must be rejected");
        assert!(
            matches!(error, DomainError::BusinessRuleViolation(message) if message.contains("重排结束时间必须保持"))
        );
    }
}
