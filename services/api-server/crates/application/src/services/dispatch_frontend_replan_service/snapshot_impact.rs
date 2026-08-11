use std::collections::{HashMap, HashSet};

use crate::schemas::dispatch_schemas::{
    DispatchReplanImpactSummary, DispatchReplanImpactWarning, DispatchReplanSnapshotOrder, DispatchReplanTurnaroundPair,
};

use super::super::super::helpers::*;
use super::super::DispatchFrontendReplanService;

impl DispatchFrontendReplanService {
    pub(super) fn build_turnaround_pairs(
        &self,
        optimizable_orders: &[DispatchReplanSnapshotOrder],
        fixed_orders: &[DispatchReplanSnapshotOrder],
    ) -> Vec<DispatchReplanTurnaroundPair> {
        let mut orders_by_pair: HashMap<String, Vec<&DispatchReplanSnapshotOrder>> = HashMap::new();
        for order in optimizable_orders.iter().chain(fixed_orders.iter()) {
            let Some(pair_key) = order.turnaround_pair_key.as_ref() else {
                continue;
            };
            let pair_key = pair_key.trim();
            if pair_key.is_empty() {
                continue;
            }
            orders_by_pair.entry(pair_key.to_string()).or_default().push(order);
        }

        let mut pairs = Vec::new();
        for (pair_key, grouped_orders) in orders_by_pair {
            if grouped_orders.len() < 2 {
                continue;
            }

            let inbound = grouped_orders
                .iter()
                .copied()
                .find(|order| order_leg_scope(order) == Some("inbound"))
                .or_else(|| {
                    grouped_orders
                        .iter()
                        .copied()
                        .min_by_key(|order| order.planned_end_time.or(order.effective_end_time))
                });
            let outbound = grouped_orders
                .iter()
                .copied()
                .find(|order| order_leg_scope(order) == Some("outbound"))
                .or_else(|| {
                    grouped_orders
                        .iter()
                        .copied()
                        .max_by_key(|order| order.planned_start_time.or(order.effective_start_time))
                });

            let (Some(inbound), Some(outbound)) = (inbound, outbound) else {
                continue;
            };
            if inbound.order_id == outbound.order_id {
                continue;
            }

            let slot_pairs = turnaround_slot_pairs(inbound, outbound);
            let inbound_slot_code = slot_pairs.first().map(|item| item.inbound_slot_code.clone());
            let outbound_slot_code = slot_pairs.first().map(|item| item.outbound_slot_code.clone());
            let planned_sta = inbound.planned_end_time.or(inbound.effective_end_time);
            let planned_std = outbound.planned_start_time.or(outbound.effective_start_time);
            let slack_minutes = planned_sta
                .zip(planned_std)
                .map(|(sta, std)| (std - sta).num_minutes() as i32);
            let constraint_mode = inbound
                .turnaround_constraint_mode
                .clone()
                .or_else(|| outbound.turnaround_constraint_mode.clone());
            let hard_continuity_required =
                matches!(constraint_mode.as_deref(), Some("same_person")) && slack_minutes.unwrap_or(0) <= 0;
            let tightness_penalty = slack_minutes.map(|value| (-value).max(0) as f64).unwrap_or(0.0);

            pairs.push(DispatchReplanTurnaroundPair {
                pair_key,
                inbound_order_id: inbound.order_id.clone(),
                outbound_order_id: outbound.order_id.clone(),
                slot_pairs,
                inbound_slot_code,
                outbound_slot_code,
                planned_sta,
                planned_std,
                minimum_turnaround_minutes: None,
                slack_minutes,
                tightness_penalty,
                hard_continuity_required,
                continuity_penalty_weight: if hard_continuity_required { 1000.0 } else { 100.0 },
                constraint_mode,
            });
        }

        pairs.sort_by(|left, right| {
            left.pair_key
                .cmp(&right.pair_key)
                .then(left.inbound_order_id.cmp(&left.inbound_order_id))
                .then(left.outbound_order_id.cmp(&right.outbound_order_id))
        });
        pairs
    }

    pub(super) fn average_workload_target(&self, orders: &[DispatchReplanSnapshotOrder]) -> f64 {
        let mut workload_weight_sum = 0.0;
        let mut distinct_candidate_users = HashSet::new();

        for order in orders {
            for slot in &order.personnel_slots {
                workload_weight_sum += slot.workload_weight;
                for user_id in &slot.candidate_user_ids {
                    let user_id = user_id.trim();
                    if !user_id.is_empty() {
                        distinct_candidate_users.insert(user_id.to_string());
                    }
                }
            }
        }

        if workload_weight_sum <= 0.0 || distinct_candidate_users.is_empty() {
            0.0
        } else {
            ((workload_weight_sum / distinct_candidate_users.len() as f64) * 10000.0).round() / 10000.0
        }
    }

    pub(super) fn build_snapshot_impact_summary(
        &self,
        orders: &[DispatchReplanSnapshotOrder],
        fixed_orders: &[DispatchReplanSnapshotOrder],
        travel_time_mode: &str,
    ) -> DispatchReplanImpactSummary {
        let affected_flights: HashSet<String> = orders.iter().map(|item| item.flight_id.clone()).collect();
        let conflicts_fixed_count = orders
            .iter()
            .filter(|item| item.conflict_state == "resource_conflict")
            .count() as i64;
        let late_assignment_count = orders.iter().filter(|item| item.conflict_state == "gap").count() as i64;
        let new_assignment_count = orders
            .iter()
            .filter(|item| {
                item.order_class == "unassigned"
                    || item
                        .current_assignment
                        .as_ref()
                        .map(|assignment| !has_primary_assignment(assignment))
                        .unwrap_or(true)
            })
            .count() as i64;
        let locked_item_count = fixed_orders
            .iter()
            .filter(|item| item.is_locked || item.is_fixed_anchor || item.order_class == "locked")
            .count() as i64;
        let qualification_gap_count = orders
            .iter()
            .map(|item| item.baseline_assignment.qualification_gap.len() as i64)
            .sum();
        let high_risk_change_count = orders
            .iter()
            .filter(|item| {
                matches!(item.conflict_state.as_str(), "resource_conflict" | "gap")
                    || !item.baseline_assignment.qualification_gap.is_empty()
            })
            .count() as i64;
        let warnings = self.build_snapshot_impact_warnings(orders, fixed_orders, travel_time_mode, locked_item_count);
        DispatchReplanImpactSummary {
            affected_order_count: orders.len() as i64,
            affected_flight_count: affected_flights.len() as i64,
            conflicts_fixed_count,
            new_assignment_count,
            late_assignment_count,
            locked_item_count,
            high_risk_change_count,
            warnings,
            affected_flights: affected_flights.len() as i64,
            changed_orders: 0,
            reassigned_orders: conflicts_fixed_count,
            delayed_orders: late_assignment_count,
            added_delay_minutes: 0.0,
            replaced_member_count: 0,
            qualification_gap_count,
        }
    }

    fn build_snapshot_impact_warnings(
        &self,
        orders: &[DispatchReplanSnapshotOrder],
        fixed_orders: &[DispatchReplanSnapshotOrder],
        travel_time_mode: &str,
        locked_item_count: i64,
    ) -> Vec<DispatchReplanImpactWarning> {
        let mut warnings = Vec::new();
        if locked_item_count > 0 {
            warnings.push(DispatchReplanImpactWarning {
                code: "locked_items_excluded".to_string(),
                label: format!("{locked_item_count} 个锁定或锚定任务不会参与优化"),
                order_id: fixed_orders.first().map(|item| item.order_id.clone()),
                flight_id: fixed_orders.first().map(|item| item.flight_id.clone()),
            });
        }
        for order in orders {
            if !order.baseline_assignment.qualification_gap.is_empty() {
                warnings.push(DispatchReplanImpactWarning {
                    code: "qualification_gap".to_string(),
                    label: "存在资质缺口，需要人工复核".to_string(),
                    order_id: Some(order.order_id.clone()),
                    flight_id: Some(order.flight_id.clone()),
                });
            }
            if order.conflict_state == "resource_conflict" {
                warnings.push(DispatchReplanImpactWarning {
                    code: "resource_conflict_candidate".to_string(),
                    label: "存在资源冲突，预览将优先尝试修复".to_string(),
                    order_id: Some(order.order_id.clone()),
                    flight_id: Some(order.flight_id.clone()),
                });
            }
            if order.conflict_state == "gap" {
                warnings.push(DispatchReplanImpactWarning {
                    code: "late_assignment_candidate".to_string(),
                    label: "可能产生晚分配或时间缺口".to_string(),
                    order_id: Some(order.order_id.clone()),
                    flight_id: Some(order.flight_id.clone()),
                });
            }
        }
        if travel_time_mode == "zero_matrix_forbidden" {
            warnings.push(DispatchReplanImpactWarning {
                code: "travel_time_degraded".to_string(),
                label: "缺少历史通行时间矩阵，影响预估以保守规则降级呈现".to_string(),
                order_id: None,
                flight_id: None,
            });
        }
        warnings
    }

    pub(super) fn build_snapshot_risk_level(&self, orders: &[DispatchReplanSnapshotOrder]) -> String {
        let conflict_count = orders
            .iter()
            .filter(|item| matches!(item.conflict_state.as_str(), "resource_conflict" | "gap"))
            .count();
        if conflict_count >= 8 {
            "critical".to_string()
        } else if conflict_count >= 4 {
            "high".to_string()
        } else if conflict_count >= 1 {
            "medium".to_string()
        } else {
            "low".to_string()
        }
    }
}
