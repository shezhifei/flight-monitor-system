//! 通知 DTO 模式
//!
//! 对应 Python `src/application/schemas/notification_schemas.py`。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub struct NotificationCreate {
    pub user_id: String,
    pub title: String,
    pub body: String,
    #[serde(default = "default_info")]
    pub notification_type: String,
    pub link: Option<String>,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

fn default_info() -> String {
    "info".to_string()
}

#[derive(Debug, Clone, Serialize)]
pub struct NotificationResponse {
    pub id: String,
    pub user_id: String,
    pub title: String,
    pub body: String,
    pub notification_type: String,
    pub is_read: bool,
    pub link: Option<String>,
    pub created_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotificationListResponse {
    pub items: Vec<NotificationResponse>,
    pub total: i64,
    pub unread_count: i64,
}
