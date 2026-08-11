//! AI Copilot service implementation. The struct, constructor, and full
//! impl live here; supporting types are split into sibling modules
//! (`schemas`, `access`, `config`).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use fms_domain::error::DomainError;
use fms_domain::models::ai_copilot::{AiCopilotBatchStatus, AiCopilotBusinessCaseBatch, AiCopilotOperationalMetrics};
use fms_domain::models::business_case::{FlightBusinessCase, VisibilityScope};
use fms_domain::models::flight::Flight;
use fms_domain::models::value_objects::FlightStatus;
use fms_domain::ports::ai_copilot_repository::{AiCopilotBusinessCaseBatchRepository, BeginCommitResult};
use fms_domain::ports::flight_repository::FlightRepository;

use crate::services::ai_admin_service::AiAdminService;
use crate::services::business_case_service::BusinessCaseServiceOps;
use crate::services::business_case_workflow_service::{BusinessCaseWorkflowBatchItem, WorkflowActor};
use crate::types::{ConcreteBusinessCaseTypeService, ConcreteBusinessCaseWorkflowService, ConcreteFlightService};

use super::access::{normalize_actor_key, AiCopilotBatchAccess};
use super::config::{
    apply_case_properties_ai_copilot_config, apply_field_hint, apply_legacy_ai_extraction_config,
    derive_ai_extraction_config_from_case_properties, normalize_business_case_ai_extraction_config,
    normalize_optional_string, parse_ai_extraction_config, parse_case_properties, string_vec_from_json, AiFieldConfig,
    AiFlightMatchingConfig, AiLegBindingConfig, BusinessCaseAiExtractionConfig, BusinessCaseProperties,
    CaseBindingPolicy, CaseDuplicatePolicy, CaseFlightMatchPolicy, CasePropertiesAiCopilotConfig,
    CopilotCaseTypeCatalogEntry, PreparedCommitAction,
};
use super::schemas::{
    AiCopilotApprovedAction, AiCopilotBatchListResponse, AiCopilotBatchStatusResponse, AiCopilotCaseTypeDiagnostic,
    AiCopilotCommitRecoveryError, AiCopilotCommitRecoverySummary, AiCopilotCommitRequest, AiCopilotCommitResponse,
    AiCopilotDraftAction, AiCopilotDraftDiagnosticResponse, AiCopilotDraftRequest, AiCopilotDraftResponse,
    AiCopilotFailedBatchResolutionAction, AiCopilotFailedBatchResolutionRequest, AiCopilotMatchedFlight,
    AiCopilotNotificationGroup, AiCopilotOperationalMetricsResponse, AiCopilotWorkflowDispatchRetryError,
    AiCopilotWorkflowDispatchRetrySummary, LlmDraftAction, LlmDraftPayload, StoredWorkflowDispatchItem,
    StoredWorkflowDispatchRequest,
};

pub(crate) use super::helpers::*;

const WORKFLOW_DISPATCH_PENDING_STALE_AFTER_SECONDS: i64 = 15 * 60;
const COMMIT_RECOVERY_INITIAL_DELAY_SECONDS: i64 = 120;
pub const DEFAULT_COMMIT_RECOVERY_MAX_ATTEMPTS: i32 = 5;

// Alias used by the original impl; kept here to avoid a sweeping rename.
type _AiCopilotCaseTypeDiagnostic = AiCopilotCaseTypeDiagnostic;

pub struct AiBusinessCaseCopilotService<R>
where
    R: AiCopilotBusinessCaseBatchRepository + Send + Sync + ?Sized,
{
    pub(crate) repo: Arc<R>,
    pub(crate) ai_admin_service: Arc<AiAdminService>,
    pub(crate) flight_repo: Arc<dyn FlightRepository + Send + Sync>,
    flight_service: Arc<ConcreteFlightService>,
    pub(crate) business_case_service: Arc<dyn BusinessCaseServiceOps>,
    workflow_service: Option<Arc<ConcreteBusinessCaseWorkflowService>>,
    business_case_type_service: Option<Arc<ConcreteBusinessCaseTypeService>>,
}

impl<R> AiBusinessCaseCopilotService<R>
where
    R: AiCopilotBusinessCaseBatchRepository + Send + Sync + ?Sized,
{
    pub fn new(
        repo: Arc<R>,
        ai_admin_service: Arc<AiAdminService>,
        flight_repo: Arc<dyn FlightRepository + Send + Sync>,
        flight_service: Arc<ConcreteFlightService>,
        business_case_service: Arc<dyn BusinessCaseServiceOps>,
    ) -> Self {
        Self {
            repo,
            ai_admin_service,
            flight_repo,
            flight_service,
            business_case_service,
            workflow_service: None,
            business_case_type_service: None,
        }
    }

    pub fn with_workflow_service(mut self, workflow_service: Arc<ConcreteBusinessCaseWorkflowService>) -> Self {
        self.workflow_service = Some(workflow_service);
        self
    }

    pub fn with_business_case_type_service(mut self, service: Arc<ConcreteBusinessCaseTypeService>) -> Self {
        self.business_case_type_service = Some(service);
        self
    }

    pub(crate) async fn load_case_type_catalog(
        &self,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common_case_types: bool,
    ) -> Result<Vec<CopilotCaseTypeCatalogEntry>, DomainError> {
        let Some(service) = self.business_case_type_service.as_ref() else {
            return Ok(vec![]);
        };
        let types = service
            .list_case_types_for_viewer(true, viewer_department_id, viewer_department_name)
            .await?;
        Ok(types
            .into_iter()
            .filter(|item| {
                if matches!(item.visibility_scope, VisibilityScope::Common) && !include_common_case_types {
                    return false;
                }
                true
            })
            .filter_map(|item| {
                let case_properties = parse_case_properties(&item.case_properties);
                let config = normalize_business_case_ai_extraction_config(
                    &item.ai_extraction_config,
                    &item.case_properties,
                    &case_properties,
                )?;
                Some(CopilotCaseTypeCatalogEntry {
                    code: item.code,
                    name: item.name,
                    description: item.description,
                    config,
                    case_properties,
                })
            })
            .collect())
    }

    pub(crate) async fn prepare_commit_actions(
        &self,
        batch: &AiCopilotBusinessCaseBatch,
        request: &AiCopilotCommitRequest,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common_case_types: bool,
        check_existing_duplicates: bool,
    ) -> Result<Vec<PreparedCommitAction>, DomainError> {
        let catalog = self
            .load_case_type_catalog(viewer_department_id, viewer_department_name, include_common_case_types)
            .await?;
        let catalog_by_code: HashMap<&str, &CopilotCaseTypeCatalogEntry> =
            catalog.iter().map(|entry| (entry.code.as_str(), entry)).collect();

        let mut prepared_actions = Vec::new();
        for action in request.actions.iter().cloned() {
            validate_approved_action(&action)?;

            let entry = catalog_by_code.get(action.case_type.as_str()).ok_or_else(|| {
                DomainError::ValidationError(format!(
                    "事项类型 {} 不在当前用户的 AI 抽取授权目录中，或未启用 AI 抽取",
                    action.case_type
                ))
            })?;

            let ai_cfg = &entry.config;
            let case_props = &entry.case_properties;

            let bound_leg = action.bound_leg_type.as_deref().unwrap_or("outbound").trim();
            if case_props.binding_policy.leg_type_required && bound_leg.is_empty() {
                return Err(DomainError::ValidationError(format!(
                    "事项类型 {} 要求绑定航段类型",
                    action.case_type
                )));
            }
            if !case_props.binding_policy.allowed_leg_types.is_empty()
                && !bound_leg.is_empty()
                && !case_props
                    .binding_policy
                    .allowed_leg_types
                    .contains(&bound_leg.to_string())
            {
                return Err(DomainError::ValidationError(format!(
                    "事项类型 {} 不允许绑定航段类型: {}",
                    action.case_type, bound_leg
                )));
            }

            for (field_name, field_schema) in &case_props.extra_info_schema.fields {
                if field_schema.required {
                    let has_val = action
                        .fields
                        .get(field_name)
                        .map(|v| match v {
                            Value::Null => false,
                            Value::String(s) => !s.trim().is_empty(),
                            _ => true,
                        })
                        .unwrap_or(false);
                    if !has_val {
                        return Err(DomainError::ValidationError(format!(
                            "事项类型 {} 提交时缺少必需字段: {}",
                            action.case_type,
                            field_schema.label.as_deref().unwrap_or(field_name)
                        )));
                    }
                }
            }

            if let Some(fields_obj) = action.fields.as_object() {
                for forbidden in &ai_cfg.forbidden_fields {
                    if fields_obj.contains_key(forbidden) {
                        return Err(DomainError::ValidationError(format!(
                            "事项类型 {} 提交时包含了被禁止的字段: {}",
                            action.case_type, forbidden
                        )));
                    }
                }
            }
            for (field_name, field_cfg) in &ai_cfg.fields {
                if field_cfg.required {
                    let has_val = action
                        .fields
                        .get(field_name)
                        .map(|v| match v {
                            Value::Null => false,
                            Value::String(s) => !s.trim().is_empty(),
                            _ => true,
                        })
                        .unwrap_or(false);
                    if !has_val {
                        return Err(DomainError::ValidationError(format!(
                            "事项类型 {} 提交时缺少必需字段: {}",
                            action.case_type,
                            field_cfg.label.as_deref().unwrap_or(field_name)
                        )));
                    }
                }
            }

            let flight = self
                .flight_service
                .get_flight(&action.flight_id)
                .await?
                .ok_or_else(|| DomainError::NotFound {
                    entity_type: "flight",
                    id: action.flight_id.clone(),
                })?;
            let flight_id = flight.flight_id.clone().unwrap_or_else(|| action.flight_id.clone());
            let flight_no = action.flight_no.trim().to_string();
            let mut context = HashMap::new();
            context.insert("source".to_string(), json!("ai_copilot_voice"));
            context.insert("copilot_batch_id".to_string(), json!(batch.batch_id.clone()));
            context.insert("copilot_action_id".to_string(), json!(action.action_id.clone()));
            if let Some(idempotency_key) = request
                .idempotency_key
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                context.insert("copilot_idempotency_key".to_string(), json!(idempotency_key));
            }
            context.insert(
                "bound_leg_type".to_string(),
                json!(action.bound_leg_type.clone().unwrap_or_else(|| "outbound".to_string())),
            );
            context.insert(
                "bound_flight_no".to_string(),
                json!(action.bound_flight_no.clone().unwrap_or_else(|| flight_no.clone())),
            );
            context.insert(
                "transcript_summary".to_string(),
                json!(batch.transcript_summary.clone()),
            );
            let remarks_text = action.remarks.as_deref().unwrap_or("").trim().to_string();
            let extra_info_value = if !remarks_text.is_empty() {
                remarks_text.clone()
            } else if let Some(ref tpl) = case_props.extra_info_schema.summary_template {
                render_action_template(tpl, &action.fields)
            } else {
                String::new()
            };
            if !extra_info_value.is_empty() {
                context.insert("extra_info".to_string(), json!(extra_info_value));
            }
            if let Some(fields_obj) = action.fields.as_object() {
                for (k, v) in fields_obj {
                    context.insert(k.clone(), v.clone());
                }
                context.insert("copilot_fields".to_string(), action.fields.clone());
            }

            let description = action
                .description
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| action.remarks.as_deref().unwrap_or("AI Copilot 创建事项"))
                .to_string();
            let status = action.status.clone().or_else(|| Some("INITIAL".to_string()));
            prepared_actions.push(PreparedCommitAction {
                action,
                flight_id,
                flight_no,
                description,
                status,
                context,
                duplicate_policy: case_props.duplicate_policy.clone(),
            });
        }

        reject_duplicate_copilot_action_ids_in_batch(&prepared_actions)?;
        reject_duplicate_copilot_actions_in_batch(&prepared_actions)?;

        if check_existing_duplicates {
            for prepared in &prepared_actions {
                reject_duplicate_copilot_action(
                    &*self.business_case_service,
                    prepared,
                    viewer_department_id,
                    viewer_department_name,
                )
                .await?;
            }
        }

        Ok(prepared_actions)
    }

    async fn dispatch_workflow_for_committed_batch(
        &self,
        batch_id: &str,
        workflow_items: &[BusinessCaseWorkflowBatchItem],
        actor: &WorkflowActor,
    ) -> Result<(String, Vec<AiCopilotNotificationGroup>), DomainError> {
        let Some(workflow_service) = self.workflow_service.as_ref() else {
            return Ok(("not_required".to_string(), Vec::new()));
        };

        let batch_result = workflow_service
            .attach_existing_cases_to_workflow_batch_detailed(batch_id, workflow_items, actor)
            .await;
        match batch_result {
            Ok(result) => {
                let mut notification_groups = Vec::new();
                for group in result.notification_groups {
                    notification_groups.push(AiCopilotNotificationGroup {
                        group_id: group
                            .receipt_group_id
                            .clone()
                            .unwrap_or_else(|| ulid::Ulid::new().to_string()),
                        case_type: group.case_type,
                        case_ids: group.case_ids,
                        title: group.title,
                        body: group.body,
                    });
                }
                let groups_value = serde_json::to_value(&notification_groups)
                    .map_err(|error| DomainError::Internal(error.to_string()))?;
                let _ = self
                    .repo
                    .mark_workflow_dispatch_succeeded(batch_id, &groups_value)
                    .await?;
                Ok(("succeeded".to_string(), notification_groups))
            }
            Err(error) => {
                let error_payload = build_commit_error_payload("attach_workflow_batch", &error, false);
                let _ = self
                    .repo
                    .mark_workflow_dispatch_failed(batch_id, &error_payload)
                    .await?;
                Ok(("failed".to_string(), Vec::new()))
            }
        }
    }

    pub async fn commit_batch(
        &self,
        batch_id: &str,
        request: AiCopilotCommitRequest,
        access: AiCopilotBatchAccess,
        actor: WorkflowActor,
        visibility_scope: VisibilityScope,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common_case_types: bool,
    ) -> Result<AiCopilotCommitResponse, DomainError> {
        let batch = self
            .repo
            .find_by_id(batch_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "ai_copilot_business_case_batch",
                id: batch_id.to_string(),
            })?;
        ensure_batch_visible(&batch, &access)?;

        match batch.status {
            AiCopilotBatchStatus::Committed => {
                return Ok(AiCopilotCommitResponse {
                    batch_id: batch.batch_id,
                    case_ids: batch.committed_case_ids,
                    notification_groups: notification_groups_from_value(&batch.notification_groups),
                    already_committed: true,
                    workflow_dispatch_status: batch.workflow_dispatch_status,
                });
            }
            AiCopilotBatchStatus::Committing => {
                return Err(DomainError::ValidationError("该批次正在提交中，请稍后重试".into()));
            }
            AiCopilotBatchStatus::Failed => {
                return Err(DomainError::ValidationError(
                    "该批次上次提交失败，需人工核对后重新生成草稿或处理失败记录".into(),
                ));
            }
            AiCopilotBatchStatus::FailedResolved => {
                return Err(DomainError::ValidationError(
                    "该失败批次已被标记为人工处理完成，请重新生成草稿".into(),
                ));
            }
            AiCopilotBatchStatus::Expired => {
                return Err(DomainError::ValidationError("草稿批次已过期".into()));
            }
            AiCopilotBatchStatus::Draft => {}
        }

        if batch.expires_at < Utc::now() {
            return Err(DomainError::ValidationError("草稿批次已过期".into()));
        }

        if request.actions.is_empty() {
            return Err(DomainError::ValidationError("approved actions are required".into()));
        }

        let prepared_actions = self
            .prepare_commit_actions(
                &batch,
                &request,
                viewer_department_id,
                viewer_department_name,
                include_common_case_types,
                true,
            )
            .await?;

        let commit_request_value =
            serde_json::to_value(&request).map_err(|error| DomainError::Internal(error.to_string()))?;
        let next_recovery_at = Some(Utc::now() + Duration::seconds(COMMIT_RECOVERY_INITIAL_DELAY_SECONDS));
        let acquired_batch = match self
            .repo
            .try_begin_commit_with_request(batch_id, &commit_request_value, next_recovery_at)
            .await?
        {
            BeginCommitResult::Acquired(batch) => batch,
            BeginCommitResult::AlreadyCommitted(batch) => {
                return Ok(AiCopilotCommitResponse {
                    batch_id: batch.batch_id,
                    case_ids: batch.committed_case_ids,
                    notification_groups: notification_groups_from_value(&batch.notification_groups),
                    already_committed: true,
                    workflow_dispatch_status: batch.workflow_dispatch_status,
                });
            }
            BeginCommitResult::Conflict(batch) => {
                if batch.status == AiCopilotBatchStatus::Expired {
                    return Err(DomainError::ValidationError("草稿批次已过期".into()));
                }
                if batch.status == AiCopilotBatchStatus::Failed {
                    return Err(DomainError::ValidationError(
                        "该批次上次提交失败，需人工核对后重新生成草稿或处理失败记录".into(),
                    ));
                }
                if batch.status == AiCopilotBatchStatus::FailedResolved {
                    return Err(DomainError::ValidationError(
                        "该失败批次已被标记为人工处理完成，请重新生成草稿".into(),
                    ));
                }
                return Err(DomainError::ValidationError("该批次正在提交中，请稍后重试".into()));
            }
            BeginCommitResult::NotFound => {
                return Err(DomainError::NotFound {
                    entity_type: "ai_copilot_business_case_batch",
                    id: batch_id.to_string(),
                });
            }
        };

        if acquired_batch.expires_at < Utc::now() {
            let _ = self.repo.reset_commit_to_draft(&acquired_batch.batch_id).await;
            return Err(DomainError::ValidationError("草稿批次已过期".into()));
        }

        let cases = self
            .create_or_reuse_commit_cases(
                &acquired_batch.batch_id,
                &prepared_actions,
                &actor,
                visibility_scope,
                viewer_department_id,
                viewer_department_name,
            )
            .await?;
        let case_ids = cases.iter().map(|case| case.case_id.clone()).collect::<Vec<_>>();
        let workflow_items = cases
            .iter()
            .map(|case| BusinessCaseWorkflowBatchItem {
                template_code: case.case_type.clone(),
                case_id: case.case_id.clone(),
            })
            .collect::<Vec<_>>();

        let initial_notification_groups = if self.workflow_service.is_none() {
            build_notification_groups(&request.actions, &case_ids)
        } else {
            Vec::new()
        };
        let initial_notification_groups_value = serde_json::to_value(&initial_notification_groups)
            .map_err(|error| DomainError::Internal(error.to_string()))?;

        let (committed, notification_groups, workflow_dispatch_status) = if self.workflow_service.is_some() {
            let workflow_dispatch_request = build_workflow_dispatch_request(&workflow_items, &actor, &case_ids);
            let committed = self
                .repo
                .mark_committed_with_workflow_dispatch_request(
                    &acquired_batch.batch_id,
                    &case_ids,
                    &initial_notification_groups_value,
                    request.idempotency_key.as_deref(),
                    &workflow_dispatch_request,
                )
                .await?
                .ok_or_else(|| {
                    DomainError::Internal("failed to mark copilot batch committed with workflow request".into())
                })?;
            let (workflow_dispatch_status, notification_groups) = self
                .dispatch_workflow_for_committed_batch(&committed.batch_id, &workflow_items, &actor)
                .await?;
            (committed, notification_groups, workflow_dispatch_status)
        } else {
            let committed = self
                .repo
                .mark_committed(
                    &acquired_batch.batch_id,
                    &case_ids,
                    &initial_notification_groups_value,
                    request.idempotency_key.as_deref(),
                )
                .await?
                .ok_or_else(|| DomainError::Internal("failed to mark copilot batch committed".into()))?;
            (committed, initial_notification_groups, "not_required".to_string())
        };

        Ok(AiCopilotCommitResponse {
            batch_id: committed.batch_id,
            notification_groups,
            case_ids,
            already_committed: false,
            workflow_dispatch_status,
        })
    }

    pub async fn recover_stale_commits_once(
        &self,
        limit: i64,
        stale_after_seconds: i64,
        max_attempts: i32,
    ) -> Result<AiCopilotCommitRecoverySummary, DomainError> {
        let stale_before = Utc::now() - Duration::seconds(stale_after_seconds.max(1));
        let max_attempts = max_attempts.max(1);
        let batches = self.repo.recover_stale_committing(stale_before, limit).await?;
        let mut summary = AiCopilotCommitRecoverySummary {
            scanned: batches.len(),
            ..Default::default()
        };
        let actor = WorkflowActor {
            actor: "ai_copilot_commit_recovery_worker".to_string(),
            username: Some("ai_copilot_commit_recovery_worker".to_string()),
            name_snapshot: Some("AI Copilot Commit Recovery Worker".to_string()),
            ..Default::default()
        };

        for batch in batches {
            let batch_id = batch.batch_id.clone();
            let Some(commit_request_value) = batch.commit_request.clone() else {
                summary.legacy_missing_request += 1;
                summary.failed += 1;
                summary.errors.push(AiCopilotCommitRecoveryError {
                    batch_id: batch_id.clone(),
                    stage: "legacy_missing_request".to_string(),
                    message: "committing batch has no durable commit_request snapshot".to_string(),
                });
                let error_payload = json!({
                    "stage": "legacy_missing_request",
                    "message": "committing batch has no durable commit_request snapshot; recovery will not create cases blindly",
                    "recorded_at": Utc::now(),
                });
                let _ = self
                    .repo
                    .mark_commit_failed(&batch_id, &batch.committed_case_ids, &error_payload)
                    .await?;
                continue;
            };

            let request = match serde_json::from_value::<AiCopilotCommitRequest>(commit_request_value) {
                Ok(request) if !request.actions.is_empty() => request,
                Ok(_) => {
                    let error_payload = json!({
                        "stage": "commit_recovery_invalid_request",
                        "message": "commit_request contains no approved actions",
                        "recorded_at": Utc::now(),
                    });
                    let _ = self
                        .repo
                        .mark_commit_failed(&batch_id, &batch.committed_case_ids, &error_payload)
                        .await?;
                    summary.failed += 1;
                    summary.errors.push(AiCopilotCommitRecoveryError {
                        batch_id: batch_id.clone(),
                        stage: "commit_recovery_invalid_request".to_string(),
                        message: "commit_request contains no approved actions".to_string(),
                    });
                    continue;
                }
                Err(error) => {
                    let message = format!("commit_request snapshot is invalid: {error}");
                    let error_payload = json!({
                        "stage": "commit_recovery_invalid_request",
                        "message": message,
                        "recorded_at": Utc::now(),
                    });
                    let _ = self
                        .repo
                        .mark_commit_failed(&batch_id, &batch.committed_case_ids, &error_payload)
                        .await?;
                    summary.failed += 1;
                    summary.errors.push(AiCopilotCommitRecoveryError {
                        batch_id: batch_id.clone(),
                        stage: "commit_recovery_invalid_request".to_string(),
                        message,
                    });
                    continue;
                }
            };

            let recovered = self.recover_one_stale_commit(&batch, &request, &actor).await;
            match recovered {
                Ok(recovery) => {
                    summary.committed += 1;
                    summary.batch_ids.push(batch_id.clone());
                    match recovery.workflow_dispatch_status.as_str() {
                        "succeeded" => summary.dispatched += 1,
                        "failed" => summary.dispatch_failed += 1,
                        "pending" => summary.skipped += 1,
                        _ => {}
                    }
                }
                Err(error) => {
                    let terminal_error = is_terminal_commit_recovery_error(&error);
                    if terminal_error || batch.commit_attempts >= max_attempts {
                        let stage = if terminal_error {
                            "commit_recovery_terminal"
                        } else {
                            "commit_recovery_max_attempts_exhausted"
                        };
                        let message = if terminal_error {
                            error.to_string()
                        } else {
                            format!(
                                "commit recovery failed after {} attempts: {}",
                                batch.commit_attempts, error
                            )
                        };
                        let known_case_ids = self.known_commit_case_ids(&batch).await;
                        let error_payload = json!({
                            "stage": stage,
                            "message": message,
                            "recorded_at": Utc::now(),
                            "commit_attempts": batch.commit_attempts,
                            "max_attempts": max_attempts,
                            "case_ids": known_case_ids,
                        });
                        let _ = self
                            .repo
                            .mark_commit_failed(&batch_id, &known_case_ids, &error_payload)
                            .await?;
                    }
                    summary.failed += 1;
                    summary.errors.push(AiCopilotCommitRecoveryError {
                        batch_id: batch_id.clone(),
                        stage: "commit_recovery".to_string(),
                        message: error.to_string(),
                    });
                }
            }
        }

        Ok(summary)
    }

    async fn recover_one_stale_commit(
        &self,
        batch: &AiCopilotBusinessCaseBatch,
        request: &AiCopilotCommitRequest,
        actor: &WorkflowActor,
    ) -> Result<AiCopilotBatchStatusResponse, DomainError> {
        let prepared_actions = self
            .prepare_commit_actions(batch, request, None, None, true, false)
            .await?;
        let cases = self
            .create_or_reuse_commit_cases(
                &batch.batch_id,
                &prepared_actions,
                actor,
                VisibilityScope::Common,
                None,
                None,
            )
            .await?;
        let case_ids = cases.iter().map(|case| case.case_id.clone()).collect::<Vec<_>>();
        let workflow_items = cases
            .iter()
            .map(|case| BusinessCaseWorkflowBatchItem {
                template_code: case.case_type.clone(),
                case_id: case.case_id.clone(),
            })
            .collect::<Vec<_>>();

        let notification_groups = if batch.workflow_dispatch_status == "succeeded" {
            notification_groups_from_value(&batch.notification_groups)
        } else if self.workflow_service.is_none() {
            build_notification_groups(&request.actions, &case_ids)
        } else {
            Vec::new()
        };
        let notification_groups_value = if batch.workflow_dispatch_status == "succeeded" {
            batch.notification_groups.clone()
        } else {
            serde_json::to_value(&notification_groups).map_err(|error| DomainError::Internal(error.to_string()))?
        };
        let has_pending_workflow_snapshot =
            batch.workflow_dispatch_status == "pending" && batch.workflow_dispatch_request.is_some();
        let committed = if self.workflow_service.is_some()
            && batch.workflow_dispatch_status != "succeeded"
            && !has_pending_workflow_snapshot
        {
            let workflow_dispatch_request = build_workflow_dispatch_request(&workflow_items, actor, &case_ids);
            self.repo
                .mark_committed_with_workflow_dispatch_request(
                    &batch.batch_id,
                    &case_ids,
                    &notification_groups_value,
                    request.idempotency_key.as_deref(),
                    &workflow_dispatch_request,
                )
                .await?
                .ok_or_else(|| {
                    DomainError::Internal("failed to mark stale copilot batch committed with workflow request".into())
                })?
        } else {
            self.repo
                .mark_committed(
                    &batch.batch_id,
                    &case_ids,
                    &notification_groups_value,
                    request.idempotency_key.as_deref(),
                )
                .await?
                .ok_or_else(|| DomainError::Internal("failed to mark stale copilot batch committed".into()))?
        };

        if committed.workflow_dispatch_status == "succeeded"
            || has_pending_workflow_snapshot
            || self.workflow_service.is_none()
        {
            return Ok(batch_to_status_response(committed));
        }

        let _ = self
            .dispatch_workflow_for_committed_batch(&committed.batch_id, &workflow_items, actor)
            .await?;

        let latest = self.repo.find_by_id(&committed.batch_id).await?.unwrap_or(committed);
        Ok(batch_to_status_response(latest))
    }

    pub async fn retry_workflow_dispatch(
        &self,
        batch_id: &str,
        actor: WorkflowActor,
        access: AiCopilotBatchAccess,
    ) -> Result<AiCopilotBatchStatusResponse, DomainError> {
        let batch = self
            .repo
            .find_by_id(batch_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "ai_copilot_business_case_batch",
                id: batch_id.to_string(),
            })?;
        ensure_batch_ops_access(&batch, &access)?;

        let Some(workflow_service) = self.workflow_service.as_ref() else {
            return Err(DomainError::ValidationError("流程服务未配置，无法重试通知派发".into()));
        };

        if batch.status != AiCopilotBatchStatus::Committed {
            return Err(DomainError::ValidationError(
                "只有已创建事项的批次可以重试流程派发".into(),
            ));
        }

        match batch.workflow_dispatch_status.as_str() {
            "succeeded" => return Ok(batch_to_status_response(batch)),
            "pending" => return Ok(batch_to_status_response(batch)),
            "failed" => {}
            "not_required" => {
                return Err(DomainError::ValidationError("该批次没有需要重试的流程派发".into()));
            }
            other => {
                return Err(DomainError::ValidationError(format!("无效流程派发状态: {other}")));
            }
        }

        let retry_batch = match self.repo.try_begin_workflow_dispatch_retry(&batch.batch_id).await? {
            Some(retry_batch) => retry_batch,
            None => {
                let latest = self.repo.find_by_id(&batch.batch_id).await?.unwrap_or(batch);
                return Ok(batch_to_status_response(latest));
            }
        };

        let request = retry_batch
            .workflow_dispatch_request
            .clone()
            .ok_or_else(|| DomainError::ValidationError("缺少流程派发请求快照，无法安全重试".into()))?;
        let workflow_items = workflow_items_from_dispatch_request(&request)?;

        let result = workflow_service
            .attach_existing_cases_to_workflow_batch_detailed(&retry_batch.batch_id, &workflow_items, &actor)
            .await;

        match result {
            Ok(result) => {
                let mut notification_groups = Vec::new();
                for group in result.notification_groups {
                    notification_groups.push(AiCopilotNotificationGroup {
                        group_id: group
                            .receipt_group_id
                            .clone()
                            .unwrap_or_else(|| ulid::Ulid::new().to_string()),
                        case_type: group.case_type,
                        case_ids: group.case_ids,
                        title: group.title,
                        body: group.body,
                    });
                }
                let groups_value = serde_json::to_value(&notification_groups)
                    .map_err(|error| DomainError::Internal(error.to_string()))?;
                let updated = self
                    .repo
                    .mark_workflow_dispatch_succeeded(&retry_batch.batch_id, &groups_value)
                    .await?
                    .ok_or_else(|| {
                        DomainError::Internal("failed to mark copilot workflow dispatch succeeded".into())
                    })?;
                Ok(batch_to_status_response(updated))
            }
            Err(error) => {
                let error_payload = build_commit_error_payload("retry_workflow_dispatch", &error, false);
                let updated = self
                    .repo
                    .mark_workflow_dispatch_failed(&retry_batch.batch_id, &error_payload)
                    .await?;
                Ok(batch_to_status_response(updated.unwrap_or(retry_batch)))
            }
        }
    }

    pub async fn retry_due_workflow_dispatches_once(
        &self,
        limit: i64,
        max_attempts: i32,
    ) -> Result<AiCopilotWorkflowDispatchRetrySummary, DomainError> {
        let stale_before = Utc::now() - Duration::seconds(WORKFLOW_DISPATCH_PENDING_STALE_AFTER_SECONDS);
        let _ = self
            .repo
            .recover_stale_workflow_dispatch_pending(stale_before, limit)
            .await?;

        let batches = self
            .repo
            .list_due_workflow_dispatch_retries(limit, max_attempts)
            .await?;
        let mut summary = AiCopilotWorkflowDispatchRetrySummary {
            scanned: batches.len(),
            ..Default::default()
        };
        let actor = WorkflowActor {
            actor: "ai_copilot_workflow_dispatch_worker".to_string(),
            username: Some("ai_copilot_workflow_dispatch_worker".to_string()),
            name_snapshot: Some("AI Copilot Workflow Dispatch Worker".to_string()),
            ..Default::default()
        };

        for batch in batches {
            let batch_id = batch.batch_id;
            match self
                .retry_workflow_dispatch(&batch_id, actor.clone(), AiCopilotBatchAccess::unrestricted())
                .await
            {
                Ok(updated) => {
                    summary.batch_ids.push(batch_id.clone());
                    match updated.workflow_dispatch_status.as_str() {
                        "succeeded" => summary.succeeded += 1,
                        "failed" => summary.failed += 1,
                        _ => summary.skipped += 1,
                    }
                }
                Err(error) => {
                    summary.failed += 1;
                    summary.errors.push(AiCopilotWorkflowDispatchRetryError {
                        batch_id,
                        message: error.to_string(),
                    });
                }
            }
        }
        Ok(summary)
    }
}
