//! OntologyService 实现。

use std::sync::Arc;

use chrono::Utc;
use fms_domain::error::DomainError;
use fms_domain::models::ontology_v1::{
    ResourceAdjustmentSuggestion, SuggestionKind, SuggestionStatus, TurnaroundLink, TurnaroundLinkSource,
    TurnaroundLinkStatus,
};
use fms_domain::models::ontology_v1_rules::{
    accept_permission_for, draft_can_be_occupied, enforce_link_health, is_ground_blacklisted_action,
    reassign_gate_violation,
};
use fms_domain::models::value_objects::{GateNumber, StandNumber};
use fms_domain::ports::flight_repository::{FlightRepository, FlightUpdatePatch, PatchField};
use fms_domain::ports::ontology_repository::{
    AircraftRepository, GateAssignmentRepository, ResourceAdjustmentSuggestionRepository,
    StandOccupationRepository, TurnaroundLinkRepository,
};
use sqlx::PgPool;
use ulid::Ulid;

use crate::schemas::ontology_schemas::{
    AircraftResourceView, ConfirmDraftFlightsRequest, ConfirmDraftFlightsResponse, FlightResourceView,
    ReassignAircraftRequest, ReassignAircraftResponse, ReassignAppliedResult, SuggestionAcceptRequest,
    SuggestionQuery, SuggestionRejectRequest,
};
use crate::services::flight_domain_events::write_flight_update_outbox_events;
use crate::services::flight_service::FlightService;
use crate::sqlx_transactional_repositories::{SqlxFlightTransactionalRepository, SqlxOntologyTransactionalRepository};

use super::error::OntologyError;

/// 本体 V1 应用服务。
pub struct OntologyService {
    pool: PgPool,
    flight_repo: Arc<dyn FlightRepository + Send + Sync>,
    flight_tx: Arc<dyn SqlxFlightTransactionalRepository>,
    aircraft_repo: Arc<dyn AircraftRepository + Send + Sync>,
    occupation_repo: Arc<dyn StandOccupationRepository + Send + Sync>,
    assignment_repo: Arc<dyn GateAssignmentRepository + Send + Sync>,
    link_repo: Arc<dyn TurnaroundLinkRepository + Send + Sync>,
    suggestion_repo: Arc<dyn ResourceAdjustmentSuggestionRepository + Send + Sync>,
    ontology_tx: Arc<dyn SqlxOntologyTransactionalRepository>,
    /// 可选：复用 FlightService 的 draft 确认语义。
    flight_service: Option<Arc<FlightService>>,
}

impl OntologyService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pool: PgPool,
        flight_repo: Arc<dyn FlightRepository + Send + Sync>,
        flight_tx: Arc<dyn SqlxFlightTransactionalRepository>,
        aircraft_repo: Arc<dyn AircraftRepository + Send + Sync>,
        occupation_repo: Arc<dyn StandOccupationRepository + Send + Sync>,
        assignment_repo: Arc<dyn GateAssignmentRepository + Send + Sync>,
        link_repo: Arc<dyn TurnaroundLinkRepository + Send + Sync>,
        suggestion_repo: Arc<dyn ResourceAdjustmentSuggestionRepository + Send + Sync>,
        ontology_tx: Arc<dyn SqlxOntologyTransactionalRepository>,
    ) -> Self {
        Self {
            pool,
            flight_repo,
            flight_tx,
            aircraft_repo,
            occupation_repo,
            assignment_repo,
            link_repo,
            suggestion_repo,
            ontology_tx,
            flight_service: None,
        }
    }

    pub fn with_flight_service(mut self, flight_service: Arc<FlightService>) -> Self {
        self.flight_service = Some(flight_service);
        self
    }

    /// §7 ReassignAircraft：批量变更执行机号。
    ///
    /// - 闸门：`reassign_gate_violation`（不变量 6）
    /// - 地服黑名单：`is_ground_blacklisted_action`（不变量 10；路由层还应校 permission）
    /// - 同事务：写机号、确保 Aircraft、拆/维持周转链接健康、过期旧建议
    pub async fn reassign_aircraft(
        &self,
        request: ReassignAircraftRequest,
        actor_id: &str,
        actor_permissions: &[String],
        actor_is_admin: bool,
    ) -> Result<ReassignAircraftResponse, OntologyError> {
        if !actor_is_admin
            && !actor_permissions
                .iter()
                .any(|p| p == "ontology.aircraft.reassign" || p == "*")
        {
            return Err(OntologyError::forbidden(
                "missing permission ontology.aircraft.reassign",
            ));
        }
        // 地服黑名单双保险：无 reassign 权限者不可改机（admin 已在上方放行）
        if !actor_is_admin {
            self.ensure_not_ground_blacklisted(actor_permissions, "Flight", "ReassignAircraft")?;
        }

        if request.changes.is_empty() {
            return Err(OntologyError::validation("changes must not be empty"));
        }

        let mut applied = Vec::with_capacity(request.changes.len());
        let mut tx = self
            .pool
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
            write_flight_update_outbox_events(&mut tx, flight_id, &patch, Some(actor_id)).await?;

            // 3) 周转链接健康维护（不变量 4）
            let mut broken_links = Vec::new();
            let mut created_links = Vec::new();
            let links = self.link_repo.list_by_flight(flight_id).await?;
            for link in links {
                if link.status != TurnaroundLinkStatus::Active {
                    continue;
                }
                let inbound = self
                    .flight_repo
                    .find_by_id(&link.inbound_flight_id.0)
                    .await?;
                let outbound = self
                    .flight_repo
                    .find_by_id(&link.outbound_flight_id.0)
                    .await?;
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
                if enforced.status == TurnaroundLinkStatus::Broken
                    && link.status == TurnaroundLinkStatus::Active
                {
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
                    .find_candidates_for_outbound(
                        new_registration,
                        flight_id,
                        updated.scheduled_departure,
                        360,
                    )
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

        tx.commit()
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;

        Ok(ReassignAircraftResponse { applied })
    }

    /// 接受资源调整建议（§4.9）：权限匹配 → 回写 Flight 计划字段 → accepted_executed。
    pub async fn accept_suggestion(
        &self,
        suggestion_id: &str,
        request: SuggestionAcceptRequest,
    ) -> Result<ResourceAdjustmentSuggestion, OntologyError> {
        let suggestion = self
            .suggestion_repo
            .find_by_id(suggestion_id)
            .await?
            .ok_or_else(|| OntologyError::not_found(format!("suggestion {suggestion_id}")))?;

        if !matches!(suggestion.status, SuggestionStatus::Pending) {
            return Err(OntologyError::conflict(format!(
                "suggestion {suggestion_id} is not pending"
            )));
        }
        if suggestion.is_expired() {
            let _ = self
                .suggestion_repo
                .update_status(suggestion_id, "expired", None, Some(Utc::now()))
                .await?;
            return Err(OntologyError::conflict(format!(
                "suggestion {suggestion_id} has expired"
            )));
        }

        let required = accept_permission_for(suggestion.kind);
        let is_admin = request.actor_permissions.iter().any(|p| p == "*");
        if !is_admin && !request.actor_permissions.iter().any(|p| p == required) {
            return Err(OntologyError::forbidden(format!(
                "missing permission {required}"
            )));
        }

        let flight_id = suggestion.flight_id.0.clone();
        let flight = self
            .flight_repo
            .find_by_id(&flight_id)
            .await?
            .ok_or_else(|| OntologyError::not_found(format!("flight {flight_id}")))?;

        let mut patch = FlightUpdatePatch {
            expected_version: Some(flight.version),
            ..FlightUpdatePatch::default()
        };
        match suggestion.kind {
            SuggestionKind::Stand => {
                if !draft_can_be_occupied(flight.is_draft) {
                    return Err(OntologyError::validation(
                        "draft flight cannot receive formal stand occupation (invariant 5)",
                    ));
                }
                patch.stand = PatchField::Set(StandNumber(suggestion.suggested_value.clone()));
            }
            SuggestionKind::Gate => {
                patch.gate = PatchField::Set(GateNumber(suggestion.suggested_value.clone()));
            }
        }

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;

        if let Some(reg) = flight.registration.as_deref() {
            self.ontology_tx.upsert_aircraft_in_tx(&mut tx, reg).await?;
        }

        let _updated = self
            .flight_tx
            .update_partial_in_tx(&mut tx, &flight_id, &patch)
            .await?
            .ok_or_else(|| OntologyError::not_found(format!("flight {flight_id}")))?;
        write_flight_update_outbox_events(
            &mut tx,
            &flight_id,
            &patch,
            Some(request.accepted_by.as_str()),
        )
        .await?;

        self.ontology_tx
            .update_suggestion_status_in_tx(
                &mut tx,
                suggestion_id,
                "accepted_executed",
                Some(request.accepted_by.as_str()),
                Some(Utc::now()),
            )
            .await?;

        tx.commit()
            .await
            .map_err(|e| OntologyError::internal(e.to_string()))?;

        self.suggestion_repo
            .find_by_id(suggestion_id)
            .await?
            .ok_or_else(|| OntologyError::not_found(format!("suggestion {suggestion_id}")))
    }

    /// 驳回建议。
    pub async fn reject_suggestion(
        &self,
        suggestion_id: &str,
        request: SuggestionRejectRequest,
    ) -> Result<ResourceAdjustmentSuggestion, OntologyError> {
        let suggestion = self
            .suggestion_repo
            .find_by_id(suggestion_id)
            .await?
            .ok_or_else(|| OntologyError::not_found(format!("suggestion {suggestion_id}")))?;
        if !matches!(suggestion.status, SuggestionStatus::Pending) {
            return Err(OntologyError::conflict(format!(
                "suggestion {suggestion_id} is not pending"
            )));
        }

        let _ = request.reason;
        self.suggestion_repo
            .update_status(
                suggestion_id,
                "rejected",
                Some(request.rejected_by.as_str()),
                Some(Utc::now()),
            )
            .await?
            .ok_or_else(|| OntologyError::not_found(format!("suggestion {suggestion_id}")))
    }

    /// 列出建议。
    pub async fn list_suggestions(
        &self,
        query: SuggestionQuery,
    ) -> Result<Vec<ResourceAdjustmentSuggestion>, OntologyError> {
        let limit = query.limit.unwrap_or(50).clamp(1, 200);
        Ok(self
            .suggestion_repo
            .list(query.flight_id.as_deref(), query.status.as_deref(), limit)
            .await?)
    }

    /// draft 整批确认（§3.3）。
    pub async fn confirm_draft_flights(
        &self,
        request: ConfirmDraftFlightsRequest,
    ) -> Result<ConfirmDraftFlightsResponse, OntologyError> {
        if request.flight_ids.is_empty() {
            return Err(OntologyError::validation("flight_ids must not be empty"));
        }

        let mut confirmed = Vec::new();
        let mut missing = Vec::new();

        if let Some(flight_svc) = &self.flight_service {
            for flight_id in &request.flight_ids {
                match flight_svc
                    .confirm_draft_flight(flight_id, Some(request.confirmed_by.clone()))
                    .await
                {
                    Ok(Some(_)) => confirmed.push(flight_id.clone()),
                    Ok(None) => missing.push(flight_id.clone()),
                    Err(DomainError::ValidationError(_)) => {
                        // 非 draft 或 kind 不符：视为本批跳过并记 missing 语义外的拒绝
                        return Err(OntologyError::from(DomainError::ValidationError(
                            format!("cannot confirm draft for {flight_id}"),
                        )));
                    }
                    Err(e) => return Err(OntologyError::from(e)),
                }
            }
        } else {
            for flight_id in &request.flight_ids {
                let Some(current) = self.flight_repo.find_by_id(flight_id).await? else {
                    missing.push(flight_id.clone());
                    continue;
                };
                if !current.is_draft {
                    return Err(OntologyError::validation(format!(
                        "航班 {flight_id} 不是 draft 状态"
                    )));
                }
                if current.flight_kind != "passenger" {
                    return Err(OntologyError::validation(format!(
                        "仅 passenger 航班支持批确认（当前 flight_kind: {}）",
                        current.flight_kind
                    )));
                }
                let patch = FlightUpdatePatch {
                    expected_version: Some(current.version),
                    is_draft: Some(false),
                    ..FlightUpdatePatch::default()
                };
                let mut tx = self
                    .pool
                    .begin()
                    .await
                    .map_err(|e| OntologyError::internal(e.to_string()))?;
                let Some(_) = self
                    .flight_tx
                    .update_partial_in_tx(&mut tx, flight_id, &patch)
                    .await?
                else {
                    missing.push(flight_id.clone());
                    continue;
                };
                write_flight_update_outbox_events(
                    &mut tx,
                    flight_id,
                    &patch,
                    Some(request.confirmed_by.as_str()),
                )
                .await?;
                tx.commit()
                    .await
                    .map_err(|e| OntologyError::internal(e.to_string()))?;
                confirmed.push(flight_id.clone());
            }
        }

        Ok(ConfirmDraftFlightsResponse { confirmed, missing })
    }

    /// §5.3 航段资源视图。
    pub async fn flight_resource_view(&self, flight_id: &str) -> Result<FlightResourceView, OntologyError> {
        let flight = self
            .flight_repo
            .find_by_id(flight_id)
            .await?
            .ok_or_else(|| OntologyError::not_found(format!("flight {flight_id}")))?;

        let occupations = self.occupation_repo.find_active_by_flight(flight_id).await?;
        let assignments = self.assignment_repo.find_active_by_flight(flight_id).await?;
        let links = self.link_repo.list_by_flight(flight_id).await?;

        Ok(FlightResourceView {
            flight_id: flight_id.to_string(),
            registration: flight.registration,
            plan_stand: flight.stand.map(|s| s.0),
            plan_gate: flight.gate.map(|g| g.0),
            occupations: occupations
                .into_iter()
                .map(|o| serde_json::to_value(o).unwrap_or(serde_json::Value::Null))
                .collect(),
            assignments: assignments
                .into_iter()
                .map(|a| serde_json::to_value(a).unwrap_or(serde_json::Value::Null))
                .collect(),
            turnaround_links: links
                .into_iter()
                .map(|l| serde_json::to_value(l).unwrap_or(serde_json::Value::Null))
                .collect(),
        })
    }

    /// §5.3 飞机资源视图。
    pub async fn aircraft_resource_view(
        &self,
        registration: &str,
    ) -> Result<AircraftResourceView, OntologyError> {
        let registration = registration.trim();
        if registration.is_empty() {
            return Err(OntologyError::validation("registration must not be empty"));
        }

        let now = Utc::now();
        let occupations = self
            .occupation_repo
            .list_active_by_registration(registration, now)
            .await?;
        let assignments = self
            .assignment_repo
            .list_active_by_registration(registration, now)
            .await?;
        let current_stand = occupations
            .iter()
            .max_by_key(|o| o.starts_at)
            .map(|o| o.stand_code.0.clone());
        let current_gate = assignments
            .iter()
            .max_by_key(|a| a.starts_at)
            .map(|a| a.gate_code.0.clone());
        let in_field = !occupations.is_empty();

        // 轻量：按 active occupation 关联的 flight 列表（不去扫全表）
        let mut flights = Vec::new();
        for occ in &occupations {
            if let Some(fid) = &occ.flight_id {
                if let Some(f) = self.flight_repo.find_by_id(&fid.0).await? {
                    flights.push(serde_json::to_value(f).unwrap_or(serde_json::Value::Null));
                }
            }
        }

        let _ = self.aircraft_repo.find_by_registration(registration).await?;

        Ok(AircraftResourceView {
            registration: registration.to_string(),
            in_field,
            current_stand,
            current_gate,
            occupations: occupations
                .into_iter()
                .map(|o| serde_json::to_value(o).unwrap_or(serde_json::Value::Null))
                .collect(),
            assignments: assignments
                .into_iter()
                .map(|a| serde_json::to_value(a).unwrap_or(serde_json::Value::Null))
                .collect(),
            flights,
        })
    }

    fn ensure_not_ground_blacklisted(
        &self,
        actor_permissions: &[String],
        object_type: &str,
        action: &str,
    ) -> Result<(), OntologyError> {
        // 若仅有 ontology.read / suggestion.reject 等 GROUND 权限而无 reassign，路由层已拦。
        // 此处对显式黑名单动作做双保险。
        if is_ground_blacklisted_action(object_type, action)
            && !actor_permissions
                .iter()
                .any(|p| p == "ontology.aircraft.reassign" || p == "*")
        {
            return Err(OntologyError::forbidden(format!(
                "action {object_type}.{action} is blacklisted for this actor"
            )));
        }
        Ok(())
    }
}
