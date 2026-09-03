use std::collections::HashMap;
use std::time::Duration;

use chrono::Utc;
use fms_domain::error::DomainError;
use fms_domain::models::business_case_workflow::BusinessCaseWorkflowRun;
use fms_domain::models::user::User;

use crate::schemas::business_case_workflow_schemas::BusinessCaseWorkflowRunDetail;
use crate::services::business_case_service::BusinessCaseTerminalUpdatePayload;
use crate::services::notification_service::DispatchBatchNotificationCreate;

use super::helpers::*;
use super::service::{BusinessCaseWorkflowService, WorkflowActor};

impl BusinessCaseWorkflowService {
    pub(super) async fn sync_receipt_group(
        &self,
        receipt_group_id: &str,
    ) -> Result<Option<BusinessCaseWorkflowRunDetail>, DomainError> {
        let runs = self.repo.list_by_receipt_group_id(receipt_group_id).await?;
        if runs.is_empty() {
            return Ok(None);
        }

        let notification_service = self.notification_service.as_ref();
        let receipt_group = if let Some(ns) = notification_service {
            ns.get_receipt_group(receipt_group_id).await?
        } else {
            None
        };

        if let Some(repo) = self.flight_runtime_projection_repository.as_ref() {
            for run in &runs {
                repo.invalidate_flight(&run.flight_id).await;
            }
        }

        let mut first_detail: Option<BusinessCaseWorkflowRunDetail> = None;
        for run in runs {
            if first_detail.is_none() {
                let mut detail = self.hydrate_run(run, None).await?;
                detail.receipt_group = receipt_group.clone();
                first_detail = Some(detail);
            }
        }

        Ok(first_detail)
    }

    pub(super) async fn hydrate_run(
        &self,
        run: BusinessCaseWorkflowRun,
        business_case_override: Option<fms_domain::models::business_case::FlightBusinessCase>,
    ) -> Result<BusinessCaseWorkflowRunDetail, DomainError> {
        let business_case = match business_case_override {
            Some(case_item) => case_item,
            None => require_linked_business_case(&run.case_id, self.business_case_service.get(&run.case_id).await?)?,
        };

        let snapshot = self.fetch_runtime_snapshot(&run).await?;
        let mut receipt_group = snapshot.receipt_group.clone();
        let mut effective_run = run.clone();
        let mut changed = false;
        if let Some(flowable_snapshot) = snapshot.flowable.as_ref() {
            changed = reconcile_run_with_snapshot(&mut effective_run, flowable_snapshot) || changed;
        }
        if changed {
            effective_run.updated_at = Utc::now();
            effective_run = self.repo.save(&effective_run).await?;
        }
        if receipt_group.is_none() {
            if let (Some(notification_service), Some(receipt_group_id)) = (
                self.notification_service.as_ref(),
                effective_run.receipt_group_id.as_deref(),
            ) {
                receipt_group = notification_service.get_receipt_group(receipt_group_id).await?;
            }
        }

        if let Some(progressed_run) = self
            .maybe_progress_receipt_workflow(effective_run.clone(), receipt_group.as_ref())
            .await?
        {
            effective_run = progressed_run;
            if let (Some(notification_service), Some(receipt_group_id)) = (
                self.notification_service.as_ref(),
                effective_run.receipt_group_id.as_deref(),
            ) {
                receipt_group = notification_service.get_receipt_group(receipt_group_id).await?;
            }
        }

        Ok(BusinessCaseWorkflowRunDetail {
            process_instance: snapshot.process_instance,
            run: effective_run,
            business_case,
            active_tasks: snapshot.active_tasks,
            historic_tasks: snapshot.historic_tasks,
            receipt_group,
        })
    }

    pub(super) async fn try_start_flowable_process(
        &self,
        template_code: &str,
        case_id: &str,
        flight_id: &str,
        flight_context: &HashMap<String, serde_json::Value>,
        description: &str,
        extra_info: &HashMap<String, serde_json::Value>,
        case_type: &str,
        actor: &WorkflowActor,
        created_at: Option<chrono::DateTime<Utc>>,
    ) -> Result<FlowableStartSnapshot, DomainError> {
        #[cfg(test)]
        {
            if *self.mock_flowable_start.lock().unwrap() {
                return Ok(FlowableStartSnapshot {
                    process_instance_id: format!("mock-process-{case_id}"),
                    process_definition_id: Some(format!("{template_code}:mock")),
                    waiting_task_id: None,
                    status: "active".to_string(),
                });
            }
        }

        let flowable_service = self.flowable_service.as_ref().ok_or_else(|| {
            DomainError::BusinessRuleViolation(
                "Flowable service unavailable for business case workflow start".to_string(),
            )
        })?;

        let definitions = self.ensure_bpmn_deployed_in_flowable(template_code).await?;
        let Some(definition) = latest_process_definition(&definitions) else {
            return Err(DomainError::BusinessRuleViolation(format!(
                "Flowable process definition not found for template={template_code}"
            )));
        };

        let variables = build_flowable_start_variables(
            template_code,
            case_id,
            flight_id,
            flight_context,
            description,
            extra_info,
            case_type,
            actor,
            created_at,
        );
        let Some(process_instance_id) = flowable_service
            .start_process_instance(case_type, Some(case_id), Some(&variables), None)
            .await
            .map_err(map_flowable_error)?
        else {
            return Err(DomainError::BusinessRuleViolation(format!(
                "Failed to start Flowable process for template={template_code}"
            )));
        };

        Ok(FlowableStartSnapshot {
            process_instance_id,
            process_definition_id: definition
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned),
            waiting_task_id: None,
            status: "active".to_string(),
        })
    }

    pub(super) async fn try_orchestrate_start(
        &self,
        run: &BusinessCaseWorkflowRun,
        case_id: &str,
        description: &str,
        extra_info: &HashMap<String, serde_json::Value>,
        actor: &WorkflowActor,
        start_payload: &mut HashMap<String, serde_json::Value>,
        start: &FlowableStartSnapshot,
        definition: &WorkflowRuntimeDefinition,
        idempotency_context: Option<&WorkflowBatchNotificationIdempotencyContext>,
    ) -> Result<Option<WorkflowStartOrchestration>, DomainError> {
        let Some(flowable_service) = self.flowable_service.as_ref() else {
            return Err(DomainError::BusinessRuleViolation(
                "Flowable service unavailable for business case workflow orchestration".to_string(),
            ));
        };

        self.continue_dispatch_tasks_inner(
            &run.process_instance_id,
            start.process_definition_id.as_deref(),
            Some(definition),
            true,
        )
        .await?;

        let active_tasks = flowable_service
            .list_tasks(&[("processInstanceId", run.process_instance_id.clone())])
            .await
            .map_err(map_flowable_error)?;

        let notification_task = locate_task_by_definition_key(&active_tasks, &definition.notification_task_id).or(self
            .wait_for_task(&run.process_instance_id, &definition.notification_task_id)
            .await);
        let notification_task = notification_task.ok_or_else(|| {
            DomainError::BusinessRuleViolation(format!(
                "Notification task not active for process={} node={}",
                run.process_instance_id, definition.notification_task_id
            ))
        })?;

        let Some(notification_service) = self.notification_service.as_ref() else {
            return Err(DomainError::BusinessRuleViolation(
                "Notification service unavailable for business case workflow orchestration".to_string(),
            ));
        };

        let recipients = self
            .resolve_recipients(&definition.notification_targets, &definition.recipient_resolver)
            .await?
            .into_iter()
            .map(user_to_recipient_snapshot)
            .collect::<Vec<_>>();

        let template_variables = build_template_variables(
            case_id,
            &run.flight_id,
            &run.flight_context_snapshot,
            extra_info,
            description,
            &recipients,
        );
        let title = render_template(&definition.notification_title, &template_variables);
        let body = build_notification_body(
            &definition.notification_body,
            &template_variables,
            definition.append_extra_info,
            extra_info,
        );

        let batch_result = notification_service
            .send_batch_with_idempotency(
                DispatchBatchNotificationCreate {
                    user_ids: recipients
                        .iter()
                        .filter_map(|item| {
                            item.get("user_id")
                                .and_then(serde_json::Value::as_str)
                                .map(ToOwned::to_owned)
                        })
                        .collect(),
                    title: title.clone(),
                    body: body.clone(),
                    category: "dispatch".to_string(),
                    severity: definition.notification_severity.clone(),
                    flight_id: Some(run.flight_id.clone()),
                    related_entity_type: Some("business_case".to_string()),
                    related_entity_id: Some(case_id.to_string()),
                    dispatch_order_id: None,
                    group_id: None,
                    sender_user_id: actor.user_id.clone(),
                    sender_username_snapshot: actor.sender_username_snapshot(),
                    origin_type: "workflow".to_string(),
                    receipt_required: definition.receipt_required,
                },
                idempotency_context.and_then(|context| context.receipt_group_id_override.clone()),
                idempotency_context.map(|context| context.notification_id_seed.clone()),
            )
            .await?;

        let receipt_group_id = batch_result
            .get("receipt_group_id")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned);
        if definition.receipt_required && receipt_group_id.is_none() {
            return Err(DomainError::BusinessRuleViolation(
                "Notification batch did not return receipt_group_id".to_string(),
            ));
        }
        start_payload.insert(
            "notification_title".to_string(),
            serde_json::Value::String(title.clone()),
        );
        start_payload.insert("notification_body".to_string(), serde_json::Value::String(body.clone()));
        start_payload.insert(
            "process_definition_key".to_string(),
            serde_json::Value::String(run.process_definition_key.clone()),
        );

        let mut notification_sent_run = run.clone();
        notification_sent_run.receipt_group_id = receipt_group_id.clone();
        notification_sent_run.recipient_snapshot = recipients.clone();
        notification_sent_run.status = "notification_sent".to_string();
        notification_sent_run.start_payload = start_payload.clone();
        notification_sent_run.updated_at = Utc::now();
        let _ = self.repo.save(&notification_sent_run).await?;

        if let Some(receipt_group_id) = receipt_group_id.as_deref() {
            if notification_service
                .get_receipt_group(receipt_group_id)
                .await?
                .is_some()
            {
                if let Some(repo) = self.flight_runtime_projection_repository.as_ref() {
                    repo.invalidate_flight(&run.flight_id).await;
                }
            }
        }

        let notification_task_id = task_identifier(&notification_task).ok_or_else(|| {
            DomainError::BusinessRuleViolation(format!(
                "Notification task missing id for process={} node={}",
                run.process_instance_id, definition.notification_task_id
            ))
        })?;
        let mut variables = serde_json::Map::new();
        if let Some(receipt_group_id) = receipt_group_id.clone() {
            variables.insert(
                "receiptGroupId".to_string(),
                serde_json::Value::String(receipt_group_id),
            );
            variables.insert(
                "receipt_group_id".to_string(),
                variables
                    .get("receiptGroupId")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
            );
        }
        variables.insert(
            "notificationTitle".to_string(),
            serde_json::Value::String(title.clone()),
        );
        variables.insert("notificationBody".to_string(), serde_json::Value::String(body.clone()));
        variables.insert(
            "recipientSnapshot".to_string(),
            serde_json::to_value(&recipients).unwrap_or_else(|_| serde_json::json!([])),
        );
        variables.insert(
            "notificationRecipientCount".to_string(),
            serde_json::Value::from(recipients.len() as i64),
        );
        variables.insert(
            "notificationRecipientIds".to_string(),
            serde_json::Value::Array(
                recipients
                    .iter()
                    .filter_map(|item| {
                        item.get("user_id")
                            .and_then(serde_json::Value::as_str)
                            .map(|value| serde_json::Value::String(value.to_string()))
                    })
                    .collect(),
            ),
        );
        variables.insert(
            "completionPolicy".to_string(),
            serde_json::Value::String(definition.completion_policy.clone()),
        );
        variables.insert(
            "rejectPolicy".to_string(),
            serde_json::Value::String(definition.reject_policy.clone()),
        );
        let notification_completed = flowable_service
            .complete_task(&notification_task_id, Some(&variables))
            .await
            .map_err(map_flowable_error)?;
        if !notification_completed {
            return Err(DomainError::BusinessRuleViolation(format!(
                "Failed to complete notification task {notification_task_id}"
            )));
        }

        let wait_task = self
            .wait_for_task(&run.process_instance_id, &definition.wait_task_id)
            .await;
        let waiting_task_id = wait_task.as_ref().and_then(task_identifier);

        Ok(Some(WorkflowStartOrchestration {
            waiting_task_id,
            receipt_group_id,
            recipient_snapshot: recipients,
            status: Some(if definition.receipt_required {
                "waiting_receipts".to_string()
            } else {
                "notification_sent".to_string()
            }),
        }))
    }

    pub(super) async fn fetch_runtime_snapshot(
        &self,
        run: &BusinessCaseWorkflowRun,
    ) -> Result<RuntimeSnapshot, DomainError> {
        if self.flowable_service.is_none() {
            return Err(DomainError::BusinessRuleViolation(
                "Flowable service unavailable for business case workflow runtime".to_string(),
            ));
        }

        let receipt_group = match (self.notification_service.as_ref(), run.receipt_group_id.as_deref()) {
            (Some(notification_service), Some(receipt_group_id)) => {
                notification_service.get_receipt_group(receipt_group_id).await?
            }
            _ => None,
        };

        let flowable_snapshot = self.fetch_flowable_snapshot(run).await?;

        Ok(RuntimeSnapshot {
            process_instance: flowable_snapshot
                .as_ref()
                .map(|snapshot| snapshot.process_instance.clone()),
            active_tasks: flowable_snapshot
                .as_ref()
                .map(|snapshot| snapshot.active_tasks.clone())
                .unwrap_or_default(),
            historic_tasks: flowable_snapshot
                .as_ref()
                .map(|snapshot| snapshot.historic_tasks.clone())
                .unwrap_or_default(),
            receipt_group,
            flowable: flowable_snapshot,
        })
    }

    pub(super) async fn fetch_flowable_snapshot(
        &self,
        run: &BusinessCaseWorkflowRun,
    ) -> Result<Option<FlowableRunSnapshot>, DomainError> {
        let flowable_service = self.flowable_service.as_ref().ok_or_else(|| {
            DomainError::BusinessRuleViolation(
                "Flowable service unavailable for business case workflow runtime".to_string(),
            )
        })?;

        let runtime_instance = flowable_service
            .get_process_instance(&run.process_instance_id)
            .await
            .map_err(map_flowable_error)?;
        let historic_instance = if runtime_instance.is_none() {
            flowable_service
                .list_historic_process_instances(&[("businessKey", run.case_id.clone())])
                .await
                .map_err(map_flowable_error)?
                .into_iter()
                .find(|item| {
                    item.get("id")
                        .and_then(serde_json::Value::as_str)
                        .map(|value| value == run.process_instance_id)
                        .unwrap_or(false)
                })
        } else {
            None
        };
        let Some(instance) = runtime_instance.or(historic_instance) else {
            return Ok(None);
        };

        let active_tasks = flowable_service
            .list_tasks(&[("processInstanceId", run.process_instance_id.clone())])
            .await
            .map_err(map_flowable_error)?;
        let historic_tasks = flowable_service
            .list_historic_tasks(&[("processInstanceId", run.process_instance_id.clone())])
            .await
            .map_err(map_flowable_error)?;

        let runtime_variables = if active_tasks.is_empty() {
            None
        } else {
            Some(normalize_variable_payload(
                flowable_service
                    .get_process_instance_variables(&run.process_instance_id)
                    .await
                    .map_err(map_flowable_error)?,
            ))
        };
        let historic_variables = if runtime_variables.is_some() {
            None
        } else {
            Some(normalize_historic_variables(
                flowable_service
                    .list_historic_variable_instances(&[("processInstanceId", run.process_instance_id.clone())])
                    .await
                    .map_err(map_flowable_error)?,
            ))
        };
        let variables = runtime_variables.or(historic_variables).unwrap_or_default();
        let wait_task = resolve_wait_task(&active_tasks, run);
        let receipt_group_id = extract_optional_string(&variables, &["receiptGroupId", "receipt_group_id"]);
        let status = derive_flowable_run_status(&active_tasks, &wait_task, run, receipt_group_id.as_deref());

        let process_instance = normalize_process_instance(instance, &active_tasks, &variables, wait_task.as_ref());

        Ok(Some(FlowableRunSnapshot {
            process_instance,
            active_tasks,
            historic_tasks,
            variables,
            wait_task_id: wait_task.as_ref().and_then(task_identifier),
            receipt_group_id,
            status,
        }))
    }

    pub(super) async fn wait_for_task(
        &self,
        process_instance_id: &str,
        task_definition_key: &str,
    ) -> Option<serde_json::Value> {
        let flowable_service = self.flowable_service.as_ref()?;
        let max_attempts = 8;
        let initial_delay_ms: u64 = 500;

        for attempt in 0..max_attempts {
            if let Ok(tasks) = flowable_service
                .list_tasks(&[("processInstanceId", process_instance_id.to_string())])
                .await
            {
                if let Some(task) = locate_task_by_definition_key(&tasks, task_definition_key) {
                    return Some(task);
                }
            }

            if attempt < max_attempts - 1 {
                let delay_ms = initial_delay_ms * 2_u64.pow(attempt);
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }
        }
        None
    }

    pub(super) async fn resolve_recipients(
        &self,
        targets: &[WorkflowNotificationTarget],
        resolver: &WorkflowRecipientResolverConfig,
    ) -> Result<Vec<User>, DomainError> {
        let Some(user_repo) = self.user_repo.as_ref() else {
            return Ok(Vec::new());
        };

        let mut offset = 0;
        let limit = 200;
        let mut users = Vec::new();
        loop {
            let page = user_repo.find_all(limit, offset).await?;
            if page.is_empty() {
                break;
            }
            offset += page.len() as i64;
            users.extend(page);
            if users.len() % limit as usize != 0 {
                break;
            }
        }

        let mut recipients = Vec::new();
        for user in users {
            if !user.is_active {
                continue;
            }
            for target in targets {
                if !matches_department(&user, &target.department) {
                    continue;
                }
                if !target.roles.is_empty() && !matches_any_role(&user, &target.roles) {
                    continue;
                }
                recipients.push(user.clone());
            }
        }

        if resolver.deduplicate {
            let mut deduped = HashMap::new();
            for user in recipients {
                deduped.entry(user.id.clone()).or_insert(user);
            }
            recipients = deduped.into_values().collect::<Vec<_>>();
        }

        recipients.sort_by(|left, right| left.id.cmp(&right.id));
        if recipients.is_empty() {
            match resolver.empty_policy.trim() {
                "skip" => {
                    return Ok(Vec::new());
                }
                _ => {
                    return Err(DomainError::BusinessRuleViolation(
                        "No recipients resolved for notification rule".to_string(),
                    ));
                }
            }
        }
        Ok(recipients)
    }

    pub(super) async fn maybe_progress_receipt_workflow(
        &self,
        run: BusinessCaseWorkflowRun,
        receipt_group: Option<&serde_json::Value>,
    ) -> Result<Option<BusinessCaseWorkflowRun>, DomainError> {
        if run.completed_at.is_some() || matches!(run.status.as_str(), "completed" | "failed") {
            return Ok(None);
        }

        let Some(receipt_group) = receipt_group else {
            return Ok(None);
        };
        let Some(outcome) = resolve_receipt_group_outcome(receipt_group) else {
            return Ok(None);
        };

        let outcome_value = outcome.as_str().to_string();
        let failed_reason = if outcome == ReceiptWorkflowOutcome::Rejected {
            derive_receipt_failed_reason(receipt_group)
        } else {
            None
        };

        if run.outcome.as_deref() == Some(outcome_value.as_str())
            && (outcome != ReceiptWorkflowOutcome::Rejected || run.failed_reason == failed_reason)
            && matches!(run.status.as_str(), "completing_case" | "completed" | "failed")
        {
            return Ok(None);
        }

        let mut run = run;
        run.outcome = Some(outcome_value.clone());
        run.failed_reason = failed_reason.clone();

        if self.flowable_service.is_none() {
            run.status = if outcome == ReceiptWorkflowOutcome::Rejected {
                "failed".to_string()
            } else {
                "completed".to_string()
            };
            run.completed_at = Some(Utc::now());
            run.updated_at = Utc::now();
            let saved = self.repo.save(&run).await?;
            return Ok(Some(saved));
        }

        let definition = self.load_runtime_definition_for_run(&run).await?;

        run.status = "completing_case".to_string();
        run.updated_at = Utc::now();
        let mut saved = self.repo.save(&run).await?;

        let wait_task_id = match saved.waiting_task_id.clone() {
            Some(waiting_task_id) => waiting_task_id,
            None => {
                let wait_task = self
                    .wait_for_task(&saved.process_instance_id, &definition.wait_task_id)
                    .await
                    .ok_or_else(|| {
                        DomainError::BusinessRuleViolation(format!(
                            "Waiting receipt task not found for process {}",
                            saved.process_instance_id
                        ))
                    })?;
                let waiting_task_id = task_identifier(&wait_task).ok_or_else(|| {
                    DomainError::BusinessRuleViolation(format!(
                        "Waiting receipt task missing id for process {}",
                        saved.process_instance_id
                    ))
                })?;
                saved.waiting_task_id = Some(waiting_task_id.clone());
                saved.updated_at = Utc::now();
                saved = self.repo.save(&saved).await?;
                waiting_task_id
            }
        };

        let variables = build_wait_receipt_completion_variables(&saved, &outcome_value, failed_reason.as_deref());

        let flowable_service = self.flowable_service.as_ref().expect("checked above");
        let wait_completed = flowable_service
            .complete_task(&wait_task_id, Some(&variables))
            .await
            .map_err(map_flowable_error)?;
        if !wait_completed {
            return Err(DomainError::BusinessRuleViolation(format!(
                "Failed to complete wait_receipts task: {wait_task_id}"
            )));
        }

        let action = match outcome {
            ReceiptWorkflowOutcome::Confirmed => &definition.success_action,
            ReceiptWorkflowOutcome::Rejected => &definition.failure_action,
        };
        let action_task = self
            .wait_for_task(&saved.process_instance_id, &action.node_id)
            .await
            .ok_or_else(|| {
                DomainError::BusinessRuleViolation(format!("Business case action task not found: {}", action.node_id))
            })?;
        let action_task_id = task_identifier(&action_task).ok_or_else(|| {
            DomainError::BusinessRuleViolation(format!("Business case action task missing id: {}", action.node_id))
        })?;

        if action.require_case_id && saved.case_id.trim().is_empty() {
            return Err(DomainError::BusinessRuleViolation(
                "businessCaseAction requires caseId but none was provided".to_string(),
            ));
        }

        let runtime_variables =
            build_runtime_variables(&saved, receipt_group, outcome_value.as_str(), failed_reason.as_deref());
        let rendered_reason = action
            .reason_template
            .as_deref()
            .map(|template| render_template(template, &runtime_variables));

        if !saved.case_id.trim().is_empty() {
            let action_result = self
                .business_case_service
                .apply_workflow_terminal_action(
                    &saved.case_id,
                    BusinessCaseTerminalUpdatePayload {
                        action: action.action.clone(),
                        target_status: action.target_status.clone(),
                        actor: saved.started_by.clone(),
                        reason: rendered_reason,
                        write_finished_at: action.write_finished_at,
                        workflow_run_id: Some(saved.run_id.clone()),
                        workflow_outcome: Some(outcome_value.clone()),
                        receipt_group_id: saved.receipt_group_id.clone(),
                    },
                )
                .await;

            match action_result {
                Ok(Some(_)) => {}
                Ok(None) => {
                    let case_id = saved.case_id.clone();
                    saved = self
                        .persist_run_as_system_error(
                            saved,
                            DomainError::NotFound {
                                entity_type: "business_case",
                                id: case_id,
                            }
                            .to_string(),
                        )
                        .await?;
                    return Ok(Some(saved));
                }
                Err(error) => {
                    saved = self.persist_run_as_system_error(saved, error.to_string()).await?;
                    return Ok(Some(saved));
                }
            }
        }

        let action_completed = flowable_service
            .complete_task(&action_task_id, None)
            .await
            .map_err(map_flowable_error)?;
        if !action_completed {
            return Err(DomainError::BusinessRuleViolation(format!(
                "Failed to complete business case action task: {action_task_id}"
            )));
        }

        saved.status = if outcome == ReceiptWorkflowOutcome::Rejected {
            "failed".to_string()
        } else {
            "completed".to_string()
        };
        saved.completed_at = Some(Utc::now());
        saved.updated_at = Utc::now();
        let saved = self.repo.save(&saved).await?;
        Ok(Some(saved))
    }

    pub(super) async fn load_start_runtime_definition(
        &self,
        case_type: &str,
        process_definition_id: Option<&str>,
    ) -> Result<(WorkflowRuntimeDefinition, String), DomainError> {
        if let Some(bpmn_xml) = self.load_bpmn_xml_from_repository(case_type).await? {
            return Ok((parse_bpmn_runtime_definition(&bpmn_xml)?, "db".to_string()));
        }
        if let Some(bpmn_xml) = self.load_bpmn_xml_from_file(case_type).await? {
            return Ok((parse_bpmn_runtime_definition(&bpmn_xml)?, "file".to_string()));
        }
        if let Some(bpmn_xml) = self
            .load_bpmn_xml_from_flowable(case_type, process_definition_id)
            .await?
        {
            return Ok((parse_bpmn_runtime_definition(&bpmn_xml)?, "flowable".to_string()));
        }
        Err(DomainError::BusinessRuleViolation(format!(
            "No BPMN definition available for case_type={case_type}"
        )))
    }

    pub(super) async fn persist_run_as_system_error(
        &self,
        run: BusinessCaseWorkflowRun,
        failed_reason: String,
    ) -> Result<BusinessCaseWorkflowRun, DomainError> {
        let failed_run = mark_run_as_system_error(run, &failed_reason);
        self.repo.save(&failed_run).await
    }

    pub(super) async fn load_bpmn_xml_from_repository(&self, case_type: &str) -> Result<Option<String>, DomainError> {
        let Some(case_type_service) = self.business_case_type_service.as_ref() else {
            return Ok(None);
        };
        Ok(case_type_service
            .find_by_code(case_type)
            .await?
            .and_then(|case_type_item| case_type_item.bpmn_xml)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()))
    }

    pub(super) async fn load_workflow_batch_policy(&self, case_type: &str) -> Result<WorkflowBatchPolicy, DomainError> {
        let Some(case_type_service) = self.business_case_type_service.as_ref() else {
            return Ok(WorkflowBatchPolicy::default());
        };
        let Some(case_type_item) = case_type_service.find_by_code(case_type).await? else {
            return Ok(WorkflowBatchPolicy::default());
        };
        Ok(parse_workflow_batch_policy(&case_type_item.case_properties))
    }

    pub(super) async fn load_bpmn_xml_from_file(&self, case_type: &str) -> Result<Option<String>, DomainError> {
        let Some(bpmn_dir) = self.bpmn_dir.as_ref() else {
            return Ok(None);
        };
        let bpmn_path = bpmn_dir.join(format!("{case_type}.bpmn"));
        let exists = tokio::fs::try_exists(&bpmn_path).await.map_err(|error| {
            DomainError::Internal(format!("Failed to inspect BPMN file {}: {error}", bpmn_path.display()))
        })?;
        if !exists {
            return Ok(None);
        }
        let bpmn_xml = tokio::fs::read_to_string(&bpmn_path).await.map_err(|error| {
            DomainError::Internal(format!("Failed to read BPMN file {}: {error}", bpmn_path.display()))
        })?;
        let bpmn_xml = bpmn_xml.trim().to_string();
        if bpmn_xml.is_empty() {
            return Ok(None);
        }
        Ok(Some(bpmn_xml))
    }

    pub(super) async fn load_bpmn_xml_from_flowable(
        &self,
        case_type: &str,
        process_definition_id: Option<&str>,
    ) -> Result<Option<String>, DomainError> {
        let flowable_service = self.flowable_service.as_ref().ok_or_else(|| {
            DomainError::BusinessRuleViolation("Flowable service unavailable for BPMN definition loading".to_string())
        })?;
        let resolved_process_definition_id = match process_definition_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
        {
            Some(value) => value,
            None => {
                let definitions = self.ensure_bpmn_deployed_in_flowable(case_type).await?;
                let Some(definition) = latest_process_definition(&definitions) else {
                    return Ok(None);
                };
                let Some(value) = definition
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                else {
                    return Ok(None);
                };
                value
            }
        };
        flowable_service
            .get_process_definition_xml(&resolved_process_definition_id)
            .await
            .map_err(map_flowable_error)
    }

    pub(super) async fn load_runtime_definition_for_run(
        &self,
        run: &BusinessCaseWorkflowRun,
    ) -> Result<WorkflowRuntimeDefinition, DomainError> {
        let flowable_service = self.flowable_service.as_ref().ok_or_else(|| {
            DomainError::BusinessRuleViolation(
                "Flowable service unavailable for receipt-driven workflow progression".to_string(),
            )
        })?;

        let process_definition_id = if let Some(value) = run.start_payload.get("process_definition_id") {
            value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        } else {
            None
        };

        let resolved_process_definition_id = match process_definition_id {
            Some(value) => value,
            None => {
                let definitions = self
                    .ensure_bpmn_deployed_in_flowable(&run.process_definition_key)
                    .await?;
                let latest = latest_process_definition(&definitions).ok_or_else(|| {
                    DomainError::BusinessRuleViolation(format!(
                        "Cannot resolve process definition for key={}",
                        run.process_definition_key
                    ))
                })?;
                latest
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| {
                        DomainError::BusinessRuleViolation(format!(
                            "Latest process definition for key={} is missing id",
                            run.process_definition_key
                        ))
                    })?
            }
        };

        let bpmn_xml = flowable_service
            .get_process_definition_xml(&resolved_process_definition_id)
            .await
            .map_err(map_flowable_error)?
            .ok_or_else(|| {
                DomainError::BusinessRuleViolation(format!(
                    "Cannot load BPMN XML for process definition {}",
                    resolved_process_definition_id
                ))
            })?;
        parse_bpmn_runtime_definition(&bpmn_xml)
    }
}
