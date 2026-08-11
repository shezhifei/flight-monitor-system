use std::collections::{BTreeSet, HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use serde_json::json;

use crate::schemas::dispatch_schemas::{
    DispatchReplanAssignment, DispatchReplanObjectiveConfig, DispatchReplanSnapshotResponse,
};
use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{DispatchOrder, Equipment};

use super::super::super::helpers::*;
use super::super::{DispatchFrontendReplanService, LOOKBACK_HOURS};

impl DispatchFrontendReplanService {
    /// Loads the department-owned replan parameters (`start_flex_minutes` and
    /// `duration_by_crew_size`) for the departments owning the orders in this
    /// window.
    ///
    /// Best-effort by design: without the repo wired, or if a department's rules
    /// fail to load, the affected orders fall back to the system default rather
    /// than failing the whole snapshot. A missing slack value degrades the plan;
    /// a failed snapshot leaves the dispatcher with nothing.
    async fn load_generation_rule_index(&self, orders: &[DispatchOrder]) -> GenerationRuleIndex {
        let Some(repo) = self.generation_rule_repo.as_ref() else {
            return GenerationRuleIndex::default();
        };
        let department_ids: BTreeSet<String> = orders
            .iter()
            .filter_map(|order| order.department_id.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect();
        if department_ids.is_empty() {
            return GenerationRuleIndex::default();
        }

        let mut rules = Vec::new();
        for department_id in &department_ids {
            // No status filter: an order already carries the id of the exact rule
            // version it was generated from, and the index resolves the rest by
            // status priority. Filtering here would drop that generating version
            // as soon as the department archived it.
            match repo.list_rules(department_id, None).await {
                Ok(items) => rules.extend(items),
                Err(error) => {
                    tracing::warn!(
                        department_id = %department_id,
                        error = %error,
                        "读取部门生成规则失败,该部门作业的开始时间窗与作业时长表回退到系统默认值"
                    );
                }
            }
        }
        GenerationRuleIndex::from_rules(rules.iter())
    }

    pub async fn build_snapshot(
        &self,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
        strategy: String,
        max_suggestions: i64,
    ) -> Result<DispatchReplanSnapshotResponse, DomainError> {
        if window_end <= window_start {
            return Err(DomainError::ValidationError(
                "window_end 必须晚于 window_start".to_string(),
            ));
        }

        let statuses = ["pending", "assigned", "in_progress", "completed"];
        let orders = self
            .order_repo
            .find_orders_in_window(window_start, window_end, &statuses, None, None, None, false)
            .await?;
        let historical_orders = self
            .order_repo
            .find_orders_in_window(
                window_start - Duration::hours(LOOKBACK_HOURS),
                window_start,
                &statuses,
                None,
                None,
                None,
                false,
            )
            .await?;

        let order_ids: HashSet<String> = orders.iter().map(|item| item.id.clone()).collect();
        let conflicts = self.detect_conflicts(&orders);
        let available_equipments = self.load_available_equipments(&orders).await?;
        let rules = self.load_generation_rule_index(&orders).await;
        // Mined once for the whole snapshot: many orders share the same
        // (department, qualification, level, time) requirement, and each distinct
        // one costs a pair of queries.
        let mined_candidates = self
            .build_slot_candidate_index(historical_orders.iter().chain(orders.iter()), &rules)
            .await;

        let mut snapshot_orders = Vec::new();
        for order in &orders {
            snapshot_orders.push(self.build_snapshot_order_base(order, conflicts.get(&order.id), &rules));
        }

        let mut fixed_orders = Vec::new();
        for order in historical_orders.iter().chain(orders.iter()) {
            let order_class = self.resolve_order_class(order, conflicts.get(&order.id));
            if !order_ids.contains(&order.id) || order_class == "locked" {
                let mut snapshot_order = self.build_snapshot_order_base(order, conflicts.get(&order.id), &rules);
                snapshot_order.order_class = order_class;
                snapshot_order.is_optimizable = false;
                snapshot_order.is_fixed_anchor = true;
                snapshot_order.conflict_state = "locked".to_string();
                snapshot_order.is_locked = true;
                fixed_orders.push(snapshot_order);
            }
        }
        fixed_orders.sort_by(|a, b| a.order_id.cmp(&b.order_id));
        fixed_orders.dedup_by(|a, b| a.order_id == b.order_id);
        for fixed_order in &mut fixed_orders {
            if fixed_order.current_assignment.is_none() {
                fixed_order.current_assignment = Some(DispatchReplanAssignment::default());
            }
            let current_assignment = fixed_order.current_assignment.as_ref().unwrap();
            fixed_order.personnel_slots =
                self.build_personnel_slots(fixed_order, current_assignment, &mined_candidates);
            fixed_order.equipment_slots = self.build_equipment_slots(fixed_order, current_assignment);
            if let Some(source_order) = historical_orders
                .iter()
                .chain(orders.iter())
                .find(|item| item.id == fixed_order.order_id)
            {
                fixed_order.baseline_assignment = self.build_baseline_assignment(
                    fixed_order,
                    current_assignment,
                    &fixed_order.personnel_slots,
                    &fixed_order.equipment_slots,
                    source_order,
                );
            }
        }

        let mut employee_anchor_context =
            self.build_resource_anchor_states("employee", window_start, window_end, &fixed_orders);
        let mut equipment_anchor_context =
            self.build_resource_anchor_states("equipment", window_start, window_end, &fixed_orders);
        for (index, order) in orders.iter().enumerate() {
            let enriched = self
                .enrich_snapshot_order(
                    snapshot_orders[index].clone(),
                    order,
                    &available_equipments,
                    &employee_anchor_context.segments,
                    &equipment_anchor_context.segments,
                    &mined_candidates,
                    &rules,
                )
                .await?;
            snapshot_orders[index] = enriched;
        }
        self.ensure_candidate_resource_windows(
            &mut employee_anchor_context,
            "employee",
            window_start,
            window_end,
            snapshot_orders
                .iter()
                .flat_map(|order| order.personnel_slots.iter())
                .flat_map(|slot| slot.candidate_user_ids.iter().cloned()),
        );
        self.ensure_candidate_resource_windows(
            &mut equipment_anchor_context,
            "equipment",
            window_start,
            window_end,
            snapshot_orders
                .iter()
                .flat_map(|order| order.equipment_slots.iter())
                .flat_map(|slot| slot.candidate_equipment_ids.iter().cloned()),
        );
        let travel_edges = self.build_travel_edges(&snapshot_orders).await;
        let employee_free_windows = employee_anchor_context
            .states
            .iter()
            .flat_map(|item| item.free_windows.clone())
            .collect::<Vec<_>>();
        let equipment_free_windows = equipment_anchor_context
            .states
            .iter()
            .flat_map(|item| item.free_windows.clone())
            .collect::<Vec<_>>();

        let optimizable_orders = snapshot_orders
            .iter()
            .filter(|item| item.is_optimizable)
            .cloned()
            .collect::<Vec<_>>();
        let travel_time_mode = if self.travel_stats_repo.is_some() {
            "historical_matrix".to_string()
        } else {
            "zero_matrix_forbidden".to_string()
        };
        let average_workload_target = self.average_workload_target(&optimizable_orders);
        let employee_unavailable_blocks =
            self.build_resource_unavailable_blocks("employee", window_start, window_end, &fixed_orders);
        let equipment_unavailable_blocks =
            self.build_resource_unavailable_blocks("equipment", window_start, window_end, &fixed_orders);
        let turnaround_pairs = self.build_turnaround_pairs(&snapshot_orders, &fixed_orders);
        // A slot with an empty candidate list is pinned to `gap` by the solver's
        // `AddExactlyOne({candidates…, gap})`, which reads as a clean OPTIMAL.
        // Counting them here makes that visible before the solve, not after.
        let slots_with_no_candidates = snapshot_orders
            .iter()
            .flat_map(|order| order.personnel_slots.iter())
            .filter(|slot| slot.candidate_user_ids.is_empty())
            .count();
        let largest_slot_candidate_pool = snapshot_orders
            .iter()
            .flat_map(|order| order.personnel_slots.iter())
            .map(|slot| slot.candidate_user_ids.len())
            .max()
            .unwrap_or(0);
        let snapshot = DispatchReplanSnapshotResponse {
            snapshot_id: ulid::Ulid::new().to_string(),
            model_version: Self::MODEL_VERSION.to_string(),
            solver_version: Self::SOLVER_VERSION.to_string(),
            generated_at: Utc::now(),
            window_start,
            window_end,
            strategy,
            max_suggestions: max_suggestions.clamp(1, 500),
            travel_time_mode: travel_time_mode.clone(),
            objective_config: DispatchReplanObjectiveConfig {
                staged_lexicographic: true,
                objective_priority: vec![
                    "minimize_slot_gap".to_string(),
                    "minimize_turnaround_break".to_string(),
                    "minimize_personnel_baseline_change".to_string(),
                    "minimize_travel_cost".to_string(),
                    "minimize_scarcity_cost".to_string(),
                    "minimize_employee_load_deviation".to_string(),
                ],
                objective_stage_keys: vec![
                    "slot_gap".to_string(),
                    "continuity_break".to_string(),
                    "personnel_baseline_change".to_string(),
                    "travel_cost".to_string(),
                    "scarcity_cost".to_string(),
                    "employee_load_deviation".to_string(),
                ],
                timeout_ms: 10000,
                travel_time_mode: travel_time_mode.clone(),
                average_workload_target,
            },
            unsupported_features: Vec::new(),
            constraints: HashMap::from([
                ("travel_time_mode".to_string(), json!(travel_time_mode)),
                ("timeout_ms".to_string(), json!(10000)),
                ("joint_solve".to_string(), json!(true)),
                ("assigned_conflict_first".to_string(), json!(true)),
                ("unassigned_can_be_late".to_string(), json!(true)),
                ("unassigned_cannot_preempt_locked_or_repaired".to_string(), json!(true)),
                (
                    "objective_layers".to_string(),
                    json!([
                        "minimize_assigned_conflict_residual",
                        "minimize_assigned_change_cost_and_time_shift",
                        "maximize_unassigned_assignment_count",
                        "minimize_unassigned_total_lateness",
                        "minimize_overall_change_cost",
                    ]),
                ),
                // Nothing truncates a candidate pool any more, so these numbers
                // are the real decision-space size. A slow run or a thin slot is
                // diagnosable from here instead of being guessed at.
                (
                    "candidate_pool".to_string(),
                    json!({
                        "mined_candidate_rows": mined_candidates.mined_candidate_count(),
                        "largest_slot_candidate_pool": largest_slot_candidate_pool,
                        "slots_with_no_candidates": slots_with_no_candidates,
                        "qualification_degraded_departments": mined_candidates.degraded_departments(),
                        "truncated": false,
                    }),
                ),
            ]),
            impact_summary: self.build_snapshot_impact_summary(&optimizable_orders, &fixed_orders, &travel_time_mode),
            changed_orders: Vec::new(),
            risk_level: self.build_snapshot_risk_level(&optimizable_orders),
            requires_manual_confirmation: optimizable_orders.iter().any(|item| {
                matches!(item.conflict_state.as_str(), "resource_conflict" | "gap")
                    || !item.baseline_assignment.qualification_gap.is_empty()
            }),
            optimizable_orders: optimizable_orders.clone(),
            fixed_anchor_orders: fixed_orders.clone(),
            orders: optimizable_orders,
            travel_edges: travel_edges.clone(),
            resource_travel_edges: travel_edges,
            fixed_orders,
            employee_anchor_states: employee_anchor_context.states,
            equipment_anchor_states: equipment_anchor_context.states,
            employee_free_windows,
            equipment_free_windows,
            employee_unavailable_blocks,
            equipment_unavailable_blocks,
            turnaround_pairs,
        };

        self.store_snapshot(snapshot.clone());
        Ok(snapshot)
    }

    fn detect_conflicts(&self, orders: &[DispatchOrder]) -> HashMap<String, Vec<String>> {
        let mut conflicts: HashMap<String, Vec<String>> = HashMap::new();
        for (index, left) in orders.iter().enumerate() {
            for right in orders.iter().skip(index + 1) {
                let Some(left_start) = effective_start_time(left) else {
                    continue;
                };
                let Some(left_end) = effective_end_time(left) else {
                    continue;
                };
                let Some(right_start) = effective_start_time(right) else {
                    continue;
                };
                let Some(right_end) = effective_end_time(right) else {
                    continue;
                };
                if left_start >= right_end || right_start >= left_end {
                    continue;
                }

                let shared = shared_resource_keys(left, right);
                if shared.is_empty() {
                    continue;
                }

                let reason = shared.join(", ");
                conflicts.entry(left.id.clone()).or_default().push(reason.clone());
                conflicts.entry(right.id.clone()).or_default().push(reason);
            }
        }
        conflicts
    }

    async fn load_available_equipments(
        &self,
        orders: &[DispatchOrder],
    ) -> Result<HashMap<Option<String>, Vec<Equipment>>, DomainError> {
        let Some(equipment_repo) = self.equipment_repo.as_ref() else {
            return Ok(HashMap::new());
        };

        let mut terminals: HashSet<Option<String>> = orders.iter().map(|item| item.terminal.clone()).collect();
        terminals.insert(None);
        let mut result = HashMap::new();
        for terminal in terminals {
            let mut equipments = equipment_repo
                .find_available_for_dispatch(None, terminal.as_deref())
                .await?;
            // Sorted, not truncated: equipment slot candidates are solver
            // decision variables, so cutting the pool here silently decided the
            // plan's quality by repository row order.
            equipments.sort_by(|left, right| left.id.cmp(&right.id));
            result.insert(terminal, equipments);
        }
        Ok(result)
    }
}
