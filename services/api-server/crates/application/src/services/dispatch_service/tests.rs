use super::DispatchService;
use crate::test_support::stub_dispatch_dependencies;
use chrono::{TimeZone, Utc};
use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{
    DispatchLockLevel, DispatchOrder, DispatchOrderStatus, DispatchType, ScheduleSource,
};
use fms_domain::ports::dispatch_repository::{CreateDispatchOrderCommand, DispatchOrderRepository};
use serde_json::json;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct RecordingDispatchOrderRepo {
    existing_orders: Mutex<Vec<DispatchOrder>>,
    saved_orders: Mutex<Vec<DispatchOrder>>,
    logs: Mutex<Vec<(String, String, Option<String>, Option<serde_json::Value>)>>,
}

#[async_trait::async_trait]
impl DispatchOrderRepository for RecordingDispatchOrderRepo {
    async fn save(&self, order: &DispatchOrder) -> Result<(), DomainError> {
        self.saved_orders.lock().unwrap().push(order.clone());
        Ok(())
    }

    async fn create_order_atomic(&self, _command: CreateDispatchOrderCommand) -> Result<(), DomainError> {
        unimplemented!()
    }

    async fn save_orders_atomic(&self, _commands: Vec<CreateDispatchOrderCommand>) -> Result<(), DomainError> {
        unimplemented!()
    }

    async fn find_by_id(
        &self,
        _id: &str,
        _load_members: bool,
        _department: Option<&str>,
    ) -> Result<Option<DispatchOrder>, DomainError> {
        unimplemented!()
    }

    async fn find_by_flight(&self, flight_id: &str) -> Result<Vec<DispatchOrder>, DomainError> {
        Ok(self
            .existing_orders
            .lock()
            .unwrap()
            .iter()
            .filter(|order| order.flight_id == flight_id)
            .cloned()
            .collect())
    }

    async fn find_by_flight_with_filters(
        &self,
        _flight_id: &str,
        _status: Option<&str>,
        _source: Option<&str>,
        _department: Option<&str>,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<DispatchOrder>, DomainError> {
        unimplemented!()
    }



    async fn find_by_user(&self, _user_id: &str, _status: Option<&str>) -> Result<Vec<DispatchOrder>, DomainError> {
        unimplemented!()
    }

    async fn find_all(
        &self,
        _status: Option<&str>,
        _department: Option<&str>,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<DispatchOrder>, DomainError> {
        unimplemented!()
    }

    async fn find_all_filtered(
        &self,
        _status: Option<&str>,
        _source: Option<&str>,
        _department: Option<&str>,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<DispatchOrder>, DomainError> {
        unimplemented!()
    }

    async fn find_orders_in_window(
        &self,
        _window_start: chrono::DateTime<Utc>,
        _window_end: chrono::DateTime<Utc>,
        _statuses: &[&str],
        _source: Option<&str>,
        _department: Option<&str>,
        _terminal: Option<&str>,
        _include_cancelled: bool,
    ) -> Result<Vec<DispatchOrder>, DomainError> {
        unimplemented!()
    }

    async fn find_overlapping_orders(
        &self,
        _window_start: chrono::DateTime<Utc>,
        _window_end: chrono::DateTime<Utc>,
        _individual_user_id: Option<&str>,
        _stand_id: Option<&str>,
        _exclude_order_id: Option<&str>,
    ) -> Result<Vec<DispatchOrder>, DomainError> {
        unimplemented!()
    }

    async fn find_equipment_conflicts(
        &self,
        _equipment_ids: &[String],
        _window_start: chrono::DateTime<Utc>,
        _window_end: chrono::DateTime<Utc>,
        _exclude_order_id: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, DomainError> {
        unimplemented!()
    }

    async fn list_logs(&self, _dispatch_order_id: &str, _limit: i64) -> Result<Vec<serde_json::Value>, DomainError> {
        unimplemented!()
    }

    async fn find_pending_for_flight(&self, _flight_id: &str) -> Result<Vec<DispatchOrder>, DomainError> {
        unimplemented!()
    }

    async fn find_publishable_orders(
        &self,
        _as_of: chrono::DateTime<Utc>,
        _limit: i64,
    ) -> Result<Vec<DispatchOrder>, DomainError> {
        unimplemented!()
    }

    async fn update_status(
        &self,
        _id: &str,
        _status: &str,
        _actor_id: Option<&str>,
        _enforce_actor_assignment: bool,
    ) -> Result<bool, DomainError> {
        unimplemented!()
    }

    async fn start_order(
        &self,
        _id: &str,
        _actual_start: chrono::DateTime<Utc>,
        _actor_id: &str,
    ) -> Result<bool, DomainError> {
        unimplemented!()
    }

    async fn complete_order(
        &self,
        _id: &str,
        _actual_end: chrono::DateTime<Utc>,
        _actor_id: &str,
        _notes: Option<&str>,
    ) -> Result<bool, DomainError> {
        unimplemented!()
    }

    async fn append_log(
        &self,
        dispatch_order_id: &str,
        action: &str,
        actor_id: Option<&str>,
        details: Option<serde_json::Value>,
    ) -> Result<(), DomainError> {
        self.logs.lock().unwrap().push((
            dispatch_order_id.to_string(),
            action.to_string(),
            actor_id.map(str::to_string),
            details,
        ));
        Ok(())
    }

    async fn append_log_once(
        &self,
        _dispatch_order_id: &str,
        _action: &str,
        _actor_id: Option<&str>,
        _details: serde_json::Value,
    ) -> Result<bool, DomainError> {
        unimplemented!()
    }

    async fn has_logged_action(
        &self,
        _dispatch_order_id: &str,
        _action: &str,
        _actor_id: Option<&str>,
        _client_action_id: Option<&str>,
    ) -> Result<bool, DomainError> {
        unimplemented!()
    }

    async fn report_estimated_completion(
        &self,
        _id: &str,
        _estimated_time: chrono::DateTime<Utc>,
        _actor_id: &str,
        _note: Option<&str>,
    ) -> Result<bool, DomainError> {
        unimplemented!()
    }

    async fn update_planned_times(
        &self,
        _id: &str,
        _planned_start: chrono::DateTime<Utc>,
        _planned_end: chrono::DateTime<Utc>,
    ) -> Result<bool, DomainError> {
        unimplemented!()
    }

    async fn replace_order_equipment_assignments(
        &self,
        _id: &str,
        _equipment_ids: &[String],
    ) -> Result<(), DomainError> {
        unimplemented!()
    }
}

fn event_generated_order() -> DispatchOrder {
    let now = Utc.with_ymd_and_hms(2026, 5, 12, 8, 0, 0).unwrap();
    DispatchOrder {
        id: "order-event-1".to_string(),
        flight_id: "flight-1".to_string(),
        task_type: "delay_support".to_string(),
        stand_id: Some("stand-1".to_string()),
        task_type_name: None,
        stand_code: None,
        terminal: Some("T1".to_string()),
        department: None,
        individual_user_id: None,
        individual_username: None,
        driver_type: None,
        driver_user_id: None,
        planned_start_time: Some(now),
        planned_end_time: Some(now + chrono::Duration::minutes(30)),
        actual_start_time: None,
        actual_end_time: None,
        estimated_completion_time: None,
        estimated_completion_reported_by: None,
        estimated_completion_reported_at: None,
        estimated_completion_note: None,
        status: DispatchOrderStatus::Pending,
        dispatch_type: DispatchType::Auto,
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
        lock_level: DispatchLockLevel::Active,
        publication_state: "prepublished".to_string(),
        source_type: "event_generated".to_string(),
        department_id: Some("dept-1".to_string()),
        leg_scope: "none".to_string(),
        generation_rule_id: Some("rule-1".to_string()),
        generation_rule_version: Some(1),
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

#[tokio::test]
async fn save_event_generated_order_persists_order_and_records_log() {
    let repo = Arc::new(RecordingDispatchOrderRepo::default());
    // 本测试只用到 order_repo；其余端口是会报错的桩（见 test_support）。
    let mut deps = stub_dispatch_dependencies();
    deps.order.order_repo = repo.clone();
    let service = DispatchService::new(deps);
    let order = event_generated_order();
    let details = json!({
        "event_id": "evt-1",
        "event_type": "flight.status_updated_v2",
        "rule_id": "rule-1",
        "rule_name": "Delay support"
    });

    service
        .save_event_generated_order(&order, details.clone())
        .await
        .expect("save event generated order");

    let saved_orders = repo.saved_orders.lock().unwrap();
    assert_eq!(saved_orders.len(), 1);
    assert_eq!(saved_orders[0].id, "order-event-1");

    let logs = repo.logs.lock().unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].0, "order-event-1");
    assert_eq!(logs[0].1, "event_rule_generated");
    assert_eq!(logs[0].2.as_deref(), Some("system:event-rules"));
    assert_eq!(logs[0].3.as_ref(), Some(&details));
}

#[tokio::test]
async fn save_event_generated_order_once_skips_existing_rule_order() {
    let repo = Arc::new(RecordingDispatchOrderRepo::default());
    // 本测试只用到 order_repo；其余端口是会报错的桩（见 test_support）。
    let mut deps = stub_dispatch_dependencies();
    deps.order.order_repo = repo.clone();
    let service = DispatchService::new(deps);
    let order = event_generated_order();
    let mut existing_order = order.clone();
    existing_order.id = "existing-order-1".to_string();
    repo.existing_orders.lock().unwrap().push(existing_order);

    let saved = service
        .save_event_generated_order_once(
            &order,
            json!({
                "event_id": "evt-duplicate",
                "rule_id": "rule-1",
            }),
        )
        .await
        .expect("save event generated order once");

    assert!(!saved);
    assert!(repo.saved_orders.lock().unwrap().is_empty());
    assert!(repo.logs.lock().unwrap().is_empty());
}

#[test]
fn normalize_optional_ref_trims_and_drops_blank_values() {
    assert_eq!(DispatchService::normalize_optional_ref(None), None);
    assert_eq!(DispatchService::normalize_optional_ref(Some("")), None);
    assert_eq!(DispatchService::normalize_optional_ref(Some("   ")), None);
    assert_eq!(
        DispatchService::normalize_optional_ref(Some("  action-1  ")),
        Some("action-1")
    );
}

#[test]
fn auto_create_checkin_member_only_for_matching_individual_assignee() {
    assert!(DispatchService::should_auto_create_checkin_member(
        Some("user-1"),
        "user-1"
    ));
    assert!(!DispatchService::should_auto_create_checkin_member(
        Some("user-1"),
        "user-2"
    ));
    assert!(!DispatchService::should_auto_create_checkin_member(None, "user-1"));
}

#[test]
fn already_started_response_uses_persisted_start_time_when_available() {
    let persisted = Utc.with_ymd_and_hms(2026, 4, 27, 8, 15, 0).unwrap();
    let fallback = Utc.with_ymd_and_hms(2026, 4, 27, 8, 16, 0).unwrap();

    assert_eq!(
        DispatchService::already_started_response(Some(persisted), fallback),
        json!({
            "success": true,
            "message": "派工单已在执行中",
            "actual_start_time": "2026-04-27T08:15:00+00:00",
            "compat_alias": true,
        })
    );
}

#[test]
fn already_completed_response_marks_request_as_idempotent_without_followup() {
    let fallback = Utc.with_ymd_and_hms(2026, 4, 27, 9, 30, 0).unwrap();

    assert_eq!(
        DispatchService::already_completed_response(None, fallback),
        json!({
            "message": "派工单已完成",
            "actual_end_time": "2026-04-27T09:30:00+00:00",
            "completion_mode": "already_completed",
            "followup_required": false,
            "followup_owner_role": null,
            "followup_todo_id": null,
            "compat_alias": true,
        })
    );
}
