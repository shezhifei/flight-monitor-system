//! 标签 API DTO。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateLabelRequest {
    pub code: String,
    pub name: String,
    #[serde(default = "default_label_color")]
    pub color: String,
    pub icon: Option<String>,
    #[serde(default = "default_label_scope")]
    pub scope: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateLabelRequest {
    pub name: Option<String>,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub is_active: Option<bool>,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachLabelRequest {
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelResponse {
    pub label_id: String,
    pub code: String,
    pub name: String,
    pub color: String,
    pub icon: Option<String>,
    pub scope: String,
    pub category: String,
    pub is_active: bool,
    pub sort_order: i32,
    pub created_by: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

fn default_label_color() -> String {
    "#6B7280".to_string()
}

fn default_label_scope() -> String {
    "flight".to_string()
}
