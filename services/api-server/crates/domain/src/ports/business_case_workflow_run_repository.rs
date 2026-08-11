use crate::error::DomainError;
use crate::models::business_case_workflow::BusinessCaseWorkflowRun;
use async_trait::async_trait;

#[async_trait]
pub trait BusinessCaseWorkflowRunRepository {
    async fn save(&self, run: &BusinessCaseWorkflowRun) -> Result<BusinessCaseWorkflowRun, DomainError>;
    async fn find_by_run_id(&self, run_id: &str) -> Result<Option<BusinessCaseWorkflowRun>, DomainError>;
    async fn find_by_case_id(&self, case_id: &str) -> Result<Option<BusinessCaseWorkflowRun>, DomainError>;
    async fn find_by_receipt_group_id(
        &self,
        receipt_group_id: &str,
    ) -> Result<Option<BusinessCaseWorkflowRun>, DomainError>;
    async fn list_by_receipt_group_id(
        &self,
        receipt_group_id: &str,
    ) -> Result<Vec<BusinessCaseWorkflowRun>, DomainError>;
}
