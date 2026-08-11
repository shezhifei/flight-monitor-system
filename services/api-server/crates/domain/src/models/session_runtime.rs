//! 会话运行时模型

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionKickEvent {
    pub reason: String,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnlineSessionStatus {
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login_time: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<DateTime<Utc>>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub department: Option<String>,
    #[serde(skip_serializing_if = "is_false")]
    pub forced_logout: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kick_event: Option<SessionKickEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEstablishResult {
    pub session: OnlineSessionStatus,
    pub created: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRuntimeStatus {
    pub mode: String,
    pub fallback_since: Option<DateTime<Utc>>,
    pub fallback_duration_seconds: Option<i64>,
    pub circuit_state: String,
    pub redis_available: bool,
}
