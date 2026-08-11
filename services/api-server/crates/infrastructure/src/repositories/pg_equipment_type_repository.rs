//! PostgreSQL 设备类型仓储实现。

use async_trait::async_trait;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::EquipmentType;
use fms_domain::ports::dispatch_repository::EquipmentTypeRepository;

pub struct PgEquipmentTypeRepository {
    pool: PgPool,
}

impl PgEquipmentTypeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn load_task_types(&self, equipment_type_id: &str) -> Result<Vec<String>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT task_type
            FROM equipment_type_steps
            WHERE equipment_type_id = $1
            ORDER BY is_required DESC, min_count DESC, task_type ASC
            "#,
        )
        .bind(equipment_type_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(rows.iter().map(|row| row.get("task_type")).collect())
    }
}

#[async_trait]
impl EquipmentTypeRepository for PgEquipmentTypeRepository {
    async fn save(&self, equipment_type: &EquipmentType) -> Result<EquipmentType, DomainError> {
        sqlx::query(
            r#"
            INSERT INTO equipment_types (
                id, name, code, category, requires_driver, driver_team_type_id, icon, description, is_active
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                code = EXCLUDED.code,
                category = EXCLUDED.category,
                requires_driver = EXCLUDED.requires_driver,
                driver_team_type_id = EXCLUDED.driver_team_type_id,
                icon = EXCLUDED.icon,
                description = EXCLUDED.description,
                is_active = EXCLUDED.is_active
            "#,
        )
        .bind(&equipment_type.id)
        .bind(&equipment_type.name)
        .bind(&equipment_type.code)
        .bind(&equipment_type.category)
        .bind(equipment_type.requires_driver)
        .bind(&equipment_type.driver_team_type_id)
        .bind(&equipment_type.icon)
        .bind(&equipment_type.description)
        .bind(equipment_type.is_active)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        self.find_by_id(&equipment_type.id)
            .await?
            .ok_or_else(|| DomainError::Internal("equipment type save returned no row".into()))
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<EquipmentType>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT id, name, code, category, requires_driver, driver_team_type_id, icon, description, created_at, is_active
            FROM equipment_types
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        match row {
            Some(item) => {
                let task_types = self.load_task_types(id).await?;
                Ok(Some(row_to_equipment_type(&item, task_types)))
            }
            None => Ok(None),
        }
    }

    async fn find_all(
        &self,
        include_inactive: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EquipmentType>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(
            "SELECT id, name, code, category, requires_driver, driver_team_type_id, icon, description, created_at, is_active FROM equipment_types WHERE 1=1",
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
            "SELECT equipment_type_id, task_type FROM equipment_type_steps WHERE equipment_type_id = ANY($1)",
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        let task_types_by_id: std::collections::HashMap<String, Vec<String>> = {
            let mut map = std::collections::HashMap::with_capacity(ids.len());
            for tr in task_type_rows {
                let eid: String = tr.get("equipment_type_id");
                let tt: String = tr.get("task_type");
                map.entry(eid).or_insert_with(Vec::new).push(tt);
            }
            map
        };

        let items = rows
            .iter()
            .map(|row| {
                let id: String = row.get("id");
                let mut task_types = task_types_by_id.get(&id).cloned().unwrap_or_default();
                task_types.sort();
                row_to_equipment_type(row, task_types)
            })
            .collect();
        Ok(items)
    }

    async fn set_active(&self, id: &str, is_active: bool) -> Result<Option<EquipmentType>, DomainError> {
        let result = sqlx::query(
            r#"
            UPDATE equipment_types
            SET is_active = $2
            WHERE id = $1
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

fn row_to_equipment_type(row: &sqlx::postgres::PgRow, task_types: Vec<String>) -> EquipmentType {
    EquipmentType {
        id: row.get("id"),
        name: row.get("name"),
        code: row.get("code"),
        category: row.get("category"),
        requires_driver: row.get::<Option<bool>, _>("requires_driver").unwrap_or(false),
        driver_team_type_id: row.get("driver_team_type_id"),
        icon: row.get("icon"),
        description: row.get("description"),
        created_at: row.get("created_at"),
        is_active: row.get::<Option<bool>, _>("is_active").unwrap_or(true),
        task_types,
    }
}
