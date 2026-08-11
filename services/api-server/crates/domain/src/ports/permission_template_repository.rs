//! 权限模板仓储接口

use crate::error::DomainError;
use crate::models::permission_template::PermissionTemplate;
use async_trait::async_trait;

#[async_trait]
pub trait PermissionTemplateRepository {
    async fn find_all(
        &self,
        category: Option<&str>,
        include_inactive: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<PermissionTemplate>, DomainError>;

    async fn find_by_id(&self, id: &str) -> Result<Option<PermissionTemplate>, DomainError>;

    async fn exists_by_name(&self, name: &str) -> Result<bool, DomainError>;

    async fn exists_by_code(&self, code: &str) -> Result<bool, DomainError>;

    async fn save(&self, template: &PermissionTemplate) -> Result<PermissionTemplate, DomainError>;

    async fn delete(&self, id: &str) -> Result<bool, DomainError>;
}
