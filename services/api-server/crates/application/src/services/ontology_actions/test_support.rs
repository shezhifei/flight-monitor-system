use async_trait::async_trait;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde_json::Value;

use fms_domain::error::DomainError;
use fms_domain::models::anomaly::{Anomaly, AnomalySeverity, AnomalyStatus, AnomalyType};
use fms_domain::models::business_case::FlightBusinessCase;
use fms_domain::models::dispatch::{DispatchOrder, Equipment, PersonnelRuntime, QualificationGrant, Stand, Team};
use fms_domain::models::flight::Flight;
use fms_domain::models::ontology_v1::{OccupationKind, OccupationStatus, StandOccupation};
use fms_domain::models::user::User;
use fms_domain::models::value_objects::FlightStatus;
use fms_domain::ports::anomaly_repository::AnomalyRepository;
use fms_domain::ports::business_case_repository::BusinessCaseRepository;
use fms_domain::ports::dispatch_repository::{
    CreateDispatchOrderCommand, DispatchOrderRepository, EquipmentRepository, PersonnelRuntimeRepository,
    QualificationGrantRepository, StandRepository, TeamRepository,
};
use fms_domain::ports::flight_repository::{FlightRepository, FlightSearchCriteria};
use fms_domain::ports::ontology_repository::StandOccupationRepository;
use fms_domain::ports::user_repository::UserRepository;

// ── fake repositories（未用到的方法统一 unimplemented!）──

#[derive(Default)]
pub(crate) struct FakeFlightRepo {
    pub(crate) flights: std::sync::Mutex<Vec<Flight>>,
}

#[async_trait]
impl FlightRepository for FakeFlightRepo {
    async fn find_by_id(&self, flight_id: &str) -> Result<Option<Flight>, DomainError> {
        Ok(self
            .flights
            .lock()
            .unwrap()
            .iter()
            .find(|f| f.flight_id.as_str() == flight_id)
            .cloned())
    }
    async fn find_all(&self, limit: i64, _offset: i64) -> Result<Vec<Flight>, DomainError> {
        Ok(self
            .flights
            .lock()
            .unwrap()
            .iter()
            .take(limit as usize)
            .cloned()
            .collect())
    }
    async fn find_by_date(&self, _date: NaiveDate) -> Result<Vec<Flight>, DomainError> {
        Ok(self.flights.lock().unwrap().clone())
    }
    async fn find_by_flight_number(&self, _flight_no: &str) -> Result<Vec<Flight>, DomainError> {
        unimplemented!()
    }
    async fn find_by_status(&self, _status: i32, _limit: i64, _offset: i64) -> Result<Vec<Flight>, DomainError> {
        unimplemented!()
    }
    async fn save(&self, _flight: &Flight) -> Result<(), DomainError> {
        unimplemented!()
    }
    async fn update_partial(
        &self,
        _flight_id: &str,
        _patch: &fms_domain::ports::flight_repository::FlightUpdatePatch,
    ) -> Result<Option<Flight>, DomainError> {
        unimplemented!()
    }
    async fn save_batch(&self, _flights: &[Flight]) -> Result<usize, DomainError> {
        unimplemented!()
    }
    async fn update_status(&self, _flight_id: &str, _status: i32) -> Result<bool, DomainError> {
        unimplemented!()
    }
    async fn delete(&self, _flight_id: &str) -> Result<bool, DomainError> {
        unimplemented!()
    }
    async fn search(
        &self,
        _criteria: &FlightSearchCriteria,
        limit: i64,
        _offset: i64,
    ) -> Result<Vec<Flight>, DomainError> {
        Ok(self
            .flights
            .lock()
            .unwrap()
            .iter()
            .take(limit as usize)
            .cloned()
            .collect())
    }
    async fn count_by_date(&self, _date: NaiveDate) -> Result<i64, DomainError> {
        unimplemented!()
    }
}

#[derive(Default)]
pub(crate) struct FakeDispatchRepo {
    pub(crate) orders: std::sync::Mutex<Vec<DispatchOrder>>,
}

#[async_trait]
impl DispatchOrderRepository for FakeDispatchRepo {
    async fn save(&self, _order: &DispatchOrder) -> Result<(), DomainError> {
        unimplemented!()
    }
    async fn create_order_atomic(&self, _command: CreateDispatchOrderCommand) -> Result<(), DomainError> {
        unimplemented!()
    }
    async fn save_orders_atomic(&self, _commands: Vec<CreateDispatchOrderCommand>) -> Result<(), DomainError> {
        unimplemented!()
    }
    async fn find_by_id(
        &self,
        id: &str,
        _load_members: bool,
        _department: Option<&str>,
    ) -> Result<Option<DispatchOrder>, DomainError> {
        Ok(self.orders.lock().unwrap().iter().find(|o| o.id == id).cloned())
    }
    async fn find_by_flight(&self, flight_id: &str) -> Result<Vec<DispatchOrder>, DomainError> {
        Ok(self
            .orders
            .lock()
            .unwrap()
            .iter()
            .filter(|o| o.flight_id == flight_id)
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
        _window_start: DateTime<Utc>,
        _window_end: DateTime<Utc>,
        _statuses: &[&str],
        _source: Option<&str>,
        _department: Option<&str>,
        _terminal: Option<&str>,
        _include_cancelled: bool,
    ) -> Result<Vec<DispatchOrder>, DomainError> {
        Ok(self.orders.lock().unwrap().clone())
    }
    async fn find_overlapping_orders(
        &self,
        _window_start: DateTime<Utc>,
        _window_end: DateTime<Utc>,
        _individual_user_id: Option<&str>,
        _stand_id: Option<&str>,
        exclude_order_id: Option<&str>,
    ) -> Result<Vec<DispatchOrder>, DomainError> {
        Ok(self
            .orders
            .lock()
            .unwrap()
            .iter()
            .filter(|o| {
                let user_ok =
                    _individual_user_id.is_none_or(|user_id| o.individual_user_id.as_deref() == Some(user_id));
                user_ok && exclude_order_id.is_none_or(|excluded| o.id != excluded)
            })
            .cloned()
            .collect())
    }
    async fn find_equipment_conflicts(
        &self,
        _equipment_ids: &[String],
        _window_start: DateTime<Utc>,
        _window_end: DateTime<Utc>,
        _exclude_order_id: Option<&str>,
    ) -> Result<Vec<Value>, DomainError> {
        unimplemented!()
    }
    async fn list_logs(&self, _dispatch_order_id: &str, _limit: i64) -> Result<Vec<Value>, DomainError> {
        unimplemented!()
    }
    async fn find_pending_for_flight(&self, _flight_id: &str) -> Result<Vec<DispatchOrder>, DomainError> {
        unimplemented!()
    }
    async fn find_publishable_orders(
        &self,
        _as_of: DateTime<Utc>,
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
    async fn start_order(&self, _id: &str, _actual_start: DateTime<Utc>, _actor_id: &str) -> Result<bool, DomainError> {
        unimplemented!()
    }
    async fn complete_order(
        &self,
        _id: &str,
        _actual_end: DateTime<Utc>,
        _actor_id: &str,
        _notes: Option<&str>,
    ) -> Result<bool, DomainError> {
        unimplemented!()
    }
    async fn append_log(
        &self,
        _dispatch_order_id: &str,
        _action: &str,
        _actor_id: Option<&str>,
        _details: Option<Value>,
    ) -> Result<(), DomainError> {
        unimplemented!()
    }
    async fn append_log_once(
        &self,
        _dispatch_order_id: &str,
        _action: &str,
        _actor_id: Option<&str>,
        _details: Value,
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
        _estimated_time: DateTime<Utc>,
        _actor_id: &str,
        _note: Option<&str>,
    ) -> Result<bool, DomainError> {
        unimplemented!()
    }
    async fn update_planned_times(
        &self,
        _id: &str,
        _planned_start: DateTime<Utc>,
        _planned_end: DateTime<Utc>,
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

#[derive(Default)]
pub(crate) struct FakeAnomalyRepo {
    pub(crate) anomalies: std::sync::Mutex<Vec<Anomaly>>,
}

#[async_trait]
impl AnomalyRepository for FakeAnomalyRepo {
    async fn find_by_id(&self, anomaly_id: &str) -> Result<Option<Anomaly>, DomainError> {
        Ok(self
            .anomalies
            .lock()
            .unwrap()
            .iter()
            .find(|a| a.anomaly_id == anomaly_id)
            .cloned())
    }
    async fn find_by_flight(&self, flight_id: &str) -> Result<Vec<Anomaly>, DomainError> {
        Ok(self
            .anomalies
            .lock()
            .unwrap()
            .iter()
            .filter(|a| a.flight_id == flight_id)
            .cloned()
            .collect())
    }
    async fn find_by_status(&self, status: AnomalyStatus) -> Result<Vec<Anomaly>, DomainError> {
        Ok(self
            .anomalies
            .lock()
            .unwrap()
            .iter()
            .filter(|a| a.status == status)
            .cloned()
            .collect())
    }
    async fn list_rules(
        &self,
        _enabled_only: bool,
    ) -> Result<Vec<fms_domain::models::anomaly::AnomalyRule>, DomainError> {
        unimplemented!()
    }
    async fn get_rule(&self, _rule_id: &str) -> Result<Option<fms_domain::models::anomaly::AnomalyRule>, DomainError> {
        unimplemented!()
    }
    async fn upsert_rule(
        &self,
        _rule: &fms_domain::models::anomaly::AnomalyRule,
    ) -> Result<fms_domain::models::anomaly::AnomalyRule, DomainError> {
        unimplemented!()
    }
    async fn save(&self, _anomaly: &Anomaly) -> Result<(), DomainError> {
        unimplemented!()
    }
    async fn update(&self, _anomaly: &Anomaly) -> Result<bool, DomainError> {
        unimplemented!()
    }
    async fn acknowledge(&self, _anomaly_id: &str) -> Result<bool, DomainError> {
        unimplemented!()
    }
    async fn resolve(&self, _anomaly_id: &str) -> Result<bool, DomainError> {
        unimplemented!()
    }
    async fn escalate(&self, _anomaly_id: &str) -> Result<bool, DomainError> {
        unimplemented!()
    }
}

#[derive(Default)]
pub(crate) struct FakeTeamRepo {
    pub(crate) teams: std::sync::Mutex<Vec<Team>>,
}

#[async_trait]
impl TeamRepository for FakeTeamRepo {
    async fn save(&self, _team: &Team) -> Result<Team, DomainError> {
        unimplemented!()
    }
    async fn find_by_id(&self, id: &str, _load_members: bool) -> Result<Option<Team>, DomainError> {
        Ok(self.teams.lock().unwrap().iter().find(|t| t.id == id).cloned())
    }
    async fn find_by_code(&self, _code: &str) -> Result<Option<Team>, DomainError> {
        unimplemented!()
    }
    async fn find_available_for_dispatch(
        &self,
        _team_type_id: Option<&str>,
        _terminal: Option<&str>,
    ) -> Result<Vec<Team>, DomainError> {
        Ok(self
            .teams
            .lock()
            .unwrap()
            .iter()
            .filter(|t| t.is_active)
            .cloned()
            .collect())
    }
    async fn find_all(
        &self,
        _include_inactive: bool,
        _team_type_id: Option<&str>,
        _terminal: Option<&str>,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<Team>, DomainError> {
        unimplemented!()
    }
    async fn update_position(
        &self,
        _id: &str,
        _lat: f64,
        _lng: f64,
        _stand_id: Option<&str>,
    ) -> Result<bool, DomainError> {
        unimplemented!()
    }
    async fn update_status(&self, _id: &str, _status: &str) -> Result<bool, DomainError> {
        unimplemented!()
    }
}

#[derive(Default)]
pub(crate) struct FakeEquipmentRepo {
    pub(crate) equipment: std::sync::Mutex<Vec<Equipment>>,
}

#[async_trait]
impl EquipmentRepository for FakeEquipmentRepo {
    async fn save(&self, _equipment: &Equipment) -> Result<Equipment, DomainError> {
        unimplemented!()
    }
    async fn find_by_id(&self, id: &str) -> Result<Option<Equipment>, DomainError> {
        Ok(self.equipment.lock().unwrap().iter().find(|e| e.id == id).cloned())
    }
    async fn find_by_code(&self, code: &str) -> Result<Option<Equipment>, DomainError> {
        Ok(self.equipment.lock().unwrap().iter().find(|e| e.code == code).cloned())
    }
    async fn find_available_for_dispatch(
        &self,
        _equipment_type_id: Option<&str>,
        _terminal: Option<&str>,
    ) -> Result<Vec<Equipment>, DomainError> {
        Ok(self
            .equipment
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.is_active)
            .cloned()
            .collect())
    }
    async fn find_all(
        &self,
        include_inactive: bool,
        _equipment_type_id: Option<&str>,
        _terminal: Option<&str>,
        _status: Option<&str>,
        limit: i64,
        _offset: i64,
    ) -> Result<Vec<Equipment>, DomainError> {
        Ok(self
            .equipment
            .lock()
            .unwrap()
            .iter()
            .filter(|e| include_inactive || e.is_active)
            .take(limit as usize)
            .cloned()
            .collect())
    }
    async fn update_position(
        &self,
        _id: &str,
        _lat: f64,
        _lng: f64,
        _stand_id: Option<&str>,
    ) -> Result<bool, DomainError> {
        unimplemented!()
    }
    async fn update_status(&self, _id: &str, _status: &str) -> Result<bool, DomainError> {
        unimplemented!()
    }
}

#[derive(Default)]
pub(crate) struct FakeStandRepo {
    pub(crate) stands: std::sync::Mutex<Vec<Stand>>,
}

#[async_trait]
impl StandRepository for FakeStandRepo {
    async fn save(&self, _stand: &Stand) -> Result<Stand, DomainError> {
        unimplemented!()
    }
    async fn find_by_id(&self, id: &str) -> Result<Option<Stand>, DomainError> {
        Ok(self.stands.lock().unwrap().iter().find(|s| s.id == id).cloned())
    }
    async fn find_by_code(&self, code: &str) -> Result<Option<Stand>, DomainError> {
        Ok(self.stands.lock().unwrap().iter().find(|s| s.code == code).cloned())
    }
    async fn find_all(
        &self,
        _terminal: Option<&str>,
        include_inactive: bool,
        limit: i64,
        _offset: i64,
    ) -> Result<Vec<Stand>, DomainError> {
        Ok(self
            .stands
            .lock()
            .unwrap()
            .iter()
            .filter(|s| include_inactive || s.is_active)
            .take(limit as usize)
            .cloned()
            .collect())
    }
    async fn is_active(&self, _id_or_code: &str) -> Result<bool, DomainError> {
        unimplemented!()
    }
}

#[derive(Default)]
pub(crate) struct FakeOccupationRepo {
    pub(crate) occupations: std::sync::Mutex<Vec<StandOccupation>>,
}

#[async_trait]
impl StandOccupationRepository for FakeOccupationRepo {
    async fn find_by_id(&self, _id: &str) -> Result<Option<StandOccupation>, DomainError> {
        unimplemented!()
    }
    async fn create(&self, _occupation: &StandOccupation) -> Result<(), DomainError> {
        unimplemented!()
    }
    async fn update(&self, _occupation: &StandOccupation) -> Result<(), DomainError> {
        unimplemented!()
    }
    async fn release(&self, _id: &str, _released_by: &str) -> Result<Option<StandOccupation>, DomainError> {
        unimplemented!()
    }
    async fn find_active_by_registration(
        &self,
        _registration: &str,
        _now: DateTime<Utc>,
    ) -> Result<Option<StandOccupation>, DomainError> {
        unimplemented!()
    }
    async fn find_active_by_flight(&self, _flight_id: &str) -> Result<Vec<StandOccupation>, DomainError> {
        unimplemented!()
    }
    async fn list_by_registration(
        &self,
        _registration: &str,
        _limit: i64,
    ) -> Result<Vec<StandOccupation>, DomainError> {
        unimplemented!()
    }
    async fn list_overlapping(
        &self,
        stand_code: &str,
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    ) -> Result<Vec<StandOccupation>, DomainError> {
        Ok(self
            .occupations
            .lock()
            .unwrap()
            .iter()
            .filter(|o| o.stand_code.as_str() == stand_code && o.starts_at < ends_at && o.ends_at > starts_at)
            .cloned()
            .collect())
    }
    async fn list_active_by_registration(
        &self,
        _registration: &str,
        _now: DateTime<Utc>,
    ) -> Result<Vec<StandOccupation>, DomainError> {
        unimplemented!()
    }
}

#[derive(Default)]
pub(crate) struct FakeBusinessCaseRepo;

#[async_trait]
impl BusinessCaseRepository for FakeBusinessCaseRepo {
    async fn save(&self, _case: &FlightBusinessCase) -> Result<(), DomainError> {
        unimplemented!()
    }
    async fn find_by_id(&self, _case_id: &str) -> Result<Option<FlightBusinessCase>, DomainError> {
        unimplemented!()
    }
    async fn find_by_id_scoped(
        &self,
        _case_id: &str,
        _viewer_department_id: Option<&str>,
        _viewer_department_name: Option<&str>,
        _include_common: bool,
    ) -> Result<Option<FlightBusinessCase>, DomainError> {
        unimplemented!()
    }
    async fn find_by_flight(&self, _flight_id: &str) -> Result<Vec<FlightBusinessCase>, DomainError> {
        Ok(vec![])
    }
    async fn find_by_flight_scoped(
        &self,
        _flight_id: &str,
        _viewer_department_id: Option<&str>,
        _viewer_department_name: Option<&str>,
        _include_common: bool,
    ) -> Result<Vec<FlightBusinessCase>, DomainError> {
        unimplemented!()
    }
    async fn find_by_flight_ids(&self, _flight_ids: &[String]) -> Result<Vec<FlightBusinessCase>, DomainError> {
        unimplemented!()
    }
    async fn find_by_copilot_batch_action(
        &self,
        _batch_id: &str,
        _action_id: &str,
    ) -> Result<Option<FlightBusinessCase>, DomainError> {
        unimplemented!()
    }
    async fn list_by_copilot_batch(&self, _batch_id: &str) -> Result<Vec<FlightBusinessCase>, DomainError> {
        unimplemented!()
    }
    async fn find_by_flight_ids_scoped(
        &self,
        _flight_ids: &[String],
        _viewer_department_id: Option<&str>,
        _viewer_department_name: Option<&str>,
        _include_common: bool,
    ) -> Result<Vec<FlightBusinessCase>, DomainError> {
        unimplemented!()
    }
    async fn find_all(
        &self,
        _status: Option<&str>,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<FlightBusinessCase>, DomainError> {
        unimplemented!()
    }
    async fn find_all_scoped(
        &self,
        _status: Option<&str>,
        _limit: i64,
        _offset: i64,
        _viewer_department_id: Option<&str>,
        _viewer_department_name: Option<&str>,
        _include_common: bool,
    ) -> Result<Vec<FlightBusinessCase>, DomainError> {
        unimplemented!()
    }
    async fn find_filtered(
        &self,
        _flight_id: Option<&str>,
        _case_type: Option<&str>,
        _status: Option<&str>,
        _limit: Option<i64>,
        _offset: Option<i64>,
    ) -> Result<Vec<FlightBusinessCase>, DomainError> {
        unimplemented!()
    }
    async fn find_filtered_scoped(
        &self,
        _flight_id: Option<&str>,
        _case_type: Option<&str>,
        _status: Option<&str>,
        _viewer_department_id: Option<&str>,
        _viewer_department_name: Option<&str>,
        _include_common: bool,
        _limit: Option<i64>,
        _offset: Option<i64>,
    ) -> Result<Vec<FlightBusinessCase>, DomainError> {
        unimplemented!()
    }
    async fn update_case(&self, _case: &FlightBusinessCase) -> Result<bool, DomainError> {
        unimplemented!()
    }
    async fn update_status(&self, _case_id: &str, _status: &str, _actor: &str) -> Result<bool, DomainError> {
        unimplemented!()
    }
    async fn insert_append(
        &self,
        _append: &fms_domain::models::business_case::BusinessCaseAppendEntry,
    ) -> Result<fms_domain::models::business_case::BusinessCaseAppendEntry, DomainError> {
        unimplemented!()
    }
    async fn insert_append_once(
        &self,
        _append: &fms_domain::models::business_case::BusinessCaseAppendEntry,
    ) -> Result<(fms_domain::models::business_case::BusinessCaseAppendEntry, bool), DomainError> {
        unimplemented!()
    }
    async fn find_append_by_id(
        &self,
        _append_id: &str,
    ) -> Result<Option<fms_domain::models::business_case::BusinessCaseAppendEntry>, DomainError> {
        unimplemented!()
    }
    async fn update_append_metadata(&self, _append_id: &str, _metadata: Value) -> Result<bool, DomainError> {
        unimplemented!()
    }
    async fn delete(&self, _case_id: &str) -> Result<bool, DomainError> {
        unimplemented!()
    }
}

#[derive(Default)]
pub(crate) struct FakeUserRepo {
    pub(crate) users: std::sync::Mutex<Vec<User>>,
}

#[async_trait]
impl UserRepository for FakeUserRepo {
    async fn find_by_id(&self, id: &str) -> Result<Option<User>, DomainError> {
        Ok(self.users.lock().unwrap().iter().find(|u| u.id == id).cloned())
    }
    async fn find_permission_version_by_id(&self, _id: &str) -> Result<Option<i32>, DomainError> {
        unimplemented!()
    }
    async fn find_by_username(&self, _username: &str) -> Result<Option<User>, DomainError> {
        unimplemented!()
    }
    async fn find_by_email(&self, _email: &str) -> Result<Option<User>, DomainError> {
        unimplemented!()
    }
    async fn find_all(&self, _limit: i64, _offset: i64) -> Result<Vec<User>, DomainError> {
        unimplemented!()
    }
    async fn list_distinct_departments_in_use(&self) -> Result<Vec<String>, DomainError> {
        unimplemented!()
    }
    async fn has_any_user_with_department_id(&self, _department_id: &str) -> Result<bool, DomainError> {
        unimplemented!()
    }
    async fn save(&self, _user: &User) -> Result<(), DomainError> {
        unimplemented!()
    }
    async fn update(&self, _user: &User) -> Result<bool, DomainError> {
        unimplemented!()
    }
    async fn delete(&self, _id: &str) -> Result<bool, DomainError> {
        unimplemented!()
    }
    async fn update_password(&self, _id: &str, _password_hash: &str) -> Result<bool, DomainError> {
        unimplemented!()
    }
    async fn update_last_login(&self, _id: &str) -> Result<bool, DomainError> {
        unimplemented!()
    }
}

#[derive(Default)]
pub(crate) struct FakePersonnelRuntimeRepo {
    pub(crate) runtimes: std::sync::Mutex<Vec<PersonnelRuntime>>,
}

#[async_trait]
impl PersonnelRuntimeRepository for FakePersonnelRuntimeRepo {
    async fn save(&self, runtime: &PersonnelRuntime) -> Result<PersonnelRuntime, DomainError> {
        Ok(runtime.clone())
    }
    async fn find_by_user(&self, user_id: &str) -> Result<Option<PersonnelRuntime>, DomainError> {
        Ok(self
            .runtimes
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.user_id == user_id)
            .cloned())
    }
    async fn update_status(
        &self,
        _user_id: &str,
        _status: &str,
        _updated_by: Option<&str>,
    ) -> Result<bool, DomainError> {
        unimplemented!()
    }
    async fn update_position(
        &self,
        _user_id: &str,
        _lat: f64,
        _lng: f64,
        _stand_id: Option<&str>,
    ) -> Result<bool, DomainError> {
        unimplemented!()
    }
}

#[derive(Default)]
pub(crate) struct FakeQualificationRepo {
    pub(crate) grants: std::sync::Mutex<Vec<QualificationGrant>>,
}

#[async_trait]
impl QualificationGrantRepository for FakeQualificationRepo {
    async fn save(&self, grant: &QualificationGrant) -> Result<QualificationGrant, DomainError> {
        Ok(grant.clone())
    }
    async fn find_by_department(
        &self,
        _department_id: &str,
        _at_time: Option<chrono::DateTime<chrono::Utc>>,
        user_ids: &[String],
        _include_inactive: bool,
    ) -> Result<Vec<QualificationGrant>, DomainError> {
        Ok(self
            .grants
            .lock()
            .unwrap()
            .iter()
            .filter(|g| user_ids.iter().any(|id| id == &g.user_id))
            .cloned()
            .collect())
    }
}

// ── fixtures ──

pub(crate) fn flight_fixture(id: &str, status: FlightStatus, inbound: bool, outbound: bool) -> Flight {
    let leg = fms_domain::models::flight_leg::FlightLeg {
        leg_type: fms_domain::models::flight_leg::LegType::Inbound,
        flight_no: "CA1234".to_string(),
        flight_type: fms_domain::models::flight_leg::FlightTypeCode::Domestic,
        mission: None,
        origin_code: Some("PEK".to_string()),
        origin_name: None,
        destination_code: Some("SHA".to_string()),
        destination_name: None,
        is_vip: false,
        stand_type: None,
        scheduled_time: Some(Utc::now()),
    };
    Flight {
        flight_id: id.into(),
        airline_code: Some("CA".to_string()),
        flight_number: Some("CA1234".into()),
        registration: Some("B-1234".to_string()),
        aircraft_type_detail: None,
        stand: None,
        gate: None,
        terminal: None,
        position: None,
        baggage_carousel: None,
        scheduled_departure: Some(Utc::now()),
        scheduled_arrival: Some(Utc::now()),
        estimated_departure: None,
        estimated_arrival: None,
        actual_departure: None,
        actual_arrival: None,
        cobt_time: None,
        codt: None,
        has_boarding_restriction: false,
        is_quick_turnaround: false,
        is_commercial_signed: true,
        status,
        inbound_leg: if inbound { Some(leg.clone()) } else { None },
        outbound_leg: if outbound {
            Some(fms_domain::models::flight_leg::FlightLeg {
                leg_type: fms_domain::models::flight_leg::LegType::Outbound,
                ..leg
            })
        } else {
            None
        },
        anomaly_summary: Default::default(),
        direction: None,
        flight_kind: "passenger".to_string(),
        is_draft: false,
        divert: false,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        version: 1,
        labels: vec!["vip".to_string()],
        flight_remarks: None,
        load_planning_remarks: None,
        aircraft_maintenance_remarks: None,
        aircraft_check_remarks: None,
    }
}

pub(crate) fn anomaly_fixture(
    id: &str,
    flight_id: &str,
    severity: AnomalySeverity,
    status: AnomalyStatus,
    minutes_ago: i64,
) -> Anomaly {
    let now = Utc::now();
    Anomaly {
        anomaly_id: id.to_string(),
        subject_type: "Flight".to_string(),
        subject_id: flight_id.to_string(),
        flight_id: flight_id.to_string(),
        anomaly_type: AnomalyType::DispatchIssue,
        severity,
        title: format!("anomaly {id}"),
        description: None,
        status,
        detected_at: now - Duration::minutes(minutes_ago),
        resolved_at: None,
        escalation_level: 0,
        last_escalated_at: None,
        linked_todo_id: None,
        rule_id: None,
        context_data: Default::default(),
        created_at: now,
        updated_at: now,
    }
}

pub(crate) fn order_fixture(id: &str, flight_id: &str, status: &str, source_team_id: Option<&str>) -> DispatchOrder {
    let mut raw = serde_json::json!({
        "id": id,
        "flight_id": flight_id,
        "task_type": "baggage_unload",
        "status": status,
        "planned_start_time": Utc::now() + Duration::minutes(30),
        "members": [],
    });
    if let Some(source_team_id) = source_team_id {
        raw["members"] = serde_json::json!([{
            "id": format!("{id}-m1"),
            "dispatch_order_id": id,
            "user_id": "user-1",
            "role": "member",
            "source_type": "team",
            "source_team_id": source_team_id,
            "is_active": true,
        }]);
    }
    serde_json::from_value(raw).expect("dispatch order fixture")
}

pub(crate) fn stand_fixture(id: &str, code: &str, active: bool) -> Stand {
    Stand {
        id: id.to_string(),
        code: code.to_string(),
        name: None,
        terminal: None,
        area: None,
        position_lat: 0.0,
        position_lng: 0.0,
        stand_type: None,
        size_category: None,
        is_active: active,
        attributes: serde_json::json!({}),
        created_at: None,
    }
}

pub(crate) fn occupation_fixture(
    stand_code: &str,
    registration: &str,
    start_offset_mins: i64,
    end_offset_mins: i64,
) -> StandOccupation {
    let now = Utc::now();
    StandOccupation {
        id: format!("occ-{stand_code}"),
        registration: registration.to_string(),
        stand_code: stand_code.into(),
        starts_at: now + Duration::minutes(start_offset_mins),
        ends_at: now + Duration::minutes(end_offset_mins),
        kind: OccupationKind::Normal,
        moving_to_stand: None,
        flight_id: Some("FL_OTHER".into()),
        status: OccupationStatus::Active,
        client_action_id: None,
        created_by: None,
        created_at: now,
        updated_at: now,
    }
}
