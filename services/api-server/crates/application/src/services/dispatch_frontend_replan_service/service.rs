use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};

use crate::schemas::dispatch_schemas::{
    DispatchReplanAnchorFreeWindow, DispatchReplanAnchorState, DispatchReplanAssignment, DispatchReplanSnapshotOrder,
    DispatchReplanSnapshotResponse, DispatchReplanSuggestion, TaskCrewMemberResponse, TaskCrewResponse,
};
use crate::services::dispatch_chat_service::DispatchChatService;
use crate::services::legal_resource_miner::LegalResourceMiner;
use crate::types::ConcreteNotificationService;
use fms_domain::error::DomainError;
use fms_domain::models::dispatch::DispatchOrder;
use fms_domain::ports::dispatch_collaboration_repository::DispatchCollaborationRepository;
use fms_domain::ports::dispatch_repository::{
    DepartmentQualificationRepository, DispatchOrderMemberRepository, DispatchOrderRepository,
    DispatchTravelStatsRepository, EquipmentRepository, FlightGenerationRuleRepository, QualificationGrantRepository,
    TeamMemberRepository, TeamRepository,
};

use super::helpers::*;

const LOOKBACK_HOURS: i64 = 8;

#[path = "apply.rs"]
mod apply;
#[path = "order_result.rs"]
mod order_result;
#[path = "snapshot.rs"]
mod snapshot;

#[derive(Clone)]
struct SnapshotCacheEntry {
    snapshot: DispatchReplanSnapshotResponse,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

#[derive(Clone)]
pub(crate) struct ApplyValidationContext {
    pub(crate) suggestion: DispatchReplanSuggestion,
    pub(crate) snapshot_order: DispatchReplanSnapshotOrder,
    pub(crate) live_order: DispatchOrder,
    pub(crate) current_assignment: DispatchReplanAssignment,
    pub(crate) suggested_assignment: DispatchReplanAssignment,
}

struct ResourceAnchorContext {
    states: Vec<DispatchReplanAnchorState>,
    segments: HashMap<String, Vec<DispatchReplanAnchorFreeWindow>>,
}

struct ResolvedCrewSlot {
    slot_code: String,
    base_slot_code: String,
    baseline_slot_code: Option<String>,
    qualification_code: Option<String>,
    qualification_level_code: Option<String>,
    workload_weight: f64,
}

pub struct DispatchFrontendReplanService {
    pub(crate) order_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
    pub(crate) team_repo: Option<Arc<dyn TeamRepository + Send + Sync>>,
    pub(crate) team_member_repo: Option<Arc<dyn TeamMemberRepository + Send + Sync>>,
    pub(crate) equipment_repo: Option<Arc<dyn EquipmentRepository + Send + Sync>>,
    pub(crate) travel_stats_repo: Option<Arc<dyn DispatchTravelStatsRepository + Send + Sync>>,
    pub(crate) notification_service: Option<Arc<ConcreteNotificationService>>,
    pub(crate) collaboration_repo: Option<Arc<dyn DispatchCollaborationRepository + Send + Sync>>,
    pub(crate) dispatch_chat_service: Option<Arc<DispatchChatService>>,
    pub(crate) generation_rule_repo: Option<Arc<dyn FlightGenerationRuleRepository + Send + Sync>>,
    pub(crate) legal_resource_miner: Option<Arc<LegalResourceMiner>>,
    snapshots: DashMap<String, SnapshotCacheEntry>,
    snapshot_ttl_seconds: i64,
}

impl DispatchFrontendReplanService {
    pub const SNAPSHOT_TTL_SECONDS: i64 = 300;
    pub const MAX_SNAPSHOTS: usize = 128;
    pub const MODEL_VERSION: &'static str = "dispatch_wasm_pdf_full_model_v2";
    pub const SOLVER_VERSION: &'static str = "dispatch_solver_ortools_wasm_strict_pdf_v3";
    /// Bound on the (user × equipment) candidate-assignment enumeration only.
    ///
    /// Not a candidate cap: slot candidates come from the qualification store
    /// and are uncapped, and every user this bound can skip is already in
    /// `candidate_users`. The former `MAX_CANDIDATE_USERS` / `_TEAMS` /
    /// `_EQUIPMENTS` did cap real candidate pools and are gone.
    pub const MAX_ENUMERATED_ASSIGNMENT_USERS: usize = 8;

    pub fn new(
        order_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
        _order_member_repo: Arc<dyn DispatchOrderMemberRepository + Send + Sync>,
    ) -> Self {
        Self {
            order_repo,
            team_repo: None,
            team_member_repo: None,
            equipment_repo: None,
            travel_stats_repo: None,
            notification_service: None,
            collaboration_repo: None,
            dispatch_chat_service: None,
            generation_rule_repo: None,
            legal_resource_miner: None,
            snapshots: DashMap::new(),
            snapshot_ttl_seconds: Self::SNAPSHOT_TTL_SECONDS,
        }
    }

    pub fn with_resource_repos(
        mut self,
        team_repo: Arc<dyn TeamRepository + Send + Sync>,
        team_member_repo: Arc<dyn TeamMemberRepository + Send + Sync>,
        equipment_repo: Arc<dyn EquipmentRepository + Send + Sync>,
        travel_stats_repo: Option<Arc<dyn DispatchTravelStatsRepository + Send + Sync>>,
    ) -> Self {
        self.team_repo = Some(team_repo);
        self.team_member_repo = Some(team_member_repo);
        self.equipment_repo = Some(equipment_repo);
        self.travel_stats_repo = travel_stats_repo;
        self
    }

    pub fn with_notification_service(mut self, notification_service: Arc<ConcreteNotificationService>) -> Self {
        self.notification_service = Some(notification_service);
        self
    }

    pub fn with_collaboration_repo(
        mut self,
        collaboration_repo: Arc<dyn DispatchCollaborationRepository + Send + Sync>,
    ) -> Self {
        self.collaboration_repo = Some(collaboration_repo);
        self
    }

    pub fn with_dispatch_chat_service(mut self, dispatch_chat_service: Arc<DispatchChatService>) -> Self {
        self.dispatch_chat_service = Some(dispatch_chat_service);
        self
    }

    /// Supplies the department-owned generation rules that carry
    /// `start_flex_minutes`. Optional: without it every order falls back to
    /// [`helpers::REPLAN_START_FLEX_MINUTES`].
    pub fn with_generation_rule_repo(
        mut self,
        generation_rule_repo: Arc<dyn FlightGenerationRuleRepository + Send + Sync>,
    ) -> Self {
        self.generation_rule_repo = Some(generation_rule_repo);
        self
    }

    /// Supplies the qualification repositories that back per-slot candidate
    /// discovery.
    ///
    /// Optional: without it a slot only sees the people already attached to its
    /// order, which is what draft orders generated by
    /// `generate_draft_orders` lack entirely — every slot would be forced to
    /// `gap` by construction. Wiring this in is what lets the solver actually
    /// pick people.
    pub fn with_qualification_repos(
        mut self,
        qualification_repo: Arc<dyn DepartmentQualificationRepository + Send + Sync>,
        qualification_grant_repo: Arc<dyn QualificationGrantRepository + Send + Sync>,
    ) -> Self {
        self.legal_resource_miner = Some(Arc::new(LegalResourceMiner::new(
            qualification_repo,
            qualification_grant_repo,
        )));
        self
    }

    fn store_snapshot(&self, snapshot: DispatchReplanSnapshotResponse) {
        let now = Utc::now();
        let expires_at = now + Duration::seconds(self.snapshot_ttl_seconds.max(1));
        self.snapshots.retain(|_, entry| entry.expires_at > now);
        while self.snapshots.len() >= Self::MAX_SNAPSHOTS {
            let oldest_key = self
                .snapshots
                .iter()
                .min_by_key(|entry| entry.value().created_at)
                .map(|entry| entry.key().clone());
            if let Some(key) = oldest_key {
                self.snapshots.remove(&key);
            } else {
                break;
            }
        }
        self.snapshots.insert(
            snapshot.snapshot_id.clone(),
            SnapshotCacheEntry {
                snapshot,
                created_at: now,
                expires_at,
            },
        );
    }

    fn get_snapshot(&self, snapshot_id: &str) -> Result<DispatchReplanSnapshotResponse, DomainError> {
        let now = Utc::now();
        self.snapshots.retain(|_, entry| entry.expires_at > now);
        let entry = self
            .snapshots
            .get(snapshot_id)
            .ok_or_else(|| DomainError::BusinessRuleViolation("重排快照已过期，请重新预览".to_string()))?;
        Ok(entry.value().snapshot.clone())
    }

    fn assignment_from_order(&self, order: &DispatchOrder) -> DispatchReplanAssignment {
        let mut member_user_ids = Vec::new();
        let mut task_crew_members = Vec::new();
        for member in &order.members {
            if !member.is_active {
                continue;
            }
            member_user_ids.push(member.user_id.clone());
            task_crew_members.push(TaskCrewMemberResponse {
                user_id: member.user_id.clone(),
                username: member.username.clone(),
                source_team_id: member.source_team_id.clone(),
                source_team_name: None,
                slot_code: member.slot_code.clone(),
                qualification_code: member.qualification_code.clone(),
                qualification_level_code: member.qualification_level_code.clone(),
            });
        }
        let equipment_ids = order.equipment_list.iter().map(|item| item.id.clone()).collect();
        DispatchReplanAssignment {
            individual_user_id: order.individual_user_id.clone(),
            equipment_ids,
            member_user_ids,
            department_rule_version: order.department_rule_version.clone(),
            crew_requirement_snapshot: order.crew_requirement_snapshot.clone(),
            equipment_requirement_snapshot: order.equipment_requirement_snapshot.clone(),
            qualification_gap: order.qualification_gap.clone(),
            task_crew: TaskCrewResponse {
                members: task_crew_members,
                ..TaskCrewResponse::default()
            },
        }
    }

    fn normalize_assignment(&self, assignment: &DispatchReplanAssignment) -> DispatchReplanAssignment {
        let mut normalized = assignment.clone();
        normalized.equipment_ids = dedupe_strings(&normalized.equipment_ids);
        normalized.member_user_ids = dedupe_strings(&normalized.member_user_ids);
        if normalized.task_crew.members.is_empty() {
            normalized.task_crew.members = normalized
                .member_user_ids
                .iter()
                .map(|user_id| TaskCrewMemberResponse {
                    user_id: user_id.clone(),
                    ..TaskCrewMemberResponse::default()
                })
                .collect();
        }
        for member in &normalized.task_crew.members {
            if !member.user_id.trim().is_empty() {
                normalized.member_user_ids.push(member.user_id.clone());
            }
        }
        normalized.member_user_ids = dedupe_strings(&normalized.member_user_ids);
        normalized
    }

    fn apply_assignment_to_order(&self, order: &mut DispatchOrder, assignment: &DispatchReplanAssignment) {
        order.individual_user_id = assignment.individual_user_id.clone();
        order.department_rule_version = assignment.department_rule_version.clone();
        order.crew_requirement_snapshot = assignment.crew_requirement_snapshot.clone();
        order.equipment_requirement_snapshot = assignment.equipment_requirement_snapshot.clone();
        order.qualification_gap = assignment.qualification_gap.clone();
        order.task_crew = serde_json::Value::Object(serde_json::Map::from_iter(vec![
            (
                "members".to_string(),
                serde_json::to_value(&assignment.task_crew.members).unwrap_or_else(|_| json!([])),
            ),
            (
                "source_team_ids".to_string(),
                json!(assignment.task_crew.source_team_ids.clone()),
            ),
            (
                "source_team_names".to_string(),
                json!(assignment.task_crew.source_team_names.clone()),
            ),
            (
                "generated_from".to_string(),
                json!(assignment.task_crew.generated_from.clone()),
            ),
        ]));
    }

    fn member_change_summary(
        &self,
        current_assignment: &DispatchReplanAssignment,
        suggested_assignment: &DispatchReplanAssignment,
    ) -> Value {
        let current_members = task_crew_members(current_assignment);
        let suggested_members = task_crew_members(suggested_assignment);
        let mut current_by_slot = HashMap::new();
        let mut suggested_by_slot = HashMap::new();
        for member in current_members {
            let key = member.slot_code.clone().unwrap_or_else(|| member.user_id.clone());
            current_by_slot.insert(key, member);
        }
        for member in suggested_members {
            let key = member.slot_code.clone().unwrap_or_else(|| member.user_id.clone());
            suggested_by_slot.insert(key, member);
        }

        let mut all_slots: Vec<String> = current_by_slot.keys().cloned().collect();
        for key in suggested_by_slot.keys() {
            if !all_slots.contains(key) {
                all_slots.push(key.clone());
            }
        }

        let mut replaced_members = Vec::new();
        let mut added_members = Vec::new();
        let mut removed_members = Vec::new();
        let mut unchanged_members = Vec::new();
        for slot in all_slots {
            let current = current_by_slot.get(&slot);
            let suggested = suggested_by_slot.get(&slot);
            match (current, suggested) {
                (Some(current), Some(suggested)) if current.user_id == suggested.user_id => {
                    unchanged_members.push(json!({ "slot_code": slot, "member": suggested }));
                }
                (Some(current), Some(suggested)) => {
                    replaced_members.push(json!({
                        "slot_code": slot,
                        "before": current,
                        "after": suggested,
                    }));
                }
                (None, Some(suggested)) => {
                    added_members.push(json!({ "slot_code": slot, "member": suggested }));
                }
                (Some(current), None) => {
                    removed_members.push(json!({ "slot_code": slot, "member": current }));
                }
                (None, None) => {}
            }
        }
        json!({
            "replaced_members": replaced_members,
            "added_members": added_members,
            "removed_members": removed_members,
            "unchanged_members": unchanged_members,
            "changed_member_count": replaced_members.len() + added_members.len() + removed_members.len(),
        })
    }
}
