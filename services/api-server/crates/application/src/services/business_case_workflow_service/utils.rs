use std::collections::HashMap;

use chrono::Utc;

pub fn insert_opt_string(target: &mut HashMap<String, serde_json::Value>, key: &str, value: Option<String>) {
    if let Some(value) = value.filter(|item| !item.trim().is_empty()) {
        target.insert(key.to_string(), serde_json::Value::String(value));
    }
}

pub fn insert_opt_datetime(
    target: &mut HashMap<String, serde_json::Value>,
    key: &str,
    value: Option<chrono::DateTime<Utc>>,
) {
    if let Some(value) = value {
        target.insert(key.to_string(), serde_json::Value::String(value.to_rfc3339()));
    }
}
