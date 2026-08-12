//! Mobile operations event feed DTOs (plan §0.5 Mobile operations).
//!
//! `GET /api/v2/mobile/operations/events` → enveloped
//! `MobileOperationsEventsResponse`.

use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct OperationsEventItem {
    pub event_id: String,
    pub event_type: String,
    #[serde(default = "default_info")]
    pub severity: String,
    #[serde(default = "default_open")]
    pub status: String,
    pub title: String,
    pub flight_id: Option<String>,
    pub occurred_at: String,
    pub source: String,
}

fn default_info() -> String {
    "info".to_string()
}
fn default_open() -> String {
    "open".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct OperationsEventsResponse {
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub generated_at: Option<String>,
    #[serde(default)]
    pub total: i64,
    #[serde(default)]
    pub event_type_counts: HashMap<String, i64>,
    #[serde(default)]
    pub severity_counts: HashMap<String, i64>,
    #[serde(default)]
    pub events: Vec<OperationsEventItem>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operations_feed_parses_live_shape() {
        let raw = r#"{
            "event_type_counts":{"anomaly":3},
            "events":[{
                "event_id":"e1","event_type":"anomaly","severity":"medium",
                "status":"open","title":"t","flight_id":"f1",
                "occurred_at":"2026-01-01T00:00:00Z","source":"anomalies",
                "payload":{"x":1}
            }]
        }"#;
        let feed: OperationsEventsResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(feed.events.len(), 1);
        assert_eq!(feed.event_type_counts.get("anomaly"), Some(&3));
    }
}
