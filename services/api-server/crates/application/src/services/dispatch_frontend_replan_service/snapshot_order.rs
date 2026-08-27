use std::collections::HashMap;

use crate::schemas::dispatch_schemas::{
    DispatchReplanAnchorFreeWindow, DispatchReplanBaselineAssignment, DispatchReplanSnapshotOrder,
};
use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{DispatchOrder, DispatchOrderStatus, Equipment};

use super::super::super::helpers::*;
use super::super::DispatchFrontendReplanService;
use super::snapshot_mining::SlotCandidateIndex;

impl DispatchFrontendReplanService {
    pub(super) fn build_snapshot_order_base(
        &self,
        order: &DispatchOrder,
        conflict_reasons: Option<&Vec<String>>,
        rules: &GenerationRuleIndex,
    ) -> DispatchReplanSnapshotOrder {
        let current_assignment = self.assignment_from_order(order);
        let order_class = self.resolve_order_class(order, conflict_reasons);
        let equipment_ids = current_assignment.equipment_ids.clone();
        let effective_start_time = effective_start_time(order);
        let effective_end_time = effective_end_time(order);
        let duration_minutes = resolve_duration_minutes(order, effective_start_time, effective_end_time);
        let is_locked = is_locked_order(order);
        let earliest_start_time = snapshot_earliest_start_time(order);
        // Earliest and latest must not share a fallback chain: when they agree the
        // solver's start variable is pinned to a constant, every timing constraint
        // (no-overlap, travel, turnaround) degrades to a feasibility check, and the
        // `resolve_start_window` pins locked and in-progress orders itself.
        let flex_minutes = rules.flex_for(order);
        let latest_start_time = resolve_start_window(order, earliest_start_time, flex_minutes);
        let availability_reason = order.availability_reason.clone();
        DispatchReplanSnapshotOrder {
            order_id: order.id.clone(),
            flight_id: order.flight_id.clone(),
            status: order_status_text(order.status).to_string(),
            is_optimizable: !is_locked,
            is_fixed_anchor: false,
            conflict_state: self.resolve_conflict_state(order, conflict_reasons, false),
            order_class,
            has_conflict: conflict_reasons.map(|items| !items.is_empty()).unwrap_or(false),
            planned_start_time: order.planned_start_time,
            planned_end_time: order.planned_end_time,
            completion_time_mode: order.completion_time_mode.clone(),
            completion_target_time: (order.completion_time_mode.as_deref() == Some("completion_anchor_offset"))
                .then_some(order.planned_end_time)
                .flatten(),
            earliest_start_time,
            latest_start_time,
            duration_minutes,
            // Filled once the personnel slots exist, since the table's length is
            // "one entry per fillable slot" — see `enrich_snapshot_order`.
            duration_by_crew_size: None,
            required_start_time: order.planned_start_time.or(effective_start_time),
            actual_start_time: order.actual_start_time,
            actual_end_time: order.actual_end_time,
            estimated_completion_time: order.estimated_completion_time,
            effective_start_time,
            effective_end_time,
            turnaround_pair_key: order.turnaround_pair_key.clone(),
            turnaround_constraint_mode: order.turnaround_constraint_mode.clone(),
            baseline_assignment: DispatchReplanBaselineAssignment::default(),
            personnel_slots: Vec::new(),
            equipment_slots: Vec::new(),
            team_id: {
                let mut ids = order
                    .members
                    .iter()
                    .filter_map(|member| {
                        member
                            .source_team_id
                            .as_deref()
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(str::to_string)
                    })
                    .collect::<Vec<_>>();
                ids.sort();
                ids.dedup();
                if ids.len() == 1 {
                    ids.pop()
                } else {
                    None
                }
            },
            department_id: order.department_id.clone(),
            individual_user_id: order.individual_user_id.clone(),
            equipment_ids,
            crew_requirement_snapshot: order.crew_requirement_snapshot.clone(),
            equipment_requirement_snapshot: order.equipment_requirement_snapshot.clone(),
            qualification_gap: order.qualification_gap.clone(),
            stand_id: order.stand_id.clone(),
            lock_level: lock_level_text(order.lock_level).to_string(),
            availability_reason,
            score_breakdown: match &order.score_breakdown {
                serde_json::Value::Object(map) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                _ => HashMap::new(),
            },
            conflict_reason: conflict_reasons.and_then(|items| items.first().cloned()),
            schedule_source: schedule_source_text(order),
            current_assignment: Some(current_assignment),
            leg_scope: Some(order.leg_scope.clone()),
            candidate_users: Vec::new(),
            candidate_teams: Vec::new(),
            candidate_equipments: Vec::new(),
            candidate_assignments: Vec::new(),
            is_completed: matches!(order.status, DispatchOrderStatus::Completed),
            is_in_progress: matches!(order.status, DispatchOrderStatus::InProgress),
            is_locked,
        }
    }

    pub(super) async fn enrich_snapshot_order(
        &self,
        mut snapshot_order: DispatchReplanSnapshotOrder,
        order: &DispatchOrder,
        available_equipments: &HashMap<Option<String>, Vec<Equipment>>,
        user_segments: &HashMap<String, Vec<DispatchReplanAnchorFreeWindow>>,
        equipment_segments: &HashMap<String, Vec<DispatchReplanAnchorFreeWindow>>,
        mined: &SlotCandidateIndex,
        rules: &GenerationRuleIndex,
    ) -> Result<DispatchReplanSnapshotOrder, DomainError> {
        let current_assignment = snapshot_order.current_assignment.take().unwrap_or_default();
        let candidate_users = self.build_candidate_users(order, &current_assignment);
        let candidate_equipments = self.build_candidate_equipments(order, available_equipments);
        let candidate_assignments = self
            .build_candidate_assignments(
                &snapshot_order,
                &current_assignment,
                &candidate_users,
                &candidate_equipments,
                user_segments,
                equipment_segments,
            )
            .await?;
        snapshot_order.candidate_users = candidate_users;
        snapshot_order.candidate_equipments = candidate_equipments;
        snapshot_order.candidate_assignments = candidate_assignments;
        snapshot_order.personnel_slots = self.build_personnel_slots(&snapshot_order, &current_assignment, mined);
        // Only optimizable orders get a variable duration. A locked or in-progress
        // order is an anchor whose end time other orders are planned against;
        // letting the solver stretch it would move a commitment the dispatcher
        // already made.
        snapshot_order.duration_by_crew_size = if snapshot_order.is_optimizable {
            resolve_duration_table(
                rules.duration_by_crew_size_for(order),
                snapshot_order.personnel_slots.len(),
                snapshot_order.duration_minutes,
            )
        } else {
            None
        };
        // Teams are derived from the slots, so this must follow them. The solver
        // decides over people; teams ride along as attribution only.
        snapshot_order.candidate_teams = self.build_candidate_teams(&snapshot_order.personnel_slots).await;
        // Do not pre-filter against the order's original effective interval. The
        // solver is responsible for moving the order within earliest/latest and
        // selecting an explicit resource free window. Filtering against the old
        // interval would remove exactly the resources that become available
        // after a legal time shift.
        for slot in &mut snapshot_order.personnel_slots {
            slot.schedule_feasible_candidate_user_ids = slot
                .qualification_feasible_candidate_user_ids
                .iter()
                .filter(|user_id| user_segments.get(*user_id).is_none_or(|segments| !segments.is_empty()))
                .cloned()
                .collect();
            slot.candidate_user_ids = slot.schedule_feasible_candidate_user_ids.clone();
        }
        snapshot_order.equipment_slots = self.build_equipment_slots(&snapshot_order, &current_assignment);
        for slot in &mut snapshot_order.equipment_slots {
            slot.schedule_feasible_candidate_equipment_ids = slot
                .candidate_equipment_ids
                .iter()
                .filter(|equipment_id| {
                    equipment_segments
                        .get(*equipment_id)
                        .is_none_or(|segments| !segments.is_empty())
                })
                .cloned()
                .collect();
            slot.candidate_equipment_ids = slot.schedule_feasible_candidate_equipment_ids.clone();
        }
        snapshot_order.baseline_assignment = self.build_baseline_assignment(
            &snapshot_order,
            &current_assignment,
            &snapshot_order.personnel_slots,
            &snapshot_order.equipment_slots,
            order,
        );
        snapshot_order.current_assignment = Some(current_assignment);
        Ok(snapshot_order)
    }

    pub(super) fn resolve_order_class(&self, order: &DispatchOrder, conflict_reasons: Option<&Vec<String>>) -> String {
        if is_locked_order(order) {
            return "locked".to_string();
        }
        if !has_primary_assignment(&self.assignment_from_order(order)) {
            return "unassigned".to_string();
        }
        if conflict_reasons.map(|items| !items.is_empty()).unwrap_or(false) {
            return "assigned_conflict".to_string();
        }
        "locked".to_string()
    }

    fn resolve_conflict_state(
        &self,
        order: &DispatchOrder,
        conflict_reasons: Option<&Vec<String>>,
        is_fixed_anchor: bool,
    ) -> String {
        if is_fixed_anchor || is_locked_order(order) {
            return "locked".to_string();
        }
        if conflict_reasons.map(|items| !items.is_empty()).unwrap_or(false) {
            return "resource_conflict".to_string();
        }
        if !has_primary_assignment(&self.assignment_from_order(order)) {
            return "gap".to_string();
        }
        "none".to_string()
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};
    use fms_domain::models::dispatch::{
        DepartmentRuleStatus, DispatchLockLevel, DispatchOrder, DispatchOrderStatus, DispatchPublicationState,
        Equipment, FlightGenerationRule, LegScope, ScheduleSource,
    };

    use super::*;
    use crate::schemas::dispatch_schemas::{
        DispatchReplanApplyRequest, DispatchReplanAssignment, DispatchReplanPersonnelSlot,
    };
    use crate::services::dispatch_frontend_replan_service::helpers::{
        effective_start_time, resolve_start_window, REPLAN_START_FLEX_MINUTES,
    };
    use crate::services::dispatch_frontend_replan_service::test_support::{
        grant, level, StubOrderMemberRepo, StubOrderRepo, StubQualificationGrantRepo, StubQualificationRepo,
    };
    use fms_domain::models::dispatch::QualificationGrant;

    fn base_order() -> DispatchOrder {
        let now = Utc.with_ymd_and_hms(2026, 5, 12, 8, 0, 0).unwrap();
        DispatchOrder {
            id: "order-1".to_string(),
            flight_id: "flight-1".to_string(),
            task_type: "boarding".to_string(),
            stand_id: None,
            task_type_name: None,
            stand_code: None,
            terminal: None,
            department: None,
            individual_user_id: Some("user-1".to_string()),
            individual_username: None,
            driver_type: None,
            driver_user_id: None,
            planned_start_time: Some(now),
            planned_end_time: Some(now + Duration::minutes(30)),
            actual_start_time: None,
            actual_end_time: None,
            estimated_completion_time: None,
            estimated_completion_reported_by: None,
            estimated_completion_reported_at: None,
            estimated_completion_note: None,
            status: DispatchOrderStatus::Assigned,
            dispatch_type: fms_domain::models::dispatch::DispatchType::Auto,
            dispatched_at: None,
            dispatched_by: None,
            snapshot_assignee_position: None,
            snapshot_equipment_positions: None,
            estimated_arrival_minutes: None,
            process_instance_id: None,
            process_task_id: None,
            workflow_context: serde_json::Value::Object(Default::default()),
            workflow_status: "pending".to_string(),
            source: "event_rule".to_string(),
            schedule_source: ScheduleSource::CurrentStatusFallback,
            lock_level: DispatchLockLevel::Optimizable,
            publication_state: "prepublished".to_string(),
            source_type: "event_generated".to_string(),
            department_id: None,
            leg_scope: "none".to_string(),
            generation_rule_id: None,
            generation_rule_version: None,
            generation_anchor_type: None,
            generation_anchor_time: None,
            completion_time_mode: None,
            completion_anchor_type: None,
            completion_anchor_time: None,
            completion_offset_minutes: None,
            completion_warning_lead_minutes: None,
            publish_trigger_mode: None,
            publish_at: None,
            turnaround_pair_key: None,
            turnaround_constraint_mode: None,
            department_rule_version: None,
            crew_requirement_snapshot: vec![],
            equipment_requirement_snapshot: vec![],
            task_crew: serde_json::Value::Object(Default::default()),
            equipment_assignment: vec![],
            qualification_gap: vec![],
            equipment_gap: vec![],
            availability_reason: None,
            score_breakdown: serde_json::Value::Object(Default::default()),
            conflict_reason: None,
            recommended_assignees: vec![],
            recommendation_score: None,
            supervisor_notified: false,
            supervisor_notified_at: None,
            assignment_deadline: None,
            completed_by: None,
            completion_notes: None,
            gate: None,
            created_at: Some(now),
            updated_at: Some(now),
            members: vec![],
            equipment_list: vec![],
        }
    }

    #[test]
    fn window_must_not_collapse_for_optimizable_order() {
        let order = base_order();
        let earliest = effective_start_time(&order);
        let latest = resolve_start_window(&order, earliest, None);

        let earliest = earliest.expect("earliest from planned start");
        let latest = latest.expect("latest resolved");

        assert!(
            latest > earliest,
            "optimizable order must get a movable start window, got earliest={earliest} latest={latest}"
        );
        assert_eq!(latest - earliest, Duration::minutes(REPLAN_START_FLEX_MINUTES));
    }

    #[test]
    fn locked_order_keeps_a_point_window() {
        let mut order = base_order();
        order.lock_level = DispatchLockLevel::Frozen;
        let earliest = effective_start_time(&order);
        let latest = resolve_start_window(&order, earliest, None);
        assert_eq!(earliest, latest, "locked order start must stay pinned");
    }

    #[test]
    fn missing_planned_end_does_not_panic_and_falls_back_to_earliest() {
        let mut order = base_order();
        order.planned_end_time = None;
        let earliest = effective_start_time(&order);
        let latest = resolve_start_window(&order, earliest, None);
        let earliest = earliest.expect("earliest from planned start");
        assert!(
            latest.expect("latest") > earliest,
            "no deadline -> still needs flex beyond earliest"
        );
    }

    fn generation_rule(flex: Option<i32>) -> FlightGenerationRule {
        FlightGenerationRule {
            id: "rule-1".to_string(),
            department_id: "dept-1".to_string(),
            task_type: "boarding".to_string(),
            leg_scope: LegScope::Outbound,
            version_no: 1,
            status: DepartmentRuleStatus::Published,
            rule_name: None,
            conditions: Default::default(),
            generation_anchor_type: "departure".to_string(),
            start_offset_minutes: -30,
            completion_time_mode: "start_plus_duration".to_string(),
            completion_anchor_type: None,
            completion_offset_minutes: None,
            completion_warning_lead_minutes: None,
            duration_minutes: Some(30),
            start_flex_minutes: flex,
            duration_by_crew_size: None,
            publication_state: DispatchPublicationState::Published,
            publish_trigger_mode: fms_domain::models::dispatch::PublishTriggerMode::Time,
            publish_at: None,
            publish_offset_minutes: None,
            publish_event_code: None,
            notes: None,
            published_at: None,
            created_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn department_configured_flex_sets_the_window_width() {
        let order = base_order();
        // No completion deadline, so the flex alone decides the upper bound.
        let mut order = order;
        order.planned_end_time = None;
        let earliest = effective_start_time(&order);
        let latest = resolve_start_window(&order, earliest, Some(10));

        let earliest = earliest.expect("earliest from planned start");
        assert_eq!(
            latest.expect("latest") - earliest,
            Duration::minutes(10),
            "window width must come from the department's start_flex_minutes"
        );
    }

    #[test]
    fn unconfigured_flex_falls_back_to_the_system_default() {
        let mut order = base_order();
        order.planned_end_time = None;
        let earliest = effective_start_time(&order);
        let latest = resolve_start_window(&order, earliest, None);

        let earliest = earliest.expect("earliest from planned start");
        assert_eq!(
            latest.expect("latest") - earliest,
            Duration::minutes(5),
            "an unconfigured department must use the five-minute fallback"
        );
        assert_eq!(REPLAN_START_FLEX_MINUTES, 5);
    }

    fn rule_with_duration_table(table: serde_json::Value) -> FlightGenerationRule {
        let mut rule = generation_rule(None);
        rule.duration_by_crew_size = Some(table);
        rule
    }

    fn order_from_rule() -> DispatchOrder {
        let mut order = base_order();
        order.department_id = Some("dept-1".to_string());
        order.leg_scope = "outbound".to_string();
        order.generation_rule_id = Some("rule-1".to_string());
        order
    }

    #[test]
    fn a_configured_duration_table_expands_to_one_entry_per_crew_size() {
        let index = GenerationRuleIndex::from_rules(
            [rule_with_duration_table(serde_json::json!({"1": 45, "2": 30, "3": 25}))].iter(),
        );
        let order = order_from_rule();

        let table = resolve_duration_table(index.duration_by_crew_size_for(&order), 3, Some(30))
            .expect("a configured department yields a table");

        assert_eq!(
            table,
            vec![45, 45, 30, 25],
            "index k must be the duration for k assigned people, and k=0 cannot beat doing it alone"
        );
    }

    #[test]
    fn an_unconfigured_crew_size_takes_the_nearest_smaller_entry() {
        // The department configured 1 and 4 only. Three people must not inherit
        // the four-person duration: fewer people are never faster.
        let index =
            GenerationRuleIndex::from_rules([rule_with_duration_table(serde_json::json!({"1": 60, "4": 20}))].iter());
        let order = order_from_rule();

        let table = resolve_duration_table(index.duration_by_crew_size_for(&order), 4, Some(30))
            .expect("a configured department yields a table");

        assert_eq!(table, vec![60, 60, 60, 60, 20]);
    }

    #[test]
    fn an_unconfigured_department_keeps_the_constant_duration() {
        let index = GenerationRuleIndex::from_rules([generation_rule(Some(10))].iter());
        let order = order_from_rule();

        assert_eq!(
            resolve_duration_table(index.duration_by_crew_size_for(&order), 3, Some(30)),
            None,
            "without configuration the solver must keep today's constant duration"
        );
    }

    #[test]
    fn a_table_that_only_restates_the_constant_is_not_emitted() {
        // Emitting this would make duration a decision variable with exactly one
        // reachable value: all cost, no modelling gain.
        let index = GenerationRuleIndex::from_rules([rule_with_duration_table(serde_json::json!({"1": 30}))].iter());
        let order = order_from_rule();

        assert_eq!(
            resolve_duration_table(index.duration_by_crew_size_for(&order), 2, Some(30)),
            None
        );
    }

    #[test]
    fn malformed_duration_entries_are_dropped_without_losing_the_valid_ones() {
        let index = GenerationRuleIndex::from_rules(
            [rule_with_duration_table(
                serde_json::json!({"0": 90, "1": 45, "2": -5, "three": 25, "4": 20}),
            )]
            .iter(),
        );
        let order = order_from_rule();

        let table = resolve_duration_table(index.duration_by_crew_size_for(&order), 4, Some(30))
            .expect("the valid entries still form a table");

        assert_eq!(
            table,
            vec![45, 45, 45, 45, 20],
            "zero crew size, a negative duration and a non-numeric key must all be ignored"
        );
    }

    #[test]
    fn flex_index_prefers_the_rule_version_the_order_was_generated_from() {
        let mut order = base_order();
        order.department_id = Some("dept-1".to_string());
        order.leg_scope = "outbound".to_string();
        order.generation_rule_id = Some("rule-1".to_string());

        let mut newer = generation_rule(Some(45));
        newer.id = "rule-2".to_string();
        newer.version_no = 2;
        let rules = vec![generation_rule(Some(10)), newer];
        let index = GenerationRuleIndex::from_rules(rules.iter());

        assert_eq!(
            index.flex_for(&order),
            Some(10),
            "a generated order must keep the flex of the rule version it came from"
        );
    }

    #[test]
    fn flex_index_falls_back_to_the_newest_rule_version_for_manual_orders() {
        let mut order = base_order();
        order.department_id = Some("dept-1".to_string());
        order.leg_scope = "outbound".to_string();
        order.generation_rule_id = None;

        let mut newer = generation_rule(Some(45));
        newer.id = "rule-2".to_string();
        newer.version_no = 2;
        let rules = vec![generation_rule(Some(10)), newer];
        let index = GenerationRuleIndex::from_rules(rules.iter());

        assert_eq!(
            index.flex_for(&order),
            Some(45),
            "an order with no rule id must take the newest published version"
        );
    }

    #[test]
    fn flex_index_prefers_a_published_rule_over_a_newer_draft() {
        let mut order = base_order();
        order.department_id = Some("dept-1".to_string());
        order.leg_scope = "outbound".to_string();
        order.generation_rule_id = None;

        // The department is drafting a change but has not published it yet: a
        // higher version_no must not let unreviewed slack reach the solver.
        let mut draft = generation_rule(Some(5));
        draft.id = "rule-2".to_string();
        draft.version_no = 9;
        draft.status = DepartmentRuleStatus::Draft;
        let index = GenerationRuleIndex::from_rules(vec![generation_rule(Some(40)), draft].iter());

        assert_eq!(index.flex_for(&order), Some(40));
    }

    #[test]
    fn flex_index_still_reads_an_archived_rule_when_nothing_is_live() {
        let mut order = base_order();
        order.department_id = Some("dept-1".to_string());
        order.leg_scope = "outbound".to_string();
        order.generation_rule_id = None;

        let mut archived = generation_rule(Some(25));
        archived.status = DepartmentRuleStatus::Archived;
        let index = GenerationRuleIndex::from_rules([archived].iter());

        assert_eq!(
            index.flex_for(&order),
            Some(25),
            "an archived-only department keeps its configured slack rather than silently losing it"
        );
    }

    #[test]
    fn flex_index_reports_nothing_for_an_unconfigured_department() {
        let mut order = base_order();
        order.department_id = Some("dept-1".to_string());
        order.leg_scope = "outbound".to_string();

        // A rule exists but the department left start_flex_minutes NULL.
        let index = GenerationRuleIndex::from_rules([generation_rule(None)].iter());
        assert!(index.is_empty());
        assert_eq!(
            index.flex_for(&order),
            None,
            "NULL means unconfigured, which the window resolver reads as the default"
        );
    }

    #[test]
    fn a_later_task_on_the_same_flight_does_not_narrow_this_window() {
        let mut order = base_order();
        order.planned_end_time = None;
        let earliest = effective_start_time(&order).expect("earliest from planned start");
        let latest = resolve_start_window(&order, Some(earliest), Some(60));

        // Start order across tasks on one flight is not modelled: a task that is
        // not ready waits, so the department's configured slack must reach the
        // solver whole rather than being trimmed by a neighbour's planned time.
        assert_eq!(
            latest.expect("latest") - earliest,
            Duration::minutes(60),
            "the configured flex must survive intact"
        );
    }

    fn bare_service() -> DispatchFrontendReplanService {
        DispatchFrontendReplanService::new(
            std::sync::Arc::new(StubOrderRepo::default()),
            std::sync::Arc::new(StubOrderMemberRepo),
        )
    }

    /// A service whose qualification store knows `senior` covers `junior`, and
    /// holds the grants passed in.
    fn service_with_qualifications(grants: Vec<QualificationGrant>) -> DispatchFrontendReplanService {
        service_with_qualification_store(grants, false)
    }

    fn service_with_qualification_store(grants: Vec<QualificationGrant>, fail: bool) -> DispatchFrontendReplanService {
        bare_service().with_qualification_repos(
            std::sync::Arc::new(StubQualificationRepo {
                levels: vec![level("senior", &["junior"]), level("junior", &[])],
            }),
            std::sync::Arc::new(StubQualificationGrantRepo { grants, fail }),
        )
    }

    /// A draft order as `generate_draft_orders` emits it: crew requirements
    /// stated, nobody attached.
    fn draft_order(requirements: serde_json::Value) -> DispatchOrder {
        let mut order = base_order();
        order.department_id = Some("dept-1".to_string());
        order.individual_user_id = None;
        order.status = DispatchOrderStatus::Pending;
        order.crew_requirement_snapshot = requirements.as_array().cloned().expect("requirements array");
        order
    }

    fn requirement(slot_code: &str, min_level_code: &str) -> serde_json::Value {
        serde_json::json!({
            "slot_code": slot_code,
            "qualification_code": "ops_license",
            "min_level_code": min_level_code,
        })
    }

    async fn slots_for(
        service: &DispatchFrontendReplanService,
        order: &DispatchOrder,
    ) -> Vec<DispatchReplanPersonnelSlot> {
        let mined = service
            .build_slot_candidate_index([order].into_iter(), &GenerationRuleIndex::default())
            .await;
        slots_with_index(service, order, &mined)
    }

    /// Mirrors the order in which `enrich_snapshot_order` populates an order:
    /// order-attached candidates first, then slots. Building slots off a bare
    /// base order would leave `candidate_users` empty and silently test a
    /// pipeline that does not exist.
    fn slots_with_index(
        service: &DispatchFrontendReplanService,
        order: &DispatchOrder,
        mined: &SlotCandidateIndex,
    ) -> Vec<DispatchReplanPersonnelSlot> {
        let mut snapshot_order = service.build_snapshot_order_base(order, None, &GenerationRuleIndex::default());
        let current_assignment = snapshot_order.current_assignment.clone().unwrap_or_default();
        snapshot_order.candidate_users = service.build_candidate_users(order, &current_assignment);
        service.build_personnel_slots(&snapshot_order, &current_assignment, mined)
    }

    #[tokio::test]
    async fn a_draft_order_with_nobody_attached_still_gets_candidates_from_the_qualification_store() {
        // The core defect: these orders reached the solver with empty candidate
        // lists, and `AddExactlyOne({candidates…, gap})` pinned gap=1 by
        // construction — a clean OPTIMAL that planned nothing.
        let service = service_with_qualifications(vec![
            grant("user-a", "ops_license", "senior"),
            grant("user-b", "ops_license", "junior"),
        ]);
        let order = draft_order(serde_json::json!([requirement("lead", "junior")]));

        let slots = slots_for(&service, &order).await;

        assert_eq!(slots.len(), 1);
        assert_eq!(
            slots[0].candidate_user_ids,
            vec!["user-a".to_string(), "user-b".to_string()],
            "a draft order must draw candidates from the department's qualification store"
        );
    }

    #[tokio::test]
    async fn a_grant_below_the_slots_required_level_is_not_a_candidate() {
        let service = service_with_qualifications(vec![
            grant("user-a", "ops_license", "senior"),
            grant("user-b", "ops_license", "junior"),
        ]);
        let order = draft_order(serde_json::json!([requirement("lead", "senior")]));

        let slots = slots_for(&service, &order).await;

        assert_eq!(
            slots[0].candidate_user_ids,
            vec!["user-a".to_string()],
            "junior does not cover senior, so user-b must not be offered for a senior slot"
        );
    }

    #[tokio::test]
    async fn a_grant_must_cover_the_orders_complete_movable_execution_interval() {
        let mut expiring_grant = grant("user-a", "ops_license", "senior");
        // Default flex is five minutes, so the latest legal execution ends at
        // 08:35. Expiring one minute earlier must exclude the grant.
        expiring_grant.valid_to = Some(Utc.with_ymd_and_hms(2026, 5, 12, 8, 34, 0).unwrap());
        let service = service_with_qualifications(vec![expiring_grant]);
        let order = draft_order(serde_json::json!([requirement("lead", "senior")]));

        let slots = slots_for(&service, &order).await;

        assert!(
            slots[0].candidate_user_ids.is_empty(),
            "a grant valid at earliest_start but expired before latest_start + duration is not a legal candidate"
        );
    }

    #[tokio::test]
    async fn qualification_is_filtered_per_slot_not_per_order() {
        // The old code dumped the order-level pool into every slot, so a person
        // qualified for any slot appeared in all of them.
        let service = service_with_qualifications(vec![
            grant("user-a", "ops_license", "senior"),
            grant("user-b", "ops_license", "junior"),
        ]);
        let order = draft_order(serde_json::json!([
            requirement("lead", "senior"),
            requirement("assist", "junior"),
        ]));

        let slots = slots_for(&service, &order).await;

        assert_eq!(slots[0].candidate_user_ids, vec!["user-a".to_string()]);
        assert_eq!(
            slots[1].candidate_user_ids,
            vec!["user-a".to_string(), "user-b".to_string()],
            "user-b qualifies for the junior slot only"
        );
    }

    #[tokio::test]
    async fn an_attached_user_without_the_qualification_is_excluded_and_reported() {
        let service = service_with_qualifications(vec![grant("user-a", "ops_license", "senior")]);
        let mut order = draft_order(serde_json::json!([requirement("lead", "senior")]));
        // Somebody was hand-assigned who does not hold the qualification.
        order.individual_user_id = Some("user-x".to_string());

        let slots = slots_for(&service, &order).await;

        assert!(
            !slots[0].candidate_user_ids.contains(&"user-x".to_string()),
            "an unqualified assignee must not be offered to the solver"
        );
        assert!(
            slots[0].qualification_excluded_user_ids.contains(&"user-x".to_string()),
            "...but must be reported rather than dropped silently"
        );
    }

    #[tokio::test]
    async fn candidate_pools_are_neither_truncated_nor_order_dependent() {
        // 20 > the old cap of 8. The cut used to depend on which of four
        // order-attached sources was walked first.
        let grants = (0..20)
            .map(|index| grant(&format!("user-{index:02}"), "ops_license", "senior"))
            .collect::<Vec<_>>();
        let service = service_with_qualifications(grants);
        let order = draft_order(serde_json::json!([requirement("lead", "junior")]));

        let first = slots_for(&service, &order).await;
        let second = slots_for(&service, &order).await;

        assert_eq!(first[0].candidate_user_ids.len(), 20, "the pool must not be truncated");
        assert_eq!(
            first[0].candidate_user_ids, second[0].candidate_user_ids,
            "repeated builds must be byte-identical so goldens stay comparable"
        );
    }

    #[tokio::test]
    async fn without_qualification_repos_a_qualified_slot_is_left_unfilled() {
        let service = bare_service();
        let mut order = draft_order(serde_json::json!([requirement("lead", "senior")]));
        order.individual_user_id = Some("user-x".to_string());

        let slots = slots_for(&service, &order).await;

        assert!(slots[0].candidate_user_ids.is_empty());
        assert_eq!(slots[0].qualification_excluded_user_ids, vec!["user-x".to_string()]);
    }

    #[tokio::test]
    async fn an_unreachable_qualification_store_degrades_instead_of_failing_the_snapshot() {
        let service = service_with_qualification_store(vec![grant("user-a", "ops_license", "senior")], true);
        let mut order = draft_order(serde_json::json!([requirement("lead", "senior")]));
        order.individual_user_id = Some("user-x".to_string());

        let mined = service
            .build_slot_candidate_index([&order].into_iter(), &GenerationRuleIndex::default())
            .await;
        assert_eq!(
            mined.degraded_departments(),
            vec!["dept-1".to_string()],
            "the failure must be recorded, not swallowed"
        );

        let slots = slots_with_index(&service, &order, &mined);

        assert!(
            slots[0].candidate_user_ids.is_empty(),
            "a store outage must produce an explicit gap, not an unverifiable assignment"
        );
    }

    #[tokio::test]
    async fn enrichment_keeps_mined_candidates_that_have_no_recorded_occupancy() {
        // A missing anchor entry means no fixed occupancy in the snapshot
        // window, so the candidate is fully available and must survive.
        let service = service_with_qualifications(vec![
            grant("user-a", "ops_license", "senior"),
            grant("user-b", "ops_license", "senior"),
        ]);
        let order = draft_order(serde_json::json!([requirement("lead", "senior")]));
        let mined = service
            .build_slot_candidate_index([&order].into_iter(), &GenerationRuleIndex::default())
            .await;
        let snapshot_order = service.build_snapshot_order_base(&order, None, &GenerationRuleIndex::default());

        let enriched = service
            .enrich_snapshot_order(
                snapshot_order,
                &order,
                &HashMap::new(),
                &HashMap::new(), // no anchor segments recorded for anyone
                &HashMap::new(),
                &mined,
                &GenerationRuleIndex::default(),
            )
            .await
            .expect("enrichment succeeds");

        assert_eq!(
            enriched.personnel_slots[0].candidate_user_ids,
            vec!["user-a".to_string(), "user-b".to_string()],
            "resources with no fixed occupancy must survive the schedule-feasibility filter"
        );
    }

    #[tokio::test]
    async fn enrichment_keeps_a_candidate_available_after_a_legal_time_shift() {
        let service = service_with_qualifications(vec![grant("user-a", "ops_license", "senior")]);
        let order = draft_order(serde_json::json!([requirement("lead", "senior")]));
        let mined = service
            .build_slot_candidate_index([&order].into_iter(), &GenerationRuleIndex::default())
            .await;
        let snapshot_order = service.build_snapshot_order_base(&order, None, &GenerationRuleIndex::default());
        let later_start = Utc.with_ymd_and_hms(2026, 5, 12, 9, 0, 0).unwrap();
        let user_segments = HashMap::from([(
            "user-a".to_string(),
            vec![DispatchReplanAnchorFreeWindow {
                resource_type: "employee".to_string(),
                resource_id: "user-a".to_string(),
                window_start: Some(later_start),
                window_end: Some(later_start + Duration::minutes(60)),
                left_anchor_order_id: None,
                left_anchor_stand_id: None,
                right_anchor_order_id: None,
                right_anchor_stand_id: None,
            }],
        )]);

        let enriched = service
            .enrich_snapshot_order(
                snapshot_order,
                &order,
                &HashMap::new(),
                &user_segments,
                &HashMap::new(),
                &mined,
                &GenerationRuleIndex::default(),
            )
            .await
            .expect("enrichment succeeds");

        assert_eq!(
            enriched.personnel_slots[0].candidate_user_ids,
            vec!["user-a".to_string()],
            "availability after the original interval must remain for the solver to schedule"
        );
    }

    #[tokio::test]
    async fn a_resource_with_no_recorded_anchor_segments_is_available() {
        let service = bare_service();
        let start = Utc.with_ymd_and_hms(2026, 5, 12, 8, 0, 0).unwrap();

        // `build_resource_anchor_states` only creates a key for resources that
        // appear in `fixed_orders`, and always pushes at least one segment when
        // it does. So "no entry" means "no fixed occupancy anywhere in the
        // window" — the idlest resource there is. The previous
        // `unwrap_or(false)` deleted exactly those first.
        assert!(
            service
                .resource_has_feasible_window(None, start, start + Duration::minutes(30), None)
                .await,
            "a resource absent from the anchor map must count as available"
        );
        assert!(
            service
                .resource_has_feasible_window(Some(&Vec::new()), start, start + Duration::minutes(30), None)
                .await,
            "an empty segment list must count as available"
        );
    }

    #[test]
    fn an_unanchored_candidate_gets_an_explicit_full_window_for_the_solver() {
        let service = bare_service();
        let start = Utc.with_ymd_and_hms(2026, 5, 12, 8, 0, 0).unwrap();
        let end = start + Duration::hours(2);
        let mut context = service.build_resource_anchor_states("employee", start, end, &[]);

        service.ensure_candidate_resource_windows(&mut context, "employee", start, end, ["user-a".to_string()]);

        assert_eq!(context.states.len(), 1);
        assert_eq!(context.states[0].resource_id, "user-a");
        assert_eq!(context.states[0].free_windows[0].window_start, Some(start));
        assert_eq!(context.states[0].free_windows[0].window_end, Some(end));
    }

    #[tokio::test]
    async fn required_count_expands_one_crew_requirement_into_stable_decision_slots() {
        let service = service_with_qualifications(vec![grant("user-a", "ops_license", "senior")]);
        let mut crew_requirement = requirement("loader", "senior");
        crew_requirement["required_count"] = serde_json::json!(3);
        let order = draft_order(serde_json::json!([crew_requirement]));

        let slots = slots_for(&service, &order).await;

        assert_eq!(
            slots.iter().map(|slot| slot.slot_code.as_str()).collect::<Vec<_>>(),
            vec!["loader#1", "loader#2", "loader#3"],
            "every required position must become an independently fillable solver slot"
        );
        assert!(slots
            .iter()
            .all(|slot| slot.candidate_user_ids == vec!["user-a".to_string()]));
    }

    #[tokio::test]
    async fn normal_apply_validation_rejects_a_missing_expanded_slot_even_when_metadata_claims_complete() {
        let service = service_with_qualifications(vec![grant("user-a", "ops_license", "senior")]);
        let mut crew_requirement = requirement("loader", "senior");
        crew_requirement["required_count"] = serde_json::json!(2);
        let order = draft_order(serde_json::json!([crew_requirement]));
        let mined = service
            .build_slot_candidate_index([&order].into_iter(), &GenerationRuleIndex::default())
            .await;
        let mut snapshot_order = service.build_snapshot_order_base(&order, None, &GenerationRuleIndex::default());
        let assignment = snapshot_order.current_assignment.clone().unwrap_or_default();
        snapshot_order.personnel_slots = service.build_personnel_slots(&snapshot_order, &assignment, &mined);
        let request = DispatchReplanApplyRequest {
            snapshot_id: "snapshot-1".to_string(),
            solver_version: DispatchFrontendReplanService::SOLVER_VERSION.to_string(),
            strategy: "balanced".to_string(),
            suggestions: Vec::new(),
            solver_metadata: HashMap::new(),
            order_results: Vec::new(),
            personnel_slot_assignments: vec![serde_json::json!({
                "dispatch_order_id": order.id,
                "slot_code": "loader#1",
                "user_id": "user-a",
            })],
            equipment_slot_assignments: Vec::new(),
            continuity_decisions: Vec::new(),
            objective_breakdown: HashMap::new(),
            solver_run_metadata: HashMap::new(),
        };
        let metadata = HashMap::from([
            ("feasible".to_string(), serde_json::json!(true)),
            ("plan_complete".to_string(), serde_json::json!(true)),
        ]);

        let result =
            DispatchFrontendReplanService::validate_complete_solver_output(&request, &[snapshot_order], &metadata);

        assert!(
            matches!(result, Err(DomainError::BusinessRuleViolation(message)) if message.contains("loader#2")),
            "the backend must recompute slot coverage instead of trusting plan_complete metadata"
        );
    }

    #[test]
    fn equipment_slots_expand_required_count_and_reject_wrong_types() {
        let service = bare_service();
        let mut order = base_order();
        order.equipment_requirement_snapshot = vec![serde_json::json!({
            "slot_code": "tractor",
            "equipment_type_id": "type-tractor",
            "required_count": 2,
        })];
        let tractor: Equipment = serde_json::from_value(serde_json::json!({
            "id": "tractor-1",
            "code": "TR-1",
            "equipment_type_id": "type-tractor",
            "status": "in_use",
            "current_dispatch_id": order.id,
            "equipment_type": {
                "id": "type-tractor",
                "name": "Tractor",
                "code": "tractor"
            }
        }))
        .expect("valid baseline equipment");
        let bus: Equipment = serde_json::from_value(serde_json::json!({
            "id": "bus-1",
            "code": "BUS-1",
            "equipment_type_id": "type-bus",
            "equipment_type": {
                "id": "type-bus",
                "name": "Bus",
                "code": "bus"
            }
        }))
        .expect("valid available equipment");
        order.equipment_list = vec![tractor];
        let mut snapshot_order = service.build_snapshot_order_base(&order, None, &GenerationRuleIndex::default());
        snapshot_order.candidate_equipments =
            service.build_candidate_equipments(&order, &HashMap::from([(None, vec![bus])]));

        let slots = service.build_equipment_slots(&snapshot_order, &DispatchReplanAssignment::default());

        assert_eq!(
            slots.iter().map(|slot| slot.slot_code.as_str()).collect::<Vec<_>>(),
            vec!["tractor#1", "tractor#2"]
        );
        assert!(slots
            .iter()
            .all(|slot| slot.candidate_equipment_ids == vec!["tractor-1".to_string()]));
    }
}
