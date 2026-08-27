use std::collections::{HashMap, HashSet};

use serde_json::json;

use crate::schemas::dispatch_schemas::{
    DispatchReplanAssignment, DispatchReplanBaselineAssignment, DispatchReplanBaselineEquipmentSlotAssignment,
    DispatchReplanBaselinePersonnelSlotAssignment, DispatchReplanEquipmentSlot, DispatchReplanPersonnelSlot,
    DispatchReplanSnapshotOrder, TaskCrewMemberResponse,
};
use fms_domain::models::dispatch::DispatchOrder;

use super::super::super::helpers::*;
use super::super::{DispatchFrontendReplanService, ResolvedCrewSlot};
use super::snapshot_mining::{mined_by_user_id, mined_source_team_ids, SlotCandidateIndex};

impl DispatchFrontendReplanService {
    pub(super) fn build_baseline_assignment(
        &self,
        order: &DispatchReplanSnapshotOrder,
        current_assignment: &DispatchReplanAssignment,
        personnel_slots: &[DispatchReplanPersonnelSlot],
        equipment_slots: &[DispatchReplanEquipmentSlot],
        source_order: &DispatchOrder,
    ) -> DispatchReplanBaselineAssignment {
        let members_by_slot = current_assignment
            .task_crew
            .members
            .iter()
            .filter_map(|member| {
                member
                    .slot_code
                    .clone()
                    .or_else(|| Some(member.user_id.clone()))
                    .map(|key| (key, member.clone()))
            })
            .collect::<HashMap<_, _>>();
        let personnel_slot_assignments = personnel_slots
            .iter()
            .map(|slot| {
                let baseline_member = members_by_slot
                    .get(&slot.slot_code)
                    .cloned()
                    .or_else(|| {
                        slot.baseline_user_id.as_deref().and_then(|baseline_user_id| {
                            current_assignment
                                .task_crew
                                .members
                                .iter()
                                .find(|member| member.user_id == baseline_user_id)
                                .cloned()
                        })
                    })
                    .or_else(|| {
                        if slot.slot_code == "primary" {
                            Some(TaskCrewMemberResponse {
                                user_id: current_assignment
                                    .individual_user_id
                                    .as_deref()
                                    .unwrap_or_default()
                                    .to_string(),
                                source_team_id: None,
                                ..TaskCrewMemberResponse::default()
                            })
                        } else {
                            None
                        }
                    });
                DispatchReplanBaselinePersonnelSlotAssignment {
                    slot_code: slot.slot_code.clone(),
                    user_id: baseline_member
                        .as_ref()
                        .map(|member| member.user_id.clone())
                        .filter(|value| !value.is_empty()),
                    username: baseline_member.as_ref().and_then(|member| member.username.clone()),
                    source_team_id: baseline_member
                        .as_ref()
                        .and_then(|member| member.source_team_id.clone()),
                    source_team_name: baseline_member
                        .as_ref()
                        .and_then(|member| member.source_team_name.clone()),
                    qualification_code: slot.qualification_code.clone(),
                    qualification_level_code: slot.qualification_level_code.clone(),
                }
            })
            .collect::<Vec<_>>();
        let equipment_code_by_id = source_order
            .equipment_list
            .iter()
            .map(|equipment| (equipment.id.clone(), equipment.code.clone()))
            .collect::<HashMap<_, _>>();
        let equipment_slot_assignments = equipment_slots
            .iter()
            .enumerate()
            .map(|(index, slot)| {
                let equipment_id = current_assignment.equipment_ids.get(index).cloned();
                DispatchReplanBaselineEquipmentSlotAssignment {
                    slot_code: slot.slot_code.clone(),
                    code: equipment_id
                        .as_ref()
                        .and_then(|id| equipment_code_by_id.get(id).cloned())
                        .or_else(|| equipment_id.clone()),
                    equipment_id,
                    equipment_type_id: slot.equipment_type_id.clone(),
                }
            })
            .collect::<Vec<_>>();

        DispatchReplanBaselineAssignment {
            individual_user_id: current_assignment.individual_user_id.clone(),
            equipment_ids: current_assignment.equipment_ids.clone(),
            member_user_ids: current_assignment.member_user_ids.clone(),
            department_rule_version: current_assignment.department_rule_version.clone(),
            crew_requirement_snapshot: order.crew_requirement_snapshot.clone(),
            equipment_requirement_snapshot: order.equipment_requirement_snapshot.clone(),
            qualification_gap: order.qualification_gap.clone(),
            task_crew: serde_json::to_value(&current_assignment.task_crew).unwrap_or_else(|_| json!({})),
            personnel_slot_assignments,
            equipment_slot_assignments,
        }
    }

    /// Builds one solver slot per crew requirement.
    ///
    /// Candidates are resolved **per slot**, not per order. A slot that states a
    /// `qualification_code` takes the people the qualification store says hold
    /// it; anyone attached to the order who does not hold it is excluded and
    /// counted in `qualification_excluded_user_ids` rather than dropped quietly.
    /// A slot with no stated qualification — the `primary` fallback — keeps the
    /// order-attached pool, since there is nothing to filter against.
    ///
    /// Nothing here truncates: `candidate_user_ids` is exactly what the solver
    /// decides over, so a cap would silently decide the answer. Order is
    /// deterministic (qualified-first, then user id) so repeated runs and golden
    /// fixtures compare cleanly.
    pub(super) fn build_personnel_slots(
        &self,
        order: &DispatchReplanSnapshotOrder,
        current_assignment: &DispatchReplanAssignment,
        mined: &SlotCandidateIndex,
    ) -> Vec<DispatchReplanPersonnelSlot> {
        let order_candidate_user_ids = order
            .candidate_users
            .iter()
            .map(|item| item.user_id.clone())
            .collect::<Vec<_>>();
        self.resolve_crew_slots(order, current_assignment)
            .into_iter()
            .map(|slot| {
                let slot_code = slot.slot_code.clone();
                let baseline_user_id = slot
                    .baseline_slot_code
                    .as_deref()
                    .and_then(|code| resolve_baseline_user_for_slot(current_assignment, code));

                // People this order already carries, in the order the four
                // order-attached sources produced them.
                let mut attached_ids = Vec::new();
                if let Some(user_id) = baseline_user_id.clone() {
                    attached_ids.push(user_id);
                }
                attached_ids.extend(order_candidate_user_ids.clone());
                attached_ids.extend(
                    order
                        .candidate_assignments
                        .iter()
                        .filter_map(|assignment| resolve_candidate_user_for_slot(assignment, &slot.slot_code)),
                );
                let attached_ids = dedupe_strings(&attached_ids);

                let mining_key = self.slot_mining_key(
                    order,
                    slot.qualification_code.as_deref(),
                    slot.qualification_level_code.as_deref(),
                );
                let mined_for_slot = mining_key.as_ref().and_then(|key| mined.lookup(key));

                let (candidate_user_ids, qualification_excluded_user_ids, source_team_ids) = match mined_for_slot {
                    Some(mined_for_slot) => {
                        let qualified = mined_by_user_id(mined_for_slot);
                        // Qualified people the order already knows about lead,
                        // so a run that changes nothing keeps its baseline.
                        let mut ordered: Vec<String> = attached_ids
                            .iter()
                            .filter(|id| qualified.contains_key(id.as_str()))
                            .cloned()
                            .collect();
                        let mut rest: Vec<String> = mined_for_slot
                            .iter()
                            .map(|candidate| candidate.user_id.clone())
                            .filter(|id| !ordered.contains(id))
                            .collect();
                        rest.sort();
                        ordered.extend(rest);
                        let excluded = attached_ids
                            .iter()
                            .filter(|id| !qualified.contains_key(id.as_str()))
                            .cloned()
                            .collect::<Vec<_>>();
                        (ordered, excluded, mined_source_team_ids(mined_for_slot))
                    }
                    // A stated qualification without a mined result is not
                    // legally verifiable. Keep the snapshot available for
                    // diagnostics, but leave the decision pool empty so the
                    // solver reports a gap instead of calling the plan complete.
                    None if mining_key.is_some() => (Vec::new(), attached_ids, Vec::new()),
                    // A slot with no qualification requirement can still use
                    // the order-attached pool.
                    None => (attached_ids, Vec::new(), Vec::new()),
                };

                let scarcity_cost = self.scarcity_cost_for_slot(order, &slot.base_slot_code);
                DispatchReplanPersonnelSlot {
                    slot_code,
                    qualification_code: slot.qualification_code,
                    qualification_level_code: slot.qualification_level_code,
                    qualification_feasible_candidate_user_ids: candidate_user_ids.clone(),
                    candidate_user_ids,
                    qualification_excluded_user_ids,
                    candidate_source_team_ids: source_team_ids,
                    schedule_feasible_candidate_user_ids: Vec::new(),
                    baseline_user_id,
                    workload_weight: slot.workload_weight,
                    scarcity_cost,
                }
            })
            .collect()
    }

    pub(super) fn build_equipment_slots(
        &self,
        order: &DispatchReplanSnapshotOrder,
        current_assignment: &DispatchReplanAssignment,
    ) -> Vec<DispatchReplanEquipmentSlot> {
        if !order.equipment_requirement_snapshot.is_empty() {
            return order
                .equipment_requirement_snapshot
                .iter()
                .enumerate()
                .flat_map(|(requirement_index, requirement)| {
                    let Some(obj) = requirement.as_object() else {
                        return Vec::new();
                    };
                    let base_slot_code = json_string_field(obj.get("slot_code"))
                        .unwrap_or_else(|| format!("equipment-{}", requirement_index + 1));
                    let equipment_type_id = json_string_field(obj.get("equipment_type_id"));
                    let equipment_type_code = json_string_field(obj.get("equipment_type_code"));
                    let required_count = requirement_count(obj);
                    let candidate_equipment_ids = order
                        .candidate_equipments
                        .iter()
                        .filter(|item| {
                            equipment_type_id
                                .as_deref()
                                .is_none_or(|required| item.equipment_type_id.as_deref() == Some(required))
                                && equipment_type_code
                                    .as_deref()
                                    .is_none_or(|required| item.equipment_type_code.as_deref() == Some(required))
                        })
                        .map(|item| item.equipment_id.clone())
                        .collect::<Vec<_>>();

                    (1..=required_count)
                        .map(|ordinal| {
                            let slot_code = expanded_slot_code(&base_slot_code, ordinal, required_count);
                            let baseline_index =
                                expanded_requirement_offset(&order.equipment_requirement_snapshot, requirement_index)
                                    + ordinal
                                    - 1;
                            DispatchReplanEquipmentSlot {
                                slot_code,
                                equipment_type_id: equipment_type_id.clone(),
                                candidate_equipment_ids: dedupe_strings(&candidate_equipment_ids),
                                schedule_feasible_candidate_equipment_ids: Vec::new(),
                                baseline_equipment_id: current_assignment.equipment_ids.get(baseline_index).cloned(),
                            }
                        })
                        .collect::<Vec<_>>()
                })
                .collect();
        }

        let mut candidate_equipment_ids = current_assignment.equipment_ids.clone();
        candidate_equipment_ids.extend(order.candidate_equipments.iter().map(|item| item.equipment_id.clone()));
        if candidate_equipment_ids.is_empty() {
            Vec::new()
        } else {
            vec![DispatchReplanEquipmentSlot {
                slot_code: "equipment-1".to_string(),
                equipment_type_id: None,
                candidate_equipment_ids: dedupe_strings(&candidate_equipment_ids),
                schedule_feasible_candidate_equipment_ids: Vec::new(),
                baseline_equipment_id: current_assignment.equipment_ids.first().cloned(),
            }]
        }
    }

    fn resolve_crew_slots(
        &self,
        order: &DispatchReplanSnapshotOrder,
        current_assignment: &DispatchReplanAssignment,
    ) -> Vec<ResolvedCrewSlot> {
        let requirement_slots = order
            .crew_requirement_snapshot
            .iter()
            .enumerate()
            .flat_map(|(index, item)| {
                let Some(obj) = item.as_object() else {
                    return Vec::new();
                };
                let base_slot_code =
                    json_string_field(obj.get("slot_code")).unwrap_or_else(|| format!("slot-{}", index + 1));
                let required_count = requirement_count(obj);
                let qualification_code = json_string_field(obj.get("qualification_code"));
                let qualification_level_code = json_string_field(obj.get("min_level_code"))
                    .or_else(|| json_string_field(obj.get("qualification_level_code")));
                let workload_weight = json_f64_field(obj.get("workload_weight")).unwrap_or(1.0);
                (1..=required_count)
                    .map(|ordinal| ResolvedCrewSlot {
                        slot_code: expanded_slot_code(&base_slot_code, ordinal, required_count),
                        base_slot_code: base_slot_code.clone(),
                        baseline_slot_code: if ordinal == 1 {
                            Some(base_slot_code.clone())
                        } else {
                            None
                        },
                        qualification_code: qualification_code.clone(),
                        qualification_level_code: qualification_level_code.clone(),
                        workload_weight,
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        if !requirement_slots.is_empty() {
            return requirement_slots;
        }

        let derived_slots = current_assignment
            .task_crew
            .members
            .iter()
            .enumerate()
            .map(|(index, item)| ResolvedCrewSlot {
                slot_code: item.slot_code.clone().unwrap_or_else(|| format!("slot-{}", index + 1)),
                base_slot_code: item.slot_code.clone().unwrap_or_else(|| format!("slot-{}", index + 1)),
                baseline_slot_code: Some(item.slot_code.clone().unwrap_or_else(|| format!("slot-{}", index + 1))),
                qualification_code: item.qualification_code.clone(),
                qualification_level_code: item.qualification_level_code.clone(),
                workload_weight: 1.0,
            })
            .collect::<Vec<_>>();
        if !derived_slots.is_empty() {
            return derived_slots;
        }

        let mut candidate_assignment_slots = Vec::new();
        let mut seen_slot_codes = HashSet::new();
        for assignment in &order.candidate_assignments {
            for (index, item) in assignment.task_crew.members.iter().enumerate() {
                let slot_code = item.slot_code.clone().unwrap_or_else(|| format!("slot-{}", index + 1));
                if !seen_slot_codes.insert(slot_code.clone()) {
                    continue;
                }
                candidate_assignment_slots.push(ResolvedCrewSlot {
                    base_slot_code: slot_code.clone(),
                    baseline_slot_code: Some(slot_code.clone()),
                    slot_code,
                    qualification_code: item.qualification_code.clone(),
                    qualification_level_code: item.qualification_level_code.clone(),
                    workload_weight: 1.0,
                });
            }
        }
        if !candidate_assignment_slots.is_empty() {
            return candidate_assignment_slots;
        }
        if current_assignment.individual_user_id.is_some() || !order.candidate_users.is_empty() {
            return vec![ResolvedCrewSlot {
                slot_code: "primary".to_string(),
                base_slot_code: "primary".to_string(),
                baseline_slot_code: Some("primary".to_string()),
                qualification_code: None,
                qualification_level_code: None,
                workload_weight: 1.0,
            }];
        }
        Vec::new()
    }

    fn scarcity_cost_for_slot(&self, order: &DispatchReplanSnapshotOrder, slot_code: &str) -> f64 {
        order
            .crew_requirement_snapshot
            .iter()
            .find_map(|item| {
                item.as_object().and_then(|obj| {
                    let candidate_slot_code = json_string_field(obj.get("slot_code"))?;
                    if candidate_slot_code == slot_code {
                        json_f64_field(obj.get("scarcity_cost")).or_else(|| json_f64_field(obj.get("scarcity_weight")))
                    } else {
                        None
                    }
                })
            })
            .unwrap_or(0.0)
    }
}

fn requirement_count(obj: &serde_json::Map<String, serde_json::Value>) -> usize {
    obj.get("required_count")
        .and_then(serde_json::Value::as_i64)
        .and_then(|value| usize::try_from(value.max(1)).ok())
        .unwrap_or(1)
}

fn expanded_slot_code(base: &str, ordinal: usize, required_count: usize) -> String {
    if required_count == 1 {
        base.to_string()
    } else {
        format!("{base}#{ordinal}")
    }
}

fn expanded_requirement_offset(requirements: &[serde_json::Value], before_index: usize) -> usize {
    requirements
        .iter()
        .take(before_index)
        .filter_map(serde_json::Value::as_object)
        .map(requirement_count)
        .sum()
}
