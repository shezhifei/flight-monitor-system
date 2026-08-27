use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::*;
use fms_domain::models::flight::Flight;
use fms_domain::models::flight_leg::FlightTypeCode;

use crate::schemas::dispatch_schemas::*;

use super::helpers;
use super::{DispatchService, GeneratedFlightDispatchRequest, PreparedWindowOrder, ReplanExecutionResult, NULL_VALUE};

impl DispatchService {
    async fn execute_replan(
        &self,
        orders: Vec<DispatchOrder>,
        strategy: &str,
        apply_changes: bool,
        max_suggestions: usize,
        mutable_order_ids: Option<&HashSet<String>>,
    ) -> Result<ReplanExecutionResult, DomainError> {
        let buffer_minutes = match strategy {
            "stability" => 10,
            "efficiency" => 0,
            _ => 5,
        };
        let min_duration = Duration::minutes(5);
        let fallback_now = Utc::now();
        let order_by_id = orders
            .iter()
            .cloned()
            .map(|order| (order.id.clone(), order))
            .collect::<HashMap<_, _>>();

        let mut grouped: HashMap<(String, String), Vec<DispatchOrder>> = HashMap::new();
        for order in &orders {
            for user_id in Self::order_member_user_ids(order) {
                grouped
                    .entry(("user".to_string(), user_id))
                    .or_default()
                    .push(order.clone());
            }
            if let Some(stand_id) = order
                .stand_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                grouped
                    .entry(("stand".to_string(), stand_id.to_string()))
                    .or_default()
                    .push(order.clone());
            }
        }

        let can_mutate = |order_id: &str| mutable_order_ids.map(|ids| ids.contains(order_id)).unwrap_or(true);

        let mut suggestions_by_order: HashMap<String, Value> = HashMap::new();
        for (_, group_orders) in grouped.iter_mut() {
            group_orders.sort_by_key(|order| Self::effective_interval(order, fallback_now).0);
            for index in 1..group_orders.len() {
                let previous = &group_orders[index - 1];
                let current = &group_orders[index];
                let (previous_start, previous_end) = Self::effective_interval(previous, fallback_now);
                let (current_start, current_end) = Self::effective_interval(current, fallback_now);
                let target_start = previous_end + Duration::minutes(buffer_minutes);
                if current_start >= target_start {
                    continue;
                }

                // 班组不再是指派对象：冲突只通过顺延（delay）建议消解
                let candidate = if can_mutate(&current.id) {
                    Some(Self::build_delay_replan_suggestion(
                        current,
                        &previous.id,
                        current_start,
                        current_end,
                        target_start,
                        min_duration,
                    ))
                } else if can_mutate(&previous.id) {
                    Some(Self::build_delay_replan_suggestion(
                        previous,
                        &current.id,
                        previous_start,
                        previous_end,
                        current_end + Duration::minutes(buffer_minutes),
                        min_duration,
                    ))
                } else {
                    None
                };

                if let Some(candidate) = candidate {
                    let order_id = candidate
                        .get("dispatch_order_id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    if order_id.is_empty() {
                        continue;
                    }
                    if Self::is_better_replan_candidate(&candidate, suggestions_by_order.get(&order_id)) {
                        suggestions_by_order.insert(order_id, candidate);
                    }
                }
            }
        }

        let mut suggestions = suggestions_by_order.into_values().collect::<Vec<_>>();
        suggestions.sort_by(|left, right| {
            left.get("impact_score")
                .and_then(Value::as_f64)
                .unwrap_or(f64::MAX)
                .partial_cmp(&right.get("impact_score").and_then(Value::as_f64).unwrap_or(f64::MAX))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        suggestions.truncate(max_suggestions);

        if apply_changes {
            for suggestion in &suggestions {
                let order_id = suggestion
                    .get("dispatch_order_id")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let new_start = suggestion
                    .get("suggested_start_time")
                    .and_then(Value::as_str)
                    .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                    .map(|value| value.with_timezone(&Utc));
                let new_end = suggestion
                    .get("suggested_end_time")
                    .and_then(Value::as_str)
                    .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                    .map(|value| value.with_timezone(&Utc));
                let suggested_assignment = suggestion.get("suggested_assignment");

                if let Some(mut order) = self.order.order_repo.find_by_id(order_id, true, None).await? {
                    if let Some(start) = new_start {
                        order.planned_start_time = Some(start);
                    }
                    if let Some(end) = new_end {
                        order.planned_end_time = Some(end);
                    }
                    Self::apply_assignment_json(&mut order, suggested_assignment);
                    if helpers::optimal_order_has_assignment(&order)
                        && matches!(order.status, DispatchOrderStatus::Pending)
                    {
                        order.status = DispatchOrderStatus::Assigned;
                        order.dispatched_at = order.dispatched_at.or(Some(Utc::now()));
                        order.dispatch_type = DispatchType::Auto;
                    }
                    order.updated_at = Some(Utc::now());
                    self.order.order_repo.save(&order).await?;
                    if let Some(assignment) = suggested_assignment {
                        self.sync_assignment_members(&order, assignment).await?;
                        self.order
                            .order_repo
                            .replace_order_equipment_assignments(&order.id, &Self::assignment_equipment_ids(assignment))
                            .await?;
                    }
                    self.sync_dispatch_chat_for_order(&order.id).await;
                    self.order
                        .order_repo
                        .append_log(
                            &order.id,
                            "replanned",
                            None,
                            Some(json!({
                                "strategy": strategy,
                                "original_start_time": suggestion.get("original_start_time").unwrap_or(&NULL_VALUE),
                                "original_end_time": suggestion.get("original_end_time").unwrap_or(&NULL_VALUE),
                                "suggested_start_time": suggestion.get("suggested_start_time").unwrap_or(&NULL_VALUE),
                                "suggested_end_time": suggestion.get("suggested_end_time").unwrap_or(&NULL_VALUE),
                                "suggested_assignment": suggested_assignment.unwrap_or(&NULL_VALUE),
                            })),
                        )
                        .await?;
                }
            }
        }

        let summary = self.summarize_replan_suggestions(&suggestions, &order_by_id).await?;

        Ok(ReplanExecutionResult { suggestions, summary })
    }

    async fn summarize_replan_suggestions(
        &self,
        suggestions: &[Value],
        order_by_id: &HashMap<String, DispatchOrder>,
    ) -> Result<Value, DomainError> {
        let mut affected_flights = HashSet::new();
        let mut changed_orders = HashSet::new();
        let mut reassigned_orders = 0i64;
        let mut delayed_orders = 0i64;
        let mut added_delay_minutes = 0.0f64;
        let mut requires_manual_confirmation = false;

        for suggestion in suggestions {
            let order_id = suggestion
                .get("dispatch_order_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if let Some(order) = order_by_id.get(order_id) {
                affected_flights.insert(order.flight_id.clone());
            }
            if !order_id.is_empty() {
                changed_orders.insert(order_id.to_string());
            }
            if suggestion.get("current_assignment") != suggestion.get("suggested_assignment") {
                reassigned_orders += 1;
            }
            let original_start = suggestion.get("original_start_time").and_then(Value::as_str);
            let suggested_start = suggestion.get("suggested_start_time").and_then(Value::as_str);
            if let (Some(original_start), Some(suggested_start)) = (original_start, suggested_start) {
                if suggested_start > original_start {
                    delayed_orders += 1;
                    added_delay_minutes += suggestion
                        .get("lateness_minutes")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0);
                }
            }
            if suggestion.get("impact_score").and_then(Value::as_f64).unwrap_or(0.0) >= 15.0
                || suggestion.get("suggestion_type").and_then(Value::as_str) == Some("assigned_conflict_resolution")
            {
                requires_manual_confirmation = true;
            }
        }

        let risk_level = if added_delay_minutes >= 60.0 || reassigned_orders >= 5 {
            "critical"
        } else if added_delay_minutes >= 30.0 || reassigned_orders >= 3 {
            "high"
        } else if added_delay_minutes > 0.0 || reassigned_orders > 0 {
            "medium"
        } else {
            "low"
        };

        let mut changed_orders = changed_orders.into_iter().collect::<Vec<_>>();
        changed_orders.sort();

        Ok(json!({
            "impact_summary": {
                "affected_flights": affected_flights.len(),
                "changed_orders": changed_orders.len(),
                "reassigned_orders": reassigned_orders,
                "delayed_orders": delayed_orders,
                "added_delay_minutes": (added_delay_minutes * 100.0).round() / 100.0,
            },
            "changed_orders": changed_orders,
            "risk_level": risk_level,
            "requires_manual_confirmation": requires_manual_confirmation,
        }))
    }
    pub async fn replan(&self, dto: ReplanRequest) -> Result<serde_json::Value, DomainError> {
        if dto.window_end <= dto.window_start {
            return Err(DomainError::ValidationError("window_end 必须晚于 window_start".into()));
        }

        let orders = self
            .order
            .order_repo
            .find_orders_in_window(
                dto.window_start,
                dto.window_end,
                &Self::ACTIVE_CONFLICT_STATUSES,
                None,
                None,
                None,
                false,
            )
            .await?;
        let execution = self
            .execute_replan(
                orders,
                &dto.strategy,
                dto.apply_changes,
                dto.max_suggestions.unwrap_or(20).clamp(1, 500) as usize,
                None,
            )
            .await?;

        let empty_obj = serde_json::json!({});
        let empty_array = serde_json::json!([]);
        let low_risk = serde_json::json!("low");
        let false_val = serde_json::json!(false);

        Ok(serde_json::json!({
            "strategy": dto.strategy,
            "window_start": dto.window_start.to_rfc3339(),
            "window_end": dto.window_end.to_rfc3339(),
            "generated_at": Utc::now().to_rfc3339(),
            "applied": dto.apply_changes,
            "impact_summary": execution.summary.get("impact_summary").unwrap_or(&empty_obj),
            "changed_orders": execution.summary.get("changed_orders").unwrap_or(&empty_array),
            "risk_level": execution.summary.get("risk_level").unwrap_or(&low_risk),
            "requires_manual_confirmation": execution.summary
                .get("requires_manual_confirmation")
                .unwrap_or(&false_val),
            "suggestions": execution.suggestions,
        }))
    }
}
