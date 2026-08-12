//! PostgreSQL 用户仓储实现
//!
//! 实现 `fms_domain::ports::user_repository::UserRepository` trait。

use async_trait::async_trait;
use std::collections::HashMap;

use chrono::Utc;
use sqlx::{PgPool, Row};

use fms_domain::error::DomainError;
use fms_domain::models::user::{Role, User};
use fms_domain::ports::user_repository::UserRepository;

use super::soft_delete_audit::record_soft_delete;

pub struct PgUserRepository {
    pool: PgPool,
}

impl PgUserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserRepository for PgUserRepository {
    async fn find_by_id(&self, user_id: &str) -> Result<Option<User>, DomainError> {
        let row = sqlx::query(
            "SELECT id, email, password_hash, username, display_name, is_active, \
             is_verified, is_admin, verification_token, verification_token_expires, \
             verified_at, password_reset_token, password_reset_token_expires, \
             password_changed_at, last_login_at, department, department_id, \
             job_level, job_title, permission_version, created_at, updated_at \
             FROM users WHERE id = $1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        match row {
            Some(r) => {
                let roles = self.load_roles(user_id).await?;
                Ok(Some(row_to_user(&r, roles)))
            }
            None => Ok(None),
        }
    }

    async fn find_permission_version_by_id(&self, user_id: &str) -> Result<Option<i32>, DomainError> {
        let row = sqlx::query("SELECT permission_version FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(|r| r.get("permission_version")))
    }

    async fn find_by_username(&self, username: &str) -> Result<Option<User>, DomainError> {
        let row = sqlx::query(
            "SELECT id, email, password_hash, username, display_name, is_active, \
             is_verified, is_admin, verification_token, verification_token_expires, \
             verified_at, password_reset_token, password_reset_token_expires, \
             password_changed_at, last_login_at, department, department_id, \
             job_level, job_title, permission_version, created_at, updated_at \
             FROM users WHERE username = $1",
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        match row {
            Some(r) => {
                let uid: String = r.get("id");
                let roles = self.load_roles(&uid).await?;
                Ok(Some(row_to_user(&r, roles)))
            }
            None => Ok(None),
        }
    }

    async fn find_by_email(&self, email: &str) -> Result<Option<User>, DomainError> {
        let row = sqlx::query(
            "SELECT id, email, password_hash, username, display_name, is_active, \
             is_verified, is_admin, verification_token, verification_token_expires, \
             verified_at, password_reset_token, password_reset_token_expires, \
             password_changed_at, last_login_at, department, department_id, \
             job_level, job_title, permission_version, created_at, updated_at \
             FROM users WHERE email = $1",
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        match row {
            Some(r) => {
                let uid: String = r.get("id");
                let roles = self.load_roles(&uid).await?;
                Ok(Some(row_to_user(&r, roles)))
            }
            None => Ok(None),
        }
    }

    async fn find_all(&self, limit: i64, offset: i64) -> Result<Vec<User>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, email, password_hash, username, display_name, is_active, \
             is_verified, is_admin, verification_token, verification_token_expires, \
             verified_at, password_reset_token, password_reset_token_expires, \
             password_changed_at, last_login_at, department, department_id, \
             job_level, job_title, permission_version, created_at, updated_at \
             FROM users WHERE is_active = TRUE ORDER BY created_at DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        if rows.is_empty() {
            return Ok(Vec::new());
        }

        let user_ids: Vec<String> = rows.iter().map(|r| r.get::<String, _>("id")).collect();
        let roles_map = self.load_roles_batch(&user_ids).await?;

        let mut users = Vec::with_capacity(rows.len());
        for r in &rows {
            let uid: String = r.get("id");
            let roles = roles_map.get(&uid).cloned().unwrap_or_default();
            users.push(row_to_user(r, roles));
        }
        Ok(users)
    }

    async fn list_distinct_departments_in_use(&self) -> Result<Vec<String>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT NULLIF(BTRIM(department), '') AS department
            FROM users
            WHERE NULLIF(BTRIM(department), '') IS NOT NULL
            ORDER BY department
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows
            .into_iter()
            .filter_map(|row| row.try_get::<Option<String>, _>("department").ok().flatten())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect())
    }

    async fn has_any_user_with_department_id(&self, department_id: &str) -> Result<bool, DomainError> {
        let row = sqlx::query("SELECT 1 FROM users WHERE department_id = $1 LIMIT 1")
            .bind(department_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.is_some())
    }

    async fn save(&self, user: &User) -> Result<(), DomainError> {
        sqlx::query(
            r#"INSERT INTO users (
                id, username, display_name, email, password_hash,
                is_active, is_verified, is_admin,
                verification_token, verification_token_expires, verified_at,
                password_reset_token, password_reset_token_expires, password_changed_at,
                last_login_at, department, department_id, job_level, job_title,
                permission_version, created_at, updated_at
            ) VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8,
                $9, $10, $11,
                $12, $13, $14,
                $15, $16, $17, $18, $19,
                $20, $21, $22
            )
            ON CONFLICT (id) DO UPDATE SET
                username = EXCLUDED.username,
                display_name = EXCLUDED.display_name,
                email = EXCLUDED.email,
                password_hash = EXCLUDED.password_hash,
                is_active = EXCLUDED.is_active,
                is_verified = EXCLUDED.is_verified,
                is_admin = EXCLUDED.is_admin,
                verification_token = EXCLUDED.verification_token,
                verification_token_expires = EXCLUDED.verification_token_expires,
                verified_at = EXCLUDED.verified_at,
                password_reset_token = EXCLUDED.password_reset_token,
                password_reset_token_expires = EXCLUDED.password_reset_token_expires,
                password_changed_at = EXCLUDED.password_changed_at,
                last_login_at = EXCLUDED.last_login_at,
                department = EXCLUDED.department,
                department_id = EXCLUDED.department_id,
                job_level = EXCLUDED.job_level,
                job_title = EXCLUDED.job_title,
                permission_version = EXCLUDED.permission_version,
                updated_at = CURRENT_TIMESTAMP"#,
        )
        .bind(&user.id)
        .bind(&user.username)
        .bind(&user.display_name)
        .bind(&user.email)
        .bind(&user.password_hash)
        .bind(user.is_active)
        .bind(user.is_verified)
        .bind(user.is_admin)
        .bind(&user.verification_token)
        .bind(user.verification_token_expires)
        .bind(user.verified_at)
        .bind(&user.password_reset_token)
        .bind(user.password_reset_token_expires)
        .bind(user.password_changed_at)
        .bind(user.last_login_at)
        .bind(&user.department)
        .bind(&user.department_id)
        .bind(user.job_level)
        .bind(&user.job_title)
        .bind(user.permission_version)
        .bind(user.created_at)
        .bind(user.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn update(&self, user: &User) -> Result<bool, DomainError> {
        // update 等同于 save (UPSERT)
        self.save(user).await?;
        Ok(true)
    }

    async fn delete(&self, user_id: &str) -> Result<bool, DomainError> {
        // 审计要求软删除：停用用户而非物理删除，行与关联数据全部保留
        let result = sqlx::query(
            "UPDATE users SET is_active = FALSE, updated_at = NOW() WHERE id = $1 AND is_active = TRUE",
        )
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        let deleted = result.rows_affected() > 0;
        if deleted {
            record_soft_delete(&self.pool, "user", user_id, "deactivate").await;
        }
        Ok(deleted)
    }

    async fn update_password(&self, user_id: &str, password_hash: &str) -> Result<bool, DomainError> {
        let result =
            sqlx::query("UPDATE users SET password_hash = $1, password_changed_at = $2, updated_at = $2 WHERE id = $3")
                .bind(password_hash)
                .bind(Utc::now())
                .bind(user_id)
                .execute(&self.pool)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn update_last_login(&self, user_id: &str) -> Result<bool, DomainError> {
        let result = sqlx::query("UPDATE users SET last_login_at = $1 WHERE id = $2")
            .bind(Utc::now())
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }
}

impl PgUserRepository {
    async fn load_roles(&self, user_id: &str) -> Result<Vec<Role>, DomainError> {
        let rows = sqlx::query(
            r#"SELECT r.id, r.name, r.description, r.is_active, r.is_system,
                      r.created_at, r.updated_at
               FROM roles r
               JOIN user_roles ur ON r.id = ur.role_id
               WHERE ur.user_id = $1 AND ur.deleted_at IS NULL AND r.deleted_at IS NULL AND r.is_active = TRUE"#,
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
            let permissions = perm_map.get(&role_id).cloned().unwrap_or_default();
            roles.push(Role {
                id: role_id,
                name: r.get("name"),
                description: r.get("description"),
                permissions,
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
                is_active: r.get("is_active"),
                is_system: r.get("is_system"),
            });
        }
        Ok(roles)
    }

    async fn load_roles_batch(&self, user_ids: &[String]) -> Result<HashMap<String, Vec<Role>>, DomainError> {
        if user_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query(
            r#"SELECT ur.user_id, r.id, r.name, r.description, r.is_active, r.is_system,
                      r.created_at, r.updated_at
               FROM roles r
               JOIN user_roles ur ON r.id = ur.role_id
               WHERE ur.user_id = ANY($1) AND ur.deleted_at IS NULL AND r.deleted_at IS NULL AND r.is_active = TRUE"#,
        )
        .bind(user_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let role_ids: Vec<String> = rows.iter().map(|r| r.get("id")).collect();
        let perm_map = self.load_permissions_batch(&role_ids).await?;

        let mut result: HashMap<String, Vec<Role>> = HashMap::new();
        for r in &rows {
            let user_id: String = r.get("user_id");
            let role_id: String = r.get("id");
            let permissions = perm_map.get(&role_id).cloned().unwrap_or_default();
            result.entry(user_id).or_default().push(Role {
                id: role_id,
                name: r.get("name"),
                description: r.get("description"),
                permissions,
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
                is_active: r.get("is_active"),
                is_system: r.get("is_system"),
            });
        }
        Ok(result)
    }

    async fn load_permissions_batch(&self, role_ids: &[String]) -> Result<HashMap<String, Vec<String>>, DomainError> {
        if role_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query(
            r#"SELECT rp.role_id, p.name
               FROM permissions p
               JOIN role_permissions rp ON p.id = rp.permission_id
               WHERE rp.role_id = ANY($1) AND rp.deleted_at IS NULL AND p.deleted_at IS NULL"#,
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
}

fn row_to_user(r: &sqlx::postgres::PgRow, roles: Vec<Role>) -> User {
    User {
        id: r.get("id"),
        email: r.get("email"),
        password_hash: r.get("password_hash"),
        username: r.get("username"),
        display_name: r.get("display_name"),
        roles,
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
        last_login_at: r.get("last_login_at"),
        is_active: r.get("is_active"),
        is_verified: r.get("is_verified"),
        is_admin: r.get("is_admin"),
        verification_token: r.get("verification_token"),
        verification_token_expires: r.get("verification_token_expires"),
        verified_at: r.get("verified_at"),
        password_reset_token: r.get("password_reset_token"),
        password_reset_token_expires: r.get("password_reset_token_expires"),
        password_changed_at: r.get("password_changed_at"),
        department: r.get("department"),
        department_id: r.get("department_id"),
        job_level: r.get("job_level"),
        job_title: r.get("job_title"),
        permission_version: r.get("permission_version"),
    }
}
