//! PostgreSQL 移动端上传仓储实现。

use async_trait::async_trait;
use sqlx::{PgPool, Row};

use fms_domain::error::DomainError;
use fms_domain::models::mobile::MobileUploadAsset;
use fms_domain::ports::mobile_repository::MobileUploadRepository;

pub struct PgMobileUploadRepository {
    pool: PgPool,
}

impl PgMobileUploadRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MobileUploadRepository for PgMobileUploadRepository {
    async fn create(&self, item: &MobileUploadAsset) -> Result<MobileUploadAsset, DomainError> {
        let row = sqlx::query(
            r#"
            INSERT INTO mobile_upload_assets (
                upload_id, user_id, storage_key, original_filename,
                content_type, file_size, checksum_sha256, created_at, metadata
            ) VALUES (
                $1, $2, $3, $4,
                $5, $6, $7, $8, $9::jsonb
            )
            RETURNING *
            "#,
        )
        .bind(&item.upload_id)
        .bind(&item.user_id)
        .bind(&item.storage_key)
        .bind(&item.original_filename)
        .bind(&item.content_type)
        .bind(item.file_size)
        .bind(&item.checksum_sha256)
        .bind(item.created_at)
        .bind(serde_json::to_value(&item.metadata).unwrap_or_else(|_| serde_json::json!({})))
        .fetch_one(&self.pool)
        .await
        .map_err(internal_error)?;
        Ok(row_to_mobile_upload(&row))
    }

    async fn get_by_id(&self, upload_id: &str) -> Result<Option<MobileUploadAsset>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT *
            FROM mobile_upload_assets
            WHERE upload_id = $1
            LIMIT 1
            "#,
        )
        .bind(upload_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(internal_error)?;
        Ok(row.as_ref().map(row_to_mobile_upload))
    }
}

fn row_to_mobile_upload(row: &sqlx::postgres::PgRow) -> MobileUploadAsset {
    MobileUploadAsset {
        upload_id: row.get("upload_id"),
        user_id: row.get("user_id"),
        storage_key: row.get("storage_key"),
        original_filename: row.get("original_filename"),
        content_type: row.try_get("content_type").ok().flatten(),
        file_size: row.try_get::<Option<i64>, _>("file_size").ok().flatten().unwrap_or(0),
        checksum_sha256: row.try_get("checksum_sha256").ok().flatten(),
        created_at: row.get("created_at"),
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
