use crate::error::DomainError;
use crate::models::workflow_form::{WorkflowFormBinding, WorkflowFormSubmission, WorkflowFormTemplate};
use async_trait::async_trait;

#[async_trait]
pub trait WorkflowFormRepository {
    async fn save_template(&self, template: &WorkflowFormTemplate) -> Result<WorkflowFormTemplate, DomainError>;

    async fn find_template_by_code_version(
        &self,
        form_code: &str,
        version: i32,
    ) -> Result<Option<WorkflowFormTemplate>, DomainError>;

    async fn find_active_template_by_code(&self, form_code: &str) -> Result<Option<WorkflowFormTemplate>, DomainError>;

    async fn save_binding(&self, binding: &WorkflowFormBinding) -> Result<WorkflowFormBinding, DomainError>;

    async fn find_bindings_by_process_task(
        &self,
        process_definition_key: &str,
        task_definition_key: &str,
    ) -> Result<Vec<WorkflowFormBinding>, DomainError>;

    async fn find_bindings_by_template_code(
        &self,
        template_code: &str,
    ) -> Result<Vec<WorkflowFormBinding>, DomainError>;

    async fn insert_submission(
        &self,
        submission: &WorkflowFormSubmission,
    ) -> Result<WorkflowFormSubmission, DomainError>;

    async fn find_submissions_by_case(&self, case_id: &str) -> Result<Vec<WorkflowFormSubmission>, DomainError>;

    async fn find_latest_submission(
        &self,
        case_id: &str,
        task_definition_key: &str,
        form_code: &str,
    ) -> Result<Option<WorkflowFormSubmission>, DomainError>;
}
