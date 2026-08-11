//! 操作员身份上下文仓储接口

use crate::error::DomainError;
use crate::models::operator_identity::OperatorIdentityContext;
use async_trait::async_trait;

#[async_trait]
pub trait OperatorIdentityRepository {
    async fn find_by_scope(
        &self,
        user_id: &str,
        context_type: &str,
        context_id: &str,
    ) -> Result<Option<OperatorIdentityContext>, DomainError>;

    async fn upsert(&self, context: &OperatorIdentityContext) -> Result<(), DomainError>;

    async fn delete(&self, user_id: &str, context_type: &str, context_id: &str) -> Result<bool, DomainError>;
}
