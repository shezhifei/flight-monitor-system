use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use crate::schemas::dispatch_schemas::{PositionUpdate, TeamMemberAdd};
use crate::schemas::ontology_schemas::{
    AdjustGateRequest, AdjustStandRequest, AllocateCarouselRequest, AllocateGateRequest, AllocateStandRequest,
    ReleaseResourceRequest,
};
use crate::services::business_case_service::{BusinessCaseTerminalUpdatePayload, BusinessCaseWriter};
use crate::services::dispatch_resource_service::DispatchResourceService;
use crate::services::dispatch_service::writer::DispatchOrderWriter;
use crate::services::dispatch_service::DispatchService;
use crate::services::flight_domain_events::write_flight_outbox_event;
use crate::services::flight_writer::FlightWriter;
use crate::services::ontology_service::{OntologyError, OntologyService};
use crate::types::{ConcreteBusinessCaseService, ConcreteDispatchResourceService, ConcreteFlightService};
use fms_domain::ontology::schema_export::FLIGHT_OPS_ONTOLOGY_VERSION;
use fms_domain::ports::anomaly_repository::AnomalyTransactionalRepository;
use fms_domain::ports::domain_event_outbox_repository::DomainEventOutboxTransactionalRepository;
use fms_domain::ports::unit_of_work::UnitOfWork;

use super::helpers::{optional_string, required_string};
use super::types::{DomainActionError, DomainActionReceipt};

/// 执行器的对象安全端口。`ai_action_proposal_service` 与 `rollback_service`
/// 只消费这一个方法；`DomainActionExecutor<U>` 的泛型在装配点具体化后从这里消失。
#[async_trait::async_trait]
pub trait DomainActionExecution: Send + Sync {
    async fn execute_approved_action(
        &self,
        object_type: &str,
        object_id: &str,
        action_name: &str,
        arguments: &Value,
        executor_id: &str,
    ) -> Result<DomainActionReceipt, DomainActionError>;
}

pub struct DomainActionExecutor<U: UnitOfWork> {
    flight_service: Arc<ConcreteFlightService>,
    flight_writer: Arc<FlightWriter<U::Tx>>,
    dispatch_service: Arc<DispatchService>,
    dispatch_writer: Arc<DispatchOrderWriter<U::Tx>>,
    business_case_service: Arc<ConcreteBusinessCaseService>,
    business_case_writer: Arc<BusinessCaseWriter<U::Tx>>,
    ontology_svc: Arc<OntologyService>,
    dispatch_resource_svc: Arc<ConcreteDispatchResourceService>,
    uow: Arc<U>,
    outbox_repo: Arc<dyn DomainEventOutboxTransactionalRepository<U::Tx> + Send + Sync>,
    anomaly_tx_repo: Arc<dyn AnomalyTransactionalRepository<U::Tx> + Send + Sync>,
}

impl<U: UnitOfWork> DomainActionExecutor<U> {
    pub fn new(
        flight_service: Arc<ConcreteFlightService>,
        flight_writer: Arc<FlightWriter<U::Tx>>,
        dispatch_service: Arc<DispatchService>,
        dispatch_writer: Arc<DispatchOrderWriter<U::Tx>>,
        business_case_service: Arc<ConcreteBusinessCaseService>,
        business_case_writer: Arc<BusinessCaseWriter<U::Tx>>,
        ontology_svc: Arc<OntologyService>,
        dispatch_resource_svc: Arc<ConcreteDispatchResourceService>,
        outbox_repo: Arc<dyn DomainEventOutboxTransactionalRepository<U::Tx> + Send + Sync>,
        anomaly_tx_repo: Arc<dyn AnomalyTransactionalRepository<U::Tx> + Send + Sync>,
        uow: Arc<U>,
    ) -> Self {
        Self {
            flight_service,
            flight_writer,
            dispatch_service,
            dispatch_writer,
            business_case_service,
            business_case_writer,
            ontology_svc,
            dispatch_resource_svc,
            uow,
            outbox_repo,
            anomaly_tx_repo,
        }
    }

    pub async fn execute_approved_action(
        &self,
        object_type: &str,
        object_id: &str,
        action_name: &str,
        arguments: &Value,
        executor_id: &str,
    ) -> Result<DomainActionReceipt, DomainActionError> {
        let action_key = format!("{}.{}", object_type, action_name);
        let now = chrono::Utc::now();

        let mut tx = self
            .uow
            .begin()
            .await
            .map_err(|e| DomainActionError::Internal(e.to_string()))?;

        let result = self
            .execute_in_tx(&mut tx, &action_key, object_type, object_id, arguments, executor_id)
            .await;

        if let Ok(ref val) = result {
            let event_type = format!("{}.{}", object_type, action_name);
            let payload = serde_json::json!({
                "executor_id": executor_id,
                "arguments": arguments,
                "result": val,
                // 受控写动作必须携带 ontology version 与审计信息；
                // correlation_id 由 proposal 管线通过 arguments 注入。
                "ontology_version": FLIGHT_OPS_ONTOLOGY_VERSION,
                "correlation_id": arguments.get("correlation_id").cloned().unwrap_or(Value::Null),
            });

            if let Err(e) = write_flight_outbox_event(
                self.outbox_repo.as_ref(),
                &mut tx,
                object_type,
                object_id,
                &event_type,
                payload,
            )
            .await
            {
                tracing::error!("Failed to write domain_event_outbox for AI action {object_type}.{action_name}: {e}");
                return Err(DomainActionError::Internal(format!("outbox write failed: {e}")));
            }
        }

        if result.is_ok() {
            self.uow
                .commit(tx)
                .await
                .map_err(|e| DomainActionError::Internal(e.to_string()))?;

            // Post-commit hooks for transactional side-effects
            if let Ok(ref val) = result {
                match action_key.as_str() {
                    "DispatchOrder.publish" => {
                        if let Ok(Some(order)) = self.dispatch_service.get_order_domain(object_id).await {
                            self.dispatch_service.sync_dispatch_chat_for_order(object_id).await;
                            self.dispatch_service.send_publication_notifications(&order).await;
                        }
                    }
                    "DispatchOrder.assign_slot" | "DispatchOrder.unassign_slot" | "DispatchOrder.add_slot"
                    | "DispatchOrder.remove_slot" => {
                        self.dispatch_service.sync_dispatch_chat_for_order(object_id).await;
                    }
                    "BusinessCase.create" => {
                        if let Some(flight_id) = val.get("flight_id").and_then(Value::as_str) {
                            self.business_case_service
                                .refresh_flight_runtime_projection(flight_id)
                                .await;
                        }
                    }
                    "BusinessCase.close_case" => {
                        if let Some(flight_id) = val.get("flight_id").and_then(Value::as_str) {
                            self.business_case_service
                                .refresh_flight_runtime_projection(flight_id)
                                .await;
                        } else if let Ok(Some(bc)) = self.business_case_service.get(object_id).await {
                            self.business_case_service
                                .refresh_flight_runtime_projection(&bc.flight_id)
                                .await;
                        }
                    }
                    _ => {}
                }
            }
        }
        // else: tx is dropped → auto-rollback

        match result {
            Ok(r) => Ok(DomainActionReceipt {
                action_name: action_name.to_string(),
                object_type: object_type.to_string(),
                object_id: object_id.to_string(),
                result: r,
                executed_at: now,
                executor_id: executor_id.to_string(),
            }),
            Err(e) => Err(e),
        }
    }

    async fn execute_in_tx(
        &self,
        tx: &mut U::Tx,
        action_key: &str,
        object_type: &str,
        object_id: &str,
        arguments: &Value,
        executor_id: &str,
    ) -> Result<Value, DomainActionError> {
        match action_key {
            "Flight.add_note" => {
                let note = required_string(arguments, &["note_content", "note"], "note")?;

                let before = self
                    .flight_service
                    .get_flight(object_id)
                    .await
                    .map_err(|e| DomainActionError::Execution(e.to_string()))?
                    .ok_or_else(|| DomainActionError::NotFound(format!("Flight {} not found", object_id)))?;
                let dto = crate::schemas::flight_schemas::FlightUpdate {
                    expected_version: Some(before.version),
                    flight_remarks: crate::schemas::flight_schemas::NullableUpdate::Set(note.to_string()),
                    ..Default::default()
                };
                let res = self
                    .flight_writer
                    .update_flight_in_tx(tx, object_id, dto, Some(executor_id.to_string()))
                    .await
                    .map_err(|e| DomainActionError::Execution(e.to_string()))?;
                if res.is_none() {
                    return Err(DomainActionError::NotFound(format!("Flight {} not found", object_id)));
                }
                self.flight_service.invalidate_hot_list().await;
                Ok(serde_json::json!({ "success": true, "note": note }))
            }
            "Flight.update_status" => {
                let status = required_string(arguments, &["new_status", "status"], "status")?;
                // 审批执行前重验对象版本，携带 expected_version 乐观锁。
                let before = self
                    .flight_service
                    .get_flight(object_id)
                    .await
                    .map_err(|e| DomainActionError::Execution(e.to_string()))?
                    .ok_or_else(|| DomainActionError::NotFound(format!("Flight {} not found", object_id)))?;
                let dto = crate::schemas::flight_schemas::FlightUpdate {
                    expected_version: Some(before.version),
                    status: Some(status.to_string()),
                    ..Default::default()
                };
                let res = self
                    .flight_writer
                    .update_flight_in_tx(tx, object_id, dto, Some(executor_id.to_string()))
                    .await
                    .map_err(|e| DomainActionError::Execution(e.to_string()))?;
                if res.is_none() {
                    return Err(DomainActionError::NotFound(format!("Flight {} not found", object_id)));
                }
                self.flight_service.invalidate_hot_list().await;
                Ok(serde_json::json!({ "success": true, "status": status }))
            }
            // `Flight.change_stand` 已废止（PR #本体两层改造）——展示列改由 StandOccupation 占用回写。
            // `Flight.update_delay`：更新预计到/离港时间（携带 before/after）。
            "Flight.update_delay" => {
                let parse_dt = |key: &str| -> Result<Option<chrono::DateTime<chrono::Utc>>, DomainActionError> {
                    match arguments.get(key) {
                        None | Some(Value::Null) => Ok(None),
                        Some(Value::String(raw)) => raw
                            .parse::<chrono::DateTime<chrono::Utc>>()
                            .map(Some)
                            .map_err(|_| DomainActionError::Validation(format!("`{key}` is not an RFC3339 datetime"))),
                        Some(_) => Err(DomainActionError::Validation(format!(
                            "`{key}` must be an RFC3339 datetime string"
                        ))),
                    }
                };
                let estimated_departure = parse_dt("estimated_departure")?;
                let estimated_arrival = parse_dt("estimated_arrival")?;
                if estimated_departure.is_none() && estimated_arrival.is_none() {
                    return Err(DomainActionError::Validation(
                        "at least one of estimated_departure / estimated_arrival is required".to_string(),
                    ));
                }
                let before = self
                    .flight_service
                    .get_flight(object_id)
                    .await
                    .map_err(|e| DomainActionError::Execution(e.to_string()))?
                    .ok_or_else(|| DomainActionError::NotFound(format!("Flight {} not found", object_id)))?;
                let before_snapshot = serde_json::json!({
                    "estimated_departure": before.estimated_departure,
                    "estimated_arrival": before.estimated_arrival,
                    "status": before.status.clone(),
                    "version": before.version,
                });
                let dto = crate::schemas::flight_schemas::FlightUpdate {
                    expected_version: Some(before.version),
                    estimated_departure: estimated_departure
                        .map(crate::schemas::flight_schemas::NullableUpdate::Set)
                        .unwrap_or_default(),
                    estimated_arrival: estimated_arrival
                        .map(crate::schemas::flight_schemas::NullableUpdate::Set)
                        .unwrap_or_default(),
                    ..Default::default()
                };
                let res = self
                    .flight_writer
                    .update_flight_in_tx(tx, object_id, dto, Some(executor_id.to_string()))
                    .await
                    .map_err(|e| DomainActionError::Execution(e.to_string()))?;
                if res.is_none() {
                    return Err(DomainActionError::NotFound(format!("Flight {} not found", object_id)));
                }
                self.flight_service.invalidate_hot_list().await;
                Ok(serde_json::json!({
                    "success": true,
                    "estimated_departure": estimated_departure,
                    "estimated_arrival": estimated_arrival,
                    "reason": arguments.get("reason"),
                    "before_snapshot": before_snapshot,
                    "after_preview": {
                        "estimated_departure": estimated_departure,
                        "estimated_arrival": estimated_arrival,
                    },
                }))
            }
            // `Notification.send` 已废止（PR #本体两层改造）——Notification 对象退出合同。
            "Anomaly.acknowledge" => {
                self.anomaly_tx_repo
                    .acknowledge_in_tx(tx, object_id)
                    .await
                    .map_err(|e| DomainActionError::Execution(e.to_string()))?;
                Ok(serde_json::json!({ "success": true }))
            }
            // `Anomaly.resolve`：解决异常（事务内写，已解决时返回 false）。
            "Anomaly.resolve" => {
                let resolution_note = optional_string(arguments, &["resolution_note", "note"]);
                let resolved = self
                    .anomaly_tx_repo
                    .resolve_in_tx(tx, object_id)
                    .await
                    .map_err(|e| DomainActionError::Execution(e.to_string()))?;
                if !resolved {
                    return Err(DomainActionError::Execution(format!(
                        "Anomaly {object_id} not found or already resolved"
                    )));
                }
                Ok(serde_json::json!({
                    "success": true,
                    "resolved": true,
                    "resolution_note": resolution_note,
                }))
            }
            "Anomaly.escalate" => {
                let severity = arguments.get("severity").and_then(|v| v.as_str()).unwrap_or("high");
                let reason = arguments
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("AI recommended escalation");

                self.anomaly_tx_repo
                    .escalate_in_tx(tx, object_id)
                    .await
                    .map_err(|e| DomainActionError::Execution(e.to_string()))?;

                Ok(serde_json::json!({
                    "success": true,
                    "escalated": true,
                    "severity": severity,
                    "reason": reason,
                }))
            }
            "DispatchOrder.recommend_replan" => {
                let strategy = arguments.get("strategy").and_then(|v| v.as_str()).unwrap_or("balanced");
                let apply = arguments.get("apply").and_then(|v| v.as_bool()).unwrap_or(false);
                let reason = arguments
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("AI recommended replan");

                if apply {
                    return Err(DomainActionError::Execution(
                        "AI recommend_replan should be advisory-only. apply=true is not permitted in this context."
                            .to_string(),
                    ));
                }

                let order = self
                    .dispatch_service
                    .get_order(object_id)
                    .await
                    .map_err(|e| DomainActionError::Execution(e.to_string()))?;
                if order.is_none() {
                    return Err(DomainActionError::NotFound(format!(
                        "DispatchOrder {} not found",
                        object_id
                    )));
                }

                let now = chrono::Utc::now();
                let window_start = now;
                let window_end = now + chrono::Duration::hours(6);

                let dto = crate::schemas::dispatch_schemas::ReplanRequest {
                    window_start,
                    window_end,
                    strategy: strategy.to_string(),
                    apply_changes: false,
                    max_suggestions: Some(20),
                };

                let replan_result = self
                    .dispatch_service
                    .replan(dto)
                    .await
                    .map_err(|e| DomainActionError::Execution(e.to_string()))?;

                Ok(serde_json::json!({
                    "success": true,
                    "order_id": object_id,
                    "replan_requested": true,
                    "reason": reason,
                    "replan_result": replan_result,
                }))
            }
            // `DispatchOrder.update_status`：受控状态更新（枚举校验 + 事务内写）。
            "DispatchOrder.update_status" => {
                let new_status = required_string(arguments, &["new_status", "status"], "new_status")?;
                let notes = optional_string(arguments, &["notes"]);
                let updated = self
                    .dispatch_writer
                    .update_order_status_in_tx(tx, object_id, new_status, executor_id, notes)
                    .await
                    .map_err(|e| match e {
                        fms_domain::error::DomainError::NotFound { entity_type, id } => {
                            DomainActionError::NotFound(format!("{entity_type} {id} not found"))
                        }
                        fms_domain::error::DomainError::ValidationError(msg) => DomainActionError::Validation(msg),
                        other => DomainActionError::Execution(other.to_string()),
                    })?;
                Ok(serde_json::json!({
                    "success": true,
                    "order_id": updated.id,
                    "status": new_status,
                }))
            }
            "DispatchOrder.publish" => {
                let result = self
                    .dispatch_writer
                    .publish_order_in_tx(tx, object_id, executor_id)
                    .await
                    .map_err(|e| DomainActionError::Execution(e.to_string()))?;

                Ok(serde_json::json!({
                    "success": true,
                    "order_id": object_id,
                    "publication": result,
                }))
            }
            // `Todo.create` / `Todo.complete` 已废止（PR #本体两层改造）——Todo 对象退出合同。
            // `Stand.reserve` 已废止（PR #本体两层改造）——机位占用一律走 `StandOccupation`。
            "BusinessCase.create" => {
                let flight_id = required_string(arguments, &["flight_id"], "flight_id")?;
                let case_type = required_string(arguments, &["case_type"], "case_type")?;
                let description = required_string(arguments, &["description", "summary"], "description")?;
                let flight = self
                    .flight_service
                    .get_flight(flight_id)
                    .await
                    .map_err(|e| DomainActionError::Execution(e.to_string()))?
                    .ok_or_else(|| DomainActionError::Execution(format!("Flight not found: {flight_id}")))?;
                let flight_no = optional_string(arguments, &["flight_no", "flight_number"])
                    .map(str::to_string)
                    .or(flight.flight_number.clone())
                    .unwrap_or_else(|| flight_id.to_string());
                let status = optional_string(arguments, &["status"]);
                let mut context: HashMap<String, serde_json::Value> = arguments
                    .get("context")
                    .and_then(Value::as_object)
                    .map(|map| map.iter().map(|(key, value)| (key.clone(), value.clone())).collect())
                    .unwrap_or_default();
                context.insert("source".to_string(), serde_json::json!("ai_action"));
                context.insert("aip_object_id".to_string(), serde_json::json!(object_id));

                let created = self
                    .business_case_writer
                    .create_in_tx(
                        tx,
                        case_type,
                        flight_id,
                        &flight_no,
                        description,
                        context,
                        status,
                        executor_id,
                    )
                    .await
                    .map_err(|e| DomainActionError::Execution(e.to_string()))?;

                Ok(serde_json::json!({
                    "success": true,
                    "case_id": created.case_id,
                    "flight_id": created.flight_id,
                    "status": created.status,
                }))
            }
            "BusinessCase.close_case" => {
                let reason = optional_string(arguments, &["reason"]).map(str::to_string);
                let target_status = optional_string(arguments, &["target_status", "status"]).unwrap_or("COMPLETED");
                let updated = self
                    .business_case_writer
                    .apply_workflow_terminal_action_in_tx(
                        tx,
                        object_id,
                        BusinessCaseTerminalUpdatePayload {
                            action: "ai_close_case".to_string(),
                            target_status: target_status.to_string(),
                            actor: executor_id.to_string(),
                            reason,
                            write_finished_at: true,
                            workflow_run_id: optional_string(arguments, &["workflow_run_id"]).map(str::to_string),
                            workflow_outcome: optional_string(arguments, &["workflow_outcome"]).map(str::to_string),
                            receipt_group_id: optional_string(arguments, &["receipt_group_id"]).map(str::to_string),
                        },
                    )
                    .await
                    .map_err(|e| DomainActionError::Execution(e.to_string()))?
                    .ok_or_else(|| DomainActionError::Execution(format!("BusinessCase not found: {object_id}")))?;

                Ok(serde_json::json!({
                    "success": true,
                    "case_id": updated.case_id,
                    "status": updated.status,
                }))
            }
            // `Label.add` 已废止（PR #本体两层改造）——标签写入口迁至 `Flight.add_label`，接线延后，执行器 fail-closed。
            // `Workflow.start` 已废止（PR #本体两层改造）——起流程是事项（BusinessCase）属性，执行器 fail-closed。
            // ⏸️ TODO: stand_occupation.allocate/adjust/release 已接线（见下方占用动作分支）。
            // ⏸️ TODO: gate_assignment.allocate/release 已接线（见下方占用动作分支）。
            // ⏸️ TODO: carousel_assignment.allocate/release 已接线（见下方占用动作分支）。

            // ⏸️ TODO: equipment.assign/release（工单设备槽，PR5 接线；当前 fail-closed）

            // PR4 人员 runtime / 入组出组：本人可直接改在岗；改别人或入组/出组须经理或 admin
            // （科室边界在 DispatchResourceService 领域层再验，执行器不拼 SQL）。
            "Personnel.update_status" => {
                let user_id = required_string(arguments, &["user_id"], "user_id")?;
                let status = required_string(arguments, &["status"], "status")?;
                let runtime = self
                    .dispatch_resource_svc
                    .update_personnel_status(user_id, status, executor_id)
                    .await
                    .map_err(map_service_error)?;
                Ok(serde_json::json!({
                    "success": true,
                    "user_id": user_id,
                    "current_status": runtime.current_status,
                }))
            }
            "Personnel.change_location" => {
                let user_id = required_string(arguments, &["user_id"], "user_id")?;
                let (lat, lng) = parse_lat_lng(arguments)?;
                let stand_id = optional_string(arguments, &["stand_id"]).map(str::to_string);
                self.dispatch_resource_svc
                    .update_personnel_position(user_id, lat, lng, stand_id.as_deref(), executor_id)
                    .await
                    .map_err(map_service_error)?;
                Ok(serde_json::json!({ "success": true, "user_id": user_id }))
            }
            "Personnel.assign_to_team" => {
                let user_id = required_string(arguments, &["user_id"], "user_id")?;
                let team_id = required_string(arguments, &["team_id"], "team_id")?;
                self.dispatch_resource_svc
                    .assign_person_to_team(user_id, team_id, executor_id)
                    .await
                    .map_err(map_service_error)?;
                Ok(serde_json::json!({ "success": true, "user_id": user_id, "team_id": team_id }))
            }
            "Personnel.leave_team" => {
                let user_id = required_string(arguments, &["user_id"], "user_id")?;
                let team_id = required_string(arguments, &["team_id"], "team_id")?;
                self.dispatch_resource_svc
                    .remove_person_from_team(user_id, team_id, executor_id)
                    .await
                    .map_err(map_service_error)?;
                Ok(serde_json::json!({ "success": true, "user_id": user_id, "team_id": team_id }))
            }

            // PR4 组织写动作：执行器只做参数映射 + 调 DispatchResourceService（禁止第二套 SQL）。
            // 权限由 AI 提案主管线在审批/执行前验过（ontology.team.manage / equipment.manage）。
            "Team.update_status" => {
                let team_id = required_string(arguments, &["team_id"], "team_id")?;
                let current_status = required_string(arguments, &["current_status"], "current_status")?;
                let member_user_ids = parse_optional_string_array(arguments, "member_user_ids")?;
                // 代签：可附全量名册，同步增删（同一服务，派生自现有 add/remove 领域方法）。
                if let Some(desired) = member_user_ids.as_deref() {
                    let team = self
                        .dispatch_resource_svc
                        .get_team(team_id, true)
                        .await
                        .map_err(map_service_error)?
                        .ok_or_else(|| DomainActionError::NotFound(format!("Team {team_id} not found")))?;
                    let current: Vec<String> = team
                        .members
                        .iter()
                        .filter(|m| m.is_active)
                        .map(|m| m.user_id.clone())
                        .collect();
                    for uid in desired.iter().filter(|uid| !current.contains(*uid)) {
                        self.dispatch_resource_svc
                            .add_team_member(team_id, TeamMemberAdd { user_id: (*uid).clone(), role: "member".into(), can_drive: false })
                            .await
                            .map_err(map_service_error)?;
                    }
                    for uid in current.iter().filter(|uid| !desired.contains(uid)) {
                        self.dispatch_resource_svc
                            .remove_team_member(team_id, uid.as_str())
                            .await
                            .map_err(map_service_error)?;
                    }
                } else {
                    self.dispatch_resource_svc
                        .update_team_status(team_id, current_status)
                        .await
                        .map_err(map_service_error)?;
                }
                Ok(serde_json::json!({ "success": true, "team_id": team_id, "current_status": current_status }))
            }
            "Team.change_location" => {
                let team_id = required_string(arguments, &["team_id"], "team_id")?;
                let (lat, lng) = parse_lat_lng(arguments)?;
                self.dispatch_resource_svc
                    .update_team_position(team_id, PositionUpdate { lat, lng, stand_id: None })
                    .await
                    .map_err(map_service_error)?;
                Ok(serde_json::json!({ "success": true, "team_id": team_id }))
            }
            "Team.add_member" => {
                let team_id = required_string(arguments, &["team_id"], "team_id")?;
                let user_id = required_string(arguments, &["user_id"], "user_id")?;
                let member = self
                    .dispatch_resource_svc
                    .add_team_member(team_id, TeamMemberAdd { user_id: user_id.to_string(), role: "member".into(), can_drive: false })
                    .await
                    .map_err(map_service_error)?;
                Ok(serde_json::json!({ "success": true, "team_id": team_id, "member_user_id": member.user_id }))
            }
            "Team.remove_member" => {
                let team_id = required_string(arguments, &["team_id"], "team_id")?;
                let user_id = required_string(arguments, &["user_id"], "user_id")?;
                self.dispatch_resource_svc
                    .remove_team_member(team_id, user_id)
                    .await
                    .map_err(map_service_error)?;
                Ok(serde_json::json!({ "success": true, "team_id": team_id, "member_user_id": user_id }))
            }
            "Equipment.update_status" => {
                let equipment_id = required_string(arguments, &["equipment_id"], "equipment_id")?;
                let status = required_string(arguments, &["status"], "status")?;
                self.dispatch_resource_svc
                    .update_equipment_status(equipment_id, status)
                    .await
                    .map_err(map_service_error)?;
                Ok(serde_json::json!({ "success": true, "equipment_id": equipment_id, "status": status }))
            }
            "Equipment.change_location" => {
                let equipment_id = required_string(arguments, &["equipment_id"], "equipment_id")?;
                let (lat, lng) = parse_lat_lng(arguments)?;
                self.dispatch_resource_svc
                    .update_equipment_position(equipment_id, PositionUpdate { lat, lng, stand_id: None })
                    .await
                    .map_err(map_service_error)?;
                Ok(serde_json::json!({ "success": true, "equipment_id": equipment_id }))
            }
            "StandOccupation.allocate" => {
                let stand_code = required_string(arguments, &["stand_code"], "stand_code")?;
                let registration = required_string(arguments, &["registration"], "registration")?;
                let request = AllocateStandRequest {
                    registration: registration.to_string(),
                    stand_code: stand_code.to_string(),
                    starts_at: parse_dt_arg(arguments, "starts_at")?,
                    ends_at: parse_dt_arg(arguments, "ends_at")?,
                    kind: optional_string(arguments, &["kind"]).unwrap_or("normal").to_string(),
                    moving_to_stand: optional_string(arguments, &["moving_to_stand"]).map(str::to_string),
                    flight_id: optional_string(arguments, &["flight_id"]).map(str::to_string),
                    client_action_id: optional_string(arguments, &["client_action_id"]).map(str::to_string),
                    sync_flight_plan: true,
                };
                let perms = vec!["ontology:stand.manage".to_string()];
                let result = self
                    .ontology_svc
                    .allocate_stand(request, executor_id, &perms, false)
                    .await
                    .map_err(map_ontology_error)?;
                Ok(serde_json::json!({
                    "success": true,
                    "occupation": result.occupation,
                    "overlap_warnings": result.overlap_warnings,
                }))
            }
            "StandOccupation.adjust" => {
                let request = AdjustStandRequest {
                    stand_code: optional_string(arguments, &["stand_code"]).map(str::to_string),
                    starts_at: parse_dt_arg_opt(arguments, "starts_at")?,
                    ends_at: parse_dt_arg_opt(arguments, "ends_at")?,
                    kind: optional_string(arguments, &["kind"]).map(str::to_string),
                    moving_to_stand: optional_string(arguments, &["moving_to_stand"]).map(str::to_string),
                    sync_flight_plan: true,
                };
                let perms = vec!["ontology:stand.manage".to_string()];
                let result = self
                    .ontology_svc
                    .adjust_stand(object_id, request, executor_id, &perms, false)
                    .await
                    .map_err(map_ontology_error)?;
                Ok(serde_json::json!({
                    "success": true,
                    "occupation": result.occupation,
                    "overlap_warnings": result.overlap_warnings,
                }))
            }
            "StandOccupation.release" => {
                let request = ReleaseResourceRequest { released_by: Some(executor_id.to_string()) };
                let perms = vec!["ontology:stand.manage".to_string()];
                let occupation = self
                    .ontology_svc
                    .release_stand(object_id, request, executor_id, &perms, false)
                    .await
                    .map_err(map_ontology_error)?;
                Ok(serde_json::json!({ "success": true, "occupation": occupation }))
            }
            "GateAssignment.allocate" => {
                let gate_code = required_string(arguments, &["gate_code"], "gate_code")?;
                let flight_id = required_string(arguments, &["flight_id"], "flight_id")?;
                let request = AllocateGateRequest {
                    registration: optional_string(arguments, &["registration"]).map(str::to_string),
                    gate_code: gate_code.to_string(),
                    starts_at: parse_dt_arg(arguments, "starts_at")?,
                    ends_at: parse_dt_arg(arguments, "ends_at")?,
                    flight_id: flight_id.to_string(),
                    client_action_id: optional_string(arguments, &["client_action_id"]).map(str::to_string),
                    sync_flight_plan: true,
                };
                let perms = vec!["ontology:gate.manage".to_string()];
                let result = self
                    .ontology_svc
                    .allocate_gate(request, executor_id, &perms, false)
                    .await
                    .map_err(map_ontology_error)?;
                Ok(serde_json::json!({
                    "success": true,
                    "assignment": result.assignment,
                    "consistency_warnings": result.consistency_warnings,
                }))
            }
            "GateAssignment.release" => {
                let request = ReleaseResourceRequest { released_by: Some(executor_id.to_string()) };
                let perms = vec!["ontology:gate.manage".to_string()];
                let assignment = self
                    .ontology_svc
                    .release_gate(object_id, request, executor_id, &perms, false)
                    .await
                    .map_err(map_ontology_error)?;
                Ok(serde_json::json!({ "success": true, "assignment": assignment }))
            }
            "CarouselAssignment.allocate" => {
                let carousel_code = required_string(arguments, &["carousel_code"], "carousel_code")?;
                let flight_id = required_string(arguments, &["flight_id"], "flight_id")?;
                let request = AllocateCarouselRequest {
                    carousel_code: carousel_code.to_string(),
                    flight_id: flight_id.to_string(),
                    registration: optional_string(arguments, &["registration"]).map(str::to_string),
                    starts_at: parse_dt_arg(arguments, "starts_at")?,
                    ends_at: parse_dt_arg(arguments, "ends_at")?,
                    client_action_id: optional_string(arguments, &["client_action_id"]).map(str::to_string),
                };
                let perms = vec!["ontology:carousel.manage".to_string()];
                let result = self
                    .ontology_svc
                    .allocate_carousel(request, executor_id, &perms, false)
                    .await
                    .map_err(map_ontology_error)?;
                Ok(serde_json::json!({
                    "success": true,
                    "inserted": result.inserted,
                    "assignment": result.assignment,
                }))
            }
            "CarouselAssignment.release" => {
                let request = ReleaseResourceRequest { released_by: Some(executor_id.to_string()) };
                let perms = vec!["ontology:carousel.manage".to_string()];
                let assignment = self
                    .ontology_svc
                    .release_carousel(object_id, request, executor_id, &perms, false)
                    .await
                    .map_err(map_ontology_error)?;
                Ok(serde_json::json!({ "success": true, "assignment": assignment }))
            }
            // `DispatchOrder.assign_slot` / `unassign_slot` / `add_slot` / `remove_slot`（PR5）
            // 命名槽派工：与预排共用「同科室 / 在岗 / 资质」校验口径，槽写入在调用方事务内。
            "DispatchOrder.assign_slot" => {
                let slot_code = required_string(arguments, &["slot_code"], "slot_code")?;
                let user_id = required_string(arguments, &["user_id"], "user_id")?;
                let order = self
                    .dispatch_writer
                    .assign_slot_in_tx(tx, object_id, slot_code, user_id, executor_id)
                    .await
                    .map_err(map_dispatch_writer_error)?;
                Ok(serde_json::json!({ "success": true, "order_id": order.id, "slot_code": slot_code, "user_id": user_id }))
            }
            "DispatchOrder.unassign_slot" => {
                let slot_code = required_string(arguments, &["slot_code"], "slot_code")?;
                let order = self
                    .dispatch_writer
                    .unassign_slot_in_tx(tx, object_id, slot_code, executor_id)
                    .await
                    .map_err(map_dispatch_writer_error)?;
                Ok(serde_json::json!({ "success": true, "order_id": order.id, "slot_code": slot_code }))
            }
            "DispatchOrder.add_slot" => {
                let slot_code = required_string(arguments, &["slot_code"], "slot_code")?;
                let slot_name = optional_string(arguments, &["slot_name"]);
                let order = self
                    .dispatch_writer
                    .add_slot_in_tx(tx, object_id, slot_code, slot_name, executor_id)
                    .await
                    .map_err(map_dispatch_writer_error)?;
                Ok(serde_json::json!({ "success": true, "order_id": order.id, "slot_code": slot_code }))
            }
            "DispatchOrder.remove_slot" => {
                let slot_code = required_string(arguments, &["slot_code"], "slot_code")?;
                let order = self
                    .dispatch_writer
                    .remove_slot_in_tx(tx, object_id, slot_code, executor_id)
                    .await
                    .map_err(map_dispatch_writer_error)?;
                Ok(serde_json::json!({ "success": true, "order_id": order.id, "slot_code": slot_code }))
            }
            _ => Err(DomainActionError::NotFound(format!("unknown action: {}", action_key))),
        }
    }
}

/// 把 `arguments` 里的 RFC3339 字符串字段解析成 `DateTime<Utc>`（必填）。
fn parse_dt_arg(arguments: &Value, key: &str) -> Result<chrono::DateTime<chrono::Utc>, DomainActionError> {
    match arguments.get(key) {
        Some(Value::String(raw)) => raw
            .parse::<chrono::DateTime<chrono::Utc>>()
            .map_err(|_| DomainActionError::Validation(format!("`{key}` is not an RFC3339 datetime"))),
        _ => Err(DomainActionError::Validation(format!("`{key}` is required as an RFC3339 datetime"))),
    }
}

/// 把 `arguments` 里的 RFC3339 字符串字段解析成 `Option<DateTime<Utc>>`（可空）。
fn parse_dt_arg_opt(arguments: &Value, key: &str) -> Result<Option<chrono::DateTime<chrono::Utc>>, DomainActionError> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(raw)) => raw
            .parse::<chrono::DateTime<chrono::Utc>>()
            .map(Some)
            .map_err(|_| DomainActionError::Validation(format!("`{key}` is not an RFC3339 datetime"))),
        Some(_) => Err(DomainActionError::Validation(format!(
            "`{key}` must be an RFC3339 datetime string"
        ))),
    }
}

/// 把 `OntologyService` 的错误映射为执行器错误（AI 提案主管线已做权限与参数校验，
/// 这里主要关心 NotFound / Validation，其余归 Execution）。
fn map_ontology_error(error: OntologyError) -> DomainActionError {
    match error {
        OntologyError::Validation(msg) | OntologyError::Forbidden(msg) => DomainActionError::Validation(msg),
        OntologyError::NotFound(msg) => DomainActionError::NotFound(msg),
        OntologyError::Conflict(msg) => DomainActionError::Execution(format!("conflict: {msg}")),
        OntologyError::Internal(msg) => DomainActionError::Execution(msg),
    }
}

/// 把 `DispatchResourceService`（领域服务）的错误映射为执行器错误。科室边界等
/// 领域再验会让它返回 `PermissionDenied`/`Conflict`，这里统一映射成校验/执行错误。
fn map_service_error(error: fms_domain::error::DomainError) -> DomainActionError {
    match error {
        fms_domain::error::DomainError::NotFound { entity_type, id } => {
            DomainActionError::NotFound(format!("{entity_type} {id} not found"))
        }
        fms_domain::error::DomainError::ValidationError(msg)
        | fms_domain::error::DomainError::PermissionDenied(msg) => DomainActionError::Validation(msg),
        fms_domain::error::DomainError::Conflict(msg) => DomainActionError::Execution(format!("conflict: {msg}")),
        other => DomainActionError::Execution(other.to_string()),
    }
}

/// 把派工单写入方（`DispatchOrderWriter`）的领域错误映射为执行器错误。
/// 槽位校验失败的 `BusinessRuleViolation`（跨科室 / 不在岗 / 资质不足）与 `ValidationError`
/// 一律视为对注入参数的拒绝，向提案管线暴露成 Validation，而非服务端失败。
fn map_dispatch_writer_error(error: fms_domain::error::DomainError) -> DomainActionError {
    match error {
        fms_domain::error::DomainError::NotFound { entity_type, id } => {
            DomainActionError::NotFound(format!("{entity_type} {id} not found"))
        }
        fms_domain::error::DomainError::ValidationError(msg)
        | fms_domain::error::DomainError::BusinessRuleViolation(msg) => DomainActionError::Validation(msg),
        other => DomainActionError::Execution(other.to_string()),
    }
}

/// 解析 `arguments` 里的 `lat`/`lng`（数字或数字字符串），两者都必须存在。
fn parse_lat_lng(arguments: &Value) -> Result<(f64, f64), DomainActionError> {
    let lat = parse_f64_arg(arguments, "lat").ok_or_else(|| DomainActionError::Validation("lat is required".into()))?;
    let lng = parse_f64_arg(arguments, "lng").ok_or_else(|| DomainActionError::Validation("lng is required".into()))?;
    Ok((lat, lng))
}

fn parse_f64_arg(arguments: &Value, key: &str) -> Option<f64> {
    match arguments.get(key) {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// 解析可选的字符串数组字段（如 `member_user_ids`）；缺省或 null 时为 `None`。
fn parse_optional_string_array(arguments: &Value, key: &str) -> Result<Option<Vec<String>>, DomainActionError> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(items)) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    Value::String(s) => out.push(s.clone()),
                    _ => {
                        return Err(DomainActionError::Validation(format!(
                            "`{key}` must be an array of strings"
                        )))
                    }
                }
            }
            Ok(Some(out))
        }
        Some(_) => Err(DomainActionError::Validation(format!("`{key}` must be an array of strings"))),
    }
}

#[async_trait::async_trait]
impl<U: UnitOfWork> DomainActionExecution for DomainActionExecutor<U> {
    async fn execute_approved_action(
        &self,
        object_type: &str,
        object_id: &str,
        action_name: &str,
        arguments: &Value,
        executor_id: &str,
    ) -> Result<DomainActionReceipt, DomainActionError> {
        DomainActionExecutor::execute_approved_action(self, object_type, object_id, action_name, arguments, executor_id)
            .await
    }
}
