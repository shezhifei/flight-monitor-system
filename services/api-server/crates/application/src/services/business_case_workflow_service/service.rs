use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use fms_domain::error::DomainError;
use fms_domain::models::business_case::VisibilityScope;
use fms_domain::models::business_case_workflow::BusinessCaseWorkflowRun;
use fms_domain::ports::business_case_workflow_run_repository::BusinessCaseWorkflowRunRepository;
use fms_domain::ports::flight_runtime_projection_repository::FlightRuntimeProjectionRepository;
use fms_domain::ports::user_repository::UserRepository;

pub(super) use super::helpers::*;
use crate::schemas::business_case_workflow_schemas::{
    BusinessCaseWorkflowRunDetail, BusinessCaseWorkflowStartData, BusinessCaseWorkflowStartRequest,
};
use crate::services::flowable_service::FlowableService;
use crate::services::notification_service::{DispatchBatchNotificationCreate, NotificationReceiptGroupSync};
use crate::types::{
    ConcreteBusinessCaseTypeService, ConcreteFlightService, ConcreteNotificationService,
    ConcreteWorkflowDispatchService,
};

pub struct BusinessCaseWorkflowService {
    pub(super) repo: Arc<dyn BusinessCaseWorkflowRunRepository + Send + Sync>,
    pub(super) business_case_service: Arc<dyn crate::services::business_case_service::BusinessCaseServiceOps>,
    pub(super) flight_service: Arc<ConcreteFlightService>,
    pub(super) business_case_type_service: Option<Arc<ConcreteBusinessCaseTypeService>>,
    pub(super) flowable_service: Option<Arc<FlowableService>>,
    pub(super) notification_service: Option<Arc<ConcreteNotificationService>>,
    pub(super) user_repo: Option<Arc<dyn UserRepository + Send + Sync>>,
    pub(super) workflow_dispatch_service: Option<Arc<ConcreteWorkflowDispatchService>>,
    pub(super) flight_runtime_projection_repository: Option<Arc<dyn FlightRuntimeProjectionRepository + Send + Sync>>,
    pub(super) bpmn_dir: Option<PathBuf>,
    #[cfg(test)]
    pub mock_dispatch_result: Arc<std::sync::Mutex<Option<Result<BusinessCaseWorkflowBatchResult, String>>>>,
    #[cfg(test)]
    pub mock_flowable_start: Arc<std::sync::Mutex<bool>>,
    #[cfg(test)]
    pub mock_batch_notification_result: Arc<std::sync::Mutex<Option<serde_json::Value>>>,
    #[cfg(test)]
    pub mock_batch_notifications: Arc<std::sync::Mutex<Vec<DispatchBatchNotificationCreate>>>,
}

#[derive(Debug, Clone, Default)]
pub struct WorkflowActor {
    pub actor: String,
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub name_snapshot: Option<String>,
    pub context_type: Option<String>,
    pub context_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BusinessCaseWorkflowBatchItem {
    pub template_code: String,
    pub case_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct BusinessCaseWorkflowBatchResult {
    pub started: Vec<BusinessCaseWorkflowStartData>,
    pub notification_groups: Vec<BusinessCaseWorkflowNotificationGroup>,
}

#[derive(Debug, Clone)]
pub struct BusinessCaseWorkflowNotificationGroup {
    pub receipt_group_id: Option<String>,
    pub case_type: String,
    pub case_ids: Vec<String>,
    pub title: String,
    pub body: String,
}

impl WorkflowActor {
    pub(super) fn operator(&self) -> String {
        let operator = self.actor.trim();
        if operator.is_empty() {
            self.started_by()
        } else {
            operator.to_string()
        }
    }

    pub(super) fn started_by(&self) -> String {
        self.username
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .or(self.user_id.as_deref())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(self.actor.trim())
            .to_string()
    }

    pub(super) fn operator_name_snapshot(&self) -> String {
        self.name_snapshot
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .or(self.username.as_deref())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(self.operator().as_str())
            .to_string()
    }

    pub(super) fn sender_username_snapshot(&self) -> Option<String> {
        self.username
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| {
                let started_by = self.started_by();
                (!started_by.trim().is_empty()).then_some(started_by)
            })
    }
}

impl BusinessCaseWorkflowService {
    pub fn new(
        repo: Arc<dyn BusinessCaseWorkflowRunRepository + Send + Sync>,
        business_case_service: Arc<dyn crate::services::business_case_service::BusinessCaseServiceOps>,
        flight_service: Arc<ConcreteFlightService>,
    ) -> Self {
        Self {
            repo,
            business_case_service,
            flight_service,
            business_case_type_service: None,
            flowable_service: None,
            notification_service: None,
            user_repo: None,
            workflow_dispatch_service: None,
            flight_runtime_projection_repository: None,
            bpmn_dir: None,
            #[cfg(test)]
            mock_dispatch_result: Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            mock_flowable_start: Arc::new(std::sync::Mutex::new(false)),
            #[cfg(test)]
            mock_batch_notification_result: Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            mock_batch_notifications: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    pub fn with_business_case_type_service(
        mut self,
        business_case_type_service: Arc<ConcreteBusinessCaseTypeService>,
    ) -> Self {
        self.business_case_type_service = Some(business_case_type_service);
        self
    }

    pub fn with_flowable_service(mut self, flowable_service: Arc<FlowableService>) -> Self {
        self.flowable_service = Some(flowable_service);
        self
    }

    pub fn with_notification_service(mut self, notification_service: Arc<ConcreteNotificationService>) -> Self {
        self.notification_service = Some(notification_service);
        self
    }

    pub fn with_workflow_dispatch_service(
        mut self,
        workflow_dispatch_service: Arc<ConcreteWorkflowDispatchService>,
    ) -> Self {
        self.workflow_dispatch_service = Some(workflow_dispatch_service);
        self
    }

    pub fn with_flight_runtime_projection_repository(
        mut self,
        repo: Arc<dyn FlightRuntimeProjectionRepository + Send + Sync>,
    ) -> Self {
        self.flight_runtime_projection_repository = Some(repo);
        self
    }

    pub fn with_bpmn_dir(mut self, bpmn_dir: impl Into<PathBuf>) -> Self {
        self.bpmn_dir = Some(bpmn_dir.into());
        self
    }
}

impl BusinessCaseWorkflowService {
    pub fn with_user_repository(mut self, user_repo: Arc<dyn UserRepository + Send + Sync>) -> Self {
        self.user_repo = Some(user_repo);
        self
    }

    pub(super) async fn ensure_bpmn_deployed_in_flowable(
        &self,
        template_code: &str,
    ) -> Result<Vec<serde_json::Value>, DomainError> {
        let flowable_service = self
            .flowable_service
            .as_ref()
            .ok_or_else(|| DomainError::Internal("Flowable service unavailable".to_string()))?;

        let mut definitions = flowable_service
            .list_process_definitions(Some(template_code), None)
            .await
            .map_err(map_flowable_error)?;

        if definitions.is_empty() {
            tracing::info!(
                "BPMN definition for template_code={} not found in Flowable, triggering on-demand auto-deploy",
                template_code
            );
            if let Some(case_type_service) = self.business_case_type_service.as_ref() {
                if let Ok(Some(case_type_entity)) = case_type_service.find_by_code(template_code).await {
                    if let Some(bpmn_xml) = case_type_entity.bpmn_xml {
                        if !bpmn_xml.trim().is_empty() {
                            let filename = format!("{}.bpmn20.xml", template_code);
                            match flowable_service.deploy_process_definition(&bpmn_xml, &filename).await {
                                Ok(_) => tracing::info!("Successfully deployed BPMN for {}", template_code),
                                Err(e) => tracing::warn!(
                                    "Failed to on-demand auto-deploy BPMN for {}: {:?}",
                                    template_code,
                                    e
                                ),
                            }
                            definitions = flowable_service
                                .list_process_definitions(Some(template_code), None)
                                .await
                                .map_err(map_flowable_error)?;
                        }
                    }
                }
            }
        }
        Ok(definitions)
    }

    pub async fn start_workflow(
        &self,
        template_code: &str,
        payload: BusinessCaseWorkflowStartRequest,
        actor: &WorkflowActor,
    ) -> Result<BusinessCaseWorkflowStartData, DomainError> {
        self.start_workflow_with_case_scope(template_code, payload, actor, VisibilityScope::Common, None, None)
            .await
    }

    pub async fn start_workflow_for_viewer(
        &self,
        template_code: &str,
        payload: BusinessCaseWorkflowStartRequest,
        actor: &WorkflowActor,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<BusinessCaseWorkflowStartData, DomainError> {
        let Some(case_type_service) = self.business_case_type_service.as_ref() else {
            return self.start_workflow(template_code, payload, actor).await;
        };
        let case_type = case_type_service
            .find_by_code_for_viewer(template_code, viewer_department_id, viewer_department_name)
            .await?
            .ok_or_else(|| DomainError::PermissionDenied("无权访问业务事项流程模板".to_string()))?;

        self.start_workflow_with_case_scope(
            template_code,
            payload,
            actor,
            case_type.visibility_scope,
            case_type.department_id.as_deref().or(viewer_department_id),
            case_type.department_name_snapshot.as_deref().or(viewer_department_name),
        )
        .await
    }

    async fn start_workflow_with_case_scope(
        &self,
        template_code: &str,
        payload: BusinessCaseWorkflowStartRequest,
        actor: &WorkflowActor,
        case_scope: VisibilityScope,
        case_department_id: Option<&str>,
        case_department_name: Option<&str>,
    ) -> Result<BusinessCaseWorkflowStartData, DomainError> {
        let normalized_template_code = template_code.trim();
        if normalized_template_code.is_empty() {
            return Err(DomainError::ValidationError("template_code is required".to_string()));
        }

        let flight = self
            .flight_service
            .get_flight(payload.flight_id.trim())
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "flight",
                id: payload.flight_id.clone(),
            })?;

        let flight_id = flight
            .flight_id
            .clone()
            .unwrap_or_else(|| payload.flight_id.trim().to_string());
        let flight_no = flight
            .flight_number
            .clone()
            .unwrap_or_else(|| payload.flight_id.trim().to_string());

        self.flowable_service
            .as_ref()
            .ok_or_else(|| DomainError::Internal("Flowable service unavailable".to_string()))?;
        let definitions = self.ensure_bpmn_deployed_in_flowable(normalized_template_code).await?;
        let definition_meta = latest_process_definition(&definitions).ok_or_else(|| {
            DomainError::BusinessRuleViolation(format!(
                "Flowable process definition not found for template={normalized_template_code}"
            ))
        })?;
        let process_definition_id = definition_meta
            .get("id")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| DomainError::BusinessRuleViolation("Flowable process definition missing id".to_string()))?;
        let (runtime_definition, bpmn_source) = self
            .load_start_runtime_definition(normalized_template_code, Some(process_definition_id))
            .await?;
        let started_by = actor.started_by();

        let flight_context_snapshot = build_flight_context_snapshot(&flight);
        let normalized_extra_info = normalize_workflow_extra_info(
            &payload.extra_info,
            payload.description.trim(),
            &flight_context_snapshot,
            flight.gate.as_deref(),
            flight.stand.as_deref(),
        );
        let case_context = HashMap::from([
            (
                "template_code".to_string(),
                serde_json::Value::String(normalized_template_code.to_string()),
            ),
            (
                "flight_context".to_string(),
                serde_json::to_value(&flight_context_snapshot).unwrap_or_else(|_| serde_json::json!({})),
            ),
            (
                "extra_info".to_string(),
                serde_json::to_value(&normalized_extra_info).unwrap_or_else(|_| serde_json::json!({})),
            ),
        ]);
        let business_case = self
            .business_case_service
            .create_workflow_case_for_viewer(
                &flight_id,
                &flight_no,
                &runtime_definition.case_type,
                payload.description.trim(),
                &started_by,
                case_context,
                flight.stand.clone(),
                flight.gate.clone(),
                case_scope,
                case_department_id,
                case_department_name,
            )
            .await?;
        let flowable_start = self
            .try_start_flowable_process(
                normalized_template_code,
                &business_case.case_id,
                &flight_id,
                &flight_context_snapshot,
                &payload.description,
                &normalized_extra_info,
                &runtime_definition.case_type,
                actor,
                Some(business_case.created_at),
            )
            .await?;
        let now = Utc::now();
        let run_id = ulid::Ulid::new().to_string();
        let process_instance_id = flowable_start.process_instance_id.clone();
        let process_definition_key = runtime_definition.case_type.clone();

        let mut start_payload = build_workflow_start_payload(
            &business_case.case_id,
            flowable_start.process_definition_id.as_deref(),
            &process_definition_key,
            &bpmn_source,
            &normalized_extra_info,
        );

        let mut run = BusinessCaseWorkflowRun {
            run_id: run_id.clone(),
            template_code: normalized_template_code.to_string(),
            case_id: business_case.case_id.clone(),
            flight_id: flight_id.clone(),
            process_definition_key,
            process_instance_id,
            waiting_task_id: flowable_start.waiting_task_id.clone(),
            receipt_group_id: None,
            status: flowable_start.status.clone(),
            outcome: None,
            recipient_snapshot: Vec::new(),
            flight_context_snapshot,
            start_payload: HashMap::new(),
            started_by: started_by.to_string(),
            completed_at: None,
            failed_reason: None,
            created_at: now,
            updated_at: now,
        };

        if let Some(orchestration) = self
            .try_orchestrate_start(
                &run,
                &business_case.case_id,
                &payload.description,
                &normalized_extra_info,
                actor,
                &mut start_payload,
                &flowable_start,
                &runtime_definition,
                None,
            )
            .await?
        {
            if let Some(waiting_task_id) = orchestration.waiting_task_id {
                run.waiting_task_id = Some(waiting_task_id);
            }
            if let Some(receipt_group_id) = orchestration.receipt_group_id {
                run.receipt_group_id = Some(receipt_group_id);
            }
            if let Some(status) = orchestration.status {
                run.status = status;
            }
            if !orchestration.recipient_snapshot.is_empty() {
                run.recipient_snapshot = orchestration.recipient_snapshot;
            }
        }

        run.start_payload = start_payload;
        let saved = self.repo.save(&run).await?;
        let workflow_triggered = self.flowable_service.is_some();
        Ok(BusinessCaseWorkflowStartData {
            receipt_group_id: saved.receipt_group_id.clone(),
            recipient_snapshot: saved.recipient_snapshot.clone(),
            process_instance_id: saved.process_instance_id.clone(),
            run: saved,
            business_case,
            workflow_triggered,
        })
    }

    pub async fn get_run_details(&self, run_id: &str) -> Result<Option<BusinessCaseWorkflowRunDetail>, DomainError> {
        let Some(run) = self.repo.find_by_run_id(run_id).await? else {
            return Ok(None);
        };
        self.hydrate_run(run, None).await.map(Some)
    }

    pub async fn get_run_details_for_viewer(
        &self,
        run_id: &str,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<Option<BusinessCaseWorkflowRunDetail>, DomainError> {
        let Some(run) = self.repo.find_by_run_id(run_id).await? else {
            return Ok(None);
        };
        let Some(case_item) = self
            .business_case_service
            .get_accessible(&run.case_id, viewer_department_id, viewer_department_name)
            .await?
        else {
            return Ok(None);
        };
        self.hydrate_run(run, Some(case_item)).await.map(Some)
    }

    pub async fn get_case_workflow(&self, case_id: &str) -> Result<Option<BusinessCaseWorkflowRunDetail>, DomainError> {
        let Some(run) = self.repo.find_by_case_id(case_id).await? else {
            return Ok(None);
        };
        self.hydrate_run(run, None).await.map(Some)
    }

    pub async fn get_case_workflow_for_viewer(
        &self,
        case_id: &str,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<Option<BusinessCaseWorkflowRunDetail>, DomainError> {
        let Some(case_item) = self
            .business_case_service
            .get_accessible(case_id, viewer_department_id, viewer_department_name)
            .await?
        else {
            return Ok(None);
        };
        let Some(run) = self.repo.find_by_case_id(case_id).await? else {
            return Ok(None);
        };
        self.hydrate_run(run, Some(case_item)).await.map(Some)
    }

    pub async fn attach_existing_case_to_workflow(
        &self,
        template_code: &str,
        case_id: &str,
        actor: &WorkflowActor,
    ) -> Result<Option<BusinessCaseWorkflowStartData>, DomainError> {
        if self.repo.find_by_case_id(case_id).await?.is_some() {
            return Ok(None);
        }

        let Some(business_case) = self.business_case_service.get(case_id).await? else {
            return Ok(None);
        };
        let flight = self
            .flight_service
            .get_flight(&business_case.flight_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "flight",
                id: business_case.flight_id.clone(),
            })?;
        let flight_id = flight
            .flight_id
            .clone()
            .unwrap_or_else(|| business_case.flight_id.clone());
        let started_by = actor.started_by();
        let flight_context_snapshot = build_flight_context_snapshot(&flight);
        let extra_info = normalize_workflow_extra_info(
            &business_case.context,
            &business_case.description,
            &flight_context_snapshot,
            business_case.gate.as_deref().or(flight.gate.as_deref()),
            business_case.stand.as_deref().or(flight.stand.as_deref()),
        );
        let (runtime_definition, bpmn_source) = self.load_start_runtime_definition(template_code.trim(), None).await?;
        let flowable_start = self
            .try_start_flowable_process(
                template_code.trim(),
                &business_case.case_id,
                &flight_id,
                &flight_context_snapshot,
                &business_case.description,
                &extra_info,
                &runtime_definition.case_type,
                actor,
                Some(business_case.created_at),
            )
            .await?;

        let now = Utc::now();
        let mut start_payload = build_workflow_start_payload(
            &business_case.case_id,
            flowable_start.process_definition_id.as_deref(),
            &runtime_definition.case_type,
            &bpmn_source,
            &extra_info,
        );

        let mut run = BusinessCaseWorkflowRun {
            run_id: ulid::Ulid::new().to_string(),
            template_code: template_code.trim().to_string(),
            case_id: business_case.case_id.clone(),
            flight_id,
            process_definition_key: runtime_definition.case_type.clone(),
            process_instance_id: flowable_start.process_instance_id.clone(),
            waiting_task_id: flowable_start.waiting_task_id.clone(),
            receipt_group_id: None,
            status: flowable_start.status.clone(),
            outcome: None,
            recipient_snapshot: Vec::new(),
            flight_context_snapshot,
            start_payload: HashMap::new(),
            started_by: started_by.to_string(),
            completed_at: None,
            failed_reason: None,
            created_at: now,
            updated_at: now,
        };

        if let Some(orchestration) = self
            .try_orchestrate_start(
                &run,
                &business_case.case_id,
                &business_case.description,
                &extra_info,
                actor,
                &mut start_payload,
                &flowable_start,
                &runtime_definition,
                None,
            )
            .await?
        {
            if let Some(waiting_task_id) = orchestration.waiting_task_id {
                run.waiting_task_id = Some(waiting_task_id);
            }
            if let Some(receipt_group_id) = orchestration.receipt_group_id {
                run.receipt_group_id = Some(receipt_group_id);
            }
            if let Some(status) = orchestration.status {
                run.status = status;
            }
            if !orchestration.recipient_snapshot.is_empty() {
                run.recipient_snapshot = orchestration.recipient_snapshot;
            }
        }

        run.start_payload = start_payload;
        let saved = self.repo.save(&run).await?;
        let workflow_triggered = self.flowable_service.is_some();
        Ok(Some(BusinessCaseWorkflowStartData {
            receipt_group_id: saved.receipt_group_id.clone(),
            recipient_snapshot: saved.recipient_snapshot.clone(),
            process_instance_id: saved.process_instance_id.clone(),
            run: saved,
            business_case,
            workflow_triggered,
        }))
    }

    pub async fn attach_existing_cases_to_workflow_batch(
        &self,
        batch_id: &str,
        items: &[BusinessCaseWorkflowBatchItem],
        actor: &WorkflowActor,
    ) -> Result<Vec<BusinessCaseWorkflowStartData>, DomainError> {
        let result = self
            .attach_existing_cases_to_workflow_batch_detailed(batch_id, items, actor)
            .await?;
        Ok(result.started)
    }

    pub async fn attach_existing_cases_to_workflow_batch_detailed(
        &self,
        batch_id: &str,
        items: &[BusinessCaseWorkflowBatchItem],
        actor: &WorkflowActor,
    ) -> Result<BusinessCaseWorkflowBatchResult, DomainError> {
        #[cfg(test)]
        {
            let guard = self.mock_dispatch_result.lock().unwrap();
            if let Some(ref res) = *guard {
                return match res {
                    Ok(val) => Ok(val.clone()),
                    Err(err) => Err(DomainError::Internal(err.clone())),
                };
            }
        }

        if items.is_empty() {
            return Ok(BusinessCaseWorkflowBatchResult::default());
        }

        // 1. Plan all items
        let mut planned_items: Vec<WorkflowBatchPlanItem> = Vec::new();
        for item in items {
            // Skip if already has a workflow run
            if self.repo.find_by_case_id(&item.case_id).await?.is_some() {
                continue;
            }

            let Some(business_case) = self.business_case_service.get(&item.case_id).await? else {
                continue;
            };
            let flight = self
                .flight_service
                .get_flight(&business_case.flight_id)
                .await?
                .ok_or_else(|| DomainError::NotFound {
                    entity_type: "flight",
                    id: business_case.flight_id.clone(),
                })?;
            let flight_id = flight
                .flight_id
                .clone()
                .unwrap_or_else(|| business_case.flight_id.clone());
            let started_by = actor.started_by();
            let flight_context_snapshot = build_flight_context_snapshot(&flight);
            let extra_info = normalize_workflow_extra_info(
                &business_case.context,
                &business_case.description,
                &flight_context_snapshot,
                business_case.gate.as_deref().or(flight.gate.as_deref()),
                business_case.stand.as_deref().or(flight.stand.as_deref()),
            );
            let (runtime_definition, bpmn_source) = self
                .load_start_runtime_definition(item.template_code.trim(), None)
                .await?;
            let batch_policy = self.load_workflow_batch_policy(item.template_code.trim()).await?;
            let flowable_start = self
                .try_start_flowable_process(
                    item.template_code.trim(),
                    &business_case.case_id,
                    &flight_id,
                    &flight_context_snapshot,
                    &business_case.description,
                    &extra_info,
                    &runtime_definition.case_type,
                    actor,
                    Some(business_case.created_at),
                )
                .await?;

            let now = Utc::now();
            let start_payload = build_workflow_start_payload(
                &business_case.case_id,
                flowable_start.process_definition_id.as_deref(),
                &runtime_definition.case_type,
                &bpmn_source,
                &extra_info,
            );

            // Resolve recipients
            let recipients = self
                .resolve_recipients(
                    &runtime_definition.notification_targets,
                    &runtime_definition.recipient_resolver,
                )
                .await?
                .into_iter()
                .map(user_to_recipient_snapshot)
                .collect::<Vec<_>>();

            let template_variables = build_template_variables(
                &business_case.case_id,
                &flight_id,
                &flight_context_snapshot,
                &extra_info,
                &business_case.description,
                &recipients,
            );
            let notification_title = render_template(&runtime_definition.notification_title, &template_variables);
            let notification_body = build_notification_body(
                &runtime_definition.notification_body,
                &template_variables,
                runtime_definition.append_extra_info,
                &extra_info,
            );

            let run = BusinessCaseWorkflowRun {
                run_id: ulid::Ulid::new().to_string(),
                template_code: item.template_code.trim().to_string(),
                case_id: business_case.case_id.clone(),
                flight_id: flight_id.clone(),
                process_definition_key: runtime_definition.case_type.clone(),
                process_instance_id: flowable_start.process_instance_id.clone(),
                waiting_task_id: flowable_start.waiting_task_id.clone(),
                receipt_group_id: None,
                status: flowable_start.status.clone(),
                outcome: None,
                recipient_snapshot: Vec::new(),
                flight_context_snapshot,
                start_payload: HashMap::new(),
                started_by: started_by.to_string(),
                completed_at: None,
                failed_reason: None,
                created_at: now,
                updated_at: now,
            };

            let receipt_required = runtime_definition.receipt_required;
            let notification_severity = runtime_definition.notification_severity.clone();

            planned_items.push(WorkflowBatchPlanItem {
                item: item.clone(),
                business_case,
                run,
                start_snapshot: flowable_start,
                definition: runtime_definition,
                recipients,
                notification_title,
                notification_body,
                receipt_required,
                notification_severity,
                extra_info,
                start_payload,
                batch_policy,
            });
        }

        if planned_items.is_empty() {
            return Ok(BusinessCaseWorkflowBatchResult::default());
        }

        // 2. Group by notification group key
        let mut groups: HashMap<WorkflowNotificationGroupKey, Vec<WorkflowBatchPlanItem>> = HashMap::new();
        let mut ungrouped: Vec<WorkflowBatchPlanItem> = Vec::new();

        for planned in planned_items {
            if planned.batch_policy.should_group(planned.receipt_required) {
                let group_key = WorkflowNotificationGroupKey {
                    template_code: planned.item.template_code.clone(),
                    case_type: planned.run.process_definition_key.clone(),
                    notification_task_id: planned.definition.notification_task_id.clone(),
                    recipient_set_hash: compute_recipient_set_hash(&planned.recipients),
                    receipt_required: planned.receipt_required,
                    severity: planned.notification_severity.clone(),
                };
                groups.entry(group_key).or_default().push(planned);
            } else {
                ungrouped.push(planned);
            }
        }

        let mut all_started: Vec<BusinessCaseWorkflowStartData> = Vec::new();
        let mut notification_groups: Vec<BusinessCaseWorkflowNotificationGroup> = Vec::new();

        // 3. Send grouped notifications with orchestration
        for (_key, group_items) in groups {
            if group_items.is_empty() {
                continue;
            }

            let first = &group_items[0];
            let case_type_name = &first.definition.case_type;
            let aggregated_title = build_batch_notification_title(case_type_name, group_items.len());
            let aggregated_body = build_batch_notification_body(&group_items);

            // Advance each process past dispatch tasks, then wait for notification task
            for planned in &group_items {
                if let Some(_flowable_service) = self.flowable_service.as_ref() {
                    self.continue_dispatch_tasks_inner(
                        &planned.run.process_instance_id,
                        planned.start_snapshot.process_definition_id.as_deref(),
                        Some(&planned.definition),
                        true,
                    )
                    .await?;

                    // Wait for notification task to become active
                    let notification_task = self
                        .wait_for_task(
                            &planned.run.process_instance_id,
                            &planned.definition.notification_task_id,
                        )
                        .await
                        .ok_or_else(|| {
                            DomainError::BusinessRuleViolation(format!(
                                "Notification task not active for process={} node={}",
                                planned.run.process_instance_id, planned.definition.notification_task_id
                            ))
                        })?;
                    let _ = notification_task; // ensure it exists
                }
            }

            let mut all_user_ids: Vec<String> = group_items
                .iter()
                .flat_map(|item| {
                    item.recipients.iter().filter_map(|r| {
                        r.get("user_id")
                            .and_then(serde_json::Value::as_str)
                            .map(ToOwned::to_owned)
                    })
                })
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            all_user_ids.sort();

            let batch_notification = DispatchBatchNotificationCreate {
                user_ids: all_user_ids.clone(),
                title: aggregated_title.clone(),
                body: aggregated_body.clone(),
                category: "dispatch".to_string(),
                severity: first.notification_severity.clone(),
                flight_id: Some(first.run.flight_id.clone()),
                related_entity_type: Some("business_case".to_string()),
                related_entity_id: Some(first.business_case.case_id.clone()),
                dispatch_order_id: None,
                group_id: None,
                sender_user_id: actor.user_id.clone(),
                sender_username_snapshot: actor.sender_username_snapshot(),
                origin_type: "workflow".to_string(),
                receipt_required: first.receipt_required,
            };
            let case_ids = group_items
                .iter()
                .map(|item| item.business_case.case_id.clone())
                .collect::<Vec<_>>();
            let idempotency_context = derive_batch_notification_idempotency_context(
                batch_id,
                &first.item.template_code,
                &first.definition.case_type,
                &first.definition.notification_task_id,
                &case_ids,
                &all_user_ids,
                first.receipt_required,
                &first.notification_severity,
            );

            #[cfg(test)]
            let batch_result = {
                let mocked = self.mock_batch_notification_result.lock().unwrap().clone();
                if let Some(value) = mocked {
                    self.mock_batch_notifications
                        .lock()
                        .unwrap()
                        .push(batch_notification.clone());
                    value
                } else {
                    let notification_service = self.notification_service.as_ref().ok_or_else(|| {
                        DomainError::BusinessRuleViolation(
                            "Notification service unavailable for business case workflow orchestration".to_string(),
                        )
                    })?;
                    notification_service
                        .send_batch_with_idempotency(
                            batch_notification,
                            idempotency_context.receipt_group_id_override.clone(),
                            Some(idempotency_context.notification_id_seed.clone()),
                        )
                        .await?
                }
            };

            #[cfg(not(test))]
            let batch_result = {
                let notification_service = self.notification_service.as_ref().ok_or_else(|| {
                    DomainError::BusinessRuleViolation(
                        "Notification service unavailable for business case workflow orchestration".to_string(),
                    )
                })?;
                notification_service
                    .send_batch_with_idempotency(
                        batch_notification,
                        idempotency_context.receipt_group_id_override.clone(),
                        Some(idempotency_context.notification_id_seed.clone()),
                    )
                    .await?
            };

            let receipt_group_id = batch_result
                .get("receipt_group_id")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned);
            if first.receipt_required && receipt_group_id.is_none() {
                return Err(DomainError::BusinessRuleViolation(
                    "Notification batch did not return receipt_group_id".to_string(),
                ));
            }

            notification_groups.push(BusinessCaseWorkflowNotificationGroup {
                receipt_group_id: receipt_group_id.clone(),
                case_type: case_type_name.clone(),
                case_ids: case_ids.clone(),
                title: aggregated_title,
                body: aggregated_body,
            });

            // Save all runs in the group with the same receipt_group_id
            for mut planned in group_items {
                planned.run.receipt_group_id = receipt_group_id.clone();
                planned.run.recipient_snapshot = planned.recipients.clone();
                planned.run.status = "notification_sent".to_string();
                planned.start_payload.insert(
                    "notification_title".to_string(),
                    serde_json::Value::String(planned.notification_title.clone()),
                );
                planned.start_payload.insert(
                    "notification_body".to_string(),
                    serde_json::Value::String(planned.notification_body.clone()),
                );
                planned.run.start_payload = planned.start_payload.clone();

                let saved = self.repo.save(&planned.run).await?;
                let workflow_triggered = self.flowable_service.is_some();

                // Complete the Flowable notification task and wait for wait_receipts
                if let Some(flowable_service) = self.flowable_service.as_ref() {
                    let active_tasks = flowable_service
                        .list_tasks(&[("processInstanceId", saved.process_instance_id.clone())])
                        .await
                        .map_err(map_flowable_error)?;

                    let notification_task =
                        locate_task_by_definition_key(&active_tasks, &planned.definition.notification_task_id);

                    if let Some(task) = notification_task {
                        let notification_task_id = task_identifier(&task).ok_or_else(|| {
                            DomainError::BusinessRuleViolation(format!(
                                "Notification task missing id for process={}",
                                saved.process_instance_id
                            ))
                        })?;

                        let mut variables = serde_json::Map::new();
                        if let Some(ref rgid) = receipt_group_id {
                            variables.insert("receiptGroupId".to_string(), serde_json::Value::String(rgid.clone()));
                            variables.insert("receipt_group_id".to_string(), serde_json::Value::String(rgid.clone()));
                        }
                        variables.insert(
                            "notificationTitle".to_string(),
                            serde_json::Value::String(planned.notification_title.clone()),
                        );
                        variables.insert(
                            "notificationBody".to_string(),
                            serde_json::Value::String(planned.notification_body.clone()),
                        );
                        variables.insert(
                            "recipientSnapshot".to_string(),
                            serde_json::to_value(&planned.recipients).unwrap_or_else(|_| serde_json::json!([])),
                        );
                        variables.insert(
                            "notificationRecipientCount".to_string(),
                            serde_json::Value::from(planned.recipients.len() as i64),
                        );
                        variables.insert(
                            "completionPolicy".to_string(),
                            serde_json::Value::String(planned.definition.completion_policy.clone()),
                        );
                        variables.insert(
                            "rejectPolicy".to_string(),
                            serde_json::Value::String(planned.definition.reject_policy.clone()),
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

                        // Wait for wait_receipts task to appear
                        let wait_task = self
                            .wait_for_task(&saved.process_instance_id, &planned.definition.wait_task_id)
                            .await;
                        if let Some(waiting_task_id) = wait_task.as_ref().and_then(task_identifier) {
                            let mut updated_run = saved.clone();
                            updated_run.waiting_task_id = Some(waiting_task_id);
                            updated_run.status = if planned.receipt_required {
                                "waiting_receipts".to_string()
                            } else {
                                "notification_sent".to_string()
                            };
                            updated_run.updated_at = Utc::now();
                            let updated_saved = self.repo.save(&updated_run).await?;
                            all_started.push(BusinessCaseWorkflowStartData {
                                receipt_group_id: updated_saved.receipt_group_id.clone(),
                                recipient_snapshot: updated_saved.recipient_snapshot.clone(),
                                process_instance_id: updated_saved.process_instance_id.clone(),
                                run: updated_saved,
                                business_case: planned.business_case,
                                workflow_triggered,
                            });
                            continue;
                        }
                    }
                }

                all_started.push(BusinessCaseWorkflowStartData {
                    receipt_group_id: saved.receipt_group_id.clone(),
                    recipient_snapshot: saved.recipient_snapshot.clone(),
                    process_instance_id: saved.process_instance_id.clone(),
                    run: saved,
                    business_case: planned.business_case,
                    workflow_triggered,
                });
            }
        }

        // 4. Handle ungrouped items (single-case notifications, process already started)
        for mut planned in ungrouped {
            let mut start_payload = planned.start_payload.clone();
            let idempotency_context = derive_per_case_batch_notification_idempotency_context(
                batch_id,
                &planned.business_case.case_id,
                &planned.item.template_code,
                &planned.definition.notification_task_id,
                planned.receipt_required,
            );
            let orchestration = self
                .try_orchestrate_start(
                    &planned.run,
                    &planned.business_case.case_id,
                    &planned.business_case.description,
                    &planned.extra_info,
                    actor,
                    &mut start_payload,
                    &planned.start_snapshot,
                    &planned.definition,
                    Some(&idempotency_context),
                )
                .await?;

            if let Some(orchestration) = orchestration {
                if let Some(waiting_task_id) = orchestration.waiting_task_id {
                    planned.run.waiting_task_id = Some(waiting_task_id);
                }
                if let Some(receipt_group_id) = orchestration.receipt_group_id {
                    planned.run.receipt_group_id = Some(receipt_group_id);
                }
                if let Some(status) = orchestration.status {
                    planned.run.status = status;
                }
                if !orchestration.recipient_snapshot.is_empty() {
                    planned.run.recipient_snapshot = orchestration.recipient_snapshot;
                }
            }

            planned.run.start_payload = start_payload;
            let saved = self.repo.save(&planned.run).await?;
            let workflow_triggered = self.flowable_service.is_some();

            all_started.push(BusinessCaseWorkflowStartData {
                receipt_group_id: saved.receipt_group_id.clone(),
                recipient_snapshot: saved.recipient_snapshot.clone(),
                process_instance_id: saved.process_instance_id.clone(),
                run: saved,
                business_case: planned.business_case,
                workflow_triggered,
            });
        }

        Ok(BusinessCaseWorkflowBatchResult {
            started: all_started,
            notification_groups,
        })
    }

    pub async fn continue_dispatch_tasks(
        &self,
        process_instance_id: &str,
        process_definition_id: Option<&str>,
        raise_on_error: bool,
    ) -> Result<(), DomainError> {
        self.continue_dispatch_tasks_inner(process_instance_id, process_definition_id, None, raise_on_error)
            .await
    }

    pub(super) async fn continue_dispatch_tasks_inner(
        &self,
        process_instance_id: &str,
        process_definition_id: Option<&str>,
        definition: Option<&WorkflowRuntimeDefinition>,
        raise_on_error: bool,
    ) -> Result<(), DomainError> {
        let process_instance_id = process_instance_id.trim();
        if process_instance_id.is_empty() {
            return Ok(());
        }

        let Some(flowable_service) = self.flowable_service.as_ref() else {
            let message = "Flowable service unavailable for dispatch task continuation".to_string();
            return handle_dispatch_task_continuation_error(message, raise_on_error);
        };
        let Some(workflow_dispatch_service) = self.workflow_dispatch_service.as_ref() else {
            let message = "workflow dispatch service unavailable".to_string();
            return handle_dispatch_task_continuation_error(message, raise_on_error);
        };

        let resolved_definition = match definition.cloned() {
            Some(definition) => definition,
            None => {
                self.load_runtime_definition_for_process(process_instance_id, process_definition_id)
                    .await?
            }
        };
        if resolved_definition.dispatch_tasks.is_empty() {
            return Ok(());
        }

        loop {
            let tasks = flowable_service
                .list_tasks(&[("processInstanceId", process_instance_id.to_string())])
                .await
                .map_err(map_flowable_error)?;
            let Some(dispatch_task) = select_dispatch_task(&tasks, &resolved_definition) else {
                break;
            };

            let task_key = task_definition_key_or_id(&dispatch_task).ok_or_else(|| {
                DomainError::BusinessRuleViolation(format!(
                    "Dispatch task missing taskDefinitionKey/id for process={process_instance_id}"
                ))
            })?;
            let config = resolved_definition.dispatch_tasks.get(&task_key).ok_or_else(|| {
                DomainError::BusinessRuleViolation(format!(
                    "Dispatch task config missing for process={process_instance_id} node={task_key}"
                ))
            })?;

            let variables = normalize_runtime_variables(
                flowable_service
                    .get_process_instance_variables(process_instance_id)
                    .await
                    .map_err(map_flowable_error)?,
            );
            let process_instance = flowable_service
                .get_process_instance(process_instance_id)
                .await
                .map_err(map_flowable_error)?
                .unwrap_or_else(|| serde_json::json!({}));

            let payload = build_dispatch_create_request(
                process_instance_id,
                &process_instance,
                &dispatch_task,
                &variables,
                config,
            )?;

            let order = match workflow_dispatch_service.create_dispatch_from_workflow(payload).await {
                Ok(order) => order,
                Err(error) => {
                    let message = format!(
                        "dispatch task auto runner failed for process={process_instance_id} node={}: {error}",
                        config.node_id
                    );
                    return handle_dispatch_task_continuation_error(message, raise_on_error);
                }
            };

            let dispatch_task_id = task_identifier(&dispatch_task).ok_or_else(|| {
                DomainError::BusinessRuleViolation(format!(
                    "Dispatch task missing id for process={process_instance_id} node={}",
                    config.node_id
                ))
            })?;
            let dispatch_order_id = order.id.clone();
            let dispatch_order_status = dispatch_order_status_value(order.status);
            let dispatch_order_refs = merge_dispatch_order_refs(
                variables.get("dispatchOrderRefs"),
                config,
                &dispatch_task_id,
                &dispatch_order_id,
                &dispatch_order_status,
            );
            let complete_variables = serde_json::Map::from_iter([
                (
                    "lastDispatchOrderId".to_string(),
                    serde_json::Value::String(dispatch_order_id),
                ),
                (
                    "lastDispatchOrderNodeId".to_string(),
                    serde_json::Value::String(config.node_id.clone()),
                ),
                (
                    "lastDispatchOrderStatus".to_string(),
                    serde_json::Value::String(dispatch_order_status),
                ),
                (
                    "dispatchOrderRefs".to_string(),
                    serde_json::Value::Array(dispatch_order_refs),
                ),
            ]);
            let completed = flowable_service
                .complete_task(&dispatch_task_id, Some(&complete_variables))
                .await
                .map_err(map_flowable_error)?;
            if !completed {
                let message = format!(
                    "failed to auto-complete dispatch task for process={process_instance_id} task={dispatch_task_id}"
                );
                return handle_dispatch_task_continuation_error(message, raise_on_error);
            }
        }

        Ok(())
    }

    async fn load_runtime_definition_for_process(
        &self,
        process_instance_id: &str,
        process_definition_id: Option<&str>,
    ) -> Result<WorkflowRuntimeDefinition, DomainError> {
        let flowable_service = self.flowable_service.as_ref().ok_or_else(|| {
            DomainError::BusinessRuleViolation(
                "Flowable service unavailable for dispatch task continuation".to_string(),
            )
        })?;

        let resolved_process_definition_id = match process_definition_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
        {
            Some(value) => value,
            None => {
                let process_instance = flowable_service
                    .get_process_instance(process_instance_id)
                    .await
                    .map_err(map_flowable_error)?
                    .ok_or_else(|| {
                        DomainError::BusinessRuleViolation(format!(
                            "process instance not found for dispatch continuation: {process_instance_id}"
                        ))
                    })?;
                process_instance
                    .get("processDefinitionId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| {
                        DomainError::BusinessRuleViolation(format!(
                            "process definition id unavailable for process={process_instance_id}"
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

impl NotificationReceiptGroupSync for BusinessCaseWorkflowService {
    fn sync_receipt_group<'a>(
        &'a self,
        receipt_group_id: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), DomainError>> + Send + 'a>> {
        Box::pin(async move {
            self.sync_receipt_group(receipt_group_id).await?;
            Ok(())
        })
    }
}
