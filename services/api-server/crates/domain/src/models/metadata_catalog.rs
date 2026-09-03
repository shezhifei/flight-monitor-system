//! 元数据码表（开放/封闭）。不是 flight-ops.v1 一等对象。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const CATALOG_AIRCRAFT_TYPE: &str = "aircraft_type";
pub const CATALOG_ICAO_SIZE: &str = "icao_size";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogEntrySource {
    Manual,
    Ingest,
}

impl CatalogEntrySource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Ingest => "ingest",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw {
            "ingest" => Self::Ingest,
            _ => Self::Manual,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataCatalog {
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub is_open: bool,
    pub is_ordered: bool,
    pub system_owned: bool,
    pub is_active: bool,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataCatalogEntry {
    pub catalog_code: String,
    pub code: String,
    pub name: String,
    pub rank: Option<i32>,
    #[serde(default)]
    pub payload: Value,
    pub is_active: bool,
    pub source: CatalogEntrySource,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

pub fn normalize_catalog_code(raw: &str) -> Result<String, String> {
    let code = raw.trim().to_ascii_lowercase();
    if code.is_empty() || code.len() > 64 {
        return Err("码表 code 不能为空且最长 64".into());
    }
    if !code
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err("码表 code 只允许小写字母、数字和下划线".into());
    }
    Ok(code)
}

pub fn normalize_entry_code(raw: &str) -> Result<String, String> {
    let code = raw.trim().to_string();
    if code.is_empty() || code.len() > 64 {
        return Err("码表项 code 不能为空且最长 64".into());
    }
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_code_must_be_lowercase_token() {
        assert_eq!(normalize_catalog_code(" ICAO_Size ").unwrap(), "icao_size");
        assert!(normalize_catalog_code("ICAO Size").is_err());
        assert!(normalize_catalog_code("").is_err());
    }

    #[test]
    fn entry_code_keeps_original_casing() {
        assert_eq!(normalize_entry_code("  A320  ").unwrap(), "A320");
        assert!(normalize_entry_code("   ").is_err());
    }
}
