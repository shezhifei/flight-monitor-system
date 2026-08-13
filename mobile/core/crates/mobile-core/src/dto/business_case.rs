//! Business case DTOs.
//!
//! List endpoint returns a **JSON array of envelopes**
//! `[{success,data,message}, ...]` (backend `list_business_cases`).
//! Detail / create / append / types / workflow use standard envelopes.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BusinessCase {
    pub case_id: String,
    pub case_type: String,
    #[serde(default)]
    pub case_type_name: Option<String>,
    pub flight_id: String,
    pub flight_no: String,
    pub created_at: String,
    pub created_by: String,
    #[serde(default)]
    pub updated_by: Option<String>,
    pub description: String,
    pub status: String,
    pub stand: Option<String>,
    pub gate: Option<String>,
    #[serde(default = "default_common")]
    pub visibility_scope: String,
    pub department_id: Option<String>,
    pub department_name_snapshot: Option<String>,
    pub finished_at: Option<String>,
    pub cancelled_at: Option<String>,
    #[serde(default)]
    pub append_count: i64,
    pub latest_append: Option<BusinessCaseAppendEntry>,
    #[serde(default)]
    pub append_entries: Vec<BusinessCaseAppendEntry>,
}

fn default_common() -> String {
    "COMMON".to_string()
}

/// List item envelope: `{success, data, message}`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BusinessCaseListItemEnvelope {
    #[serde(default = "default_true")]
    pub success: bool,
    pub data: Option<BusinessCase>,
    pub message: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BusinessCaseAppendEntry {
    pub append_id: String,
    pub case_id: String,
    pub content: String,
    pub submitted_by: String,
    pub submitted_operator_name: Option<String>,
    pub appended_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BusinessCaseType {
    pub id: String,
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(default = "default_true")]
    pub is_active: bool,
    #[serde(default = "default_common")]
    pub visibility_scope: String,
    pub department_id: Option<String>,
    pub department_name_snapshot: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct BusinessCaseCreateRequest {
    pub case_type: String,
    pub flight_id: String,
    pub description: String,
    pub visibility_scope: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct BusinessCaseAppendRequest {
    pub content: String,
    #[serde(default)]
    pub mention_user_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BusinessCaseAppendAcknowledgement {
    pub acknowledged: bool,
    pub acknowledged_at: Option<String>,
    pub append_id: String,
    pub user_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct BusinessCaseWorkflowStartRequest {
    pub flight_id: String,
    pub description: String,
}

/// Essential fields from workflow start / detail responses.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BusinessCaseWorkflowStartData {
    pub process_instance_id: Option<String>,
    #[serde(default)]
    pub workflow_triggered: bool,
    pub business_case: Option<BusinessCase>,
    pub run: Option<BusinessCaseWorkflowRun>,
    pub receipt_group_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BusinessCaseWorkflowRun {
    pub run_id: String,
    pub template_code: String,
    pub case_id: String,
    pub flight_id: String,
    pub process_instance_id: String,
    pub status: String,
    pub outcome: Option<String>,
    pub started_by: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BusinessCaseWorkflowRunDetail {
    pub run: Option<BusinessCaseWorkflowRun>,
    pub business_case: Option<BusinessCase>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_envelope_array_parses() {
        let raw = r#"[{"success":true,"data":{
            "case_id":"c1","case_type":"t","flight_id":"f","flight_no":"CA1",
            "created_at":"t","created_by":"u","description":"d","status":"PENDING"
        },"message":"ok"}]"#;
        let list: Vec<BusinessCaseListItemEnvelope> = serde_json::from_str(raw).unwrap();
        assert_eq!(list[0].data.as_ref().unwrap().case_id, "c1");
    }
}
