use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OntologyFieldType {
    String,
    Number,
    Boolean,
    Datetime,
    CatalogRef,
    CatalogRefArray,
    ObjectRef,
    ObjectRefArray,
}

impl OntologyFieldType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Datetime => "datetime",
            Self::CatalogRef => "catalog_ref",
            Self::CatalogRefArray => "catalog_ref[]",
            Self::ObjectRef => "object_ref",
            Self::ObjectRefArray => "object_ref[]",
        }
    }
    pub fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "string" => Self::String,
            "number" => Self::Number,
            "boolean" => Self::Boolean,
            "datetime" => Self::Datetime,
            "catalog_ref" => Self::CatalogRef,
            "catalog_ref[]" => Self::CatalogRefArray,
            "object_ref" => Self::ObjectRef,
            "object_ref[]" => Self::ObjectRefArray,
            _ => return None,
        })
    }
    pub fn is_catalog(self) -> bool {
        matches!(self, Self::CatalogRef | Self::CatalogRefArray)
    }
    pub fn is_object(self) -> bool {
        matches!(self, Self::ObjectRef | Self::ObjectRefArray)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldOverlay {
    pub object_name: String,
    pub field_name: String,
    pub field_type: String,
    pub catalog_code: Option<String>,
    pub object_name_target: Option<String>,
    pub required: bool,
    pub list_visible: bool,
    pub filterable: bool,
    pub widget: Option<String>,
    pub description: Option<String>,
    pub visible_when: Option<Value>,
    pub max_length: Option<i32>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub is_active: bool,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::OntologyFieldType;

    #[test]
    fn parses_all_supported_overlay_types() {
        for raw in [
            "string",
            "number",
            "boolean",
            "datetime",
            "catalog_ref",
            "catalog_ref[]",
            "object_ref",
            "object_ref[]",
        ] {
            assert!(OntologyFieldType::parse(raw).is_some(), "{raw}");
        }
        assert!(OntologyFieldType::parse("enum").is_none());
    }
}
