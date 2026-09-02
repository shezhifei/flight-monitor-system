//! PostgreSQL 人员在岗运行时仓储实现。
//!
//! 对应 `personnel_runtime` 表（迁移 137）。参照完整性在应用层保证，
//! 因此本仓储不做 FK（guard: tests/tools/test_no_new_foreign_keys.py）。

use async_trait::async_trait;
use sqlx::{PgPool, Row};

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{PersonnelRuntime, PersonnelStatus};
use fms_domain::ports::dispatch_repository::{PersonnelRuntimeRepository, PersonnelRuntimeTransactionalRepository};

pub struct PgPersonnelRuntimeRepository {
    pool: PgPool,
}

impl PgPersonnelRuntimeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PersonnelRuntimeRepository for PgPersonnelRuntimeRepository {
    async fn save(&self, runtime: &PersonnelRuntime) -> Result<PersonnelRuntime, DomainError> {
        sqlx::query(
            r#"
            INSERT INTO personnel_runtime (
                user_id, current_status, current_stand_id,
                current_position_lat, current_position_lng,
                last_position_update, updated_at, updated_by, attributes
            ) VALUES ($1, $2, $3, $4, $5, $6, CURRENT_TIMESTAMP, $7, $8)
            ON CONFLICT (user_id) DO UPDATE SET
                current_status = EXCLUDED.current_status,
                current_stand_id = EXCLUDED.current_stand_id,
                current_position_lat = EXCLUDED.current_position_lat,
                current_position_lng = EXCLUDED.current_position_lng,
                last_position_update = EXCLUDED.last_position_update,
                updated_at = CURRENT_TIMESTAMP,
                updated_by = EXCLUDED.updated_by
                ,attributes = EXCLUDED.attributes
            "#,
        )
        .bind(&runtime.user_id)
        .bind(personnel_status_value(runtime.current_status))
        .bind(&runtime.current_stand_id)
        .bind(runtime.current_position_lat)
        .bind(runtime.current_position_lng)
        .bind(runtime.last_position_update)
        .bind(&runtime.updated_by)
        .bind(&runtime.attributes)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        self.find_by_user(&runtime.user_id)
            .await?
            .ok_or_else(|| DomainError::Internal("personnel_runtime save returned no row".into()))
    }

    async fn find_by_user(&self, user_id: &str) -> Result<Option<PersonnelRuntime>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT user_id, current_status, current_stand_id,
                   current_position_lat::double precision AS current_position_lat,
                   current_position_lng::double precision AS current_position_lng,
                   last_position_update, updated_at, updated_by, attributes
            FROM personnel_runtime
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(row.map(|row| row_to_personnel_runtime(&row)))
    }

    async fn update_status(
        &self,
        user_id: &str,
        status: &str,
        updated_by: Option<&str>,
    ) -> Result<bool, DomainError> {
        let result = sqlx::query(
            r#"
            UPDATE personnel_runtime
            SET current_status = $2, updated_by = $3, updated_at = CURRENT_TIMESTAMP
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .bind(status)
        .bind(updated_by)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    async fn update_position(
        &self,
        user_id: &str,
        lat: f64,
        lng: f64,
        stand_id: Option<&str>,
    ) -> Result<bool, DomainError> {
        let result = sqlx::query(
            r#"
            UPDATE personnel_runtime
            SET current_position_lat = $2, current_position_lng = $3,
                current_stand_id = $4, last_position_update = CURRENT_TIMESTAMP,
                updated_at = CURRENT_TIMESTAMP
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .bind(lat)
        .bind(lng)
        .bind(stand_id)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;

        Ok(result.rows_affected() > 0)
    }
}

#[async_trait]
impl<'tx> PersonnelRuntimeTransactionalRepository<sqlx::Transaction<'tx, sqlx::Postgres>>
    for PgPersonnelRuntimeRepository
{
    async fn save_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'tx, sqlx::Postgres>,
        runtime: &PersonnelRuntime,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO personnel_runtime (
                user_id, current_status, current_stand_id,
                current_position_lat, current_position_lng,
                last_position_update, updated_at, updated_by, attributes
            ) VALUES ($1, $2, $3, $4, $5, $6, CURRENT_TIMESTAMP, $7, $8)
            ON CONFLICT (user_id) DO UPDATE SET
                current_status = EXCLUDED.current_status,
                current_stand_id = EXCLUDED.current_stand_id,
                current_position_lat = EXCLUDED.current_position_lat,
                current_position_lng = EXCLUDED.current_position_lng,
                last_position_update = EXCLUDED.last_position_update,
                updated_at = CURRENT_TIMESTAMP,
                updated_by = EXCLUDED.updated_by,
                attributes = EXCLUDED.attributes
            "#,
        )
        .bind(&runtime.user_id)
        .bind(personnel_status_value(runtime.current_status))
        .bind(&runtime.current_stand_id)
        .bind(runtime.current_position_lat)
        .bind(runtime.current_position_lng)
        .bind(runtime.last_position_update)
        .bind(&runtime.updated_by)
        .bind(&runtime.attributes)
        .execute(&mut **tx)
        .await
        .map_err(|err| DomainError::Internal(err.to_string()))?;
        Ok(())
    }
}

fn row_to_personnel_runtime(row: &sqlx::postgres::PgRow) -> PersonnelRuntime {
    PersonnelRuntime {
        user_id: row.get("user_id"),
        current_status: parse_personnel_status(row.get::<String, _>("current_status")),
        current_stand_id: row.get("current_stand_id"),
        current_position_lat: row.get("current_position_lat"),
        current_position_lng: row.get("current_position_lng"),
        last_position_update: row.get("last_position_update"),
        updated_at: row.get("updated_at"),
        updated_by: row.get("updated_by"),
        attributes: row.try_get("attributes").unwrap_or_else(|_| serde_json::json!({})),
    }
}

fn parse_personnel_status(value: String) -> PersonnelStatus {
    match value.as_str() {
        "on_duty" => PersonnelStatus::OnDuty,
        "break" => PersonnelStatus::Break,
        "on_leave" => PersonnelStatus::OnLeave,
        _ => PersonnelStatus::OffDuty,
    }
}

fn personnel_status_value(status: PersonnelStatus) -> &'static str {
    match status {
        PersonnelStatus::OnDuty => "on_duty",
        PersonnelStatus::OffDuty => "off_duty",
        PersonnelStatus::Break => "break",
        PersonnelStatus::OnLeave => "on_leave",
    }
}
