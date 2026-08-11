//! 标签领域模型。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LabelScope {
    Flight,
    Leg,
    Both,
}

impl LabelScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Flight => "flight",
            Self::Leg => "leg",
            Self::Both => "both",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value.trim() {
            "flight" => Some(Self::Flight),
            "leg" => Some(Self::Leg),
            "both" => Some(Self::Both),
            _ => None,
        }
    }

    pub fn from_api(value: &str) -> Option<Self> {
        Self::from_db(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LabelCategory {
    System,
    Custom,
}

impl LabelCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Custom => "custom",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value.trim() {
            "system" => Some(Self::System),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelDefinition {
    pub label_id: String,
    pub code: String,
    pub name: String,
    pub color: String,
    pub icon: Option<String>,
    pub scope: LabelScope,
    pub category: LabelCategory,
    pub is_active: bool,
    pub sort_order: i32,
    pub created_by: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl LabelDefinition {
    pub fn is_system(&self) -> bool {
        self.category == LabelCategory::System
    }
}
