use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};

use crate::schemas::dispatch_schemas::{DispatchReplanOrderResult, DispatchReplanSuggestion};

use super::super::helpers::*;
use super::DispatchFrontendReplanService;

impl DispatchFrontendReplanService {
    pub(crate) fn order_result_to_suggestion(order_result: &DispatchReplanOrderResult) -> DispatchReplanSuggestion {
        DispatchReplanSuggestion {
            dispatch_order_id: order_result.dispatch_order_id.clone(),
            order_id: order_result
                .order_id
                .clone()
                .or_else(|| Some(order_result.dispatch_order_id.clone())),
            order_ids: if order_result.order_ids.is_empty() {
                vec![order_result.dispatch_order_id.clone()]
            } else {
                order_result.order_ids.clone()
            },
            flight_id: order_result.flight_id.clone(),
            reason: order_result.reason.clone(),
            suggestion_type: order_result.suggestion_type.clone(),
            risk_level: order_result.risk_level.clone(),
            safety_gate_state: order_result.safety_gate_state.clone(),
            order_class: order_result.order_class.clone(),
            original_start_time: order_result.original_start_time,
            original_end_time: order_result.original_end_time,
            suggested_start_time: order_result.suggested_start_time,
            suggested_end_time: order_result.suggested_end_time,
            related_dispatch_order_id: None,
            current_assignment: order_result.current_assignment.clone(),
            suggested_assignment: order_result.suggested_assignment.clone(),
            task_crew: order_result.task_crew.clone(),
            crew_requirement_snapshot: order_result.crew_requirement_snapshot.clone(),
            qualification_gap: order_result.qualification_gap.clone(),
            department_rule_version: order_result
                .suggested_assignment
                .as_ref()
                .and_then(|item| item.department_rule_version.clone())
                .or_else(|| {
                    order_result
                        .current_assignment
                        .as_ref()
                        .and_then(|item| item.department_rule_version.clone())
                }),
            member_change_summary: order_result.member_change_summary.clone(),
            requires_manual_confirmation: order_result.requires_manual_confirmation,
            lateness_minutes: order_result.lateness_minutes,
            travel_minutes: order_result.travel_minutes,
            impact_score: order_result.impact_score,
        }
    }

    pub fn suggestion_to_order_result(suggestion: DispatchReplanSuggestion) -> DispatchReplanOrderResult {
        let mut start_times = HashMap::new();
        if let Some(value) = suggestion.original_start_time {
            start_times.insert("original_start_time".to_string(), json!(value));
        }
        if let Some(value) = suggestion.suggested_start_time {
            start_times.insert("suggested_start_time".to_string(), json!(value));
        }

        let mut lateness = HashMap::new();
        lateness.insert("minutes".to_string(), json!(suggestion.lateness_minutes));

        let mut gap_summary = HashMap::new();
        gap_summary.insert(
            "qualification_gap_count".to_string(),
            json!(suggestion.qualification_gap.len()),
        );

        let mut change_summary = HashMap::new();
        let baseline_change_count = suggestion
            .member_change_summary
            .get("changed_count")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        change_summary.insert("baseline_change_count".to_string(), json!(baseline_change_count));

        let mut travel_summary = HashMap::new();
        travel_summary.insert("travel_minutes".to_string(), json!(suggestion.travel_minutes));
        let order_id = suggestion
            .order_id
            .clone()
            .or_else(|| Some(suggestion.dispatch_order_id.clone()));
        let order_ids = if suggestion.order_ids.is_empty() {
            vec![suggestion.dispatch_order_id.clone()]
        } else {
            suggestion.order_ids.clone()
        };
        let risk_level = suggestion
            .risk_level
            .clone()
            .or_else(|| Some(suggestion_risk_level(&suggestion).to_string()));
        let safety_gate_state = suggestion.safety_gate_state.clone().or_else(|| {
            Some(if suggestion.requires_manual_confirmation {
                "manual_review_required".to_string()
            } else {
                "pass".to_string()
            })
        });

        DispatchReplanOrderResult {
            dispatch_order_id: suggestion.dispatch_order_id.clone(),
            order_id,
            order_ids,
            flight_id: suggestion.flight_id.clone(),
            reason: suggestion.reason,
            suggestion_type: suggestion.suggestion_type,
            risk_level,
            safety_gate_state,
            order_class: suggestion.order_class,
            original_start_time: suggestion.original_start_time,
            original_end_time: suggestion.original_end_time,
            suggested_start_time: suggestion.suggested_start_time,
            suggested_end_time: suggestion.suggested_end_time,
            lateness_minutes: suggestion.lateness_minutes,
            gap_count: suggestion.qualification_gap.len() as i64,
            travel_minutes: suggestion.travel_minutes,
            baseline_change_count,
            impact_score: suggestion.impact_score,
            current_assignment: suggestion.current_assignment,
            suggested_assignment: suggestion.suggested_assignment,
            task_crew: suggestion.task_crew,
            crew_requirement_snapshot: suggestion.crew_requirement_snapshot,
            qualification_gap: suggestion.qualification_gap,
            member_change_summary: suggestion.member_change_summary,
            requires_manual_confirmation: suggestion.requires_manual_confirmation,
            start_times,
            lateness,
            gap_summary,
            continuity_summary: HashMap::new(),
            change_summary,
            travel_summary,
            personnel_slot_assignments: Vec::new(),
            equipment_slot_assignments: Vec::new(),
            continuity_decisions: Vec::new(),
            objective_breakdown: HashMap::new(),
        }
    }

    pub(crate) fn merge_order_results(
        source_order_results: &[DispatchReplanOrderResult],
        applied_suggestions: &[DispatchReplanSuggestion],
    ) -> Vec<DispatchReplanOrderResult> {
        let suggestion_by_order_id = applied_suggestions
            .iter()
            .cloned()
            .map(|item| (item.dispatch_order_id.clone(), item))
            .collect::<HashMap<_, _>>();

        let mut merged = Vec::with_capacity(applied_suggestions.len().max(source_order_results.len()));
        for order_result in source_order_results {
            if let Some(suggestion) = suggestion_by_order_id.get(&order_result.dispatch_order_id) {
                merged.push(Self::merge_order_result_with_suggestion(order_result, suggestion));
            }
        }

        let merged_order_ids = merged
            .iter()
            .map(|item| item.dispatch_order_id.clone())
            .collect::<HashSet<_>>();
        for suggestion in applied_suggestions {
            if !merged_order_ids.contains(&suggestion.dispatch_order_id) {
                merged.push(Self::suggestion_to_order_result(suggestion.clone()));
            }
        }

        merged
    }

    fn merge_order_result_with_suggestion(
        source: &DispatchReplanOrderResult,
        suggestion: &DispatchReplanSuggestion,
    ) -> DispatchReplanOrderResult {
        let synthesized = Self::suggestion_to_order_result(suggestion.clone());
        let baseline_change_count = if source.baseline_change_count > 0 {
            source.baseline_change_count
        } else {
            synthesized.baseline_change_count
        };
        let gap_count = if source.gap_count > 0 {
            source.gap_count
        } else {
            synthesized.gap_count
        };

        DispatchReplanOrderResult {
            dispatch_order_id: suggestion.dispatch_order_id.clone(),
            order_id: suggestion
                .order_id
                .clone()
                .or_else(|| source.order_id.clone())
                .or_else(|| Some(suggestion.dispatch_order_id.clone())),
            order_ids: if !suggestion.order_ids.is_empty() {
                suggestion.order_ids.clone()
            } else if !source.order_ids.is_empty() {
                source.order_ids.clone()
            } else {
                vec![suggestion.dispatch_order_id.clone()]
            },
            flight_id: suggestion.flight_id.clone().or_else(|| source.flight_id.clone()),
            reason: suggestion.reason.clone(),
            suggestion_type: suggestion.suggestion_type.clone(),
            risk_level: suggestion
                .risk_level
                .clone()
                .or_else(|| source.risk_level.clone())
                .or_else(|| Some(suggestion_risk_level(suggestion).to_string())),
            safety_gate_state: suggestion
                .safety_gate_state
                .clone()
                .or_else(|| source.safety_gate_state.clone())
                .or_else(|| {
                    Some(if suggestion.requires_manual_confirmation {
                        "manual_review_required".to_string()
                    } else {
                        "pass".to_string()
                    })
                }),
            order_class: suggestion.order_class.clone(),
            original_start_time: suggestion.original_start_time.or(source.original_start_time),
            original_end_time: suggestion.original_end_time.or(source.original_end_time),
            suggested_start_time: suggestion.suggested_start_time.or(source.suggested_start_time),
            suggested_end_time: suggestion.suggested_end_time.or(source.suggested_end_time),
            lateness_minutes: suggestion.lateness_minutes,
            gap_count,
            travel_minutes: suggestion.travel_minutes,
            baseline_change_count,
            impact_score: suggestion.impact_score,
            current_assignment: suggestion
                .current_assignment
                .clone()
                .or_else(|| source.current_assignment.clone()),
            suggested_assignment: suggestion
                .suggested_assignment
                .clone()
                .or_else(|| source.suggested_assignment.clone()),
            task_crew: suggestion.task_crew.clone().or_else(|| source.task_crew.clone()),
            crew_requirement_snapshot: if source.crew_requirement_snapshot.is_empty() {
                synthesized.crew_requirement_snapshot
            } else {
                source.crew_requirement_snapshot.clone()
            },
            qualification_gap: if source.qualification_gap.is_empty() {
                suggestion.qualification_gap.clone()
            } else {
                source.qualification_gap.clone()
            },
            member_change_summary: if source.member_change_summary.is_null() {
                suggestion.member_change_summary.clone()
            } else {
                source.member_change_summary.clone()
            },
            requires_manual_confirmation: suggestion.requires_manual_confirmation,
            start_times: if source.start_times.is_empty() {
                synthesized.start_times
            } else {
                source.start_times.clone()
            },
            lateness: if source.lateness.is_empty() {
                synthesized.lateness
            } else {
                source.lateness.clone()
            },
            gap_summary: if source.gap_summary.is_empty() {
                synthesized.gap_summary
            } else {
                source.gap_summary.clone()
            },
            continuity_summary: source.continuity_summary.clone(),
            change_summary: if source.change_summary.is_empty() {
                synthesized.change_summary
            } else {
                source.change_summary.clone()
            },
            travel_summary: if source.travel_summary.is_empty() {
                synthesized.travel_summary
            } else {
                source.travel_summary.clone()
            },
            personnel_slot_assignments: if source.personnel_slot_assignments.is_empty() {
                synthesized.personnel_slot_assignments
            } else {
                source.personnel_slot_assignments.clone()
            },
            equipment_slot_assignments: if source.equipment_slot_assignments.is_empty() {
                synthesized.equipment_slot_assignments
            } else {
                source.equipment_slot_assignments.clone()
            },
            continuity_decisions: if source.continuity_decisions.is_empty() {
                synthesized.continuity_decisions
            } else {
                source.continuity_decisions.clone()
            },
            objective_breakdown: if source.objective_breakdown.is_empty() {
                synthesized.objective_breakdown
            } else {
                source.objective_breakdown.clone()
            },
        }
    }
}
