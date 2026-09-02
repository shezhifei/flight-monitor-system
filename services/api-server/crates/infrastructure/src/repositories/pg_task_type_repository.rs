//! PostgreSQL 作业类型仓储实现。

use async_trait::async_trait;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::TaskType;
use fms_domain::ports::dispatch_repository::{TaskTypeRepository, TaskTypeTransactionalRepository};

pub struct PgTaskTypeRepository {
    pool: PgPool,
}

impl PgTaskTypeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TaskTypeRepository for PgTaskTypeRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<TaskType>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT id, code, name, default_department_id, category, sequence_order, default_duration_minutes,
                   trigger_offset_minutes, trigger_type, description, anchor, is_active, attributes, created_at
            FROM task_types
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(row.map(row_to_task_type))
    }

    async fn find_by_code(&self, code: &str) -> Result<Option<TaskType>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT id, code, name, default_department_id, category, sequence_order, default_duration_minutes,
                   trigger_offset_minutes, trigger_type, description, anchor, is_active, attributes, created_at
            FROM task_types
            WHERE code = $1
            "#,
        )
        .bind(code)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(row.map(row_to_task_type))
    }

    async fn find_all(&self, category: Option<&str>, limit: i64, offset: i64) -> Result<Vec<TaskType>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(
            r#"
            SELECT id, code, name, default_department_id, category, sequence_order, default_duration_minutes,
                   trigger_offset_minutes, trigger_type, description, is_active, attributes, created_at
            FROM task_types
            WHERE is_active = TRUE
            "#,
        );
        if let Some(value) = category {
            builder.push(" AND category = ").push_bind(value);
        }
        builder
            .push(" ORDER BY sequence_order NULLS LAST, code LIMIT ")
            .push_bind(limit.max(1))
            .push(" OFFSET ")
            .push_bind(offset.max(0));

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(rows.into_iter().map(row_to_task_type).collect())
    }

    async fn save(&self, task_type: &TaskType) -> Result<TaskType, DomainError> {
        let row = sqlx::query(
            r#"
            INSERT INTO task_types (id, code, name, default_department_id, category, sequence_order,
                default_duration_minutes, trigger_offset_minutes, trigger_type, description, anchor, is_active, attributes, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, COALESCE($14, NOW()))
            ON CONFLICT (id) DO UPDATE SET
                code = EXCLUDED.code,
                name = EXCLUDED.name,
                default_department_id = EXCLUDED.default_department_id,
                category = EXCLUDED.category,
                sequence_order = EXCLUDED.sequence_order,
                default_duration_minutes = EXCLUDED.default_duration_minutes,
                trigger_offset_minutes = EXCLUDED.trigger_offset_minutes,
                trigger_type = EXCLUDED.trigger_type,
                description = EXCLUDED.description,
                anchor = EXCLUDED.anchor,
                is_active = EXCLUDED.is_active,
                attributes = EXCLUDED.attributes
            RETURNING id, code, name, default_department_id, category, sequence_order, default_duration_minutes,
                trigger_offset_minutes, trigger_type, description, anchor, is_active, attributes, created_at
            "#,
        )
        .bind(&task_type.id)
        .bind(&task_type.code)
        .bind(&task_type.name)
        .bind(&task_type.default_department_id)
        .bind(&task_type.category)
        .bind(task_type.sequence_order)
        .bind(task_type.default_duration_minutes)
        .bind(task_type.trigger_offset_minutes)
        .bind(&task_type.trigger_type)
        .bind(&task_type.description)
        .bind(&task_type.anchor)
        .bind(task_type.is_active)
        .bind(&task_type.attributes)
        .bind(task_type.created_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(row_to_task_type(row))
    }
}

#[async_trait]
impl<'tx> TaskTypeTransactionalRepository<sqlx::Transaction<'tx, sqlx::Postgres>>
    for PgTaskTypeRepository
{
    async fn save_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'tx, sqlx::Postgres>,
        task_type: &TaskType,
    ) -> Result<TaskType, DomainError> {
        let row = sqlx::query(
            r#"
            INSERT INTO task_types (id, code, name, default_department_id, category, sequence_order,
                default_duration_minutes, trigger_offset_minutes, trigger_type, description, anchor, is_active, attributes, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, COALESCE($14, NOW()))
            ON CONFLICT (id) DO UPDATE SET
                code = EXCLUDED.code,
                name = EXCLUDED.name,
                default_department_id = EXCLUDED.default_department_id,
                category = EXCLUDED.category,
                sequence_order = EXCLUDED.sequence_order,
                default_duration_minutes = EXCLUDED.default_duration_minutes,
                trigger_offset_minutes = EXCLUDED.trigger_offset_minutes,
                trigger_type = EXCLUDED.trigger_type,
                description = EXCLUDED.description,
                anchor = EXCLUDED.anchor,
                is_active = EXCLUDED.is_active,
                attributes = EXCLUDED.attributes
            RETURNING id, code, name, default_department_id, category, sequence_order, default_duration_minutes,
                trigger_offset_minutes, trigger_type, description, anchor, is_active, attributes, created_at
            "#,
        )
        .bind(&task_type.id)
        .bind(&task_type.code)
        .bind(&task_type.name)
        .bind(&task_type.default_department_id)
        .bind(&task_type.category)
        .bind(task_type.sequence_order)
        .bind(task_type.default_duration_minutes)
        .bind(task_type.trigger_offset_minutes)
        .bind(&task_type.trigger_type)
        .bind(&task_type.description)
        .bind(&task_type.anchor)
        .bind(task_type.is_active)
        .bind(&task_type.attributes)
        .bind(task_type.created_at)
        .fetch_one(&mut **tx)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(row_to_task_type(row))
    }
}

fn row_to_task_type(row: sqlx::postgres::PgRow) -> TaskType {
    TaskType {
        id: row.get("id"),
        code: row.get("code"),
        name: row.get("name"),
        default_department_id: row.get("default_department_id"),
        category: row.get("category"),
        anchor: row
            .get::<Option<String>, _>("anchor")
            .unwrap_or_else(|| "link".to_string()),
        sequence_order: row.get("sequence_order"),
        default_duration_minutes: row.get("default_duration_minutes"),
        trigger_offset_minutes: row.get::<Option<i32>, _>("trigger_offset_minutes").unwrap_or(30),
        trigger_type: row
            .get::<Option<String>, _>("trigger_type")
            .unwrap_or_else(|| "before_eta".to_string()),
        description: row.get("description"),
        is_active: row.get::<Option<bool>, _>("is_active").unwrap_or(true),
        attributes: row.try_get("attributes").unwrap_or_else(|_| serde_json::json!({})),
        created_at: row.get("created_at"),
    }
}
