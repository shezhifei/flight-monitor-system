//! PostgreSQL 角色仓储实现

use async_trait::async_trait;
use std::collections::HashMap;

use chrono::Utc;
use sqlx::{PgPool, Row};

use fms_domain::error::DomainError;
use fms_domain::models::user::Role;
use fms_domain::ports::user_repository::RoleRepository;

pub struct PgRoleRepository {
    pool: PgPool,
}

impl PgRoleRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn load_permissions(&self, role_id: &str) -> Result<Vec<String>, DomainError> {
        let rows = sqlx::query(
            r#"SELECT p.name FROM permissions p
               JOIN role_permissions rp ON p.id = rp.permission_id
               WHERE rp.role_id = $1"#,
        )
        .bind(role_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.iter().map(|r| r.get("name")).collect())
    }

    async fn load_permissions_batch(&self, role_ids: &[String]) -> Result<HashMap<String, Vec<String>>, DomainError> {
        if role_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query(
            r#"SELECT rp.role_id, p.name
               FROM permissions p
               JOIN role_permissions rp ON p.id = rp.permission_id
               WHERE rp.role_id = ANY($1)"#,
        )
        .bind(role_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let mut result: HashMap<String, Vec<String>> = HashMap::new();
        for r in rows {
            let role_id: String = r.get("role_id");
            let perm_name: String = r.get("name");
            result.entry(role_id).or_default().push(perm_name);
        }
        Ok(result)
    }

    fn row_to_role(r: &sqlx::postgres::PgRow, permissions: Vec<String>) -> Role {
        Role {
            id: r.get("id"),
            name: r.get("name"),
            description: r.get("description"),
            permissions,
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
            is_active: r.get("is_active"),
            is_system: r.get("is_system"),
        }
    }

    /// 统计角色下的用户数
    pub async fn count_users(&self, role_id: &str) -> Result<i64, DomainError> {
        let row = sqlx::query("SELECT COUNT(*) as cnt FROM user_roles WHERE role_id = $1")
            .bind(role_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.get::<i64, _>("cnt"))
    }

    /// 设置角色权限（全量替换）
    pub async fn set_permissions(&self, role_id: &str, permission_names: &[String]) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        sqlx::query("DELETE FROM role_permissions WHERE role_id = $1")
            .bind(role_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        if !permission_names.is_empty() {
            sqlx::query(
                r#"INSERT INTO role_permissions (role_id, permission_id)
                   SELECT $1, p.id FROM permissions p
                   WHERE p.name = ANY($2)
                   ON CONFLICT DO NOTHING"#,
            )
            .bind(role_id)
            .bind(permission_names)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        }

        tx.commit().await.map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    /// 为用户分配角色
    pub async fn assign_role_to_user(&self, user_id: &str, role_id: &str) -> Result<(), DomainError> {
        sqlx::query("INSERT INTO user_roles (user_id, role_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(user_id)
            .bind(role_id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    pub async fn remove_user_from_role(&self, user_id: &str, role_id: &str) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM user_roles WHERE user_id = $1 AND role_id = $2")
            .bind(user_id)
            .bind(role_id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    /// 为角色添加单个权限
    pub async fn add_permission(&self, role_id: &str, permission_name: &str) -> Result<bool, DomainError> {
        let result = sqlx::query(
            r#"INSERT INTO role_permissions (role_id, permission_id)
               SELECT $1, id FROM permissions WHERE name = $2
               ON CONFLICT DO NOTHING"#,
        )
        .bind(role_id)
        .bind(permission_name)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    /// 移除角色的单个权限
    pub async fn remove_permission(&self, role_id: &str, permission_name: &str) -> Result<bool, DomainError> {
        let result = sqlx::query(
            r#"DELETE FROM role_permissions WHERE role_id = $1
               AND permission_id = (SELECT id FROM permissions WHERE name = $2)"#,
        )
        .bind(role_id)
        .bind(permission_name)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    /// 更新角色
    pub async fn update(&self, role: &Role) -> Result<bool, DomainError> {
        let result = sqlx::query(
            r#"UPDATE roles SET name = $1, description = $2, is_active = $3, updated_at = $4
               WHERE id = $5"#,
        )
        .bind(&role.name)
        .bind(&role.description)
        .bind(role.is_active)
        .bind(Utc::now())
        .bind(&role.id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }
}

#[async_trait]
impl RoleRepository for PgRoleRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Role>, DomainError> {
        let row = sqlx::query("SELECT * FROM roles WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        match row {
            Some(r) => {
                let perms = self.load_permissions(id).await?;
                Ok(Some(Self::row_to_role(&r, perms)))
            }
            None => Ok(None),
        }
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<Role>, DomainError> {
        let row = sqlx::query("SELECT * FROM roles WHERE name = $1")
            .bind(name)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        match row {
            Some(r) => {
                let role_id: String = r.get("id");
                let perms = self.load_permissions(&role_id).await?;
                Ok(Some(Self::row_to_role(&r, perms)))
            }
            None => Ok(None),
        }
    }

    async fn find_all(&self) -> Result<Vec<Role>, DomainError> {
        let rows = sqlx::query("SELECT * FROM roles ORDER BY name")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let role_ids: Vec<String> = rows.iter().map(|r| r.get("id")).collect();
        let perm_map = self.load_permissions_batch(&role_ids).await?;

        let mut roles = Vec::with_capacity(rows.len());
        for r in &rows {
            let role_id: String = r.get("id");
            let perms = perm_map.get(&role_id).cloned().unwrap_or_default();
            roles.push(Self::row_to_role(r, perms));
        }
        Ok(roles)
    }

    async fn save(&self, role: &Role) -> Result<(), DomainError> {
        sqlx::query(
            r#"INSERT INTO roles (id, name, description, is_system, is_active, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7)
               ON CONFLICT (id) DO UPDATE SET
                   name = EXCLUDED.name,
                   description = EXCLUDED.description,
                   is_active = EXCLUDED.is_active,
                   updated_at = CURRENT_TIMESTAMP"#,
        )
        .bind(&role.id)
        .bind(&role.name)
        .bind(&role.description)
        .bind(role.is_system)
        .bind(role.is_active)
        .bind(role.created_at)
        .bind(role.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<bool, DomainError> {
        let result = sqlx::query("DELETE FROM roles WHERE id = $1 AND is_system = FALSE")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn find_by_user_id(&self, user_id: &str) -> Result<Vec<Role>, DomainError> {
        let rows = sqlx::query(
            r#"SELECT r.* FROM roles r
               JOIN user_roles ur ON r.id = ur.role_id
               WHERE ur.user_id = $1 AND r.is_active = TRUE"#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let role_ids: Vec<String> = rows.iter().map(|r| r.get("id")).collect();
        let perm_map = self.load_permissions_batch(&role_ids).await?;

        let mut roles = Vec::with_capacity(rows.len());
        for r in &rows {
            let role_id: String = r.get("id");
            let perms = perm_map.get(&role_id).cloned().unwrap_or_default();
            roles.push(Self::row_to_role(r, perms));
        }
        Ok(roles)
    }

    async fn count_users(&self, role_id: &str) -> Result<i64, DomainError> {
        Self::count_users(self, role_id).await
    }

    async fn set_permissions(&self, role_id: &str, permission_names: &[String]) -> Result<(), DomainError> {
        Self::set_permissions(self, role_id, permission_names).await
    }

    async fn assign_role_to_user(&self, user_id: &str, role_id: &str) -> Result<(), DomainError> {
        Self::assign_role_to_user(self, user_id, role_id).await
    }

    async fn remove_user_from_role(&self, user_id: &str, role_id: &str) -> Result<(), DomainError> {
        Self::remove_user_from_role(self, user_id, role_id).await
    }

    async fn add_permission(&self, role_id: &str, permission_name: &str) -> Result<bool, DomainError> {
        Self::add_permission(self, role_id, permission_name).await
    }

    async fn remove_permission(&self, role_id: &str, permission_name: &str) -> Result<bool, DomainError> {
        Self::remove_permission(self, role_id, permission_name).await
    }
}
