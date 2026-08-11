use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessCaseWorkflowRun {
    pub run_id: String,
    pub template_code: String,
    pub case_id: String,
    pub flight_id: String,
    pub process_definition_key: String,
    pub process_instance_id: String,
    pub waiting_task_id: Option<String>,
    pub receipt_group_id: Option<String>,
    #[serde(default = "default_pending_status")]
    pub status: String,
    pub outcome: Option<String>,
    #[serde(default)]
    pub recipient_snapshot: Vec<HashMap<String, serde_json::Value>>,
    #[serde(default)]
    pub flight_context_snapshot: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub start_payload: HashMap<String, serde_json::Value>,
    #[serde(default = "default_system")]
    pub started_by: String,
    pub completed_at: Option<DateTime<Utc>>,
    pub failed_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn default_system() -> String {
    "system".to_string()
}

fn default_pending_status() -> String {
    "pending".to_string()
}
