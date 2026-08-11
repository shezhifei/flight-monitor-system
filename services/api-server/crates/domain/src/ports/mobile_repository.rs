//! 移动端设备与上传仓储接口。

use crate::error::DomainError;
use crate::models::mobile::{MobileDeviceRegistration, MobileUploadAsset};
use async_trait::async_trait;

#[async_trait]
pub trait MobileDeviceRepository {
    async fn upsert_device(&self, item: &MobileDeviceRegistration) -> Result<MobileDeviceRegistration, DomainError>;

    async fn list_active_devices(
        &self,
        user_id: &str,
        limit: i64,
    ) -> Result<Vec<MobileDeviceRegistration>, DomainError>;

    async fn deactivate_device(&self, user_id: &str, device_id: &str) -> Result<bool, DomainError>;

    async fn heartbeat_device(
        &self,
        user_id: &str,
        device_id: &str,
        metadata_patch: &serde_json::Value,
    ) -> Result<Option<MobileDeviceRegistration>, DomainError>;
}

#[async_trait]
pub trait MobileUploadRepository {
    async fn create(&self, item: &MobileUploadAsset) -> Result<MobileUploadAsset, DomainError>;

    async fn get_by_id(&self, upload_id: &str) -> Result<Option<MobileUploadAsset>, DomainError>;
}
