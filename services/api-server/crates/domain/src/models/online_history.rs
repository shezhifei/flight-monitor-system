//! 在线历史记录模型

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnlineHistoryRecord {
    pub id: String,
    pub user_id: String,
    pub username: String,
    pub login_time: DateTime<Utc>,
    pub logout_time: Option<DateTime<Utc>>,
    pub duration_seconds: Option<i32>,
    pub ip_address: Option<String>,
    pub device_info: Option<String>,
    pub forced_logout: bool,
}
