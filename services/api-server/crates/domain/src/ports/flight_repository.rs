//! 航班仓储 trait
//!
//! 对应 Python `FlightRepository` 及 `AsyncFlightRepository`。

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};

use crate::error::DomainError;
use crate::models::flight::Flight;
use crate::models::flight_leg::FlightLeg;
use crate::models::value_objects::{AircraftType, FlightStatus, GateNumber, StandNumber};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchField<T> {
    Unset,
    Clear,
    Set(T),
}

impl<T> Default for PatchField<T> {
    fn default() -> Self {
        Self::Unset
    }
}

impl<T> PatchField<T> {
    pub fn is_touched(&self) -> bool {
        !matches!(self, Self::Unset)
    }

    pub fn as_ref(&self) -> PatchField<&T> {
        match self {
            Self::Unset => PatchField::Unset,
            Self::Clear => PatchField::Clear,
            Self::Set(value) => PatchField::Set(value),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FlightSearchCriteria {
    pub flight_no: Option<String>,
    pub status: Option<String>,
    pub origin: Option<String>,
    pub destination: Option<String>,
    pub has_open_anomaly: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct FlightUpdatePatch {
    pub expected_version: Option<i32>,
    pub status: Option<FlightStatus>,
    pub gate: PatchField<GateNumber>,
    pub terminal: PatchField<String>,
    pub stand: PatchField<StandNumber>,
    pub position: PatchField<String>,
    pub baggage_carousel: PatchField<String>,
    pub scheduled_departure: PatchField<DateTime<Utc>>,
    pub scheduled_arrival: PatchField<DateTime<Utc>>,
    pub estimated_departure: PatchField<DateTime<Utc>>,
    pub estimated_arrival: PatchField<DateTime<Utc>>,
    pub actual_departure: PatchField<DateTime<Utc>>,
    pub actual_arrival: PatchField<DateTime<Utc>>,
    pub cobt_time: PatchField<DateTime<Utc>>,
    pub aircraft_type_detail: PatchField<AircraftType>,
    pub registration: PatchField<String>,
    pub has_boarding_restriction: Option<bool>,
    pub is_quick_turnaround: Option<bool>,
    pub is_commercial_signed: Option<bool>,
    pub inbound_leg: PatchField<FlightLeg>,
    pub outbound_leg: PatchField<FlightLeg>,
    pub flight_remarks: PatchField<String>,
    pub load_planning_remarks: PatchField<String>,
    pub aircraft_maintenance_remarks: PatchField<String>,
    pub aircraft_check_remarks: PatchField<String>,
}

impl FlightUpdatePatch {
    pub fn has_main_table_changes(&self) -> bool {
        self.status.is_some()
            || self.gate.is_touched()
            || self.terminal.is_touched()
            || self.stand.is_touched()
            || self.position.is_touched()
            || self.baggage_carousel.is_touched()
            || self.scheduled_departure.is_touched()
            || self.scheduled_arrival.is_touched()
            || self.estimated_departure.is_touched()
            || self.estimated_arrival.is_touched()
            || self.actual_departure.is_touched()
            || self.actual_arrival.is_touched()
            || self.cobt_time.is_touched()
            || self.aircraft_type_detail.is_touched()
            || self.registration.is_touched()
            || self.has_boarding_restriction.is_some()
            || self.is_quick_turnaround.is_some()
            || self.is_commercial_signed.is_some()
            || self.flight_remarks.is_touched()
            || self.load_planning_remarks.is_touched()
            || self.aircraft_maintenance_remarks.is_touched()
            || self.aircraft_check_remarks.is_touched()
    }

    pub fn has_any_changes(&self) -> bool {
        self.has_main_table_changes() || self.inbound_leg.is_touched() || self.outbound_leg.is_touched()
    }
}

/// 航班仓储接口 — 由 infrastructure 层实现
#[async_trait]
pub trait FlightRepository {
    /// 根据 flight_id 查询航班
    async fn find_by_id(&self, flight_id: &str) -> Result<Option<Flight>, DomainError>;

    /// 分页查询航班列表
    async fn find_all(&self, limit: i64, offset: i64) -> Result<Vec<Flight>, DomainError>;

    /// 按日期查询航班
    async fn find_by_date(&self, date: NaiveDate) -> Result<Vec<Flight>, DomainError>;

    /// 按航班号查询
    async fn find_by_flight_number(&self, flight_no: &str) -> Result<Vec<Flight>, DomainError>;

    /// 按状态查询
    async fn find_by_status(&self, status: i32, limit: i64, offset: i64) -> Result<Vec<Flight>, DomainError>;

    /// 保存航班 (upsert)
    async fn save(&self, flight: &Flight) -> Result<(), DomainError>;

    /// 字段级更新航班，避免并发时用旧快照覆盖未提交字段。
    async fn update_partial(&self, flight_id: &str, patch: &FlightUpdatePatch) -> Result<Option<Flight>, DomainError>;

    /// 批量保存
    async fn save_batch(&self, flights: &[Flight]) -> Result<usize, DomainError>;

    /// 更新航班状态
    async fn update_status(&self, flight_id: &str, status: i32) -> Result<bool, DomainError>;

    /// 删除航班
    async fn delete(&self, flight_id: &str) -> Result<bool, DomainError>;

    /// 条件搜索航班
    async fn search(
        &self,
        criteria: &FlightSearchCriteria,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Flight>, DomainError>;

    /// 按日期统计航班数量
    async fn count_by_date(&self, date: NaiveDate) -> Result<i64, DomainError>;
}

#[async_trait]
pub trait FlightTransactionalRepository<Tx>: Send + Sync {
    /// 在外部事务中 upsert 航班（与 outbox 等同事务写入）。
    async fn save_in_tx(&self, tx: &mut Tx, flight: &Flight) -> Result<(), DomainError>;

    /// 字段级更新航班（使用外部事务）。
    async fn update_partial_in_tx(
        &self,
        tx: &mut Tx,
        flight_id: &str,
        patch: &FlightUpdatePatch,
    ) -> Result<Option<Flight>, DomainError>;

    /// 在外部事务中删除航班（与 outbox 等同事务写入）。
    async fn delete_in_tx(&self, tx: &mut Tx, flight_id: &str) -> Result<bool, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    proptest::proptest! {
        #[test]
        fn patch_field_unset_is_never_touched(_val in proptest::option::of(0i32..100)) {
            let field: PatchField<i32> = PatchField::Unset;
            assert!(!field.is_touched());
        }

        #[test]
        fn patch_field_clear_is_always_touched(_val in 0i32..100) {
            let field: PatchField<i32> = PatchField::Clear;
            assert!(field.is_touched());
        }

        #[test]
        fn patch_field_set_is_always_touched(val in 0i32..100) {
            let field: PatchField<i32> = PatchField::Set(val);
            assert!(field.is_touched());
        }

        #[test]
        fn patch_field_as_ref_preserves_variant(val in proptest::option::of(0i32..100)) {
            let field: PatchField<i32> = match val {
                Some(v) => PatchField::Set(v),
                None => PatchField::Unset,
            };
            let ref_field = field.as_ref();
            assert_eq!(field.is_touched(), ref_field.is_touched());
        }
    }
}
