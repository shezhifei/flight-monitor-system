use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use fms_domain::models::business_case::FlightBusinessCase;
use fms_domain::models::business_case_workflow::BusinessCaseWorkflowRun;

#[derive(Debug, Clone, Deserialize)]
pub struct BusinessCaseWorkflowStartRequest {
    pub flight_id: String,
    pub description: String,
    #[serde(default)]
    pub extra_info: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BusinessCaseWorkflowRunDetail {
    pub run: BusinessCaseWorkflowRun,
    pub business_case: FlightBusinessCase,
    pub process_instance: Option<serde_json::Value>,
    pub active_tasks: Vec<serde_json::Value>,
    pub historic_tasks: Vec<serde_json::Value>,
    pub receipt_group: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BusinessCaseWorkflowStartData {
    pub run: BusinessCaseWorkflowRun,
    pub business_case: FlightBusinessCase,
    pub receipt_group_id: Option<String>,
    pub recipient_snapshot: Vec<HashMap<String, serde_json::Value>>,
    pub process_instance_id: String,
    pub workflow_triggered: bool,
}
