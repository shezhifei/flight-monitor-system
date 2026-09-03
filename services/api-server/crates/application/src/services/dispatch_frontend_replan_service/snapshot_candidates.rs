use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::schemas::dispatch_schemas::{
    DispatchReplanAnchorFreeWindow, DispatchReplanAssignment, DispatchReplanCandidateEquipment,
    DispatchReplanCandidateUser, DispatchReplanSnapshotOrder, TaskCrewMemberResponse, TaskCrewResponse,
};
use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{DispatchOrder, Equipment};

use super::super::super::helpers::*;
use super::super::DispatchFrontendReplanService;

impl DispatchFrontendReplanService {
    /// The people this order already carries, from its four order-attached
    /// sources.
    ///
    /// Uncapped on purpose. The previous `MAX_CANDIDATE_USERS` early returns
    /// meant which candidates survived depended on the order these four sources
    /// happened to be visited in, and the cut was invisible downstream. Slot
    /// candidates now come from the qualification store anyway
    /// (`build_personnel_slots`), so a cap here would only hide part of the
    /// baseline from the dispatcher.
    pub(super) fn build_candidate_users(
        &self,
        order: &DispatchOrder,
        current_assignment: &DispatchReplanAssignment,
    ) -> Vec<DispatchReplanCandidateUser> {
        let mut result = Vec::new();
        let mut seen = HashSet::new();

        for value in &order.recommended_assignees {
            let Some(user_id) = value.get("user_id").and_then(Value::as_str).map(str::trim) else {
                continue;
            };
            if user_id.is_empty() || !seen.insert(user_id.to_string()) {
                continue;
            }
            result.push(DispatchReplanCandidateUser {
                user_id: user_id.to_string(),
                username: value
                    .get("username")
                    .and_then(Value::as_str)
                    .unwrap_or(user_id)
                    .to_string(),
                score: value.get("score").and_then(Value::as_f64).unwrap_or(0.0),
                source_team_id: json_string_field(value.get("source_team_id")),
                source_team_name: json_string_field(value.get("source_team_name")),
                qualification_code: json_string_field(value.get("qualification_code")),
                qualification_level_code: json_string_field(value.get("qualification_level_code")),
            });
        }

        for member in &current_assignment.task_crew.members {
            let user_id = member.user_id.trim();
            if user_id.is_empty() || !seen.insert(user_id.to_string()) {
                continue;
            }
            result.push(DispatchReplanCandidateUser {
                user_id: user_id.to_string(),
                username: member.username.clone().unwrap_or_else(|| user_id.to_string()),
                score: 95.0,
                source_team_id: member.source_team_id.clone(),
                source_team_name: member.source_team_name.clone(),
                qualification_code: member.qualification_code.clone(),
                qualification_level_code: member.qualification_level_code.clone(),
            });
        }

        if let Some(user_id) = current_assignment.individual_user_id.as_deref() {
            let user_id = user_id.trim();
            if !user_id.is_empty() && seen.insert(user_id.to_string()) {
                result.push(DispatchReplanCandidateUser {
                    user_id: user_id.to_string(),
                    username: user_id.to_string(),
                    score: 100.0,
                    source_team_id: None,
                    source_team_name: None,
                    qualification_code: None,
                    qualification_level_code: None,
                });
            }
        }

        for member in &order.members {
            let user_id = member.user_id.trim();
            if !member.is_active || user_id.is_empty() || !seen.insert(user_id.to_string()) {
                continue;
            }
            result.push(DispatchReplanCandidateUser {
                user_id: user_id.to_string(),
                username: member.username.clone().unwrap_or_else(|| user_id.to_string()),
                score: 60.0,
                source_team_id: member.source_team_id.clone(),
                source_team_name: None,
                qualification_code: member.qualification_code.clone(),
                qualification_level_code: member.qualification_level_code.clone(),
            });
        }

        // Highest-scoring first, `user_id` breaking ties, so an uncapped pool is
        // still byte-identical run to run and stays comparable against goldens.
        result.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.user_id.cmp(&right.user_id))
        });
        result
    }

    pub(super) fn build_candidate_equipments(
        &self,
        order: &DispatchOrder,
        available_equipments: &HashMap<Option<String>, Vec<Equipment>>,
    ) -> Vec<DispatchReplanCandidateEquipment> {
        let mut equipments = available_equipments
            .get(&order.terminal.clone())
            .cloned()
            .unwrap_or_default();
        if equipments.is_empty() {
            equipments = available_equipments.get(&None).cloned().unwrap_or_default();
        }
        // `find_available_for_dispatch` intentionally excludes equipment that is
        // already in use. The order's current equipment is still a legal
        // baseline option for that same order, so merge the fully hydrated
        // records before applying slot type filters. Keeping the records (rather
        // than appending bare ids later) preserves the type id/code needed for
        // validation.
        equipments.extend(order.equipment_list.iter().cloned());
        equipments.sort_by(|left, right| left.id.cmp(&right.id));
        equipments.dedup_by(|left, right| left.id == right.id);
        equipments
            .into_iter()
            .map(|equipment| DispatchReplanCandidateEquipment {
                equipment_id: equipment.id,
                code: equipment.code,
                equipment_type_id: equipment.equipment_type_id,
                equipment_type_code: equipment.equipment_type.and_then(|equipment_type| equipment_type.code),
            })
            .collect()
    }

    pub(super) async fn build_candidate_assignments(
        &self,
        order: &DispatchReplanSnapshotOrder,
        current_assignment: &DispatchReplanAssignment,
        candidate_users: &[DispatchReplanCandidateUser],
        candidate_equipments: &[DispatchReplanCandidateEquipment],
        user_segments: &HashMap<String, Vec<DispatchReplanAnchorFreeWindow>>,
        equipment_segments: &HashMap<String, Vec<DispatchReplanAnchorFreeWindow>>,
    ) -> Result<Vec<DispatchReplanAssignment>, DomainError> {
        let mut assignments = Vec::new();
        let mut seen = HashSet::new();
        if has_primary_assignment(current_assignment) {
            let current = current_assignment.clone();
            if self
                .assignment_fits_anchor_windows(order, &current, user_segments, equipment_segments)
                .await
            {
                push_candidate_assignment(&mut assignments, &mut seen, current);
            }
        }

        let equipment_options = self.build_equipment_options(current_assignment, candidate_equipments);
        // Bounded, and this bound is NOT a candidate cap: every user reachable
        // here is already in `candidate_users`, and slot candidates come from
        // the qualification store, so nothing the solver decides over is lost.
        // What this bounds is the (user × equipment) convenience enumeration,
        // whose cost is one async anchor-window check per pair. The count that
        // was actually enumerated is reported on the snapshot.
        for user in candidate_users.iter().take(Self::MAX_ENUMERATED_ASSIGNMENT_USERS) {
            for equipment_ids in &equipment_options {
                let assignment = DispatchReplanAssignment {
                    individual_user_id: Some(user.user_id.clone()),
                    equipment_ids: equipment_ids.clone(),
                    member_user_ids: vec![user.user_id.clone()],
                    department_rule_version: current_assignment.department_rule_version.clone(),
                    crew_requirement_snapshot: current_assignment.crew_requirement_snapshot.clone(),
                    equipment_requirement_snapshot: order.equipment_requirement_snapshot.clone(),
                    qualification_gap: Vec::new(),
                    task_crew: TaskCrewResponse {
                        members: vec![TaskCrewMemberResponse {
                            user_id: user.user_id.clone(),
                            username: Some(user.username.clone()),
                            source_team_id: user.source_team_id.clone(),
                            source_team_name: user.source_team_name.clone(),
                            slot_code: Some("lead".to_string()),
                            qualification_code: user.qualification_code.clone(),
                            qualification_level_code: user.qualification_level_code.clone(),
                        }],
                        source_team_ids: user.source_team_id.clone().into_iter().collect(),
                        source_team_names: user.source_team_name.clone().into_iter().collect(),
                        generated_from: "frontend_snapshot_personal".to_string(),
                    },
                };
                if self
                    .assignment_fits_anchor_windows(order, &assignment, user_segments, equipment_segments)
                    .await
                {
                    push_candidate_assignment(&mut assignments, &mut seen, assignment);
                }
            }
        }
        Ok(assignments)
    }

    fn build_equipment_options(
        &self,
        current_assignment: &DispatchReplanAssignment,
        candidate_equipments: &[DispatchReplanCandidateEquipment],
    ) -> Vec<Vec<String>> {
        let mut options = Vec::new();
        let mut seen = HashSet::new();
        let push = |values: Vec<String>, options: &mut Vec<Vec<String>>, seen: &mut HashSet<String>| {
            let normalized = dedupe_strings(&values);
            let key = normalized.join(",");
            if seen.insert(key) {
                options.push(normalized);
            }
        };

        if !current_assignment.equipment_ids.is_empty() {
            push(current_assignment.equipment_ids.clone(), &mut options, &mut seen);
        }
        for equipment in candidate_equipments {
            push(vec![equipment.equipment_id.clone()], &mut options, &mut seen);
        }
        if options.is_empty() {
            options.push(Vec::new());
        }
        options
    }
}
