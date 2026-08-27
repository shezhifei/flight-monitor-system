//! Stub repositories for the replan snapshot tests.
//!
//! `DispatchFrontendReplanService` reaches five ports to build a snapshot. Every
//! method here that a test does not exercise is left `unimplemented!()` on
//! purpose: if a change starts calling a port the tests never intended to
//! reach, the test panics with the method name instead of silently passing on
//! an empty default.

use chrono::{DateTime, Utc};

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{
    DepartmentQualificationCatalog, DepartmentQualificationLevel, DispatchOrder, DispatchOrderMember,
    QualificationGrant, QualificationGrantStatus,
};
use fms_domain::ports::dispatch_repository::{
    CreateDispatchOrderCommand, DepartmentQualificationRepository, DispatchOrderMemberRepository,
    DispatchOrderRepository, QualificationGrantRepository,
};

#[derive(Default)]
pub(super) struct StubOrderRepo {
    pub orders_in_window: Vec<DispatchOrder>,
}

#[async_trait::async_trait]
impl DispatchOrderRepository for StubOrderRepo {
    async fn find_orders_in_window(
        &self,
        _window_start: DateTime<Utc>,
        _window_end: DateTime<Utc>,
        _statuses: &[&str],
        _source: Option<&str>,
        _department: Option<&str>,
        _terminal: Option<&str>,
        _include_cancelled: bool,
    ) -> Result<Vec<DispatchOrder>, DomainError> {
        Ok(self.orders_in_window.clone())
    }

    async fn save(&self, _order: &DispatchOrder) -> Result<(), DomainError> {
        unimplemented!("save")
    }
    async fn create_order_atomic(&self, _command: CreateDispatchOrderCommand) -> Result<(), DomainError> {
        unimplemented!("create_order_atomic")
    }
    async fn save_orders_atomic(&self, _commands: Vec<CreateDispatchOrderCommand>) -> Result<(), DomainError> {
        unimplemented!("save_orders_atomic")
    }
    async fn find_by_id(
        &self,
        _id: &str,
        _load_members: bool,
        _department: Option<&str>,
    ) -> Result<Option<DispatchOrder>, DomainError> {
        unimplemented!("find_by_id")
    }
    async fn find_by_flight(&self, _flight_id: &str) -> Result<Vec<DispatchOrder>, DomainError> {
        unimplemented!("find_by_flight")
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
        unimplemented!("find_by_flight_with_filters")
    }
    async fn find_by_user(&self, _user_id: &str, _status: Option<&str>) -> Result<Vec<DispatchOrder>, DomainError> {
        unimplemented!("find_by_user")
    }
    async fn find_all(
        &self,
        _status: Option<&str>,
        _department: Option<&str>,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<DispatchOrder>, DomainError> {
        unimplemented!("find_all")
    }
    async fn find_all_filtered(
        &self,
        _status: Option<&str>,
        _source: Option<&str>,
        _department: Option<&str>,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<DispatchOrder>, DomainError> {
        unimplemented!("find_all_filtered")
    }
    async fn find_overlapping_orders(
        &self,
        _window_start: DateTime<Utc>,
        _window_end: DateTime<Utc>,
        _individual_user_id: Option<&str>,
        _stand_id: Option<&str>,
        _exclude_order_id: Option<&str>,
    ) -> Result<Vec<DispatchOrder>, DomainError> {
        unimplemented!("find_overlapping_orders")
    }
    async fn find_equipment_conflicts(
        &self,
        _equipment_ids: &[String],
        _window_start: DateTime<Utc>,
        _window_end: DateTime<Utc>,
        _exclude_order_id: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, DomainError> {
        unimplemented!("find_equipment_conflicts")
    }
    async fn list_logs(&self, _dispatch_order_id: &str, _limit: i64) -> Result<Vec<serde_json::Value>, DomainError> {
        unimplemented!("list_logs")
    }
    async fn find_pending_for_flight(&self, _flight_id: &str) -> Result<Vec<DispatchOrder>, DomainError> {
        unimplemented!("find_pending_for_flight")
    }
    async fn find_publishable_orders(
        &self,
        _as_of: DateTime<Utc>,
        _limit: i64,
    ) -> Result<Vec<DispatchOrder>, DomainError> {
        unimplemented!("find_publishable_orders")
    }
    async fn update_status(
        &self,
        _id: &str,
        _status: &str,
        _actor_id: Option<&str>,
        _enforce_actor_assignment: bool,
    ) -> Result<bool, DomainError> {
        unimplemented!("update_status")
    }
    async fn start_order(&self, _id: &str, _actual_start: DateTime<Utc>, _actor_id: &str) -> Result<bool, DomainError> {
        unimplemented!("start_order")
    }
    async fn complete_order(
        &self,
        _id: &str,
        _actual_end: DateTime<Utc>,
        _actor_id: &str,
        _notes: Option<&str>,
    ) -> Result<bool, DomainError> {
        unimplemented!("complete_order")
    }
    async fn append_log(
        &self,
        _dispatch_order_id: &str,
        _action: &str,
        _actor_id: Option<&str>,
        _details: Option<serde_json::Value>,
    ) -> Result<(), DomainError> {
        unimplemented!("append_log")
    }
    async fn append_log_once(
        &self,
        _dispatch_order_id: &str,
        _action: &str,
        _actor_id: Option<&str>,
        _details: serde_json::Value,
    ) -> Result<bool, DomainError> {
        unimplemented!("append_log_once")
    }
    async fn has_logged_action(
        &self,
        _dispatch_order_id: &str,
        _action: &str,
        _actor_id: Option<&str>,
        _client_action_id: Option<&str>,
    ) -> Result<bool, DomainError> {
        unimplemented!("has_logged_action")
    }
    async fn report_estimated_completion(
        &self,
        _id: &str,
        _estimated_time: DateTime<Utc>,
        _actor_id: &str,
        _note: Option<&str>,
    ) -> Result<bool, DomainError> {
        unimplemented!("report_estimated_completion")
    }
    async fn update_planned_times(
        &self,
        _id: &str,
        _planned_start: DateTime<Utc>,
        _planned_end: DateTime<Utc>,
    ) -> Result<bool, DomainError> {
        unimplemented!("update_planned_times")
    }
    async fn replace_order_equipment_assignments(
        &self,
        _id: &str,
        _equipment_ids: &[String],
    ) -> Result<(), DomainError> {
        unimplemented!("replace_order_equipment_assignments")
    }
}
#[derive(Default)]
pub(super) struct StubOrderMemberRepo;

#[async_trait::async_trait]
impl DispatchOrderMemberRepository for StubOrderMemberRepo {
    async fn save(&self, _member: &DispatchOrderMember) -> Result<(), DomainError> {
        unimplemented!("save")
    }
    async fn find_by_order(&self, _order_id: &str) -> Result<Vec<DispatchOrderMember>, DomainError> {
        Ok(Vec::new())
    }
    async fn find_by_order_and_user(
        &self,
        _order_id: &str,
        _user_id: &str,
    ) -> Result<Option<DispatchOrderMember>, DomainError> {
        unimplemented!("find_by_order_and_user")
    }
    async fn find_latest_checkout_for_user(
        &self,
        _user_id: &str,
        _before: DateTime<Utc>,
    ) -> Result<Option<serde_json::Value>, DomainError> {
        unimplemented!("find_latest_checkout_for_user")
    }
}

/// Qualification levels, keyed by nothing: every department gets the same
/// ladder. Coverage is what the miner consults to decide whether a grant
/// satisfies a slot's `min_level_code`.
#[derive(Default)]
pub(super) struct StubQualificationRepo {
    pub levels: Vec<DepartmentQualificationLevel>,
}

#[async_trait::async_trait]
impl DepartmentQualificationRepository for StubQualificationRepo {
    async fn list_levels(
        &self,
        _department_id: &str,
        _qualification_code: Option<&str>,
        _include_inactive: bool,
    ) -> Result<Vec<DepartmentQualificationLevel>, DomainError> {
        Ok(self.levels.clone())
    }
    async fn save_catalog(
        &self,
        _catalog: &DepartmentQualificationCatalog,
    ) -> Result<DepartmentQualificationCatalog, DomainError> {
        unimplemented!("save_catalog")
    }
    async fn list_catalogs(
        &self,
        _department_id: &str,
        _include_inactive: bool,
    ) -> Result<Vec<DepartmentQualificationCatalog>, DomainError> {
        unimplemented!("list_catalogs")
    }
    async fn save_level(
        &self,
        _level: &DepartmentQualificationLevel,
    ) -> Result<DepartmentQualificationLevel, DomainError> {
        unimplemented!("save_level")
    }
}

#[derive(Default)]
pub(super) struct StubQualificationGrantRepo {
    pub grants: Vec<QualificationGrant>,
    /// `true` makes every lookup fail, so tests can assert the snapshot
    /// degrades instead of erroring out.
    pub fail: bool,
}

#[async_trait::async_trait]
impl QualificationGrantRepository for StubQualificationGrantRepo {
    async fn find_by_department(
        &self,
        department_id: &str,
        _at_time: Option<DateTime<Utc>>,
        user_ids: &[String],
        _include_inactive: bool,
    ) -> Result<Vec<QualificationGrant>, DomainError> {
        if self.fail {
            return Err(DomainError::Internal("qualification store unavailable".to_string()));
        }
        // Mirrors the production SQL: an empty `user_ids` adds no IN clause, so
        // the whole department comes back.
        Ok(self
            .grants
            .iter()
            .filter(|grant| grant.department_id == department_id)
            .filter(|grant| user_ids.is_empty() || user_ids.contains(&grant.user_id))
            .cloned()
            .collect())
    }
    async fn save(&self, _grant: &QualificationGrant) -> Result<QualificationGrant, DomainError> {
        unimplemented!("save")
    }
}

pub(super) fn level(level_code: &str, covered: &[&str]) -> DepartmentQualificationLevel {
    DepartmentQualificationLevel {
        id: format!("level-{level_code}"),
        department_id: "dept-1".to_string(),
        qualification_code: "ops_license".to_string(),
        level_code: level_code.to_string(),
        level_name: level_code.to_string(),
        level_rank: 0,
        covered_level_codes: covered.iter().map(|item| item.to_string()).collect(),
        is_active: true,
        created_at: None,
        updated_at: None,
    }
}

pub(super) fn grant(user_id: &str, qualification_code: &str, level_code: &str) -> QualificationGrant {
    QualificationGrant {
        id: format!("grant-{user_id}-{qualification_code}-{level_code}"),
        user_id: user_id.to_string(),
        department_id: "dept-1".to_string(),
        qualification_code: qualification_code.to_string(),
        level_code: level_code.to_string(),
        valid_from: None,
        valid_to: None,
        status: QualificationGrantStatus::Active,
        source_team_id: None,
        metadata: std::collections::HashMap::new(),
        created_at: None,
        updated_at: None,
    }
}
