//! PostgreSQL 移动设备仓储实现。

use async_trait::async_trait;
use sqlx::{PgPool, Row};

use fms_domain::error::DomainError;
use fms_domain::models::mobile::MobileDeviceRegistration;
use fms_domain::ports::mobile_repository::MobileDeviceRepository;

pub struct PgMobileDeviceRepository {
    pool: PgPool,
}

impl PgMobileDeviceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MobileDeviceRepository for PgMobileDeviceRepository {
    async fn upsert_device(&self, item: &MobileDeviceRegistration) -> Result<MobileDeviceRegistration, DomainError> {
        let row = sqlx::query(
            r#"
            INSERT INTO mobile_device_registrations (
                device_id, user_id, platform, push_channel, push_token,
                app_version, os_version, device_model, manufacturer,
                is_active, last_heartbeat_at, registered_at, updated_at, metadata
            ) VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8, $9,
                $10, $11, $12, $13, $14::jsonb
            )
            ON CONFLICT (device_id) DO UPDATE SET
                user_id = EXCLUDED.user_id,
                platform = EXCLUDED.platform,
                push_channel = EXCLUDED.push_channel,
                push_token = EXCLUDED.push_token,
                app_version = EXCLUDED.app_version,
                os_version = EXCLUDED.os_version,
                device_model = EXCLUDED.device_model,
                manufacturer = EXCLUDED.manufacturer,
                is_active = EXCLUDED.is_active,
                last_heartbeat_at = EXCLUDED.last_heartbeat_at,
                updated_at = EXCLUDED.updated_at,
                metadata = EXCLUDED.metadata
            RETURNING *
            "#,
        )
        .bind(&item.device_id)
        .bind(&item.user_id)
        .bind(&item.platform)
        .bind(&item.push_channel)
        .bind(&item.push_token)
        .bind(&item.app_version)
        .bind(&item.os_version)
        .bind(&item.device_model)
        .bind(&item.manufacturer)
        .bind(item.is_active)
        .bind(item.last_heartbeat_at)
        .bind(item.registered_at)
        .bind(item.updated_at)
        .bind(serde_json::to_value(&item.metadata).unwrap_or_else(|_| serde_json::json!({})))
        .fetch_one(&self.pool)
        .await
        .map_err(internal_error)?;
        Ok(row_to_mobile_device(&row))
    }

    async fn list_active_devices(
        &self,
        user_id: &str,
        limit: i64,
    ) -> Result<Vec<MobileDeviceRegistration>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT *
            FROM mobile_device_registrations
            WHERE user_id = $1
              AND is_active = TRUE
            ORDER BY last_heartbeat_at DESC, updated_at DESC
            LIMIT $2
            "#,
        )
        .bind(user_id)
        .bind(limit.clamp(1, 100))
        .fetch_all(&self.pool)
        .await
        .map_err(internal_error)?;
        Ok(rows.iter().map(row_to_mobile_device).collect())
    }

    async fn deactivate_device(&self, user_id: &str, device_id: &str) -> Result<bool, DomainError> {
        let result = sqlx::query(
            r#"
            UPDATE mobile_device_registrations
            SET is_active = FALSE,
                updated_at = CURRENT_TIMESTAMP
            WHERE user_id = $1
              AND device_id = $2
              AND is_active = TRUE
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .execute(&self.pool)
        .await
        .map_err(internal_error)?;
        Ok(result.rows_affected() > 0)
    }

    async fn heartbeat_device(
        &self,
        user_id: &str,
        device_id: &str,
        metadata_patch: &serde_json::Value,
    ) -> Result<Option<MobileDeviceRegistration>, DomainError> {
        let row = sqlx::query(
            r#"
            UPDATE mobile_device_registrations
            SET last_heartbeat_at = CURRENT_TIMESTAMP,
                updated_at = CURRENT_TIMESTAMP,
                is_active = TRUE,
                metadata = COALESCE(metadata, '{}'::jsonb) || $1::jsonb
            WHERE user_id = $2
              AND device_id = $3
            RETURNING *
            "#,
        )
        .bind(metadata_patch)
        .bind(user_id)
        .bind(device_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(internal_error)?;
        Ok(row.as_ref().map(row_to_mobile_device))
    }
}

fn row_to_mobile_device(row: &sqlx::postgres::PgRow) -> MobileDeviceRegistration {
    MobileDeviceRegistration {
        device_id: row.get("device_id"),
        user_id: row.get("user_id"),
        platform: row
            .try_get::<Option<String>, _>("platform")
            .ok()
            .flatten()
            .unwrap_or_else(|| "android".to_string()),
        push_channel: row
            .try_get::<Option<String>, _>("push_channel")
            .ok()
            .flatten()
            .unwrap_or_else(|| "none".to_string()),
        push_token: row.try_get("push_token").ok().flatten(),
        app_version: row.try_get("app_version").ok().flatten(),
        os_version: row.try_get("os_version").ok().flatten(),
        device_model: row.try_get("device_model").ok().flatten(),
        manufacturer: row.try_get("manufacturer").ok().flatten(),
        is_active: row
            .try_get::<Option<bool>, _>("is_active")
            .ok()
            .flatten()
            .unwrap_or(true),
        last_heartbeat_at: row.get("last_heartbeat_at"),
        registered_at: row.get("registered_at"),
        updated_at: row.get("updated_at"),
        metadata: row
            .try_get::<serde_json::Value, _>("metadata")
            .ok()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default(),
    }
}

fn internal_error(error: sqlx::Error) -> DomainError {
    DomainError::Internal(error.to_string())
}
