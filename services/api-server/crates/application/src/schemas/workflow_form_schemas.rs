use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use fms_domain::models::workflow_form::{
    WorkflowFormAssignmentMode, WorkflowFormBinding, WorkflowFormBindingSource, WorkflowFormSubmission,
    WorkflowFormSubmissionStatus, WorkflowFormTemplate, WorkflowFormTemplateStatus, WorkflowFormWriteBackMode,
};

fn empty_object() -> Value {
    serde_json::json!({})
}

fn default_complete_task_on_submit() -> bool {
    true
}

fn default_assignment_mode() -> WorkflowFormAssignmentMode {
    WorkflowFormAssignmentMode::DepartmentRoles
}

fn default_write_back_mode() -> WorkflowFormWriteBackMode {
    WorkflowFormWriteBackMode::BusinessCaseContext
}

fn default_binding_source() -> WorkflowFormBindingSource {
    WorkflowFormBindingSource::Db
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateWorkflowFormTemplateRequest {
    pub form_code: String,
    pub name: String,
    pub version: i32,
    pub schema_json: Value,
    #[serde(default = "empty_object")]
    pub ui_schema_json: Value,
    #[serde(default)]
    pub status: Option<WorkflowFormTemplateStatus>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowFormTemplateResponse {
    pub id: String,
    pub form_code: String,
    pub name: String,
    pub version: i32,
    pub schema_json: Value,
    pub ui_schema_json: Value,
    pub status: WorkflowFormTemplateStatus,
    pub description: Option<String>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<WorkflowFormTemplate> for WorkflowFormTemplateResponse {
    fn from(value: WorkflowFormTemplate) -> Self {
        Self {
            id: value.id,
            form_code: value.form_code,
            name: value.name,
            version: value.version,
            schema_json: value.schema_json,
            ui_schema_json: value.ui_schema_json,
            status: value.status,
            description: value.description,
            created_by: value.created_by,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateWorkflowFormBindingRequest {
    pub template_code: String,
    pub process_definition_key: String,
    pub task_definition_key: String,
    pub form_code: String,
    pub form_version: Option<i32>,
    pub target_department_id: Option<String>,
    pub target_department_name: Option<String>,
    #[serde(default)]
    pub target_roles: Vec<String>,
    #[serde(default = "default_assignment_mode")]
    pub assignment_mode: WorkflowFormAssignmentMode,
    #[serde(default = "default_write_back_mode")]
    pub write_back_mode: WorkflowFormWriteBackMode,
    pub write_back_key: String,
    pub flowable_variable_prefix: Option<String>,
    #[serde(default = "default_complete_task_on_submit")]
    pub complete_task_on_submit: bool,
    #[serde(default)]
    pub allow_resubmit: bool,
    #[serde(default = "default_binding_source")]
    pub source: WorkflowFormBindingSource,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowFormBindingResponse {
    pub id: String,
    pub template_code: String,
    pub process_definition_key: String,
    pub task_definition_key: String,
    pub form_code: String,
    pub form_version: Option<i32>,
    pub target_department_id: Option<String>,
    pub target_department_name: Option<String>,
    pub target_roles: Vec<String>,
    pub assignment_mode: WorkflowFormAssignmentMode,
    pub write_back_mode: WorkflowFormWriteBackMode,
    pub write_back_key: String,
    pub flowable_variable_prefix: Option<String>,
    pub complete_task_on_submit: bool,
    pub allow_resubmit: bool,
    pub source: WorkflowFormBindingSource,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<WorkflowFormBinding> for WorkflowFormBindingResponse {
    fn from(value: WorkflowFormBinding) -> Self {
        Self {
            id: value.id,
            template_code: value.template_code,
            process_definition_key: value.process_definition_key,
            task_definition_key: value.task_definition_key,
            form_code: value.form_code,
            form_version: value.form_version,
            target_department_id: value.target_department_id,
            target_department_name: value.target_department_name,
            target_roles: value.target_roles,
            assignment_mode: value.assignment_mode,
            write_back_mode: value.write_back_mode,
            write_back_key: value.write_back_key,
            flowable_variable_prefix: value.flowable_variable_prefix,
            complete_task_on_submit: value.complete_task_on_submit,
            allow_resubmit: value.allow_resubmit,
            source: value.source,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubmitWorkflowFormRequest {
    pub task_id: String,
    pub data: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowFormSubmissionResponse {
    pub submission_id: String,
    pub case_id: String,
    pub run_id: Option<String>,
    pub process_instance_id: String,
    pub task_id: String,
    pub task_definition_key: String,
    pub form_code: String,
    pub form_version: i32,
    pub data: Value,
    pub summary: Value,
    pub submitted_by: String,
    pub submitted_operator_name: Option<String>,
    pub submitted_department_id: Option<String>,
    pub submitted_department_name: Option<String>,
    pub submitted_at: DateTime<Utc>,
    pub status: WorkflowFormSubmissionStatus,
}

impl From<WorkflowFormSubmission> for WorkflowFormSubmissionResponse {
    fn from(value: WorkflowFormSubmission) -> Self {
        Self {
            submission_id: value.id,
            case_id: value.case_id,
            run_id: value.run_id,
            process_instance_id: value.process_instance_id,
            task_id: value.task_id,
            task_definition_key: value.task_definition_key,
            form_code: value.form_code,
            form_version: value.form_version,
            data: value.data_json,
            summary: value.normalized_summary_json,
            submitted_by: value.submitted_by,
            submitted_operator_name: value.submitted_operator_name,
            submitted_department_id: value.submitted_department_id,
            submitted_department_name: value.submitted_department_name,
            submitted_at: value.submitted_at,
            status: value.status,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowTaskFormView {
    pub task_id: String,
    pub task_definition_key: String,
    pub task_name: String,
    pub form_code: String,
    pub form_version: i32,
    pub name: String,
    pub schema: Value,
    pub ui_schema: Value,
    pub can_submit: bool,
    pub readonly_reason: Option<String>,
    pub latest_submission: Option<WorkflowFormSubmissionResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CaseWorkflowFormsResponse {
    pub case_id: String,
    pub run_id: String,
    pub process_instance_id: String,
    pub forms: Vec<WorkflowTaskFormView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubmitWorkflowFormResponse {
    pub submission_id: String,
    pub case_id: String,
    pub form_code: String,
    pub form_version: i32,
    pub flowable_task_completed: bool,
    pub business_case: serde_json::Value,
}
