//! 用户仓储 trait

use crate::error::DomainError;
use crate::models::user::{Permission, Role, User};
use async_trait::async_trait;

/// 用户仓储接口
#[async_trait]
pub trait UserRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<User>, DomainError>;
    async fn find_permission_version_by_id(&self, id: &str) -> Result<Option<i32>, DomainError>;
    async fn find_by_username(&self, username: &str) -> Result<Option<User>, DomainError>;
    async fn find_by_email(&self, email: &str) -> Result<Option<User>, DomainError>;
    async fn find_all(&self, limit: i64, offset: i64) -> Result<Vec<User>, DomainError>;
    /// 查找当前由 `personal_user_id` 占用的岗位账号（占用校验：停个人时若被某岗占用 → 409）。
    /// 默认实现返回 `None`（视为未占用）仅用于测试替身；真实仓储必须覆写为按
    /// `account_type='position' AND current_occupant_user_id=$1` 查询。
    async fn find_position_occupied_by(&self, _personal_user_id: &str) -> Result<Option<User>, DomainError> {
        Ok(None)
    }
    async fn list_distinct_departments_in_use(&self) -> Result<Vec<String>, DomainError>;
    async fn has_any_user_with_department_id(&self, department_id: &str) -> Result<bool, DomainError>;
    async fn save(&self, user: &User) -> Result<(), DomainError>;
    async fn update(&self, user: &User) -> Result<bool, DomainError>;
    async fn delete(&self, id: &str) -> Result<bool, DomainError>;
    async fn update_password(&self, id: &str, password_hash: &str) -> Result<bool, DomainError>;
    async fn update_last_login(&self, id: &str) -> Result<bool, DomainError>;
}

/// 角色仓储接口
#[async_trait]
pub trait RoleRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Role>, DomainError>;
    async fn find_by_name(&self, name: &str) -> Result<Option<Role>, DomainError>;
    async fn find_all(&self) -> Result<Vec<Role>, DomainError>;
    async fn save(&self, role: &Role) -> Result<(), DomainError>;
    async fn delete(&self, id: &str) -> Result<bool, DomainError>;
    async fn find_by_user_id(&self, user_id: &str) -> Result<Vec<Role>, DomainError>;
    async fn count_users(&self, role_id: &str) -> Result<i64, DomainError>;
    async fn set_permissions(&self, role_id: &str, permission_names: &[String]) -> Result<(), DomainError>;
    async fn assign_role_to_user(&self, user_id: &str, role_id: &str) -> Result<(), DomainError>;
    async fn remove_user_from_role(&self, user_id: &str, role_id: &str) -> Result<(), DomainError>;
    async fn add_permission(&self, role_id: &str, permission_name: &str) -> Result<bool, DomainError>;
    async fn remove_permission(&self, role_id: &str, permission_name: &str) -> Result<bool, DomainError>;
}

/// 权限仓储接口
#[async_trait]
pub trait PermissionRepository {
    async fn find_all(&self) -> Result<Vec<Permission>, DomainError>;
    async fn find_by_role_id(&self, role_id: &str) -> Result<Vec<Permission>, DomainError>;
}
