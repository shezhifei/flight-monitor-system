use serde_json::Value;
use sqlx::{Postgres, Transaction};
use std::collections::HashMap;
use std::sync::Arc;

use crate::schemas::todo_schemas::{TodoComplete, TodoCreateCommand};
use crate::services::business_case_service::BusinessCaseTerminalUpdatePayload;
use crate::services::dispatch_service::DispatchService;
use crate::services::flight_domain_events::write_flight_outbox_event;
use crate::services::flowable_service::FlowableService;
use crate::services::notification_service::NotificationCreate;
use crate::sqlx_transactional_repositories::SqlxDomainEventOutboxTransactionalRepository;
use crate::services::todo_service::TodoWriter;
use crate::types::{
    ConcreteBusinessCaseService, ConcreteFlightService, ConcreteLabelService,
    ConcreteNotificationService,
};
use fms_domain::ontology::schema_export::FLIGHT_OPS_ONTOLOGY_VERSION;
use fms_domain::ports::anomaly_repository::AnomalyTransactionalRepository;

use super::helpers::{optional_string, required_string};
use super::types::{DomainActionError, DomainActionReceipt};

pub struct DomainActionExecutor {
    flight_service: Arc<ConcreteFlightService>,
    dispatch_service: Arc<DispatchService>,
    notification_service: Arc<ConcreteNotificationService>,
    label_service: Arc<ConcreteLabelService>,
    todo_writer: Arc<TodoWriter<Transaction<'static, Postgres>>>,
    business_case_service: Arc<ConcreteBusinessCaseService>,
    flowable_service: Option<FlowableService>,
    pool: sqlx::PgPool,
    outbox_repo: Arc<dyn SqlxDomainEventOutboxTransactionalRepository>,
    anomaly_tx_repo: Arc<dyn AnomalyTransactionalRepository<Transaction<'static, Postgres>>>,
}

impl DomainActionExecutor {
    pub fn new(
        flight_service: Arc<ConcreteFlightService>,
        dispatch_service: Arc<DispatchService>,
        notification_service: Arc<ConcreteNotificationService>,
        label_service: Arc<ConcreteLabelService>,
        todo_writer: Arc<TodoWriter<Transaction<'static, Postgres>>>,
        business_case_service: Arc<ConcreteBusinessCaseService>,
        outbox_repo: Arc<dyn SqlxDomainEventOutboxTransactionalRepository>,
        anomaly_tx_repo: Arc<dyn AnomalyTransactionalRepository<Transaction<'static, Postgres>>>,
        pool: sqlx::PgPool,
    ) -> Self {
        Self {
            flight_service,
            dispatch_service,
            notification_service,
            label_service,
            todo_writer,
            business_case_service,
            flowable_service: None,
            outbox_repo,
            anomaly_tx_repo,
            pool,
        }
    }

    /// `Workflow.start` 依赖的 Flowable 网关（可选注入）。
    pub fn with_flowable_service(mut self, flowable_service: FlowableService) -> Self {
        self.flowable_service = Some(flowable_service);
        self
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
            .pool
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
            tx.commit()
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
        tx: &mut Transaction<'static, Postgres>,
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
                    .flight_service
                    .update_flight_in_tx(tx, object_id, dto, Some(executor_id.to_string()))
                    .await
                    .map_err(|e| DomainActionError::Execution(e.to_string()))?;
                if res.is_none() {
                    return Err(DomainActionError::NotFound(format!("Flight {} not found", object_id)));
                }
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
                    .flight_service
                    .update_flight_in_tx(tx, object_id, dto, Some(executor_id.to_string()))
                    .await
                    .map_err(|e| DomainActionError::Execution(e.to_string()))?;
                if res.is_none() {
                    return Err(DomainActionError::NotFound(format!("Flight {} not found", object_id)));
                }
                Ok(serde_json::json!({ "success": true, "status": status }))
            }
            "Flight.change_stand" => {
                let new_stand_id = required_string(arguments, &["new_stand_id", "stand_id"], "new_stand_id")?;
                let reason = optional_string(arguments, &["reason"]);
                let before = self
                    .flight_service
                    .get_flight(object_id)
                    .await
                    .map_err(|e| DomainActionError::Execution(e.to_string()))?
                    .ok_or_else(|| DomainActionError::NotFound(format!("Flight {} not found", object_id)))?;
                let before_snapshot = serde_json::json!({
                    "stand": before.stand.as_ref().map(|s| s.as_str()),
                    "status": before.status.clone(),
                    "version": before.version,
                });
                let dto = crate::schemas::flight_schemas::FlightUpdate {
                    expected_version: Some(before.version),
                    stand: crate::schemas::flight_schemas::NullableUpdate::Set(new_stand_id.to_string()),
                    ..Default::default()
                };
                let res = self
                    .flight_service
                    .update_flight_in_tx(tx, object_id, dto, Some(executor_id.to_string()))
                    .await
                    .map_err(|e| DomainActionError::Execution(e.to_string()))?;
                if res.is_none() {
                    return Err(DomainActionError::NotFound(format!("Flight {} not found", object_id)));
                }
                let mut result = serde_json::json!({
                    "success": true,
                    "stand": new_stand_id,
                    "before_snapshot": before_snapshot,
                    "after_preview": { "stand": new_stand_id },
                });
                if let Some(reason) = reason {
                    result["reason"] = serde_json::json!(reason);
                }
                Ok(result)
            }
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
                    .flight_service
                    .update_flight_in_tx(tx, object_id, dto, Some(executor_id.to_string()))
                    .await
                    .map_err(|e| DomainActionError::Execution(e.to_string()))?;
                if res.is_none() {
                    return Err(DomainActionError::NotFound(format!("Flight {} not found", object_id)));
                }
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
            "Notification.send" => {
                let user_id = required_string(arguments, &["user_id", "recipient_user_id"], "user_id")?;
                let title = required_string(arguments, &["title"], "title")?;
                let body = required_string(arguments, &["body", "message"], "body")?;
                let notification = self
                    .notification_service
                    .send_notification_in_tx(
                        tx,
                        NotificationCreate {
                            user_id: user_id.to_string(),
                            title: title.to_string(),
                            body: body.to_string(),
                            category: optional_string(arguments, &["category"])
                                .map(str::to_string)
                                .or_else(|| Some("ai_action".to_string())),
                            severity: optional_string(arguments, &["severity"]).map(str::to_string),
                            flight_id: optional_string(arguments, &["flight_id"]).map(str::to_string),
                            related_entity_type: optional_string(arguments, &["related_entity_type"])
                                .map(str::to_string)
                                .or_else(|| Some(object_type.to_string())),
                            related_entity_id: optional_string(arguments, &["related_entity_id"])
                                .map(str::to_string)
                                .or_else(|| Some(object_id.to_string())),
                            dispatch_order_id: optional_string(arguments, &["dispatch_order_id"]).map(str::to_string),
                            group_id: optional_string(arguments, &["group_id"]).map(str::to_string),
                            sender_user_id: Some(executor_id.to_string()),
                            sender_username_snapshot: None,
                            origin_type: Some("ai_action".to_string()),
                            receipt_required: arguments
                                .get("receipt_required")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                            receipt_group_id: optional_string(arguments, &["receipt_group_id"]).map(str::to_string),
                        },
                    )
                    .await
                    .map_err(|e| DomainActionError::Execution(e.to_string()))?;
                Ok(serde_json::json!({
                    "success": true,
                    "notification_id": notification.notification_id,
                }))
            }
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
                    .dispatch_service
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
            "DispatchOrder.reassign" => {
                let assignee_id = required_string(arguments, &["assignee_id", "team_id", "user_id"], "assignee_id")?;
                let assignee_type = optional_string(arguments, &["assignee_type"])
                    .or_else(|| arguments.get("team_id").and_then(Value::as_str).map(|_| "team"))
                    .or_else(|| arguments.get("user_id").and_then(Value::as_str).map(|_| "individual"));
                let updated = self
                    .dispatch_service
                    .reassign_order_in_tx(tx, object_id, assignee_id, assignee_type, executor_id, Some(arguments))
                    .await
                    .map_err(|e| DomainActionError::Execution(e.to_string()))?;

                Ok(serde_json::json!({
                    "success": true,
                    "order_id": updated.id,
                    "assignee_type": updated.assignee_type,
                    "team_id": updated.team_id,
                    "individual_user_id": updated.individual_user_id,
                }))
            }
            "DispatchOrder.publish" => {
                let result = self
                    .dispatch_service
                    .publish_order_in_tx(tx, object_id, executor_id)
                    .await
                    .map_err(|e| DomainActionError::Execution(e.to_string()))?;

                Ok(serde_json::json!({
                    "success": true,
                    "order_id": object_id,
                    "publication": result,
                }))
            }
            "Todo.create" => {
                let title = required_string(arguments, &["title"], "title")?;
                let todo = self
                    .todo_writer
                    .create_todo_in_tx(
                        tx,
                        TodoCreateCommand {
                            title: title.to_string(),
                            description: optional_string(arguments, &["description", "body"]).map(str::to_string),
                            priority: optional_string(arguments, &["priority"]).map(str::to_string),
                            category: optional_string(arguments, &["category"]).map(str::to_string),
                            due_date: None,
                            estimated_duration: arguments
                                .get("estimated_duration")
                                .and_then(Value::as_i64)
                                .and_then(|value| i32::try_from(value).ok()),
                            tags: arguments.get("tags").and_then(|value| {
                                value.as_array().map(|items| {
                                    items
                                        .iter()
                                        .filter_map(Value::as_str)
                                        .map(str::to_string)
                                        .collect::<Vec<_>>()
                                })
                            }),
                            agent_entity_id: optional_string(arguments, &["agent_entity_id"]).map(str::to_string),
                            source_type: Some("ai_action".to_string()),
                            source_id: Some(object_id.to_string()),
                            created_by: Some(executor_id.to_string()),
                            assigned_to: optional_string(arguments, &["assigned_to", "assignee_id"])
                                .map(str::to_string),
                        },
                        executor_id,
                    )
                    .await
                    .map_err(|e| DomainActionError::Execution(e.to_string()))?;
                Ok(serde_json::json!({
                    "success": true,
                    "todo_id": todo.id,
                }))
            }
            "Todo.complete" => {
                let actual_duration = arguments
                    .get("actual_duration")
                    .and_then(Value::as_i64)
                    .and_then(|value| i32::try_from(value).ok());
                let todo = self
                    .todo_writer
                    .complete_todo_in_tx(
                        tx,
                        object_id,
                        TodoComplete {
                            actual_duration,
                            completed_by: Some(executor_id.to_string()),
                        },
                        executor_id,
                    )
                    .await
                    .map_err(|e| DomainActionError::Execution(e.to_string()))?
                    .ok_or_else(|| DomainActionError::Execution(format!("Todo not found: {object_id}")))?;
                Ok(serde_json::json!({
                    "success": true,
                    "todo_id": todo.id,
                    "status": todo.status,
                }))
            }
            "Stand.reserve" => {
                let flight_id = required_string(arguments, &["flight_id"], "flight_id")?;
                let dto = crate::schemas::flight_schemas::FlightUpdate {
                    stand: crate::schemas::flight_schemas::NullableUpdate::Set(object_id.to_string()),
                    ..Default::default()
                };
                let res = self
                    .flight_service
                    .update_flight_in_tx(tx, flight_id, dto, Some(executor_id.to_string()))
                    .await
                    .map_err(|e| DomainActionError::Execution(e.to_string()))?;
                if res.is_none() {
                    return Err(DomainActionError::Execution(format!("Flight {} not found", flight_id)));
                }
                Ok(serde_json::json!({
                    "success": true,
                    "stand_id": object_id,
                    "flight_id": flight_id,
                }))
            }
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
                    .business_case_service
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
                    .business_case_service
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
            // `Label.add`：为航班添加标签（唯一业务写入口为 LabelService）。
            "Label.add" => {
                let flight_id = required_string(arguments, &["flight_id"], "flight_id")?;
                let label = required_string(arguments, &["label", "label_code", "code"], "label")?;
                self.label_service
                    .add_to_flight(flight_id, label, Some(executor_id))
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
                    "flight_id": flight_id,
                    "label": label,
                }))
            }
            // `Workflow.start`：发起 Flowable 流程（网关未配置时拒绝）。
            "Workflow.start" => {
                let flowable = self.flowable_service.as_ref().ok_or_else(|| {
                    DomainActionError::Execution("workflow.start requires a configured Flowable gateway".to_string())
                })?;
                let template_id = required_string(
                    arguments,
                    &["workflow_template_id", "process_key"],
                    "workflow_template_id",
                )?;
                let flight_id = optional_string(arguments, &["flight_id"]);
                let mut variables: serde_json::Map<String, Value> = arguments
                    .get("context")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                variables.insert(
                    "ontology_version".to_string(),
                    serde_json::json!(FLIGHT_OPS_ONTOLOGY_VERSION),
                );
                variables.insert("started_by".to_string(), serde_json::json!(executor_id));
                let process_instance_id = flowable
                    .start_process_instance(template_id, flight_id, Some(&variables), None)
                    .await
                    .map_err(|e| DomainActionError::Execution(e.to_string()))?;
                Ok(serde_json::json!({
                    "success": true,
                    "workflow_template_id": template_id,
                    "flight_id": flight_id,
                    "process_instance_id": process_instance_id,
                }))
            }
            _ => Err(DomainActionError::NotFound(format!("unknown action: {}", action_key))),
        }
    }
}
