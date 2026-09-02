//! PostgreSQL 部门仓储实现。

use async_trait::async_trait;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::Department;
use fms_domain::ports::dispatch_repository::{DepartmentRepository, DepartmentTransactionalRepository};

use super::soft_delete_audit::record_soft_delete;

pub struct PgDepartmentRepository {
    pool: PgPool,
}

impl PgDepartmentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DepartmentRepository for PgDepartmentRepository {
    async fn save(&self, dept: &Department) -> Result<Department, DomainError> {
        sqlx::query(
            r#"
            INSERT INTO departments (
                id, name, code, description, manager_id, terminal, is_active, attributes
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                code = EXCLUDED.code,
                description = EXCLUDED.description,
                manager_id = EXCLUDED.manager_id,
                terminal = EXCLUDED.terminal,
                is_active = EXCLUDED.is_active,
                attributes = EXCLUDED.attributes,
                deleted_at = NULL,
                updated_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(&dept.id)
        .bind(&dept.name)
        .bind(&dept.code)
        .bind(&dept.description)
        .bind(&dept.manager_id)
        .bind(&dept.terminal)
        .bind(dept.is_active)
        .bind(&dept.attributes)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        self.find_by_id(&dept.id)
            .await?
            .ok_or_else(|| DomainError::Internal("department save returned no row".into()))
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<Department>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT id, name, code, description, manager_id, terminal, created_at, updated_at, is_active, attributes
            FROM departments
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(row.map(|item| row_to_department(&item)))
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<Department>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT id, name, code, description, manager_id, terminal, created_at, updated_at, is_active, attributes
            FROM departments
            WHERE name = $1 AND deleted_at IS NULL
            LIMIT 1
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(row.map(|item| row_to_department(&item)))
    }

    async fn find_all(&self, include_inactive: bool, limit: i64, offset: i64) -> Result<Vec<Department>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(
            "SELECT id, name, code, description, manager_id, terminal, created_at, updated_at, is_active, attributes FROM departments WHERE deleted_at IS NULL",
        );
        if !include_inactive {
            builder.push(" AND is_active = TRUE");
        }
        builder
            .push(" ORDER BY name LIMIT ")
            .push_bind(limit.max(1))
            .push(" OFFSET ")
            .push_bind(offset.max(0));

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(rows.iter().map(row_to_department).collect())
    }

    async fn has_dependencies(&self, department_id: &str) -> Result<bool, DomainError> {
        let checks = [
            ("team_types", "department_id"),
            ("department_qualification_catalog", "department_id"),
            ("department_qualification_levels", "department_id"),
            ("qualification_grants", "department_id"),
            ("department_task_type_requirement_versions", "department_id"),
            ("task_types", "default_department_id"),
            ("department_flight_generation_rules", "department_id"),
            ("department_generation_adjustment_rules", "department_id"),
            ("dispatch_temporary_task_templates", "department_id"),
            ("dispatch_orders", "department_id"),
        ];

        for (table_name, column_name) in checks {
            let sql = format!("SELECT 1 FROM {table_name} WHERE {column_name} = $1 LIMIT 1");
            let row = sqlx::query(&sql)
                .bind(department_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|err| DomainError::Internal(err.to_string()))?;
            if row.is_some() {
                return Ok(true);
            }
        }

        Ok(false)
    }

    async fn delete_permanently(&self, department_id: &str) -> Result<bool, DomainError> {
        // 审计要求软删除：仅标记 deleted_at，行保留
        let result = sqlx::query(
            "UPDATE departments SET deleted_at = NOW(), updated_at = NOW() \
             WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(department_id)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        let deleted = result.rows_affected() > 0;
        if deleted {
            record_soft_delete(&self.pool, "department", department_id, "soft_delete").await;
        }
        Ok(deleted)
    }
}

#[async_trait]
impl<'tx> DepartmentTransactionalRepository<sqlx::Transaction<'tx, sqlx::Postgres>>
    for PgDepartmentRepository
{
    async fn save_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'tx, sqlx::Postgres>,
        dept: &Department,
    ) -> Result<Department, DomainError> {
        sqlx::query(
            r#"
            INSERT INTO departments (
                id, name, code, description, manager_id, terminal, is_active, attributes
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                code = EXCLUDED.code,
                description = EXCLUDED.description,
                manager_id = EXCLUDED.manager_id,
                terminal = EXCLUDED.terminal,
                is_active = EXCLUDED.is_active,
                attributes = EXCLUDED.attributes,
                deleted_at = NULL,
                updated_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(&dept.id)
        .bind(&dept.name)
        .bind(&dept.code)
        .bind(&dept.description)
        .bind(&dept.manager_id)
        .bind(&dept.terminal)
        .bind(dept.is_active)
        .bind(&dept.attributes)
        .execute(&mut **tx)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        let row = sqlx::query(
            r#"
            SELECT id, name, code, description, manager_id, terminal,
                   created_at, updated_at, is_active, attributes
            FROM departments
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(&dept.id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?
        .ok_or_else(|| DomainError::Internal("department transactional save returned no row".into()))?;
        Ok(row_to_department(&row))
    }
}

fn row_to_department(row: &sqlx::postgres::PgRow) -> Department {
    Department {
        id: row.get("id"),
        name: row.get("name"),
        code: row.get("code"),
        description: row.get("description"),
        manager_id: row.get("manager_id"),
        terminal: row.get("terminal"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        is_active: row.get::<Option<bool>, _>("is_active").unwrap_or(true),
        attributes: row.try_get("attributes").unwrap_or_else(|_| serde_json::json!({})),
    }
}
