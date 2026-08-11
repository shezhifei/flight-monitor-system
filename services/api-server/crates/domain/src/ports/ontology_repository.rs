//! 本体 V1 仓储接口（ONTOLOGY_V1.md §4）
//!
//! Aircraft / StandOccupation / GateAssignment / TurnaroundLink /
//! ResourceAdjustmentSuggestion 五个聚合的持久化抽象。

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::error::DomainError;
use crate::models::ontology_v1::{
    Aircraft, GateAssignment, ResourceAdjustmentSuggestion, StandOccupation, TurnaroundLink,
};

#[async_trait]
pub trait AircraftRepository: Send + Sync {
    /// 按机号查询（registration 原样）
    async fn find_by_registration(&self, registration: &str) -> Result<Option<Aircraft>, DomainError>;

    /// upsert 飞机（确保存在）
    async fn upsert(&self, aircraft: &Aircraft) -> Result<(), DomainError>;

    /// 触碰 last_seen_at（写入/占用时调用）
    async fn touch(&self, registration: &str) -> Result<(), DomainError>;
}

#[async_trait]
pub trait StandOccupationRepository: Send + Sync {
    /// 新建占用（不变量 3: registration 非空由表约束保证）
    async fn create(&self, occupation: &StandOccupation) -> Result<(), DomainError>;

    /// 调整占用（改时段/机位）
    async fn update(&self, occupation: &StandOccupation) -> Result<(), DomainError>;

    /// 释放占用（status → Released）
    async fn release(&self, id: &str, released_by: &str) -> Result<Option<StandOccupation>, DomainError>;

    /// 按机号取当前 active 占用（时间上仍未结束的最新一条）
    async fn find_active_by_registration(
        &self,
        registration: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<StandOccupation>, DomainError>;

    /// 按航段取 active 占用
    async fn find_active_by_flight(&self, flight_id: &str) -> Result<Vec<StandOccupation>, DomainError>;

    /// 按机号列全部占用（时间倒序，limit）
    async fn list_by_registration(
        &self,
        registration: &str,
        limit: i64,
    ) -> Result<Vec<StandOccupation>, DomainError>;

    /// 按机位号 + 时段列占用（用于冲突告警展示，不硬拦）
    async fn list_overlapping(
        &self,
        stand_code: &str,
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    ) -> Result<Vec<StandOccupation>, DomainError>;

    /// 按机号全部 active 占用（在场判定用）
    async fn list_active_by_registration(
        &self,
        registration: &str,
        now: DateTime<Utc>,
    ) -> Result<Vec<StandOccupation>, DomainError>;
}

#[async_trait]
pub trait GateAssignmentRepository: Send + Sync {
    /// 新建分配（首次分配即生效，§4.5）
    async fn create(&self, assignment: &GateAssignment) -> Result<(), DomainError>;

    /// 调整分配
    async fn update(&self, assignment: &GateAssignment) -> Result<(), DomainError>;

    /// 释放分配
    async fn release(&self, id: &str, released_by: &str) -> Result<Option<GateAssignment>, DomainError>;

    /// 按机号取当前 active 分配
    async fn find_active_by_registration(
        &self,
        registration: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<GateAssignment>, DomainError>;

    /// 按航段取 active 分配
    async fn find_active_by_flight(&self, flight_id: &str) -> Result<Vec<GateAssignment>, DomainError>;

    /// 按机号列分配（时间倒序）
    async fn list_by_registration(&self, registration: &str, limit: i64) -> Result<Vec<GateAssignment>, DomainError>;

    /// 按机号全部 active 分配
    async fn list_active_by_registration(
        &self,
        registration: &str,
        now: DateTime<Utc>,
    ) -> Result<Vec<GateAssignment>, DomainError>;
}

#[async_trait]
pub trait TurnaroundLinkRepository: Send + Sync {
    /// 建链接（自动/手工）
    async fn create(&self, link: &TurnaroundLink) -> Result<(), DomainError>;

    /// 更新链接（拆链/状态变化）
    async fn update(&self, link: &TurnaroundLink) -> Result<(), DomainError>;

    /// 按出港航段取 active 链接
    async fn find_active_by_outbound(&self, outbound_flight_id: &str) -> Result<Option<TurnaroundLink>, DomainError>;

    /// 按进港航段取 active 链接
    async fn find_active_by_inbound(&self, inbound_flight_id: &str) -> Result<Option<TurnaroundLink>, DomainError>;

    /// 按航段（任一端）取全部链接
    async fn list_by_flight(&self, flight_id: &str) -> Result<Vec<TurnaroundLink>, DomainError>;

    /// 候选自动建链：给定出港航段，找同机、进港已落地（实际到达不晚于出港计划前 `window` 分钟）的进港任务
    async fn find_candidates_for_outbound(
        &self,
        registration: &str,
        outbound_flight_id: &str,
        outbound_sched_departure: Option<DateTime<Utc>>,
        window_minutes: i64,
    ) -> Result<Vec<(String, DateTime<Utc>)>, DomainError>;
}

#[async_trait]
pub trait ResourceAdjustmentSuggestionRepository: Send + Sync {
    /// 新建建议
    async fn create(&self, suggestion: &ResourceAdjustmentSuggestion) -> Result<(), DomainError>;

    /// 更新状态（accept/reject/expire）
    async fn update_status(
        &self,
        id: &str,
        status: &str,
        decided_by: Option<&str>,
        decided_at: Option<DateTime<Utc>>,
    ) -> Result<Option<ResourceAdjustmentSuggestion>, DomainError>;

    /// 按航段 + 种类取 pending 建议（连续换机: 旧建议过期 + 新建议生成）
    async fn find_pending(
        &self,
        flight_id: &str,
        kind: &str,
    ) -> Result<Vec<ResourceAdjustmentSuggestion>, DomainError>;

    /// 列表（按航段 / 按状态）
    async fn list(
        &self,
        flight_id: Option<&str>,
        status: Option<&str>,
        limit: i64,
    ) -> Result<Vec<ResourceAdjustmentSuggestion>, DomainError>;

    async fn find_by_id(&self, id: &str) -> Result<Option<ResourceAdjustmentSuggestion>, DomainError>;
}

/// 本体 V1 事务仓储：跨聚合原子写（ReassignAircraft / Suggestion.Accept 等）。
#[async_trait]
pub trait OntologyTransactionalRepository<Tx>: Send + Sync {
    /// 确保飞机存在（不变量 1）
    async fn upsert_aircraft_in_tx(&self, tx: &mut Tx, registration: &str) -> Result<(), DomainError>;

    /// 新建占用
    async fn create_occupation_in_tx(&self, tx: &mut Tx, occupation: &StandOccupation) -> Result<(), DomainError>;

    /// 释放占用
    async fn release_occupation_in_tx(
        &self,
        tx: &mut Tx,
        id: &str,
        released_by: &str,
    ) -> Result<Option<StandOccupation>, DomainError>;

    /// 新建口分配
    async fn create_assignment_in_tx(&self, tx: &mut Tx, assignment: &GateAssignment) -> Result<(), DomainError>;

    /// 释放口分配
    async fn release_assignment_in_tx(
        &self,
        tx: &mut Tx,
        id: &str,
        released_by: &str,
    ) -> Result<Option<GateAssignment>, DomainError>;

    /// 建链接（自动/手工）
    async fn create_link_in_tx(&self, tx: &mut Tx, link: &TurnaroundLink) -> Result<(), DomainError>;

    /// 更新链接（拆链）
    async fn update_link_in_tx(&self, tx: &mut Tx, link: &TurnaroundLink) -> Result<(), DomainError>;

    /// 新建建议
    async fn create_suggestion_in_tx(
        &self,
        tx: &mut Tx,
        suggestion: &ResourceAdjustmentSuggestion,
    ) -> Result<(), DomainError>;

    /// 建议状态更新（accept/reject/expire）
    async fn update_suggestion_status_in_tx(
        &self,
        tx: &mut Tx,
        id: &str,
        status: &str,
        decided_by: Option<&str>,
        decided_at: Option<DateTime<Utc>>,
    ) -> Result<(), DomainError>;

    /// 同一航段 + 种类的旧 pending 建议全部过期（连续换机，§4.9）
    async fn expire_pending_suggestions_in_tx(&self, tx: &mut Tx, flight_id: &str, kind: &str) -> Result<usize, DomainError>;
}
