use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use crate::schemas::ontology_schemas::{
    AdjustGateRequest, AdjustStandRequest, AllocateCarouselRequest, AllocateGateRequest, AllocateStandRequest,
    ReleaseResourceRequest,
};
use crate::services::business_case_service::{BusinessCaseTerminalUpdatePayload, BusinessCaseWriter};
use crate::services::dispatch_service::writer::DispatchOrderWriter;
use crate::services::dispatch_service::DispatchService;
use crate::services::flight_domain_events::write_flight_outbox_event;
use crate::services::flight_writer::FlightWriter;
use crate::services::ontology_service::{OntologyError, OntologyService};
use crate::types::{ConcreteBusinessCaseService, ConcreteFlightService};
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
            outbox_repo,
            anomaly_tx_repo,
            uow,
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
                    "DispatchOrder.reassign" => {
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

            // ⏸️ TODO: team.update_status/change_location/add_member/remove_member
            // ⏸️ TODO: personnel.update_status/change_location (with department boundary enforcement)
            // ⏸️ TODO: equipment.assign/release with slot integration

            // 占用三对象已接线（PR #本体两层改造）：执行器只做参数映射 + 调 OntologyService，
            // 禁止第二套 SQL。权限（ontology.stand.manage / gate.manage / carousel.manage）由
            // AI 提案主管线在审批/执行前验过，这里按动作声明传入对应权限码即可。
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
                    registration: optional_string(arguments, &["registration"]).unwrap_or("")
                        .to_string(),
                    gate_code: gate_code.to_string(),
                    starts_at: parse_dt_arg(arguments, "starts_at")?,
                    ends_at: parse_dt_arg(arguments, "ends_at")?,
                    flight_id: Some(flight_id.to_string()),
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
