//! Shift handover DTOs.
//!
//! List / detail / item-ack return **raw** objects.
//! Whole-handover ack returns an envelope wrapping `ShiftHandover`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ShiftHandoverItem {
    pub item_id: String,
    pub handover_id: String,
    pub item_type: String,
    pub title: String,
    pub detail: Option<String>,
    pub owner_user_id: Option<String>,
    pub due_at: Option<String>,
    #[serde(default = "default_true")]
    pub is_mandatory: bool,
    #[serde(default)]
    pub acknowledged: bool,
    pub acknowledged_at: Option<String>,
    pub acknowledged_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ShiftHandover {
    pub handover_id: String,
    pub shift_date: String,
    pub shift_code: String,
    pub from_user_id: String,
    pub to_user_id: String,
    pub from_operator_name: Option<String>,
    pub from_operator_job_title: Option<String>,
    pub from_operator_label: Option<String>,
    pub to_operator_name: Option<String>,
    pub to_operator_job_title: Option<String>,
    pub to_operator_label: Option<String>,
    pub status: String,
    pub summary: Option<String>,
    #[serde(default = "default_medium")]
    pub risk_level: String,
    pub signed_at: Option<String>,
    pub submitted_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub items: Vec<ShiftHandoverItem>,
}

fn default_medium() -> String {
    "medium".to_string()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ShiftHandoverItemAcknowledgeRequest {
    #[serde(default = "default_true")]
    pub acknowledged: bool,
}

impl Default for ShiftHandoverItemAcknowledgeRequest {
    fn default() -> Self {
        Self { acknowledged: true }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_list_parses() {
        let list: Vec<ShiftHandover> = serde_json::from_str("[]").unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn handover_detail_parses_minimal() {
        let raw = r#"{
            "handover_id":"h1","shift_date":"2026-08-12","shift_code":"morning",
            "from_user_id":"u1","to_user_id":"u2","status":"pending",
            "created_at":"2026-08-12T00:00:00Z","updated_at":"2026-08-12T00:00:00Z",
            "items":[]
        }"#;
        let h: ShiftHandover = serde_json::from_str(raw).unwrap();
        assert_eq!(h.risk_level, "medium");
        assert!(h.items.is_empty());
    }
}
