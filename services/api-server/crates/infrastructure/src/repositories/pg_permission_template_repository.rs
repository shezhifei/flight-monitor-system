//! PostgreSQL 权限模板仓储实现

use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Row};

use fms_domain::error::DomainError;
use fms_domain::models::permission_template::PermissionTemplate;
use fms_domain::ports::permission_template_repository::PermissionTemplateRepository;

use super::soft_delete_audit::record_soft_delete;

pub struct PgPermissionTemplateRepository {
    pool: PgPool,
}

impl PgPermissionTemplateRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn row_to_template(row: &sqlx::postgres::PgRow) -> PermissionTemplate {
        PermissionTemplate {
            id: row.get("id"),
            name: row.get("name"),
            code: row.get("code"),
            description: row.get("description"),
            permissions: row.get::<Vec<String>, _>("permissions"),
            is_system: row.get("is_system"),
            category: row.get("category"),
            display_order: row.get("display_order"),
            is_active: row.get("is_active"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }
    }
}

#[async_trait]
impl PermissionTemplateRepository for PgPermissionTemplateRepository {
    async fn find_all(
        &self,
        category: Option<&str>,
        include_inactive: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<PermissionTemplate>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT *
            FROM permission_templates
            WHERE ($1::text IS NULL OR category = $1)
              AND ($2::bool OR is_active = TRUE)
              AND deleted_at IS NULL
            ORDER BY category NULLS LAST, display_order ASC, name ASC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(category)
        .bind(include_inactive)
        .bind(limit.max(1))
        .bind(offset.max(0))
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        Ok(rows.iter().map(Self::row_to_template).collect())
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<PermissionTemplate>, DomainError> {
        let row = sqlx::query("SELECT * FROM permission_templates WHERE id = $1 AND deleted_at IS NULL")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(row.as_ref().map(Self::row_to_template))
    }

    async fn exists_by_name(&self, name: &str) -> Result<bool, DomainError> {
        let row = sqlx::query("SELECT 1 FROM permission_templates WHERE name = $1 AND deleted_at IS NULL")
            .bind(name)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(row.is_some())
    }

    async fn exists_by_code(&self, code: &str) -> Result<bool, DomainError> {
        let row = sqlx::query("SELECT 1 FROM permission_templates WHERE code = $1 AND deleted_at IS NULL")
            .bind(code)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(row.is_some())
    }

    async fn save(&self, template: &PermissionTemplate) -> Result<PermissionTemplate, DomainError> {
        let now = Utc::now();
        let row = sqlx::query(
            r#"
            INSERT INTO permission_templates (
                id, name, code, description, permissions, is_system,
                category, display_order, is_active, created_at, updated_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6,
                $7, $8, $9, $10, $11
            )
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                code = EXCLUDED.code,
                description = EXCLUDED.description,
                permissions = EXCLUDED.permissions,
                category = EXCLUDED.category,
                display_order = EXCLUDED.display_order,
                is_active = EXCLUDED.is_active,
                deleted_at = NULL,
                updated_at = EXCLUDED.updated_at
            RETURNING *
            "#,
        )
        .bind(&template.id)
        .bind(&template.name)
        .bind(&template.code)
        .bind(&template.description)
        .bind(&template.permissions)
        .bind(template.is_system)
        .bind(&template.category)
        .bind(template.display_order)
        .bind(template.is_active)
        .bind(template.created_at)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        Ok(Self::row_to_template(&row))
    }

    async fn delete(&self, id: &str) -> Result<bool, DomainError> {
        // 审计要求软删除：仅标记 deleted_at，行保留
        let result = sqlx::query(
            "UPDATE permission_templates SET deleted_at = NOW(), updated_at = NOW() \
             WHERE id = $1 AND is_system = FALSE AND deleted_at IS NULL",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        let deleted = result.rows_affected() > 0;
        if deleted {
            record_soft_delete(&self.pool, "permission_template", id, "soft_delete").await;
        }
        Ok(deleted)
    }
}
