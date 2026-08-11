//! 权限模板领域模型

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionTemplate {
    pub id: String,
    pub name: String,
    pub code: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub is_system: bool,
    pub category: Option<String>,
    #[serde(default)]
    pub display_order: i32,
    #[serde(default = "default_true")]
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn default_true() -> bool {
    true
}
