//! 本体服务的受控事务写入方。
//!
//! `OntologyService` 在 api 层有 22 处 `web::Data` 注入，必须保持非泛型；
//! 它的 8 个自开事务（reassign / 建议 / draft 确认 / 机位与登机口的双写
//! 路径）全部下沉到这里。事务由 `UnitOfWork` 开启与提交，`Tx` 只出现在
//! 本模块内部；`OntologyService` 通过对象安全的 [`OntologyTransactions`]
//! 端口调用，泛型到此为止。

use std::sync::Arc;

use chrono::{DateTime, Utc};
use fms_domain::models::ontology_v1::{CarouselAssignment, GateAssignment, ResourceAdjustmentSuggestion, StandOccupation, TurnaroundLink};
use fms_domain::models::ontology_v1_rules::enforce_link_health;
use fms_domain::models::value_objects::{FlightId, GateNumber, StandNumber};
use fms_domain::ports::domain_event_outbox_repository::DomainEventOutboxTransactionalRepository;
use fms_domain::ports::flight_repository::{
    FlightRepository, FlightTransactionalRepository, FlightUpdatePatch, PatchField,
};
use fms_domain::ports::ontology_repository::{
    CarouselCreateOutcome, GateCreateOutcome, OntologyTransactionalRepository, StandCreateOutcome,
    TurnaroundLinkRepository,
};
use fms_domain::ports::unit_of_work::UnitOfWork;
use ulid::Ulid;

use crate::schemas::ontology_schemas::{ReassignAircraftRequest, ReassignAircraftResponse, ReassignAppliedResult};
use crate::services::flight_domain_events::write_flight_update_outbox_events;

use super::error::OntologyError;

/// `OntologyService` 的事务端口：一个方法对应一个事务边界。
/// 具体实现由装配点的 [`OntologyWriter`] 给出。
#[async_trait::async_trait]
pub trait OntologyTransactions: Send + Sync {
    /// §7 ReassignAircraft：同事务内写机号、确保 Aircraft、维护周转链接健康、过期旧建议。
    async fn reassign_aircraft(
        &self,
        request: ReassignAircraftRequest,
        actor_id: &str,
    ) -> Result<ReassignAircraftResponse, OntologyError>;

    /// §4.9 接受建议的事务段：回写 Flight 计划字段、落正式 Occupation/Assignment、
    /// 建议置 accepted_executed。事务外的预校验与后续自动建链留在服务里。
    async fn accept_suggestion_tx(
        &self,
        registration: Option<&str>,
        flight_id: &str,
        patch: FlightUpdatePatch,
        suggestion: &ResourceAdjustmentSuggestion,
        accepted_by: &str,
    ) -> Result<(), OntologyError>;

    /// draft 确认事务段（服务未接线 FlightService 时的回退路径）。
    /// 返回 false 表示航班不存在（事务回滚）。
    async fn confirm_draft_flight_tx(
        &self,
        flight_id: &str,
        patch: FlightUpdatePatch,
        confirmed_by: &str,
    ) -> Result<bool, OntologyError>;

    /// 分配机位占用事务段：确保 Aircraft + 落 Occupation + 可选同步 Flight 计划字段。
    /// 返回幂等结果：`Inserted` 或 `Deduplicated(既有行)`。
    /// `terminal_code` 为该机位所属已启用 `Terminal.code`（PR3 展示列楼推导）。
    async fn allocate_stand_tx(
        &self,
        registration: &str,
        occupation: &StandOccupation,
        sync_flight_plan: bool,
        stand_code: &str,
        terminal_code: &str,
        actor_id: &str,
    ) -> Result<StandCreateOutcome, OntologyError>;

    /// 调整机位占用事务段。`terminal_code` 为该机位所属已启用 `Terminal.code`
    /// （PR3 展示列楼推导，调整机位时同步）。
    async fn adjust_stand_tx(
        &self,
        updated: &StandOccupation,
        sync_flight_plan: bool,
        terminal_code: &str,
        actor_id: &str,
    ) -> Result<(), OntologyError>;

    /// 释放机位占用事务段：release + 同航班展示列清空 `stand`/`terminal`。
    /// 返回被释放的占用。
    async fn release_stand_tx(
        &self,
        occupation_id: &str,
        released_by: &str,
    ) -> Result<StandOccupation, OntologyError>;

    /// 释放登机口事务段：release + 同航班展示列清空 `gate`。返回被释放的分配。
    async fn release_gate_tx(
        &self,
        assignment_id: &str,
        released_by: &str,
    ) -> Result<GateAssignment, OntologyError>;

    /// 分配登机口事务段。返回幂等结果：`Inserted` 或 `Deduplicated(既有行)`。
    async fn allocate_gate_tx(
        &self,
        registration: &str,
        assignment: &GateAssignment,
        sync_flight_plan: bool,
        gate_code: &str,
        actor_id: &str,
    ) -> Result<GateCreateOutcome, OntologyError>;

    /// 调整登机口事务段。
    async fn adjust_gate_tx(
        &self,
        updated: &GateAssignment,
        sync_flight_plan: bool,
        actor_id: &str,
    ) -> Result<(), OntologyError>;

    /// 新建转盘分配事务段。返回幂等结果：`Inserted` 或 `Deduplicated(既有行)`。
    /// 仅 `Inserted` 时回写展示列 `baggage_carousel`（同一事务内重算）。
    async fn allocate_carousel_tx(
        &self,
        assignment: &CarouselAssignment,
        actor_id: &str,
    ) -> Result<CarouselCreateOutcome, OntologyError>;

    /// 调整转盘分配事务段（改转盘/时段）+ 重算展示列。
    async fn adjust_carousel_tx(
        &self,
        updated: &CarouselAssignment,
        actor_id: &str,
    ) -> Result<(), OntologyError>;

    /// 释放转盘分配事务段 + 重算展示列（可能清空）。返回被释放的分配。
    async fn release_carousel_tx(
        &self,
        id: &str,
        released_by: &str,
    ) -> Result<CarouselAssignment, OntologyError>;

    /// 新建建议事务段：旧 pending 过期 + 落新建议。
    async fn create_suggestion_tx(
        &self,
        flight_id: &str,
        kind_str: &str,
        suggestion: &ResourceAdjustmentSuggestion,
    ) -> Result<(), OntologyError>;
}

pub struct OntologyWriter<U: UnitOfWork> {
    flight_repo: Arc<dyn FlightRepository + Send + Sync>,
    link_repo: Arc<dyn TurnaroundLinkRepository + Send + Sync>,
    ontology_tx: Arc<dyn OntologyTransactionalRepository<U::Tx> + Send + Sync>,
    flight_tx: Arc<dyn FlightTransactionalRepository<U::Tx> + Send + Sync>,
    outbox_repo: Arc<dyn DomainEventOutboxTransactionalRepository<U::Tx> + Send + Sync>,
    uow: Arc<U>,
}

impl<U: UnitOfWork> OntologyWriter<U> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        flight_repo: Arc<dyn FlightRepository + Send + Sync>,
        link_repo: Arc<dyn TurnaroundLinkRepository + Send + Sync>,
        ontology_tx: Arc<dyn OntologyTransactionalRepository<U::Tx> + Send + Sync>,
        flight_tx: Arc<dyn FlightTransactionalRepository<U::Tx> + Send + Sync>,
        outbox_repo: Arc<dyn DomainEventOutboxTransactionalRepository<U::Tx> + Send + Sync>,
        uow: Arc<U>,
    ) -> Self {
        Self {
            flight_repo,
            link_repo,
            ontology_tx,
            flight_tx,
            outbox_repo,
            uow,
        }
    }

    /// 在事务内把 Flight 计划机位/登机口/楼与正式资源对齐（含 outbox 事件）。
    async fn sync_flight_plan_field(
        &self,
        tx: &mut U::Tx,
        flight_id: &str,
        stand: Option<PatchField<StandNumber>>,
        gate: Option<PatchField<GateNumber>>,
        terminal: Option<PatchField<String>>,
        actor_id: &str,
    ) -> Result<(), OntologyError> {
        let flight = self
            .flight_repo
            .find_by_id(flight_id)
            .await?
            .ok_or_else(|| OntologyError::not_found(format!("flight {flight_id}")))?;
        let mut patch = FlightUpdatePatch {
            expected_version: Some(flight.version),
            ..FlightUpdatePatch::default()
        };
        if let Some(s) = stand {
            patch.stand = s;
        }
        if let Some(g) = gate {
            patch.gate = g;
        }
        if let Some(t) = terminal {
            patch.terminal = t;
        }
        if !patch.has_any_changes() {
            return Ok(());
        }
        let _ = self
            .flight_tx
            .update_partial_in_tx(tx, flight_id, &patch)
            .await?
            .ok_or_else(|| OntologyError::not_found(format!("flight {flight_id}")))?;
        write_flight_update_outbox_events(self.outbox_repo.as_ref(), tx, flight_id, &patch, Some(actor_id)).await?;
        Ok(())
    }

    /// 同一事务内按该航班所有 active 转盘 code 去重重算展示列 `baggage_carousel`。
    /// 空则清空（`Clear` → NULL）。展示拼接不是约束：多条占用照样都能 allocate。
    async fn sync_carousel_display_in_tx(
        &self,
        tx: &mut U::Tx,
        flight_id: &str,
        actor_id: &str,
    ) -> Result<(), OntologyError> {
        let codes = self
            .ontology_tx
            .list_active_carousel_codes_in_tx(tx, flight_id)
            .await?;
        let flight = self
            .flight_repo
            .find_by_id(flight_id)
            .await?
            .ok_or_else(|| OntologyError::not_found(format!("flight {flight_id}")))?;
        let patch = FlightUpdatePatch {
            expected_version: Some(flight.version),
            baggage_carousel: if codes.is_empty() {
                PatchField::Clear
            } else {
                PatchField::Set(codes.join(", "))
            },
            ..FlightUpdatePatch::default()
        };
        if !patch.has_any_changes() {
            return Ok(());
        }
        let _ = self
            .flight_tx
            .update_partial_in_tx(tx, flight_id, &patch)
            .await?
            .ok_or_else(|| OntologyError::not_found(format!("flight {flight_id}")))?;
        write_flight_update_outbox_events(self.outbox_repo.as_ref(), tx, flight_id, &patch, Some(actor_id)).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl<U: UnitOfWork> OntologyTransactions for OntologyWriter<U> {
    async fn reassign_aircraft(
        &self,
        request: ReassignAircraftRequest,
        actor_id: &str,
    ) -> Result<ReassignAircraftResponse, OntologyError> {
        use fms_domain::models::ontology_v1::{TurnaroundLinkSource, TurnaroundLinkStatus};
        use fms_domain::models::ontology_v1_rules::reassign_gate_violation;

        let mut applied = Vec::with_capacity(request.changes.len());
        let mut tx = self
            .uow
            .begin()
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;

        for change in &request.changes {
            let flight_id = change.flight_id.trim();
            let new_registration = change.new_registration.trim();
            if flight_id.is_empty() {
                return Err(OntologyError::validation("flight_id must not be empty"));
            }
            if new_registration.is_empty() {
                return Err(OntologyError::validation(
                    "new_registration must not be empty (registration is stored as-is)",
                ));
            }

            let flight = self
                .flight_repo
                .find_by_id(flight_id)
                .await?
                .ok_or_else(|| OntologyError::not_found(format!("flight {flight_id}")))?;

            if let Some(reason) = reassign_gate_violation(&flight) {
                return Err(OntologyError::validation(format!(
                    "reassign blocked for {flight_id}: {reason}"
                )));
            }

            let old_registration = flight.registration.clone();
            if old_registration.as_deref() == Some(new_registration) {
                applied.push(ReassignAppliedResult {
                    flight_id: flight_id.to_string(),
                    old_registration,
                    new_registration: new_registration.to_string(),
                    broken_links: vec![],
                    created_links: vec![],
                    suggestions: vec![],
                });
                continue;
            }

            // 1) 确保飞机存在（不变量 1）
            self.ontology_tx
                .upsert_aircraft_in_tx(&mut tx, new_registration)
                .await?;

            // 2) 写航段机号
            let patch = FlightUpdatePatch {
                expected_version: Some(flight.version),
                registration: PatchField::Set(new_registration.to_string()),
                ..FlightUpdatePatch::default()
            };
            let updated = self
                .flight_tx
                .update_partial_in_tx(&mut tx, flight_id, &patch)
                .await?
                .ok_or_else(|| OntologyError::not_found(format!("flight {flight_id}")))?;
            write_flight_update_outbox_events(self.outbox_repo.as_ref(), &mut tx, flight_id, &patch, Some(actor_id))
                .await?;

            // 3) 周转链接健康维护（不变量 4）
            let mut broken_links = Vec::new();
            let mut created_links = Vec::new();
            let links = self.link_repo.list_by_flight(flight_id).await?;
            for link in links {
                if link.status != TurnaroundLinkStatus::Active {
                    continue;
                }
                let inbound = self.flight_repo.find_by_id(&link.inbound_flight_id.0).await?;
                let outbound = self.flight_repo.find_by_id(&link.outbound_flight_id.0).await?;
                // 当前事务尚未对另一端可见时：若 link 端点是本 flight，用 new_registration
                let inbound_reg = if link.inbound_flight_id.0 == flight_id {
                    Some(new_registration)
                } else {
                    inbound.as_ref().and_then(|f| f.registration.as_deref())
                };
                let outbound_reg = if link.outbound_flight_id.0 == flight_id {
                    Some(new_registration)
                } else {
                    outbound.as_ref().and_then(|f| f.registration.as_deref())
                };
                let enforced = enforce_link_health(&link, inbound_reg, outbound_reg);
                if enforced.status == TurnaroundLinkStatus::Broken && link.status == TurnaroundLinkStatus::Active {
                    let mut broken = enforced;
                    broken.updated_at = Utc::now();
                    self.ontology_tx.update_link_in_tx(&mut tx, &broken).await?;
                    broken_links.push(broken.id);
                }
            }

            // 4) 出港侧尝试自动建链（同机 + 时间窗候选）
            if updated.outbound_leg.is_some() {
                let candidates = self
                    .link_repo
                    .find_candidates_for_outbound(new_registration, flight_id, updated.scheduled_departure, 360)
                    .await?;
                if let Some((inbound_id, _)) = candidates.into_iter().next() {
                    let existing = self.link_repo.find_active_by_outbound(flight_id).await?;
                    if existing.is_none() {
                        let now = Utc::now();
                        let link = TurnaroundLink {
                            id: Ulid::new().to_string(),
                            inbound_flight_id: inbound_id.clone().into(),
                            outbound_flight_id: flight_id.to_string().into(),
                            status: TurnaroundLinkStatus::Active,
                            source: TurnaroundLinkSource::Auto,
                            broken_reason: None,
                            created_by: Some(actor_id.to_string()),
                            created_at: now,
                            updated_at: now,
                        };
                        self.ontology_tx.create_link_in_tx(&mut tx, &link).await?;
                        created_links.push(link.id);
                    }
                }
            }

            // 5) 连续换机：旧 pending 建议过期（§4.9）
            self.ontology_tx
                .expire_pending_suggestions_in_tx(&mut tx, flight_id, "stand")
                .await?;
            self.ontology_tx
                .expire_pending_suggestions_in_tx(&mut tx, flight_id, "gate")
                .await?;

            let _ = &updated;
            applied.push(ReassignAppliedResult {
                flight_id: flight_id.to_string(),
                old_registration,
                new_registration: new_registration.to_string(),
                broken_links,
                created_links,
                suggestions: vec![],
            });
        }

        self.uow
            .commit(tx)
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;

        Ok(ReassignAircraftResponse { applied })
    }

    async fn accept_suggestion_tx(
        &self,
        registration: Option<&str>,
        flight_id: &str,
        patch: FlightUpdatePatch,
        suggestion: &ResourceAdjustmentSuggestion,
        accepted_by: &str,
    ) -> Result<(), OntologyError> {
        use fms_domain::models::ontology_v1::{AssignmentStatus, OccupationKind, OccupationStatus, SuggestionKind};

        let mut tx = self
            .uow
            .begin()
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;

        if let Some(reg) = registration {
            self.ontology_tx.upsert_aircraft_in_tx(&mut tx, reg).await?;
        }

        let _updated = self
            .flight_tx
            .update_partial_in_tx(&mut tx, flight_id, &patch)
            .await?
            .ok_or_else(|| OntologyError::not_found(format!("flight {flight_id}")))?;
        write_flight_update_outbox_events(self.outbox_repo.as_ref(), &mut tx, flight_id, &patch, Some(accepted_by))
            .await?;

        // §4.9 接受即执行：若有机号，则落正式 Occupation / Assignment（时段可从 payload 解析）
        if let Some(reg) = registration {
            let (starts_at, ends_at) = suggestion_time_window(&suggestion.payload);
            let now = Utc::now();
            match suggestion.kind {
                SuggestionKind::Stand => {
                    let occupation = StandOccupation {
                        id: Ulid::new().to_string(),
                        registration: reg.to_string(),
                        stand_code: StandNumber(suggestion.suggested_value.clone()),
                        starts_at,
                        ends_at,
                        kind: OccupationKind::Normal,
                        moving_to_stand: None,
                        flight_id: Some(FlightId(flight_id.to_string())),
                        status: OccupationStatus::Active,
                        client_action_id: None,
                        created_by: Some(accepted_by.to_string()),
                        created_at: now,
                        updated_at: now,
                    };
                    self.ontology_tx.create_occupation_in_tx(&mut tx, &occupation).await?;
                }
                SuggestionKind::Gate => {
                    let assignment = GateAssignment {
                        id: Ulid::new().to_string(),
                        registration: reg.to_string(),
                        gate_code: GateNumber(suggestion.suggested_value.clone()),
                        starts_at,
                        ends_at,
                        flight_id: FlightId(flight_id.to_string()),
                        status: AssignmentStatus::Active,
                        client_action_id: None,
                        created_by: Some(accepted_by.to_string()),
                        created_at: now,
                        updated_at: now,
                    };
                    self.ontology_tx.create_assignment_in_tx(&mut tx, &assignment).await?;
                }
            }
        }

        self.ontology_tx
            .update_suggestion_status_in_tx(
                &mut tx,
                &suggestion.id,
                "accepted_executed",
                Some(accepted_by),
                Some(Utc::now()),
            )
            .await?;

        self.uow
            .commit(tx)
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;
        Ok(())
    }

    async fn confirm_draft_flight_tx(
        &self,
        flight_id: &str,
        patch: FlightUpdatePatch,
        confirmed_by: &str,
    ) -> Result<bool, OntologyError> {
        let mut tx = self
            .uow
            .begin()
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;
        let Some(_) = self.flight_tx.update_partial_in_tx(&mut tx, flight_id, &patch).await? else {
            // 未写入任何行，事务随 drop 回滚，与原实现一致。
            return Ok(false);
        };
        write_flight_update_outbox_events(
            self.outbox_repo.as_ref(),
            &mut tx,
            flight_id,
            &patch,
            Some(confirmed_by),
        )
        .await?;
        self.uow
            .commit(tx)
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;
        Ok(true)
    }

    async fn allocate_stand_tx(
        &self,
        registration: &str,
        occupation: &StandOccupation,
        sync_flight_plan: bool,
        stand_code: &str,
        terminal_code: &str,
        actor_id: &str,
    ) -> Result<StandCreateOutcome, OntologyError> {
        let mut tx = self
            .uow
            .begin()
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;
        self.ontology_tx.upsert_aircraft_in_tx(&mut tx, registration).await?;
        let outcome = self.ontology_tx.create_occupation_in_tx(&mut tx, occupation).await?;

        if matches!(outcome, StandCreateOutcome::Inserted) && sync_flight_plan {
            if let Some(flight_id) = occupation.flight_id.as_ref() {
                self.sync_flight_plan_field(
                    &mut tx,
                    &flight_id.0,
                    Some(PatchField::Set(StandNumber(stand_code.to_string()))),
                    None,
                    Some(PatchField::Set(terminal_code.to_string())),
                    actor_id,
                )
                .await?;
            }
        }

        self.uow
            .commit(tx)
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;
        Ok(outcome)
    }

    async fn adjust_stand_tx(
        &self,
        updated: &StandOccupation,
        sync_flight_plan: bool,
        terminal_code: &str,
        actor_id: &str,
    ) -> Result<(), OntologyError> {
        let mut tx = self
            .uow
            .begin()
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;
        self.ontology_tx.update_occupation_in_tx(&mut tx, updated).await?;
        if sync_flight_plan {
            if let Some(flight_id) = updated.flight_id.as_ref() {
                self.sync_flight_plan_field(
                    &mut tx,
                    &flight_id.0,
                    Some(PatchField::Set(StandNumber(updated.stand_code.0.clone()))),
                    None,
                    Some(PatchField::Set(terminal_code.to_string())),
                    actor_id,
                )
                .await?;
            }
        }
        self.uow
            .commit(tx)
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;
        Ok(())
    }

    async fn release_stand_tx(
        &self,
        occupation_id: &str,
        released_by: &str,
    ) -> Result<StandOccupation, OntologyError> {
        let mut tx = self
            .uow
            .begin()
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;
        let occupation = self
            .ontology_tx
            .release_occupation_in_tx(&mut tx, occupation_id, released_by)
            .await?
            .ok_or_else(|| OntologyError::not_found(format!("active occupation {occupation_id}")))?;
        if let Some(flight_id) = occupation.flight_id.as_ref() {
            self.sync_flight_plan_field(
                &mut tx,
                &flight_id.0,
                Some(PatchField::Clear),
                None,
                Some(PatchField::Clear),
                released_by,
            )
            .await?;
        }
        self.uow
            .commit(tx)
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;
        Ok(occupation)
    }

    async fn release_gate_tx(
        &self,
        assignment_id: &str,
        released_by: &str,
    ) -> Result<GateAssignment, OntologyError> {
        let mut tx = self
            .uow
            .begin()
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;
        let assignment = self
            .ontology_tx
            .release_assignment_in_tx(&mut tx, assignment_id, released_by)
            .await?
            .ok_or_else(|| OntologyError::not_found(format!("active assignment {assignment_id}")))?;
        // flight_id 必填（PR3）：release 后清空该航班的 gate 展示列
        self.sync_flight_plan_field(
            &mut tx,
            &assignment.flight_id.0,
            None,
            Some(PatchField::Clear),
            None,
            released_by,
        )
        .await?;
        self.uow
            .commit(tx)
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;
        Ok(assignment)
    }

    async fn allocate_gate_tx(
        &self,
        registration: &str,
        assignment: &GateAssignment,
        sync_flight_plan: bool,
        gate_code: &str,
        actor_id: &str,
    ) -> Result<GateCreateOutcome, OntologyError> {
        let mut tx = self
            .uow
            .begin()
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;
        self.ontology_tx.upsert_aircraft_in_tx(&mut tx, registration).await?;
        let outcome = self.ontology_tx.create_assignment_in_tx(&mut tx, assignment).await?;

        if matches!(outcome, GateCreateOutcome::Inserted) && sync_flight_plan {
            // flight_id 必填（PR3）：allocate 后回写该航班的 gate 展示列
            self.sync_flight_plan_field(
                &mut tx,
                &assignment.flight_id.0,
                None,
                Some(PatchField::Set(GateNumber(gate_code.to_string()))),
                None,
                actor_id,
            )
            .await?;
        }

        self.uow
            .commit(tx)
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;
        Ok(outcome)
    }

    async fn adjust_gate_tx(
        &self,
        updated: &GateAssignment,
        sync_flight_plan: bool,
        actor_id: &str,
    ) -> Result<(), OntologyError> {
        let mut tx = self
            .uow
            .begin()
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;
        self.ontology_tx.update_assignment_in_tx(&mut tx, updated).await?;
        if sync_flight_plan {
            self.sync_flight_plan_field(
                &mut tx,
                &updated.flight_id.0,
                None,
                Some(PatchField::Set(GateNumber(updated.gate_code.0.clone()))),
                None,
                actor_id,
            )
            .await?;
        }
        self.uow
            .commit(tx)
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;
        Ok(())
    }

    async fn allocate_carousel_tx(
        &self,
        assignment: &CarouselAssignment,
        actor_id: &str,
    ) -> Result<CarouselCreateOutcome, OntologyError> {
        let mut tx = self
            .uow
            .begin()
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;
        let outcome = self.ontology_tx.create_carousel_in_tx(&mut tx, assignment).await?;
        if matches!(outcome, CarouselCreateOutcome::Inserted) {
            if let Some(flight_id) = &assignment.flight_id {
                self.sync_carousel_display_in_tx(&mut tx, &flight_id.0, actor_id).await?;
            }
        }
        self.uow
            .commit(tx)
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;
        Ok(outcome)
    }

    async fn adjust_carousel_tx(
        &self,
        updated: &CarouselAssignment,
        actor_id: &str,
    ) -> Result<(), OntologyError> {
        let mut tx = self
            .uow
            .begin()
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;
        self.ontology_tx.update_carousel_in_tx(&mut tx, updated).await?;
        if let Some(flight_id) = &updated.flight_id {
            self.sync_carousel_display_in_tx(&mut tx, &flight_id.0, actor_id).await?;
        }
        self.uow
            .commit(tx)
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;
        Ok(())
    }

    async fn release_carousel_tx(
        &self,
        id: &str,
        released_by: &str,
    ) -> Result<CarouselAssignment, OntologyError> {
        let mut tx = self
            .uow
            .begin()
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;
        let assignment = self
            .ontology_tx
            .release_carousel_in_tx(&mut tx, id, released_by)
            .await?
            .ok_or_else(|| OntologyError::not_found(format!("active carousel assignment {id}")))?;
        if let Some(flight_id) = &assignment.flight_id {
            self.sync_carousel_display_in_tx(&mut tx, &flight_id.0, released_by).await?;
        }
        self.uow
            .commit(tx)
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;
        Ok(assignment)
    }

    async fn create_suggestion_tx(
        &self,
        flight_id: &str,
        kind_str: &str,
        suggestion: &ResourceAdjustmentSuggestion,
    ) -> Result<(), OntologyError> {
        let mut tx = self
            .uow
            .begin()
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;
        self.ontology_tx
            .expire_pending_suggestions_in_tx(&mut tx, flight_id, kind_str)
            .await?;
        self.ontology_tx.create_suggestion_in_tx(&mut tx, suggestion).await?;
        self.uow
            .commit(tx)
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;
        Ok(())
    }
}

/// payload 可选 `starts_at` / `ends_at`（RFC3339）；缺省 now → now+2h。
fn suggestion_time_window(payload: &serde_json::Value) -> (DateTime<Utc>, DateTime<Utc>) {
    let now = Utc::now();
    let starts = payload
        .get("starts_at")
        .and_then(|v| v.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or(now);
    let ends = payload
        .get("ends_at")
        .and_then(|v| v.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or(starts + chrono::Duration::hours(2));
    if ends <= starts {
        (starts, starts + chrono::Duration::hours(2))
    } else {
        (starts, ends)
    }
}
