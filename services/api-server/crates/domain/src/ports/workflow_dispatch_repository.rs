use async_trait::async_trait;
use std::collections::HashMap;

use crate::error::DomainError;

#[async_trait]
pub trait WorkflowDispatchRepository {
    async fn replace_assignment_members(
        &self,
        dispatch_order_id: &str,
        assigned_user_ids: &[String],
    ) -> Result<(), DomainError>;

    async fn get_active_workload_by_users(&self, user_ids: &[String]) -> Result<HashMap<String, i64>, DomainError>;
}
