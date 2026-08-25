//! 航班受控写入方。
//!
//! `FlightService` 的查询/校验/缓存侧保持非泛型；本写入方承接它原先的
//! 数据库事务面，与 `TodoWriter` / `BusinessCaseWriter` / `DispatchOrderWriter`
//! 同形：方法体把 `&mut Tx` 转发给本来就对 `Tx` 泛型的仓储端口，`Tx`
//! 由适配层选定。自开事务的三个入口（create / update / delete 的
//! outbox 同事务写入）通过 `FlightTransactionalWrites` 端口暴露给
//! `FlightService`，事务由 `UnitOfWork` 开启——泛型到此为止，
//! 具体的数据库事务类型不再出现在应用层服务里。

use std::sync::Arc;

use fms_domain::error::DomainError;
use fms_domain::models::flight::Flight;
use fms_domain::ports::domain_event_outbox_repository::DomainEventOutboxTransactionalRepository;
use fms_domain::ports::flight_repository::{FlightRepository, FlightTransactionalRepository, FlightUpdatePatch};
use fms_domain::ports::unit_of_work::UnitOfWork;

use crate::schemas::flight_schemas::{FlightResponse, FlightUpdate};
use crate::services::flight_command_validator;
use crate::services::flight_domain_events::{
    build_created_payload, build_deleted_payload, write_flight_outbox_event, write_flight_update_outbox_events,
    FLIGHT_AGGREGATE_TYPE, FLIGHT_CREATED_EVENT, FLIGHT_DELETED_EVENT,
};
use crate::services::flight_mappers::{to_response, update_patch_from_dto};

pub struct FlightWriter<Tx> {
    repo: Arc<dyn FlightRepository + Send + Sync>,
    tx_repo: Arc<dyn FlightTransactionalRepository<Tx> + Send + Sync>,
    outbox_repo: Arc<dyn DomainEventOutboxTransactionalRepository<Tx> + Send + Sync>,
}

impl<Tx> FlightWriter<Tx> {
    pub fn new(
        repo: Arc<dyn FlightRepository + Send + Sync>,
        tx_repo: Arc<dyn FlightTransactionalRepository<Tx> + Send + Sync>,
        outbox_repo: Arc<dyn DomainEventOutboxTransactionalRepository<Tx> + Send + Sync>,
    ) -> Self {
        Self {
            repo,
            tx_repo,
            outbox_repo,
        }
    }
}

impl<Tx: Send> FlightWriter<Tx> {
    /// `Flight.update_*` 受控动作的写入口：在调用方事务内更新航班并写
    /// 同事务 outbox 事件（调用方可能还写 action 级 outbox 行，一并提交）。
    pub async fn update_flight_in_tx(
        &self,
        tx: &mut Tx,
        flight_id: &str,
        dto: FlightUpdate,
        updated_by: Option<String>,
    ) -> Result<Option<FlightResponse>, DomainError> {
        flight_command_validator::validate_update_payload(&dto)?;
        let patch = update_patch_from_dto(dto)?;
        flight_command_validator::ensure_status_transition(self.repo.as_ref(), flight_id, &patch).await?;
        let Some(flight) = self.tx_repo.update_partial_in_tx(tx, flight_id, &patch).await? else {
            return Ok(None);
        };
        write_flight_update_outbox_events(self.outbox_repo.as_ref(), tx, flight_id, &patch, updated_by.as_deref())
            .await?;
        let mut response = to_response(&flight);
        response.updated_by = updated_by;
        Ok(Some(response))
    }

    pub async fn save_with_created_event(
        &self,
        tx: &mut Tx,
        flight: &Flight,
        created_by: Option<&str>,
    ) -> Result<(), DomainError> {
        self.tx_repo.save_in_tx(tx, flight).await?;
        write_flight_outbox_event(
            self.outbox_repo.as_ref(),
            tx,
            FLIGHT_AGGREGATE_TYPE,
            flight.flight_id.as_str(),
            FLIGHT_CREATED_EVENT,
            build_created_payload(flight.flight_id.as_str(), &flight.status.to_string(), created_by),
        )
        .await
    }

    pub async fn update_partial_with_events(
        &self,
        tx: &mut Tx,
        flight_id: &str,
        patch: &FlightUpdatePatch,
        actor: Option<&str>,
    ) -> Result<Option<Flight>, DomainError> {
        let Some(flight) = self.tx_repo.update_partial_in_tx(tx, flight_id, patch).await? else {
            return Ok(None);
        };
        write_flight_update_outbox_events(self.outbox_repo.as_ref(), tx, flight_id, patch, actor).await?;
        Ok(Some(flight))
    }

    pub async fn delete_with_deleted_event(&self, tx: &mut Tx, flight_id: &str) -> Result<bool, DomainError> {
        let deleted = self.tx_repo.delete_in_tx(tx, flight_id).await?;
        if deleted {
            write_flight_outbox_event(
                self.outbox_repo.as_ref(),
                tx,
                FLIGHT_AGGREGATE_TYPE,
                flight_id,
                FLIGHT_DELETED_EVENT,
                build_deleted_payload(flight_id, None),
            )
            .await?;
        }
        Ok(deleted)
    }
}

/// `FlightService` 的自开事务端口：save/update/delete 连同 outbox 事件
/// 原子提交。具体实现由 `UowFlightWriter` 在装配点给出。
#[async_trait::async_trait]
pub trait FlightTransactionalWrites: Send + Sync {
    async fn save_with_created_event(&self, flight: &Flight, created_by: Option<&str>) -> Result<(), DomainError>;
    async fn update_partial_with_events(
        &self,
        flight_id: &str,
        patch: &FlightUpdatePatch,
        actor: Option<&str>,
    ) -> Result<Option<Flight>, DomainError>;
    async fn delete_with_deleted_event(&self, flight_id: &str) -> Result<bool, DomainError>;
}

/// [`FlightTransactionalWrites`] 的 UnitOfWork 适配器。
///
/// 事务从这里开、从这里提交；`FlightWriter` 本身只认 `&mut Tx`。
pub struct UowFlightWriter<U: UnitOfWork> {
    writer: FlightWriter<U::Tx>,
    uow: Arc<U>,
}

impl<U: UnitOfWork> UowFlightWriter<U> {
    pub fn new(writer: FlightWriter<U::Tx>, uow: Arc<U>) -> Self {
        Self { writer, uow }
    }
}

#[async_trait::async_trait]
impl<U: UnitOfWork> FlightTransactionalWrites for UowFlightWriter<U> {
    async fn save_with_created_event(&self, flight: &Flight, created_by: Option<&str>) -> Result<(), DomainError> {
        let mut tx = self.uow.begin().await?;
        self.writer.save_with_created_event(&mut tx, flight, created_by).await?;
        self.uow.commit(tx).await
    }

    async fn update_partial_with_events(
        &self,
        flight_id: &str,
        patch: &FlightUpdatePatch,
        actor: Option<&str>,
    ) -> Result<Option<Flight>, DomainError> {
        let mut tx = self.uow.begin().await?;
        let flight = self
            .writer
            .update_partial_with_events(&mut tx, flight_id, patch, actor)
            .await?;
        if flight.is_some() {
            self.uow.commit(tx).await?;
        }
        // None（航班不存在）：未写入任何行，事务随 drop 回滚，与原实现一致。
        Ok(flight)
    }

    async fn delete_with_deleted_event(&self, flight_id: &str) -> Result<bool, DomainError> {
        let mut tx = self.uow.begin().await?;
        let deleted = self.writer.delete_with_deleted_event(&mut tx, flight_id).await?;
        self.uow.commit(tx).await?;
        Ok(deleted)
    }
}
