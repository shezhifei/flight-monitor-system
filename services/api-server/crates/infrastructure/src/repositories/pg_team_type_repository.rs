//! PostgreSQL 班组类型仓储实现。

use async_trait::async_trait;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::TeamType;
use fms_domain::ports::dispatch_repository::TeamTypeRepository;

pub struct PgTeamTypeRepository {
    pool: PgPool,
}

impl PgTeamTypeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn load_task_types(&self, team_type_id: &str) -> Result<Vec<String>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT task_type
            FROM team_type_steps
            WHERE team_type_id = $1 AND deleted_at IS NULL
            ORDER BY priority DESC, task_type ASC
            "#,
        )
        .bind(team_type_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(rows.iter().map(|row| row.get("task_type")).collect())
    }
}

#[async_trait]
impl TeamTypeRepository for PgTeamTypeRepository {
    async fn save(&self, team_type: &TeamType) -> Result<TeamType, DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;

        sqlx::query(
            r#"
            INSERT INTO team_types (
                id, department_id, name, code, description, color, is_driver_type, is_active
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (id) DO UPDATE SET
                department_id = EXCLUDED.department_id,
                name = EXCLUDED.name,
                code = EXCLUDED.code,
                description = EXCLUDED.description,
                color = EXCLUDED.color,
                is_driver_type = EXCLUDED.is_driver_type,
                is_active = EXCLUDED.is_active,
                deleted_at = NULL,
                updated_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(&team_type.id)
        .bind(&team_type.department_id)
        .bind(&team_type.name)
        .bind(&team_type.code)
        .bind(&team_type.description)
        .bind(&team_type.color)
        .bind(team_type.is_driver_type)
        .bind(team_type.is_active)
        .execute(&mut *tx)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        sqlx::query(
            "UPDATE team_type_steps SET deleted_at = NOW() WHERE team_type_id = $1 AND deleted_at IS NULL",
        )
        .bind(&team_type.id)
        .execute(&mut *tx)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        if !team_type.task_types.is_empty() {
            let mut query_builder = QueryBuilder::<Postgres>::new(
                "INSERT INTO team_type_steps (team_type_id, task_type, priority) ",
            );
            query_builder.push_values(&team_type.task_types, |mut b, task_type| {
                let priority = team_type.task_types.len() as i32;
                b.push_bind(&team_type.id).push_bind(task_type).push_bind(priority);
            });
            query_builder.push(
                " ON CONFLICT (team_type_id, task_type) DO UPDATE SET \
                 priority = EXCLUDED.priority, deleted_at = NULL",
            );
            query_builder
                .build()
                .execute(&mut *tx)
                .await
                .map_err(|err| DomainError::Internal(err.to_string()))?;
        }

        tx.commit()
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;

        self.find_by_id(&team_type.id)
            .await?
            .ok_or_else(|| DomainError::Internal("team type save returned no row".into()))
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<TeamType>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT id, department_id, name, code, description, color, is_driver_type, created_at, updated_at, is_active
            FROM team_types
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        match row {
            Some(item) => {
                let task_types = self.load_task_types(id).await?;
                Ok(Some(row_to_team_type(&item, task_types)))
            }
            None => Ok(None),
        }
    }

    async fn find_all(&self, include_inactive: bool, limit: i64, offset: i64) -> Result<Vec<TeamType>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(
            "SELECT id, department_id, name, code, description, color, is_driver_type, created_at, updated_at, is_active FROM team_types WHERE deleted_at IS NULL",
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

        if rows.is_empty() {
            return Ok(Vec::new());
        }

        let ids: Vec<String> = rows.iter().map(|row| row.get("id")).collect();
        let task_type_rows = sqlx::query(
            "SELECT team_type_id, task_type FROM team_type_steps WHERE team_type_id = ANY($1) AND deleted_at IS NULL ORDER BY priority DESC, task_type ASC",
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        let task_types_by_id: std::collections::HashMap<String, Vec<String>> = {
            let mut map = std::collections::HashMap::with_capacity(ids.len());
            for tr in task_type_rows {
                let tid: String = tr.get("team_type_id");
                let tt: String = tr.get("task_type");
                map.entry(tid).or_insert_with(Vec::new).push(tt);
            }
            map
        };

        let items = rows
            .iter()
            .map(|row| {
                let id: String = row.get("id");
                let task_types = task_types_by_id.get(&id).cloned().unwrap_or_default();
                row_to_team_type(row, task_types)
            })
            .collect();
        Ok(items)
    }

    async fn find_by_task_type(&self, task_type: &str) -> Result<Vec<TeamType>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT tt.id, tt.department_id, tt.name, tt.code, tt.description,
                   tt.color, tt.is_driver_type, tt.created_at, tt.updated_at, tt.is_active
            FROM team_types tt
            INNER JOIN team_type_steps tts ON tts.team_type_id = tt.id
            WHERE tts.task_type = $1 AND tts.deleted_at IS NULL AND tt.deleted_at IS NULL AND tt.is_active = TRUE
            ORDER BY tts.priority DESC, tt.name
            "#,
        )
        .bind(task_type)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        if rows.is_empty() {
            return Ok(Vec::new());
        }

        let ids: Vec<String> = rows.iter().map(|row| row.get("id")).collect();
        let task_type_rows = sqlx::query(
            "SELECT team_type_id, task_type FROM team_type_steps WHERE team_type_id = ANY($1) AND deleted_at IS NULL ORDER BY priority DESC, task_type ASC",
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        let task_types_by_id: std::collections::HashMap<String, Vec<String>> = {
            let mut map = std::collections::HashMap::with_capacity(ids.len());
            for tr in task_type_rows {
                let tid: String = tr.get("team_type_id");
                let tt: String = tr.get("task_type");
                map.entry(tid).or_insert_with(Vec::new).push(tt);
            }
            map
        };

        let items = rows
            .iter()
            .map(|row| {
                let id: String = row.get("id");
                let task_types = task_types_by_id.get(&id).cloned().unwrap_or_default();
                row_to_team_type(row, task_types)
            })
            .collect();
        Ok(items)
    }

    async fn set_active(&self, id: &str, is_active: bool) -> Result<Option<TeamType>, DomainError> {
        let result = sqlx::query(
            r#"
            UPDATE team_types
            SET is_active = $2, updated_at = CURRENT_TIMESTAMP
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(is_active)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        if result.rows_affected() == 0 {
            return Ok(None);
        }
        self.find_by_id(id).await
    }
}

fn row_to_team_type(row: &sqlx::postgres::PgRow, task_types: Vec<String>) -> TeamType {
    TeamType {
        id: row.get("id"),
        name: row.get("name"),
        department_id: row.get("department_id"),
        code: row.get("code"),
        description: row.get("description"),
        color: row.get("color"),
        is_driver_type: row.get::<Option<bool>, _>("is_driver_type").unwrap_or(false),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        is_active: row.get::<Option<bool>, _>("is_active").unwrap_or(true),
        task_types,
    }
}
