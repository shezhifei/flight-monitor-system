//! PostgreSQL 设备仓储实现。

use async_trait::async_trait;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{Equipment, EquipmentStatus, EquipmentType};
use fms_domain::ports::dispatch_repository::{EquipmentRepository, EquipmentTransactionalRepository};

pub struct PgEquipmentRepository {
    pool: PgPool,
}

impl PgEquipmentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EquipmentRepository for PgEquipmentRepository {
    async fn save(&self, equipment: &Equipment) -> Result<Equipment, DomainError> {
        // PR2：不再写 terminal（保留列仅供历史查询，见迁移 141）；department_id 为科室归属。
        sqlx::query(
            r#"
            INSERT INTO equipment (
                id, code, equipment_type_id, department_id, name, license_plate,
                status, current_position_lat, current_position_lng, current_stand_id,
                last_position_update, current_dispatch_id, last_maintenance_date,
                next_maintenance_date, metadata, is_active, attributes
            ) VALUES (
                $1, $2, $3, $4, $5, $6,
                $7, $8, $9, $10,
                $11, $12, $13,
                $14, $15, $16, $17
            )
            ON CONFLICT (id) DO UPDATE SET
                code = EXCLUDED.code,
                equipment_type_id = EXCLUDED.equipment_type_id,
                department_id = EXCLUDED.department_id,
                name = EXCLUDED.name,
                license_plate = EXCLUDED.license_plate,
                status = EXCLUDED.status,
                current_position_lat = EXCLUDED.current_position_lat,
                current_position_lng = EXCLUDED.current_position_lng,
                current_stand_id = EXCLUDED.current_stand_id,
                last_position_update = EXCLUDED.last_position_update,
                current_dispatch_id = EXCLUDED.current_dispatch_id,
                last_maintenance_date = EXCLUDED.last_maintenance_date,
                next_maintenance_date = EXCLUDED.next_maintenance_date,
                metadata = EXCLUDED.metadata,
                is_active = EXCLUDED.is_active,
                attributes = EXCLUDED.attributes,
                updated_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(&equipment.id)
        .bind(&equipment.code)
        .bind(&equipment.equipment_type_id)
        .bind(&equipment.department_id)
        .bind(&equipment.name)
        .bind(&equipment.license_plate)
        .bind(equipment_status_value(equipment.status))
        .bind(equipment.current_position_lat)
        .bind(equipment.current_position_lng)
        .bind(&equipment.current_stand_id)
        .bind(equipment.last_position_update)
        .bind(&equipment.current_dispatch_id)
        .bind(equipment.last_maintenance_date)
        .bind(equipment.next_maintenance_date)
        .bind({
            let meta_json: Option<serde_json::Value> = equipment
                .metadata
                .as_ref()
                .map(|m| serde_json::to_value(m).unwrap_or_default());
            meta_json
        })
        .bind(equipment.is_active)
        .bind(&equipment.attributes)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        self.find_by_id(&equipment.id)
            .await?
            .ok_or_else(|| DomainError::Internal("equipment save returned no row".into()))
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<Equipment>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT e.id, e.code, e.equipment_type_id, e.department_id, e.name, e.license_plate, e.status,
                   e.current_position_lat::double precision AS current_position_lat,
                   e.current_position_lng::double precision AS current_position_lng,
                   e.current_stand_id, e.last_position_update, e.current_dispatch_id,
                   e.last_maintenance_date, e.next_maintenance_date, e.metadata,
                   e.created_at, e.updated_at, e.is_active, e.attributes,
                   et.id AS joined_equipment_type_id, et.name AS equipment_type_name, et.code AS equipment_type_code,
                   et.category AS equipment_type_category, et.requires_driver,
                   et.icon AS equipment_type_icon, et.description AS equipment_type_description,
                   et.created_at AS equipment_type_created_at, et.is_active AS equipment_type_is_active, et.attributes AS equipment_type_attributes
            FROM equipment e
            LEFT JOIN equipment_types et ON et.id = e.equipment_type_id
            WHERE e.id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(row.map(row_to_equipment))
    }

    async fn find_by_code(&self, code: &str) -> Result<Option<Equipment>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT e.id, e.code, e.equipment_type_id, e.department_id, e.name, e.license_plate, e.status,
                   e.current_position_lat::double precision AS current_position_lat,
                   e.current_position_lng::double precision AS current_position_lng,
                   e.current_stand_id, e.last_position_update, e.current_dispatch_id,
                   e.last_maintenance_date, e.next_maintenance_date, e.metadata,
                   e.created_at, e.updated_at, e.is_active, e.attributes,
                   et.id AS joined_equipment_type_id, et.name AS equipment_type_name, et.code AS equipment_type_code,
                   et.category AS equipment_type_category, et.requires_driver,
                   et.icon AS equipment_type_icon, et.description AS equipment_type_description,
                   et.created_at AS equipment_type_created_at, et.is_active AS equipment_type_is_active, et.attributes AS equipment_type_attributes
            FROM equipment e
            LEFT JOIN equipment_types et ON et.id = e.equipment_type_id
            WHERE e.code = $1
            "#,
        )
        .bind(code)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(row.map(row_to_equipment))
    }

    async fn find_available_for_dispatch(
        &self,
        equipment_type_id: Option<&str>,
        terminal: Option<&str>,
    ) -> Result<Vec<Equipment>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(
            r#"
            SELECT e.id, e.code, e.equipment_type_id, e.department_id, e.name, e.license_plate, e.status,
                   e.current_position_lat::double precision AS current_position_lat,
                   e.current_position_lng::double precision AS current_position_lng,
                   e.current_stand_id, e.last_position_update, e.current_dispatch_id,
                   e.last_maintenance_date, e.next_maintenance_date, e.metadata,
                   e.created_at, e.updated_at, e.is_active,
                   et.id AS joined_equipment_type_id, et.name AS equipment_type_name, et.code AS equipment_type_code,
                   et.category AS equipment_type_category, et.requires_driver,
                   et.icon AS equipment_type_icon, et.description AS equipment_type_description,
                   et.created_at AS equipment_type_created_at, et.is_active AS equipment_type_is_active
            FROM equipment e
            LEFT JOIN equipment_types et ON et.id = e.equipment_type_id
            WHERE e.is_active = TRUE AND e.status = 'available'
            "#,
        );
        if let Some(value) = equipment_type_id {
            builder.push(" AND e.equipment_type_id = ").push_bind(value);
        }
        if let Some(value) = terminal {
            builder.push(" AND e.terminal = ").push_bind(value);
        }
        builder.push(" ORDER BY e.code");

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(rows.into_iter().map(row_to_equipment).collect())
    }

    async fn find_all(
        &self,
        include_inactive: bool,
        equipment_type_id: Option<&str>,
        terminal: Option<&str>,
        status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Equipment>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new(
            r#"
            SELECT e.id, e.code, e.equipment_type_id, e.department_id, e.name, e.license_plate, e.status,
                   e.current_position_lat::double precision AS current_position_lat,
                   e.current_position_lng::double precision AS current_position_lng,
                   e.current_stand_id, e.last_position_update, e.current_dispatch_id,
                   e.last_maintenance_date, e.next_maintenance_date, e.metadata,
                   e.created_at, e.updated_at, e.is_active,
                   et.id AS joined_equipment_type_id, et.name AS equipment_type_name, et.code AS equipment_type_code,
                   et.category AS equipment_type_category, et.requires_driver,
                   et.icon AS equipment_type_icon, et.description AS equipment_type_description,
                   et.created_at AS equipment_type_created_at, et.is_active AS equipment_type_is_active
            FROM equipment e
            LEFT JOIN equipment_types et ON et.id = e.equipment_type_id
            WHERE 1=1
            "#,
        );
        if !include_inactive {
            builder.push(" AND e.is_active = TRUE");
        }
        if let Some(value) = equipment_type_id {
            builder.push(" AND e.equipment_type_id = ").push_bind(value);
        }
        if let Some(value) = terminal {
            builder.push(" AND e.terminal = ").push_bind(value);
        }
        if let Some(value) = status {
            builder.push(" AND e.status = ").push_bind(value);
        }
        builder
            .push(" ORDER BY e.code LIMIT ")
            .push_bind(limit.max(1))
            .push(" OFFSET ")
            .push_bind(offset.max(0));

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(rows.into_iter().map(row_to_equipment).collect())
    }

    async fn update_position(&self, id: &str, lat: f64, lng: f64, stand_id: Option<&str>) -> Result<bool, DomainError> {
        let result = sqlx::query(
            r#"
            UPDATE equipment
            SET current_position_lat = $1,
                current_position_lng = $2,
                current_stand_id = $3,
                last_position_update = CURRENT_TIMESTAMP,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = $4
            "#,
        )
        .bind(lat)
        .bind(lng)
        .bind(stand_id)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    async fn update_status(&self, id: &str, status: &str) -> Result<bool, DomainError> {
        let result = sqlx::query(
            r#"
            UPDATE equipment
            SET status = $1,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = $2
            "#,
        )
        .bind(status)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(result.rows_affected() > 0)
    }
}

#[async_trait]
impl<'tx> EquipmentTransactionalRepository<sqlx::Transaction<'tx, sqlx::Postgres>>
    for PgEquipmentRepository
{
    async fn save_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'tx, sqlx::Postgres>,
        equipment: &Equipment,
    ) -> Result<Equipment, DomainError> {
        sqlx::query(
            r#"
            INSERT INTO equipment (
                id, code, equipment_type_id, department_id, name, license_plate,
                status, current_position_lat, current_position_lng, current_stand_id,
                last_position_update, current_dispatch_id, last_maintenance_date,
                next_maintenance_date, metadata, is_active, attributes
            ) VALUES (
                $1, $2, $3, $4, $5, $6,
                $7, $8, $9, $10,
                $11, $12, $13,
                $14, $15, $16, $17
            )
            ON CONFLICT (id) DO UPDATE SET
                code = EXCLUDED.code,
                equipment_type_id = EXCLUDED.equipment_type_id,
                department_id = EXCLUDED.department_id,
                name = EXCLUDED.name,
                license_plate = EXCLUDED.license_plate,
                status = EXCLUDED.status,
                current_position_lat = EXCLUDED.current_position_lat,
                current_position_lng = EXCLUDED.current_position_lng,
                current_stand_id = EXCLUDED.current_stand_id,
                last_position_update = EXCLUDED.last_position_update,
                current_dispatch_id = EXCLUDED.current_dispatch_id,
                last_maintenance_date = EXCLUDED.last_maintenance_date,
                next_maintenance_date = EXCLUDED.next_maintenance_date,
                metadata = EXCLUDED.metadata,
                is_active = EXCLUDED.is_active,
                attributes = EXCLUDED.attributes,
                updated_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(&equipment.id)
        .bind(&equipment.code)
        .bind(&equipment.equipment_type_id)
        .bind(&equipment.department_id)
        .bind(&equipment.name)
        .bind(&equipment.license_plate)
        .bind(equipment_status_value(equipment.status))
        .bind(equipment.current_position_lat)
        .bind(equipment.current_position_lng)
        .bind(&equipment.current_stand_id)
        .bind(equipment.last_position_update)
        .bind(&equipment.current_dispatch_id)
        .bind(equipment.last_maintenance_date)
        .bind(equipment.next_maintenance_date)
        .bind(equipment.metadata.as_ref().map(|m| serde_json::to_value(m).unwrap_or_default()))
        .bind(equipment.is_active)
        .bind(&equipment.attributes)
        .execute(&mut **tx)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        let row = sqlx::query(
            r#"
            SELECT e.id, e.code, e.equipment_type_id, e.department_id, e.name, e.license_plate, e.status,
                   e.current_position_lat::double precision AS current_position_lat,
                   e.current_position_lng::double precision AS current_position_lng,
                   e.current_stand_id, e.last_position_update, e.current_dispatch_id,
                   e.last_maintenance_date, e.next_maintenance_date, e.metadata,
                   e.created_at, e.updated_at, e.is_active, e.attributes,
                   et.id AS joined_equipment_type_id, et.name AS equipment_type_name, et.code AS equipment_type_code,
                   et.category AS equipment_type_category, et.requires_driver,
                   et.icon AS equipment_type_icon, et.description AS equipment_type_description,
                   et.created_at AS equipment_type_created_at, et.is_active AS equipment_type_is_active,
                   et.attributes AS equipment_type_attributes
            FROM equipment e
            LEFT JOIN equipment_types et ON et.id = e.equipment_type_id
            WHERE e.id = $1
            "#,
        )
        .bind(&equipment.id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?
        .ok_or_else(|| DomainError::Internal("equipment transactional save returned no row".into()))?;

        Ok(row_to_equipment(row))
    }
}

fn row_to_equipment(row: sqlx::postgres::PgRow) -> Equipment {
    let equipment_type = row
        .get::<Option<String>, _>("joined_equipment_type_id")
        .map(|id| EquipmentType {
            id,
            name: row.get::<Option<String>, _>("equipment_type_name").unwrap_or_default(),
            code: row.get("equipment_type_code"),
            category: row.get("equipment_type_category"),
            requires_driver: row.get::<Option<bool>, _>("requires_driver").unwrap_or(false),
            icon: row.get("equipment_type_icon"),
            description: row.get("equipment_type_description"),
            created_at: row.get("equipment_type_created_at"),
            is_active: row.get::<Option<bool>, _>("equipment_type_is_active").unwrap_or(true),
            task_types: Vec::new(),
            attributes: row.try_get("equipment_type_attributes").unwrap_or_else(|_| serde_json::json!({})),
        });

    Equipment {
        id: row.get("id"),
        code: row.get("code"),
        equipment_type_id: row.get("equipment_type_id"),
        department_id: row.get("department_id"),
        name: row.get("name"),
        license_plate: row.get("license_plate"),
        status: parse_equipment_status(row.get::<Option<String>, _>("status").as_deref()),
        current_position_lat: row.get("current_position_lat"),
        current_position_lng: row.get("current_position_lng"),
        current_stand_id: row.get("current_stand_id"),
        last_position_update: row.get("last_position_update"),
        current_dispatch_id: row.get("current_dispatch_id"),
        last_maintenance_date: row.get("last_maintenance_date"),
        next_maintenance_date: row.get("next_maintenance_date"),
        metadata: {
            let raw: Option<serde_json::Value> = row.get("metadata");
            raw.and_then(|v| serde_json::from_value(v).ok())
        },
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        is_active: row.get::<Option<bool>, _>("is_active").unwrap_or(true),
        equipment_type,
        attributes: row.try_get("attributes").unwrap_or_else(|_| serde_json::json!({})),
    }
}

fn parse_equipment_status(value: Option<&str>) -> EquipmentStatus {
    match value.unwrap_or("available") {
        "in_use" => EquipmentStatus::InUse,
        "maintenance" => EquipmentStatus::Maintenance,
        "retired" => EquipmentStatus::Retired,
        _ => EquipmentStatus::Available,
    }
}

fn equipment_status_value(status: EquipmentStatus) -> &'static str {
    match status {
        EquipmentStatus::Available => "available",
        EquipmentStatus::InUse => "in_use",
        EquipmentStatus::Maintenance => "maintenance",
        EquipmentStatus::Retired => "retired",
    }
}
