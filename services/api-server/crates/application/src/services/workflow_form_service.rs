use std::collections::HashSet;
use std::sync::Arc;

use chrono::Utc;
use serde_json::{Map, Value};

use fms_domain::error::DomainError;
use fms_domain::models::business_case::FlightBusinessCase;
use fms_domain::models::workflow_form::{
    WorkflowFormBinding, WorkflowFormSubmission, WorkflowFormSubmissionStatus, WorkflowFormTemplate,
    WorkflowFormTemplateStatus,
};
use fms_domain::ports::business_case_repository::BusinessCaseRepository;
use fms_domain::ports::business_case_workflow_run_repository::BusinessCaseWorkflowRunRepository;
use fms_domain::ports::workflow_form_repository::WorkflowFormRepository;

use crate::schemas::workflow_form_schemas::{
    CaseWorkflowFormsResponse, CreateWorkflowFormBindingRequest, CreateWorkflowFormTemplateRequest,
    SubmitWorkflowFormResponse, WorkflowTaskFormView,
};
use crate::services::flowable_service::{FlowableService, FlowableServiceError};

#[derive(Debug, Clone)]
pub struct WorkflowFormActor {
    pub user_id: String,
    pub username: Option<String>,
    pub operator_name: Option<String>,
    pub department_id: Option<String>,
    pub department_name: Option<String>,
    pub roles: Vec<String>,
}

pub struct WorkflowFormService {
    repo: Arc<dyn WorkflowFormRepository + Send + Sync>,
    business_case_repo: Arc<dyn BusinessCaseRepository + Send + Sync>,
    workflow_run_repo: Arc<dyn BusinessCaseWorkflowRunRepository + Send + Sync>,
    flowable_svc: Option<Arc<FlowableService>>,
}

impl WorkflowFormService {
    pub fn new(
        repo: Arc<dyn WorkflowFormRepository + Send + Sync>,
        business_case_repo: Arc<dyn BusinessCaseRepository + Send + Sync>,
        workflow_run_repo: Arc<dyn BusinessCaseWorkflowRunRepository + Send + Sync>,
    ) -> Self {
        Self {
            repo,
            business_case_repo,
            workflow_run_repo,
            flowable_svc: None,
        }
    }

    pub fn with_flowable_service(mut self, flowable_svc: Arc<FlowableService>) -> Self {
        self.flowable_svc = Some(flowable_svc);
        self
    }

    pub async fn create_template(
        &self,
        request: CreateWorkflowFormTemplateRequest,
        actor: &str,
    ) -> Result<WorkflowFormTemplate, DomainError> {
        let existing = self
            .repo
            .find_template_by_code_version(&request.form_code, request.version)
            .await?;

        let now = Utc::now();
        let template = WorkflowFormTemplate {
            id: existing
                .as_ref()
                .map(|value| value.id.clone())
                .unwrap_or_else(|| ulid::Ulid::new().to_string()),
            form_code: request.form_code.trim().to_string(),
            name: request.name.trim().to_string(),
            version: request.version,
            schema_json: request.schema_json,
            ui_schema_json: request.ui_schema_json,
            status: request.status.unwrap_or(WorkflowFormTemplateStatus::Draft),
            description: normalize_optional(&request.description),
            created_by: existing
                .as_ref()
                .map(|value| value.created_by.clone())
                .unwrap_or_else(|| actor.trim().to_string()),
            created_at: existing.as_ref().map(|value| value.created_at).unwrap_or(now),
            updated_at: now,
        };
        template.validate()?;
        self.repo.save_template(&template).await
    }

    pub async fn get_template(
        &self,
        form_code: &str,
        version: Option<i32>,
    ) -> Result<WorkflowFormTemplate, DomainError> {
        let template = if let Some(version) = version {
            self.repo.find_template_by_code_version(form_code, version).await?
        } else {
            self.repo.find_active_template_by_code(form_code).await?
        };
        template.ok_or_else(|| DomainError::NotFound {
            entity_type: "workflow_form_template",
            id: version
                .map(|value| format!("{form_code}@{value}"))
                .unwrap_or_else(|| form_code.to_string()),
        })
    }

    pub async fn create_binding(
        &self,
        request: CreateWorkflowFormBindingRequest,
    ) -> Result<WorkflowFormBinding, DomainError> {
        let template_exists = if let Some(version) = request.form_version {
            self.repo
                .find_template_by_code_version(&request.form_code, version)
                .await?
                .is_some()
        } else {
            self.repo
                .find_active_template_by_code(&request.form_code)
                .await?
                .is_some()
        };
        if !template_exists {
            return Err(DomainError::NotFound {
                entity_type: "workflow_form_template",
                id: request.form_code.clone(),
            });
        }

        let existing = self.repo.find_bindings_by_template_code(&request.template_code).await?;
        let matched_existing = existing.iter().find(|binding| {
            binding.task_definition_key == request.task_definition_key.trim()
                && binding.form_code == request.form_code.trim()
        });

        let now = Utc::now();
        let binding = WorkflowFormBinding {
            id: matched_existing
                .map(|value| value.id.clone())
                .unwrap_or_else(|| ulid::Ulid::new().to_string()),
            template_code: request.template_code.trim().to_string(),
            process_definition_key: request.process_definition_key.trim().to_string(),
            task_definition_key: request.task_definition_key.trim().to_string(),
            form_code: request.form_code.trim().to_string(),
            form_version: request.form_version,
            target_department_id: normalize_optional(&request.target_department_id),
            target_department_name: normalize_optional(&request.target_department_name),
            target_roles: request
                .target_roles
                .iter()
                .filter_map(|value| {
                    let normalized = value.trim();
                    (!normalized.is_empty()).then(|| normalized.to_string())
                })
                .collect(),
            assignment_mode: request.assignment_mode,
            write_back_mode: request.write_back_mode,
            write_back_key: request.write_back_key.trim().to_string(),
            flowable_variable_prefix: normalize_optional(&request.flowable_variable_prefix),
            complete_task_on_submit: request.complete_task_on_submit,
            allow_resubmit: request.allow_resubmit,
            source: request.source,
            created_at: matched_existing.map(|value| value.created_at).unwrap_or(now),
            updated_at: now,
        };
        binding.validate()?;
        self.repo.save_binding(&binding).await
    }

    pub async fn list_bindings(&self, template_code: &str) -> Result<Vec<WorkflowFormBinding>, DomainError> {
        self.repo.find_bindings_by_template_code(template_code).await
    }

    pub async fn get_forms_for_case_workflow(
        &self,
        case_id: &str,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        viewer_roles: &[String],
    ) -> Result<CaseWorkflowFormsResponse, DomainError> {
        let business_case =
            self.business_case_repo
                .find_by_id(case_id)
                .await?
                .ok_or_else(|| DomainError::NotFound {
                    entity_type: "business_case",
                    id: case_id.to_string(),
                })?;
        let run = self
            .workflow_run_repo
            .find_by_case_id(&business_case.case_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "business_case_workflow_run",
                id: business_case.case_id.clone(),
            })?;

        let flowable_svc = self.flowable_svc()?;
        let tasks = flowable_svc
            .list_tasks(&[("processInstanceId", run.process_instance_id.clone())])
            .await
            .map_err(map_flowable_error)?;

        let mut forms = Vec::new();
        for task in tasks {
            let Some(task_id) = task_field(&task, "id") else {
                continue;
            };
            let Some(task_definition_key) = task_field(&task, "taskDefinitionKey") else {
                continue;
            };
            let task_name = task_field(&task, "name").unwrap_or_else(|| task_definition_key.clone());
            let bindings = self
                .repo
                .find_bindings_by_process_task(&run.process_definition_key, &task_definition_key)
                .await?;

            for binding in bindings {
                let template = self.resolve_template_for_binding(&binding).await?;
                let latest_submission = self
                    .repo
                    .find_latest_submission(&business_case.case_id, &task_definition_key, &binding.form_code)
                    .await?;

                let can_submit = latest_submission.is_none() || binding.allow_resubmit;
                let actor_allowed = binding.matches_actor(viewer_department_id, viewer_department_name, viewer_roles);
                let readonly_reason = if latest_submission.is_some() && !binding.allow_resubmit {
                    Some("表单已提交且不允许重复提交".to_string())
                } else if !actor_allowed {
                    Some("当前用户不满足表单填写条件".to_string())
                } else {
                    None
                };

                forms.push(WorkflowTaskFormView {
                    task_id: task_id.clone(),
                    task_definition_key: task_definition_key.clone(),
                    task_name: task_name.clone(),
                    form_code: binding.form_code.clone(),
                    form_version: template.version,
                    name: template.name.clone(),
                    schema: template.schema_json.clone(),
                    ui_schema: template.ui_schema_json.clone(),
                    can_submit: can_submit && actor_allowed,
                    readonly_reason,
                    latest_submission: latest_submission.map(Into::into),
                });
            }
        }

        Ok(CaseWorkflowFormsResponse {
            case_id: business_case.case_id,
            run_id: run.run_id,
            process_instance_id: run.process_instance_id,
            forms,
        })
    }

    pub async fn submit_task_form(
        &self,
        case_id: &str,
        form_code: &str,
        task_id: &str,
        payload: Value,
        actor: &WorkflowFormActor,
    ) -> Result<SubmitWorkflowFormResponse, DomainError> {
        validate_actor(actor)?;
        let mut business_case =
            self.business_case_repo
                .find_by_id(case_id)
                .await?
                .ok_or_else(|| DomainError::NotFound {
                    entity_type: "business_case",
                    id: case_id.to_string(),
                })?;
        let run = self
            .workflow_run_repo
            .find_by_case_id(&business_case.case_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "business_case_workflow_run",
                id: business_case.case_id.clone(),
            })?;

        let flowable_svc = self.flowable_svc()?;
        let task = flowable_svc
            .get_task(task_id)
            .await
            .map_err(map_flowable_error)?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "workflow_task",
                id: task_id.to_string(),
            })?;

        let task_process_instance_id = task_field(&task, "processInstanceId")
            .ok_or_else(|| DomainError::Internal("flowable task missing processInstanceId".to_string()))?;
        if task_process_instance_id != run.process_instance_id {
            return Err(DomainError::Conflict(format!(
                "task {} does not belong to business case {}",
                task_id, case_id
            )));
        }

        let task_definition_key = task_field(&task, "taskDefinitionKey")
            .ok_or_else(|| DomainError::Internal("flowable task missing taskDefinitionKey".to_string()))?;
        let binding = self
            .repo
            .find_bindings_by_process_task(&run.process_definition_key, &task_definition_key)
            .await?
            .into_iter()
            .find(|candidate| candidate.form_code == form_code.trim())
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "workflow_form_binding",
                id: format!("{}:{}", task_definition_key, form_code),
            })?;

        if !binding.matches_actor(
            actor.department_id.as_deref(),
            actor.department_name.as_deref(),
            &actor.roles,
        ) {
            return Err(DomainError::PermissionDenied(
                "当前用户不具备该表单的填写权限".to_string(),
            ));
        }

        let latest_submission = self
            .repo
            .find_latest_submission(&business_case.case_id, &task_definition_key, form_code)
            .await?;
        ensure_resubmission_allowed(&binding, latest_submission.as_ref())?;

        let template = self.resolve_template_for_binding(&binding).await?;
        validate_submission_payload(&template.schema_json, &payload)?;

        let now = Utc::now();
        let submission = WorkflowFormSubmission {
            id: ulid::Ulid::new().to_string(),
            case_id: business_case.case_id.clone(),
            run_id: Some(run.run_id.clone()),
            process_instance_id: run.process_instance_id.clone(),
            task_id: task_id.trim().to_string(),
            task_definition_key: task_definition_key.clone(),
            form_code: form_code.trim().to_string(),
            form_version: template.version,
            data_json: payload.clone(),
            normalized_summary_json: build_summary(&payload),
            submitted_by: actor.user_id.clone(),
            submitted_operator_name: actor.operator_name.clone(),
            submitted_department_id: actor.department_id.clone(),
            submitted_department_name: actor.department_name.clone(),
            submitted_at: now,
            status: WorkflowFormSubmissionStatus::Submitted,
        };
        submission.validate()?;

        let saved_submission = self.repo.insert_submission(&submission).await?;
        merge_form_projection(&mut business_case, &saved_submission, &binding);
        business_case.updated_by = actor.username.as_deref().unwrap_or(actor.user_id.as_str()).to_string();
        let updated = self.business_case_repo.update_case(&business_case).await?;
        if !updated {
            return Err(DomainError::ConcurrencyConflict(
                "business case projection update did not persist".to_string(),
            ));
        }

        let variable_prefix = binding
            .flowable_variable_prefix
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| default_variable_prefix(&binding.form_code));
        let variables = build_flowable_variables(&variable_prefix, &saved_submission);
        flowable_svc
            .set_process_instance_variables(&run.process_instance_id, &variables)
            .await
            .map_err(map_flowable_error)?;

        let flowable_task_completed = if binding.complete_task_on_submit {
            flowable_svc
                .complete_task(task_id, None)
                .await
                .map_err(map_flowable_error)?
        } else {
            false
        };

        Ok(SubmitWorkflowFormResponse {
            submission_id: saved_submission.id,
            case_id: business_case.case_id.clone(),
            form_code: saved_submission.form_code,
            form_version: saved_submission.form_version,
            flowable_task_completed,
            business_case: serde_json::to_value(business_case).unwrap_or(Value::Null),
        })
    }

    async fn resolve_template_for_binding(
        &self,
        binding: &WorkflowFormBinding,
    ) -> Result<WorkflowFormTemplate, DomainError> {
        if let Some(version) = binding.form_version {
            self.repo
                .find_template_by_code_version(&binding.form_code, version)
                .await?
                .ok_or_else(|| DomainError::NotFound {
                    entity_type: "workflow_form_template",
                    id: format!("{}@{}", binding.form_code, version),
                })
        } else {
            self.repo
                .find_active_template_by_code(&binding.form_code)
                .await?
                .ok_or_else(|| DomainError::NotFound {
                    entity_type: "workflow_form_template",
                    id: binding.form_code.clone(),
                })
        }
    }

    fn flowable_svc(&self) -> Result<&Arc<FlowableService>, DomainError> {
        self.flowable_svc
            .as_ref()
            .ok_or_else(|| DomainError::Internal("workflow form flowable service is not configured".to_string()))
    }
}

fn validate_actor(actor: &WorkflowFormActor) -> Result<(), DomainError> {
    if actor.user_id.trim().is_empty() {
        return Err(DomainError::Unauthorized(
            "workflow form actor is missing user_id".to_string(),
        ));
    }
    Ok(())
}

fn normalize_optional(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
}

fn task_field(task: &Value, field: &str) -> Option<String> {
    task.get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn ensure_resubmission_allowed(
    binding: &WorkflowFormBinding,
    latest_submission: Option<&WorkflowFormSubmission>,
) -> Result<(), DomainError> {
    if latest_submission.is_some() && !binding.allow_resubmit {
        return Err(DomainError::Conflict(format!(
            "form {} for task {} has already been submitted",
            binding.form_code, binding.task_definition_key
        )));
    }
    Ok(())
}

fn validate_submission_payload(schema: &Value, payload: &Value) -> Result<(), DomainError> {
    let schema_obj = schema
        .as_object()
        .ok_or_else(|| DomainError::ValidationError("workflow form schema must be a JSON object".to_string()))?;
    let payload_obj = payload.as_object().ok_or_else(|| {
        DomainError::ValidationError("workflow form submission payload must be a JSON object".to_string())
    })?;

    let allowed_fields = schema_obj
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    for field in payload_obj.keys() {
        if !allowed_fields.is_empty() && !allowed_fields.contains_key(field) {
            return Err(DomainError::ValidationError(format!(
                "field '{field}' is not defined in workflow form schema"
            )));
        }
    }

    let required_fields: HashSet<String> = schema_obj
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    for field in required_fields {
        let missing = payload_obj.get(&field).map(Value::is_null).unwrap_or(true);
        if missing {
            return Err(DomainError::ValidationError(format!("field '{field}' is required")));
        }
    }

    for (field, value) in payload_obj {
        if let Some(field_schema) = allowed_fields.get(field) {
            validate_field_against_schema(field, value, field_schema)?;
        }
    }

    Ok(())
}

fn validate_field_against_schema(field: &str, value: &Value, field_schema: &Value) -> Result<(), DomainError> {
    if let Some(enum_values) = field_schema.get("enum").and_then(Value::as_array) {
        if !enum_values.iter().any(|candidate| candidate == value) {
            return Err(DomainError::ValidationError(format!(
                "field '{field}' is outside enum constraints"
            )));
        }
    }

    let Some(field_type) = field_schema.get("type").and_then(Value::as_str) else {
        return Ok(());
    };

    let matches_type = match field_type {
        "string" => value.is_string(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "object" => value.is_object(),
        "array" => value.is_array(),
        _ => true,
    };
    if matches_type {
        Ok(())
    } else {
        Err(DomainError::ValidationError(format!(
            "field '{field}' does not match schema type '{field_type}'"
        )))
    }
}

fn build_summary(payload: &Value) -> Value {
    let Some(object) = payload.as_object() else {
        return serde_json::json!({});
    };

    let summary = object
        .iter()
        .filter_map(|(key, value)| match value {
            Value::String(_) | Value::Number(_) | Value::Bool(_) => Some((key.clone(), value.clone())),
            Value::Array(values) if values.iter().all(Value::is_string) => Some((key.clone(), value.clone())),
            _ => None,
        })
        .collect::<Map<String, Value>>();
    Value::Object(summary)
}

fn merge_form_projection(
    business_case: &mut FlightBusinessCase,
    submission: &WorkflowFormSubmission,
    binding: &WorkflowFormBinding,
) {
    let mut forms_map = match business_case.context.remove("forms") {
        Some(Value::Object(existing)) => existing,
        _ => Map::new(),
    };

    forms_map.insert(
        submission.form_code.clone(),
        serde_json::json!({
            "form_code": submission.form_code,
            "form_version": submission.form_version,
            "task_definition_key": submission.task_definition_key,
            "submission_id": submission.id,
            "submitted_by": submission.submitted_by,
            "submitted_operator_name": submission.submitted_operator_name,
            "submitted_department_id": submission.submitted_department_id,
            "submitted_department_name": submission.submitted_department_name,
            "submitted_at": submission.submitted_at.to_rfc3339(),
            "write_back_key": binding.write_back_key,
            "data": submission.data_json,
            "summary": submission.normalized_summary_json,
        }),
    );
    business_case
        .context
        .insert("forms".to_string(), Value::Object(forms_map));
}

fn build_flowable_variables(prefix: &str, submission: &WorkflowFormSubmission) -> Map<String, Value> {
    let normalized_prefix = prefix.trim();
    let prefix = if normalized_prefix.is_empty() {
        default_variable_prefix(&submission.form_code)
    } else {
        normalized_prefix.to_string()
    };

    let mut variables = Map::new();
    if let Some(object) = submission.data_json.as_object() {
        for (field, value) in object {
            if matches!(value, Value::String(_) | Value::Number(_) | Value::Bool(_)) {
                variables.insert(format!("{prefix}.{}", sanitize_variable_key(field)), value.clone());
            }
        }
    }
    variables.insert(
        format!("{prefix}.submittedAt"),
        Value::String(submission.submitted_at.to_rfc3339()),
    );
    variables.insert(
        format!("{prefix}.submittedBy"),
        Value::String(submission.submitted_by.clone()),
    );
    variables.insert(format!("{prefix}.submissionId"), Value::String(submission.id.clone()));
    if let Some(operator_name) = submission.submitted_operator_name.as_ref() {
        variables.insert(
            format!("{prefix}.submittedOperatorName"),
            Value::String(operator_name.clone()),
        );
    }
    variables
}

fn default_variable_prefix(form_code: &str) -> String {
    let mut output = String::new();
    let mut capitalize_next = false;
    for ch in form_code.chars() {
        if ch.is_ascii_alphanumeric() {
            if output.is_empty() {
                output.push(ch.to_ascii_lowercase());
            } else if capitalize_next {
                output.push(ch.to_ascii_uppercase());
                capitalize_next = false;
            } else {
                output.push(ch);
            }
        } else {
            capitalize_next = !output.is_empty();
        }
    }
    if output.is_empty() {
        "workflowForm".to_string()
    } else {
        output
    }
}

fn sanitize_variable_key(value: &str) -> String {
    let mut output = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            output.push(ch);
        } else {
            output.push('_');
        }
    }
    output
}

fn map_flowable_error(error: FlowableServiceError) -> DomainError {
    match error {
        FlowableServiceError::Validation(message) => DomainError::ValidationError(message),
        FlowableServiceError::NotFound(message) => DomainError::NotFound {
            entity_type: "flowable",
            id: message,
        },
        FlowableServiceError::Upstream(message) => DomainError::Internal(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use fms_domain::models::workflow_form::WorkflowFormBindingSource;

    #[test]
    fn payload_validation_rejects_missing_and_unknown_fields() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "result": {"type": "string", "enum": ["approved", "rejected"]},
                "confirmed": {"type": "boolean"}
            },
            "required": ["result"]
        });

        let missing = validate_submission_payload(&schema, &serde_json::json!({"confirmed": true}));
        assert!(matches!(missing, Err(DomainError::ValidationError(_))));

        let unknown = validate_submission_payload(&schema, &serde_json::json!({"result": "approved", "extra": 1}));
        assert!(matches!(unknown, Err(DomainError::ValidationError(_))));
    }

    #[test]
    fn resubmission_is_blocked_when_binding_disallows_it() {
        let binding = WorkflowFormBinding {
            id: "01BINDING".to_string(),
            template_code: "delay_case".to_string(),
            process_definition_key: "delay_case".to_string(),
            task_definition_key: "ground_confirm".to_string(),
            form_code: "ground_confirm_form".to_string(),
            form_version: Some(1),
            target_department_id: Some("ops-1".to_string()),
            target_department_name: None,
            target_roles: vec![],
            assignment_mode: fms_domain::models::workflow_form::WorkflowFormAssignmentMode::DepartmentRoles,
            write_back_mode: fms_domain::models::workflow_form::WorkflowFormWriteBackMode::BusinessCaseContext,
            write_back_key: "forms.ground_confirm".to_string(),
            flowable_variable_prefix: None,
            complete_task_on_submit: true,
            allow_resubmit: false,
            source: WorkflowFormBindingSource::Db,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let submission = WorkflowFormSubmission {
            id: "01SUBMISSION".to_string(),
            case_id: "case-1".to_string(),
            run_id: Some("run-1".to_string()),
            process_instance_id: "proc-1".to_string(),
            task_id: "task-1".to_string(),
            task_definition_key: "ground_confirm".to_string(),
            form_code: "ground_confirm_form".to_string(),
            form_version: 1,
            data_json: serde_json::json!({"result": "approved"}),
            normalized_summary_json: serde_json::json!({"result": "approved"}),
            submitted_by: "user-1".to_string(),
            submitted_operator_name: None,
            submitted_department_id: None,
            submitted_department_name: None,
            submitted_at: Utc::now(),
            status: WorkflowFormSubmissionStatus::Submitted,
        };

        assert!(matches!(
            ensure_resubmission_allowed(&binding, Some(&submission)),
            Err(DomainError::Conflict(_))
        ));
    }

    #[test]
    fn projection_merge_preserves_existing_context_fields() {
        let mut business_case = FlightBusinessCase {
            case_id: "case-1".to_string(),
            case_type: "delay_case".to_string(),
            case_type_name: Some("航班延误".to_string()),
            flight_id: "flight-1".to_string(),
            flight_no: "MU123".to_string(),
            created_at: Utc::now(),
            created_by: "creator".to_string(),
            updated_by: "creator".to_string(),
            description: "desc".to_string(),
            status: "PENDING".to_string(),
            stand: None,
            gate: None,
            visibility_scope: fms_domain::models::business_case::VisibilityScope::Department,
            department_id: Some("ops-1".to_string()),
            department_name_snapshot: Some("运行控制".to_string()),
            finished_at: None,
            cancelled_at: None,
            log: vec![],
            context: HashMap::from([("existing".to_string(), serde_json::json!({"x": 1}))]),
            workflow_receipt: None,
            terminal_metadata: None,
            append_count: 0,
            latest_append: None,
            append_entries: vec![],
        };
        let submission = WorkflowFormSubmission {
            id: "01SUBMISSION".to_string(),
            case_id: "case-1".to_string(),
            run_id: Some("run-1".to_string()),
            process_instance_id: "proc-1".to_string(),
            task_id: "task-1".to_string(),
            task_definition_key: "ground_confirm".to_string(),
            form_code: "ground_confirm_form".to_string(),
            form_version: 1,
            data_json: serde_json::json!({"result": "approved"}),
            normalized_summary_json: serde_json::json!({"result": "approved"}),
            submitted_by: "user-1".to_string(),
            submitted_operator_name: Some("值班员".to_string()),
            submitted_department_id: Some("ops-1".to_string()),
            submitted_department_name: Some("运行控制".to_string()),
            submitted_at: Utc::now(),
            status: WorkflowFormSubmissionStatus::Submitted,
        };
        let binding = WorkflowFormBinding {
            id: "01BINDING".to_string(),
            template_code: "delay_case".to_string(),
            process_definition_key: "delay_case".to_string(),
            task_definition_key: "ground_confirm".to_string(),
            form_code: "ground_confirm_form".to_string(),
            form_version: Some(1),
            target_department_id: Some("ops-1".to_string()),
            target_department_name: None,
            target_roles: vec![],
            assignment_mode: fms_domain::models::workflow_form::WorkflowFormAssignmentMode::DepartmentRoles,
            write_back_mode: fms_domain::models::workflow_form::WorkflowFormWriteBackMode::BusinessCaseContext,
            write_back_key: "forms.ground_confirm".to_string(),
            flowable_variable_prefix: None,
            complete_task_on_submit: true,
            allow_resubmit: false,
            source: WorkflowFormBindingSource::Db,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        merge_form_projection(&mut business_case, &submission, &binding);

        assert_eq!(business_case.context["existing"]["x"], 1);
        assert_eq!(
            business_case.context["forms"]["ground_confirm_form"]["submission_id"],
            "01SUBMISSION"
        );
    }

    #[test]
    fn flowable_variables_include_payload_and_metadata() {
        let submission = WorkflowFormSubmission {
            id: "01SUBMISSION".to_string(),
            case_id: "case-1".to_string(),
            run_id: Some("run-1".to_string()),
            process_instance_id: "proc-1".to_string(),
            task_id: "task-1".to_string(),
            task_definition_key: "ground_confirm".to_string(),
            form_code: "ground_confirm_form".to_string(),
            form_version: 1,
            data_json: serde_json::json!({"result": "approved", "score": 5, "detail": {"nested": true}}),
            normalized_summary_json: serde_json::json!({"result": "approved"}),
            submitted_by: "user-1".to_string(),
            submitted_operator_name: Some("值班员".to_string()),
            submitted_department_id: None,
            submitted_department_name: None,
            submitted_at: Utc::now(),
            status: WorkflowFormSubmissionStatus::Submitted,
        };

        let variables = build_flowable_variables("groundConfirm", &submission);

        assert_eq!(variables["groundConfirm.result"], "approved");
        assert_eq!(variables["groundConfirm.score"], 5);
        assert!(variables.get("groundConfirm.detail").is_none());
        assert_eq!(variables["groundConfirm.submissionId"], "01SUBMISSION");
        assert_eq!(default_variable_prefix("ground_confirm_form"), "groundConfirmForm");
    }
}
