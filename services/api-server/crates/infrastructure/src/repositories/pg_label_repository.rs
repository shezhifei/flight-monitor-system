//! PostgreSQL 标签仓储实现。

use async_trait::async_trait;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};

use fms_domain::error::DomainError;
use fms_domain::models::label::{LabelCategory, LabelDefinition, LabelScope};
use fms_domain::ports::label_repository::{CreateLabelDefinitionParams, LabelRepository, UpdateLabelDefinitionParams};

use super::soft_delete_audit::record_soft_delete;

pub struct PgLabelRepository {
    pool: PgPool,
}

impl PgLabelRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LabelRepository for PgLabelRepository {
    async fn get_all_definitions(&self, active_only: bool) -> Result<Vec<LabelDefinition>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(
            r#"
            SELECT label_id, code, name, color, icon, scope, category,
                   is_active, sort_order, created_by, created_at, updated_at
            FROM label_definitions
            WHERE deleted_at IS NULL
            "#,
        );
        if active_only {
            builder.push(" AND is_active = TRUE");
        }
        builder.push(" ORDER BY sort_order ASC, created_at ASC");

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        rows.iter().map(row_to_definition).collect()
    }

    async fn get_definition_by_code(&self, code: &str) -> Result<Option<LabelDefinition>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT label_id, code, name, color, icon, scope, category,
                   is_active, sort_order, created_by, created_at, updated_at
            FROM label_definitions
            WHERE code = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(code)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        row.as_ref().map(row_to_definition).transpose()
    }

    async fn create_definition(&self, params: CreateLabelDefinitionParams) -> Result<LabelDefinition, DomainError> {
        let label_id = ulid::Ulid::new().to_string();
        let row = sqlx::query(
            r#"
            INSERT INTO label_definitions (
                label_id, code, name, color, icon, scope, category,
                is_active, sort_order, created_by, created_at, updated_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, 'custom',
                TRUE, 0, $7, NOW(), NOW()
            )
            RETURNING label_id, code, name, color, icon, scope, category,
                      is_active, sort_order, created_by, created_at, updated_at
            "#,
        )
        .bind(&label_id)
        .bind(&params.code)
        .bind(&params.name)
        .bind(&params.color)
        .bind(&params.icon)
        .bind(params.scope.as_str())
        .bind(&params.created_by)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        row_to_definition(&row)
    }

    async fn update_definition(
        &self,
        label_id: &str,
        params: UpdateLabelDefinitionParams,
    ) -> Result<bool, DomainError> {
        let has_updates = params.name.is_some()
            || params.color.is_some()
            || params.icon.is_some()
            || params.is_active.is_some()
            || params.sort_order.is_some();
        if !has_updates {
            return Ok(false);
        }

        let mut builder = QueryBuilder::<Postgres>::new("UPDATE label_definitions SET updated_at = NOW()");
        if let Some(name) = params.name {
            builder.push(", name = ");
            builder.push_bind(name);
        }
        if let Some(color) = params.color {
            builder.push(", color = ");
            builder.push_bind(color);
        }
        if let Some(icon) = params.icon {
            builder.push(", icon = ");
            builder.push_bind(icon);
        }
        if let Some(is_active) = params.is_active {
            builder.push(", is_active = ");
            builder.push_bind(is_active);
        }
        if let Some(sort_order) = params.sort_order {
            builder.push(", sort_order = ");
            builder.push_bind(sort_order);
        }
        builder.push(" WHERE deleted_at IS NULL AND label_id = ");
        builder.push_bind(label_id);

        let result = builder
            .build()
            .execute(&self.pool)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn delete_definition(&self, label_id: &str) -> Result<bool, DomainError> {
        // 审计要求软删除：仅标记 deleted_at，行保留
        let result = sqlx::query(
            "UPDATE label_definitions SET deleted_at = NOW(), updated_at = NOW() \
             WHERE label_id = $1 AND category = 'custom' AND deleted_at IS NULL",
        )
        .bind(label_id)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        let deleted = result.rows_affected() > 0;
        if deleted {
            record_soft_delete(&self.pool, "label_definition", label_id, "soft_delete").await;
        }
        Ok(deleted)
    }

    async fn attach_flight_label(&self, flight_id: &str, code: &str) -> Result<(), DomainError> {
        let label_json = serde_json::json!([code]);
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;

        sqlx::query(
            r#"
            UPDATE flights
            SET labels = CASE
                WHEN NOT labels @> $1::jsonb THEN labels || $1::jsonb
                ELSE labels
            END,
            updated_at = NOW()
            WHERE flight_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(&label_json)
        .bind(flight_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        match code {
            "quick_turnaround" => {
                sqlx::query(
                    "UPDATE flights SET is_quick_turnaround = TRUE WHERE flight_id = $1 AND deleted_at IS NULL",
                )
                .bind(flight_id)
                .execute(&mut *tx)
                .await
                .map_err(|error| DomainError::Internal(error.to_string()))?;
            }
            "boarding_restriction" => {
                sqlx::query(
                    "UPDATE flights SET has_boarding_restriction = TRUE WHERE flight_id = $1 AND deleted_at IS NULL",
                )
                .bind(flight_id)
                .execute(&mut *tx)
                .await
                .map_err(|error| DomainError::Internal(error.to_string()))?;
            }
            _ => {}
        }

        tx.commit()
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(())
    }

    async fn detach_flight_label(&self, flight_id: &str, code: &str) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;

        sqlx::query(
            r#"
            UPDATE flights
            SET labels = labels - $1,
                updated_at = NOW()
            WHERE flight_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(code)
        .bind(flight_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        match code {
            "quick_turnaround" => {
                sqlx::query(
                    "UPDATE flights SET is_quick_turnaround = FALSE WHERE flight_id = $1 AND deleted_at IS NULL",
                )
                .bind(flight_id)
                .execute(&mut *tx)
                .await
                .map_err(|error| DomainError::Internal(error.to_string()))?;
            }
            "boarding_restriction" => {
                sqlx::query(
                    "UPDATE flights SET has_boarding_restriction = FALSE WHERE flight_id = $1 AND deleted_at IS NULL",
                )
                .bind(flight_id)
                .execute(&mut *tx)
                .await
                .map_err(|error| DomainError::Internal(error.to_string()))?;
            }
            _ => {}
        }

        tx.commit()
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(())
    }

    async fn attach_leg_label(&self, flight_id: &str, leg_type: &str, code: &str) -> Result<(), DomainError> {
        let label_json = serde_json::json!([code]);
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;

        sqlx::query(
            r#"
            UPDATE flight_legs
            SET labels = CASE
                WHEN NOT labels @> $1::jsonb THEN labels || $1::jsonb
                ELSE labels
            END,
            updated_at = NOW()
            WHERE flight_id = $2 AND leg_type = $3 AND deleted_at IS NULL
            "#,
        )
        .bind(&label_json)
        .bind(flight_id)
        .bind(leg_type)
        .execute(&mut *tx)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        if code == "vip" {
            sqlx::query(
                "UPDATE flight_legs SET is_vip = TRUE WHERE flight_id = $1 AND leg_type = $2 AND deleted_at IS NULL",
            )
            .bind(flight_id)
            .bind(leg_type)
            .execute(&mut *tx)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        }

        tx.commit()
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(())
    }

    async fn detach_leg_label(&self, flight_id: &str, leg_type: &str, code: &str) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;

        sqlx::query(
            r#"
            UPDATE flight_legs
            SET labels = labels - $1,
                updated_at = NOW()
            WHERE flight_id = $2 AND leg_type = $3 AND deleted_at IS NULL
            "#,
        )
        .bind(code)
        .bind(flight_id)
        .bind(leg_type)
        .execute(&mut *tx)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;

        if code == "vip" {
            sqlx::query(
                "UPDATE flight_legs SET is_vip = FALSE WHERE flight_id = $1 AND leg_type = $2 AND deleted_at IS NULL",
            )
            .bind(flight_id)
            .bind(leg_type)
            .execute(&mut *tx)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        }

        tx.commit()
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(())
    }

    async fn get_flight_labels(&self, flight_id: &str) -> Result<Vec<String>, DomainError> {
        let row = sqlx::query("SELECT labels FROM flights WHERE flight_id = $1 AND deleted_at IS NULL")
            .bind(flight_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;

        match row {
            Some(row) => {
                let labels: serde_json::Value = row.get("labels");
                let arr = labels.as_array().cloned().unwrap_or_default();
                Ok(arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            }
            None => Ok(vec![]),
        }
    }

    async fn get_leg_labels(&self, flight_id: &str, leg_type: &str) -> Result<Vec<String>, DomainError> {
        let row =
            sqlx::query("SELECT labels FROM flight_legs WHERE flight_id = $1 AND leg_type = $2 AND deleted_at IS NULL")
                .bind(flight_id)
                .bind(leg_type)
                .fetch_optional(&self.pool)
                .await
                .map_err(|error| DomainError::Internal(error.to_string()))?;

        match row {
            Some(row) => {
                let labels: serde_json::Value = row.get("labels");
                let arr = labels.as_array().cloned().unwrap_or_default();
                Ok(arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            }
            None => Ok(vec![]),
        }
    }
}

fn row_to_definition(row: &sqlx::postgres::PgRow) -> Result<LabelDefinition, DomainError> {
    let scope_raw = row.get::<String, _>("scope");
    let category_raw = row.get::<String, _>("category");
    let scope = LabelScope::from_db(&scope_raw)
        .ok_or_else(|| DomainError::Internal(format!("invalid label scope in db: {scope_raw}")))?;
    let category = LabelCategory::from_db(&category_raw)
        .ok_or_else(|| DomainError::Internal(format!("invalid label category in db: {category_raw}")))?;

    Ok(LabelDefinition {
        label_id: row.get("label_id"),
        code: row.get("code"),
        name: row.get("name"),
        color: row.get("color"),
        icon: row.get("icon"),
        scope,
        category,
        is_active: row.get("is_active"),
        sort_order: row.get("sort_order"),
        created_by: row.get("created_by"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}
