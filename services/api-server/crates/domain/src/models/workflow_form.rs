use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkflowFormTemplateStatus {
    Draft,
    Active,
    Retired,
}

impl WorkflowFormTemplateStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Active => "ACTIVE",
            Self::Retired => "RETIRED",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "DRAFT" => Some(Self::Draft),
            "ACTIVE" => Some(Self::Active),
            "RETIRED" => Some(Self::Retired),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkflowFormAssignmentMode {
    DepartmentRoles,
    TaskCandidate,
    ExplicitUsers,
}

impl WorkflowFormAssignmentMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DepartmentRoles => "DEPARTMENT_ROLES",
            Self::TaskCandidate => "TASK_CANDIDATE",
            Self::ExplicitUsers => "EXPLICIT_USERS",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "DEPARTMENT_ROLES" => Some(Self::DepartmentRoles),
            "TASK_CANDIDATE" => Some(Self::TaskCandidate),
            "EXPLICIT_USERS" => Some(Self::ExplicitUsers),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkflowFormWriteBackMode {
    BusinessCaseContext,
    AppendEntry,
    Both,
}

impl WorkflowFormWriteBackMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BusinessCaseContext => "BUSINESS_CASE_CONTEXT",
            Self::AppendEntry => "APPEND_ENTRY",
            Self::Both => "BOTH",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "BUSINESS_CASE_CONTEXT" => Some(Self::BusinessCaseContext),
            "APPEND_ENTRY" => Some(Self::AppendEntry),
            "BOTH" => Some(Self::Both),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkflowFormBindingSource {
    Db,
    Bpmn,
    DbOverride,
}

impl WorkflowFormBindingSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Db => "DB",
            Self::Bpmn => "BPMN",
            Self::DbOverride => "DB_OVERRIDE",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "DB" => Some(Self::Db),
            "BPMN" => Some(Self::Bpmn),
            "DB_OVERRIDE" => Some(Self::DbOverride),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkflowFormSubmissionStatus {
    Submitted,
    Replaced,
    Revoked,
}

impl WorkflowFormSubmissionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Submitted => "SUBMITTED",
            Self::Replaced => "REPLACED",
            Self::Revoked => "REVOKED",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "SUBMITTED" => Some(Self::Submitted),
            "REPLACED" => Some(Self::Replaced),
            "REVOKED" => Some(Self::Revoked),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowFormTemplate {
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

impl WorkflowFormTemplate {
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.form_code.trim().is_empty() {
            return Err(DomainError::ValidationError("form_code is required".to_string()));
        }
        if self.name.trim().is_empty() {
            return Err(DomainError::ValidationError("name is required".to_string()));
        }
        if self.version <= 0 {
            return Err(DomainError::ValidationError(
                "version must be greater than 0".to_string(),
            ));
        }
        if !self.schema_json.is_object() {
            return Err(DomainError::ValidationError(
                "schema_json must be a JSON object".to_string(),
            ));
        }
        if !self.ui_schema_json.is_object() {
            return Err(DomainError::ValidationError(
                "ui_schema_json must be a JSON object".to_string(),
            ));
        }
        if self.created_by.trim().is_empty() {
            return Err(DomainError::ValidationError("created_by is required".to_string()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowFormBinding {
    pub id: String,
    pub template_code: String,
    pub process_definition_key: String,
    pub task_definition_key: String,
    pub form_code: String,
    pub form_version: Option<i32>,
    pub target_department_id: Option<String>,
    pub target_department_name: Option<String>,
    #[serde(default)]
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

impl WorkflowFormBinding {
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.template_code.trim().is_empty() {
            return Err(DomainError::ValidationError("template_code is required".to_string()));
        }
        if self.process_definition_key.trim().is_empty() {
            return Err(DomainError::ValidationError(
                "process_definition_key is required".to_string(),
            ));
        }
        if self.task_definition_key.trim().is_empty() {
            return Err(DomainError::ValidationError(
                "task_definition_key is required".to_string(),
            ));
        }
        if self.form_code.trim().is_empty() {
            return Err(DomainError::ValidationError("form_code is required".to_string()));
        }
        if self.form_version.is_some_and(|value| value <= 0) {
            return Err(DomainError::ValidationError(
                "form_version must be greater than 0 when provided".to_string(),
            ));
        }
        if self.write_back_key.trim().is_empty() {
            return Err(DomainError::ValidationError("write_back_key is required".to_string()));
        }
        let has_department_scope = self
            .target_department_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
            || self
                .target_department_name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_some();
        let has_role_scope = self.target_roles.iter().any(|value| !value.trim().is_empty());
        if !has_department_scope && !has_role_scope {
            return Err(DomainError::ValidationError(
                "binding must specify at least one department or role target".to_string(),
            ));
        }
        Ok(())
    }

    pub fn matches_actor(&self, department_id: Option<&str>, department_name: Option<&str>, roles: &[String]) -> bool {
        let department_constraint = if self
            .target_department_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
            || self
                .target_department_name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_some()
        {
            self.target_department_id
                .as_deref()
                .is_some_and(|target| department_matches(Some(target), department_id))
                || self
                    .target_department_name
                    .as_deref()
                    .is_some_and(|target| department_matches(Some(target), department_name))
        } else {
            true
        };

        let role_constraint = if self.target_roles.iter().any(|value| !value.trim().is_empty()) {
            self.target_roles.iter().any(|target| {
                roles
                    .iter()
                    .any(|role| normalized_eq(Some(target), Some(role.as_str())))
            })
        } else {
            true
        };

        department_constraint && role_constraint
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowFormSubmission {
    pub id: String,
    pub case_id: String,
    pub run_id: Option<String>,
    pub process_instance_id: String,
    pub task_id: String,
    pub task_definition_key: String,
    pub form_code: String,
    pub form_version: i32,
    pub data_json: Value,
    pub normalized_summary_json: Value,
    pub submitted_by: String,
    pub submitted_operator_name: Option<String>,
    pub submitted_department_id: Option<String>,
    pub submitted_department_name: Option<String>,
    pub submitted_at: DateTime<Utc>,
    pub status: WorkflowFormSubmissionStatus,
}

impl WorkflowFormSubmission {
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.case_id.trim().is_empty() {
            return Err(DomainError::ValidationError("case_id is required".to_string()));
        }
        if self.process_instance_id.trim().is_empty() {
            return Err(DomainError::ValidationError(
                "process_instance_id is required".to_string(),
            ));
        }
        if self.task_id.trim().is_empty() {
            return Err(DomainError::ValidationError("task_id is required".to_string()));
        }
        if self.task_definition_key.trim().is_empty() {
            return Err(DomainError::ValidationError(
                "task_definition_key is required".to_string(),
            ));
        }
        if self.form_code.trim().is_empty() {
            return Err(DomainError::ValidationError("form_code is required".to_string()));
        }
        if self.form_version <= 0 {
            return Err(DomainError::ValidationError(
                "form_version must be greater than 0".to_string(),
            ));
        }
        if !self.data_json.is_object() {
            return Err(DomainError::ValidationError(
                "data_json must be a JSON object".to_string(),
            ));
        }
        if !self.normalized_summary_json.is_object() {
            return Err(DomainError::ValidationError(
                "normalized_summary_json must be a JSON object".to_string(),
            ));
        }
        if self.submitted_by.trim().is_empty() {
            return Err(DomainError::ValidationError("submitted_by is required".to_string()));
        }
        Ok(())
    }
}

fn normalized_eq(left: Option<&str>, right: Option<&str>) -> bool {
    match (
        left.map(str::trim).filter(|value| !value.is_empty()),
        right.map(str::trim).filter(|value| !value.is_empty()),
    ) {
        (Some(left), Some(right)) => left.eq_ignore_ascii_case(right),
        _ => false,
    }
}

fn department_matches(target: Option<&str>, actual: Option<&str>) -> bool {
    normalized_eq(target, actual)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binding_requires_department_or_role_scope() {
        let binding = WorkflowFormBinding {
            id: "01TEST".to_string(),
            template_code: "delay_case".to_string(),
            process_definition_key: "delay_case".to_string(),
            task_definition_key: "ground_confirm".to_string(),
            form_code: "ground_confirm_form".to_string(),
            form_version: Some(1),
            target_department_id: None,
            target_department_name: None,
            target_roles: vec![],
            assignment_mode: WorkflowFormAssignmentMode::DepartmentRoles,
            write_back_mode: WorkflowFormWriteBackMode::BusinessCaseContext,
            write_back_key: "forms.ground_confirm".to_string(),
            flowable_variable_prefix: None,
            complete_task_on_submit: true,
            allow_resubmit: false,
            source: WorkflowFormBindingSource::Db,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        assert!(matches!(binding.validate(), Err(DomainError::ValidationError(_))));
    }
}
