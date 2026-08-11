//! 权限模板 DTO

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct PermissionTemplateCreate {
    pub name: String,
    pub code: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub category: Option<String>,
    #[serde(default)]
    pub display_order: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PermissionTemplateUpdate {
    pub name: Option<String>,
    pub code: Option<String>,
    pub description: Option<String>,
    pub permissions: Option<Vec<String>>,
    pub category: Option<String>,
    pub display_order: Option<i32>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PermissionTemplateResponse {
    pub id: String,
    pub name: String,
    pub code: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub is_system: bool,
    pub category: Option<String>,
    pub display_order: i32,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApplyTemplateRequest {
    pub template_id: String,
    #[serde(default = "default_apply_mode")]
    pub mode: String,
}

fn default_apply_mode() -> String {
    "replace".to_string()
}
