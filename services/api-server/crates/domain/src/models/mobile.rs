//! 移动端领域模型
//!
//! 对应 Python `src/domain/models/mobile.py`。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 移动设备注册记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileDeviceRegistration {
    pub device_id: String,
    pub user_id: String,
    #[serde(default = "default_android")]
    pub platform: String,
    #[serde(default = "default_none_channel")]
    pub push_channel: String,
    pub push_token: Option<String>,
    pub app_version: Option<String>,
    pub os_version: Option<String>,
    pub device_model: Option<String>,
    pub manufacturer: Option<String>,
    #[serde(default = "default_true")]
    pub is_active: bool,
    pub last_heartbeat_at: DateTime<Utc>,
    pub registered_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// 移动端上传资产
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileUploadAsset {
    pub upload_id: String,
    pub user_id: String,
    pub storage_key: String,
    pub original_filename: String,
    pub content_type: Option<String>,
    #[serde(default)]
    pub file_size: i64,
    pub checksum_sha256: Option<String>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

fn default_true() -> bool {
    true
}
fn default_android() -> String {
    "android".to_string()
}
fn default_none_channel() -> String {
    "none".to_string()
}
