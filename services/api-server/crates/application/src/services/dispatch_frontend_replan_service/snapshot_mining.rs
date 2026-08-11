//! Per-slot candidate discovery.
//!
//! A slot's candidates used to be whoever was already attached to the order.
//! Draft orders produced by `generate_draft_orders` carry nobody, so every slot
//! reached `AddExactlyOne({gap})` and the solver returned `feasible=true` with
//! an empty plan. This module asks the qualification store instead: who in the
//! owning department actually holds the qualification this slot requires.
//!
//! Mining is keyed and cached per `(department, qualification, min level, time)`
//! so a snapshot spanning many orders of the same task type pays for one pair of
//! queries, not one per order.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{DateTime, Utc};

use crate::schemas::dispatch_schemas::{
    DispatchReplanCandidateTeam, DispatchReplanPersonnelSlot, DispatchReplanSnapshotOrder,
};
use fms_domain::models::dispatch::DispatchOrder;

use super::super::super::helpers::*;
use super::super::DispatchFrontendReplanService;

/// One qualified person, as the qualification store sees them.
///
/// The grant's level is deliberately not carried here: the miner has already
/// applied `min_level_code` through the level-coverage index, so every row that
/// reaches this struct satisfies the slot. Keeping the level would invite a
/// second, weaker check downstream.
#[derive(Clone, Debug)]
pub(crate) struct MinedCandidate {
    pub(crate) user_id: String,
    pub(crate) source_team_id: Option<String>,
}

/// What a slot needs, normalized into a cache key. `at_time` is part of the key
/// because grant validity is time-bounded: mining once for the whole window
/// would hand the solver people whose grant expires mid-window.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) struct SlotMiningKey {
    pub(crate) department_id: String,
    pub(crate) qualification_code: String,
    pub(crate) min_level_code: Option<String>,
    pub(crate) at_time: DateTime<Utc>,
}

/// Mined candidates for every distinct slot requirement in a snapshot.
///
/// `degraded_departments` records departments whose qualification lookup failed.
/// Those slots fall back to order-attached candidates rather than failing the
/// snapshot, and the reason is reported so a dispatcher sees why a slot looks
/// thin instead of guessing.
#[derive(Default)]
pub(crate) struct SlotCandidateIndex {
    entries: BTreeMap<SlotMiningKey, Vec<MinedCandidate>>,
    degraded_departments: BTreeSet<String>,
}

impl SlotCandidateIndex {
    pub(crate) fn lookup(&self, key: &SlotMiningKey) -> Option<&[MinedCandidate]> {
        self.entries.get(key).map(|items| items.as_slice())
    }

    pub(crate) fn degraded_departments(&self) -> Vec<String> {
        self.degraded_departments.iter().cloned().collect()
    }

    /// Total mined rows, for the snapshot's pool-size reporting. Nothing in the
    /// pipeline truncates a pool, so this is the number the dispatcher can trust
    /// when a run turns out slow.
    pub(crate) fn mined_candidate_count(&self) -> usize {
        self.entries.values().map(|items| items.len()).sum()
    }
}

impl DispatchFrontendReplanService {
    /// Collects every distinct slot requirement across the window's orders and
    /// mines each one once.
    pub(super) async fn build_slot_candidate_index<'a>(
        &self,
        orders: impl Iterator<Item = &'a DispatchOrder>,
        rules: &GenerationRuleIndex,
    ) -> SlotCandidateIndex {
        let mut index = SlotCandidateIndex::default();
        let Some(miner) = self.legal_resource_miner.as_ref() else {
            return index;
        };

        let mut wanted: BTreeMap<SlotMiningKey, DateTime<Utc>> = BTreeMap::new();
        for order in orders {
            let Some(department_id) = normalized_department_id(order) else {
                continue;
            };
            // Must be the same helper that fills the snapshot order's
            // `earliest_start_time`, since `slot_mining_key` keys lookups on
            // that field. Any other clock here and every lookup misses.
            let Some(at_time) = snapshot_earliest_start_time(order) else {
                continue;
            };
            let duration_minutes =
                resolve_duration_minutes(order, effective_start_time(order), effective_end_time(order));
            let maximum_duration = rules
                .duration_by_crew_size_for(order)
                .and_then(|table| table.values().copied().max())
                .or(duration_minutes)
                .unwrap_or(15)
                .max(1);
            let latest_start = resolve_start_window(order, Some(at_time), rules.flex_for(order));
            let valid_through =
                latest_start.unwrap_or(at_time) + chrono::Duration::minutes(i64::from(maximum_duration));
            for requirement in &order.crew_requirement_snapshot {
                let Some(obj) = requirement.as_object() else {
                    continue;
                };
                let Some(qualification_code) = json_string_field(obj.get("qualification_code")) else {
                    // A slot with no stated qualification cannot be mined; it
                    // keeps the order-attached fallback.
                    continue;
                };
                let key = SlotMiningKey {
                    department_id: department_id.clone(),
                    qualification_code,
                    min_level_code: json_string_field(obj.get("min_level_code"))
                        .or_else(|| json_string_field(obj.get("qualification_level_code"))),
                    at_time,
                };
                wanted
                    .entry(key)
                    .and_modify(|existing| *existing = (*existing).max(valid_through))
                    .or_insert(valid_through);
            }
        }

        for (key, valid_through) in wanted {
            match miner
                .mine_resources(
                    &key.department_id,
                    &key.qualification_code,
                    key.min_level_code.as_deref(),
                    key.at_time,
                    // Empty list on purpose: the repository adds no `user_id IN`
                    // clause, so this returns the whole department. That is the
                    // point — discovery, not verification of a shortlist.
                    &[],
                )
                .await
            {
                Ok(grants) => {
                    let candidates = grants
                        .into_iter()
                        .filter(|grant| grant.valid_from.is_none_or(|value| value <= key.at_time))
                        .filter(|grant| grant.valid_to.is_none_or(|value| value >= valid_through))
                        .map(|grant| MinedCandidate {
                            user_id: grant.user_id,
                            source_team_id: grant.source_team_id,
                        })
                        .collect::<Vec<_>>();
                    index.entries.insert(key, candidates);
                }
                Err(error) => {
                    tracing::warn!(
                        department_id = %key.department_id,
                        qualification_code = %key.qualification_code,
                        error = %error,
                        "挖掘合法人员失败,该部门作业的候选人回退到工单已有人员"
                    );
                    index.degraded_departments.insert(key.department_id.clone());
                }
            }
        }
        index
    }

    /// The mining key for one slot of one order, or `None` when the slot states
    /// no qualification or the order states no department.
    pub(super) fn slot_mining_key(
        &self,
        order: &DispatchReplanSnapshotOrder,
        qualification_code: Option<&str>,
        min_level_code: Option<&str>,
    ) -> Option<SlotMiningKey> {
        let department_id = order
            .department_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())?;
        let qualification_code = qualification_code.map(str::trim).filter(|value| !value.is_empty())?;
        Some(SlotMiningKey {
            department_id: department_id.to_string(),
            qualification_code: qualification_code.to_string(),
            min_level_code: min_level_code
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            at_time: order.earliest_start_time?,
        })
    }

    /// Teams the slots' candidates hold their qualifications through.
    ///
    /// Attribution, not a decision dimension: expanding a team into its members
    /// would add nobody, because mining already returned every qualified person
    /// in the department, and adding unqualified members would break the very
    /// constraint the per-slot filter exists to enforce. Unresolvable ids are
    /// skipped rather than shown as blank rows.
    pub(super) async fn build_candidate_teams(
        &self,
        personnel_slots: &[DispatchReplanPersonnelSlot],
    ) -> Vec<DispatchReplanCandidateTeam> {
        let Some(team_repo) = self.team_repo.as_ref() else {
            return Vec::new();
        };
        let team_ids = personnel_slots
            .iter()
            .flat_map(|slot| slot.candidate_source_team_ids.iter().cloned())
            .collect::<BTreeSet<_>>();

        let mut teams = Vec::new();
        for team_id in team_ids {
            match team_repo.find_by_id(&team_id, false).await {
                Ok(Some(team)) => teams.push(DispatchReplanCandidateTeam {
                    team_id: team.id,
                    team_name: team.name,
                    team_type_id: team.team_type_id,
                }),
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(team_id = %team_id, error = %error, "解析候选编组名称失败,跳过该编组");
                }
            }
        }
        teams
    }
}

fn normalized_department_id(order: &DispatchOrder) -> Option<String> {
    order
        .department_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Distinct teams the mined people were granted through, so the board can show
/// where a candidate comes from. Team names are resolved by the caller.
pub(crate) fn mined_source_team_ids(candidates: &[MinedCandidate]) -> Vec<String> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for candidate in candidates {
        if let Some(team_id) = candidate.source_team_id.as_deref().map(str::trim) {
            if !team_id.is_empty() {
                seen.insert(team_id.to_string());
            }
        }
    }
    seen.into_iter().collect()
}

/// Mined users indexed by id, for merging team/level metadata onto candidates
/// that the order already carried.
pub(crate) fn mined_by_user_id(candidates: &[MinedCandidate]) -> HashMap<&str, &MinedCandidate> {
    candidates
        .iter()
        .map(|candidate| (candidate.user_id.as_str(), candidate))
        .collect()
}
