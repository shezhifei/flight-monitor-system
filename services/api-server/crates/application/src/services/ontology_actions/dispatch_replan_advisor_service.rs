//! Generates a dispatch replan suggestion (slot-based; teams are no longer assignment targets).

use std::sync::Arc;

use serde_json::{json, Value};

use fms_domain::ports::dispatch_repository::DispatchOrderRepository;

use super::error::{repo_err, OntologyActionError};
use super::support::{arg_str, constraint, required_str, suggestion_envelope};

pub struct DispatchReplanAdvisorService {
    dispatch_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
}

impl DispatchReplanAdvisorService {
    pub fn new(dispatch_repo: Arc<dyn DispatchOrderRepository + Send + Sync>) -> Self {
        Self { dispatch_repo }
    }

    pub async fn suggest(&self, args: &Value) -> Result<Value, OntologyActionError> {
        let order_id = required_str(args, "dispatch_order_id")?;
        let reason = required_str(args, "reason")?;
        let order = self
            .dispatch_repo
            .find_by_id(order_id, true, None)
            .await
            .map_err(repo_err)?
            .ok_or_else(|| OntologyActionError::NotFound(format!("dispatch order {order_id}")))?;

        // 槽位语义：冲突按「人 × 时间窗」检测，不再按班组
        let member_user_ids = order
            .members
            .iter()
            .filter(|member| member.is_active)
            .map(|member| member.user_id.trim().to_string())
            .filter(|user_id| !user_id.is_empty())
            .collect::<Vec<_>>();

        let mut member_conflicts = Vec::<Value>::new();
        if let (Some(start), Some(end)) = (order.planned_start_time, order.planned_end_time) {
            if end > start {
                for user_id in &member_user_ids {
                    let overlaps = self
                        .dispatch_repo
                        .find_overlapping_orders(start, end, Some(user_id), None, Some(&order.id))
                        .await
                        .map_err(repo_err)?;
                    // find_overlapping_orders 按 individual_user_id 过滤，槽成员还需按 members 复核
                    let overlaps = overlaps
                        .into_iter()
                        .filter(|candidate| {
                            candidate.individual_user_id.as_deref() == Some(user_id.as_str())
                                || candidate
                                    .members
                                    .iter()
                                    .any(|member| member.is_active && member.user_id == *user_id)
                        })
                        .collect::<Vec<_>>();
                    if !overlaps.is_empty() {
                        member_conflicts.push(json!({
                            "user_id": user_id,
                            "conflicting_order_ids": overlaps.iter().map(|item| item.id.clone()).collect::<Vec<_>>(),
                        }));
                    }
                }
            }
        }

        let has_window = matches!(
            (order.planned_start_time, order.planned_end_time),
            (Some(start), Some(end)) if end > start
        );

        let members_conflict_message = if member_conflicts.is_empty() {
            None
        } else {
            Some(format!("{} member(s) have overlapping orders", member_conflicts.len()))
        };
        let constraint_results = vec![
            constraint("order_exists", true, "error", None),
            constraint("has_time_window", has_window, "warning", None),
            constraint(
                "members_free_in_window",
                member_conflicts.is_empty(),
                "warning",
                members_conflict_message.as_deref(),
            ),
        ];

        let focus_user_id = arg_str(args, "focus_user_id");
        let score_before = 0.5f64;
        let score_after = if member_conflicts.is_empty() { 0.9 } else { 0.55 };
        let confidence = if member_conflicts.is_empty() { 0.85 } else { 0.5 };

        Ok(suggestion_envelope(
            "DispatchOrder",
            order_id,
            "suggest_replan",
            json!({ "dispatch_order_id": order_id, "reason": reason, "focus_user_id": focus_user_id }),
            "high",
            constraint_results,
            json!({ "status": order.status.as_ref(), "member_user_ids": member_user_ids }),
            json!({ "hint": "adjust_slots_or_delay", "focus_user_id": focus_user_id }),
            confidence,
            &format!("replan order {order_id}: {reason}"),
            json!({
                "resource_changes": [{
                    "kind": "crew_slots",
                    "member_user_ids": member_user_ids,
                }],
                "score_before": score_before,
                "score_after": score_after,
                "conflicts": member_conflicts,
            }),
        ))
    }
}
