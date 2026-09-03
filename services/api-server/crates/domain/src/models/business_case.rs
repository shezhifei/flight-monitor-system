//! 业务事项领域模型
//!
//! 对应 Python `src/domain/models/business_cases.py`。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VisibilityScope {
    #[default]
    Common,
    Department,
}

impl VisibilityScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Common => "COMMON",
            Self::Department => "DEPARTMENT",
        }
    }

    pub fn from_optional_str(
        raw: Option<&str>,
        department_id: Option<&str>,
        department_name_snapshot: Option<&str>,
    ) -> Self {
        match raw
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_uppercase())
            .as_deref()
        {
            Some("DEPARTMENT") => Self::Department,
            Some("COMMON") => Self::Common,
            _ => {
                if department_id.map(str::trim).filter(|value| !value.is_empty()).is_some()
                    || department_name_snapshot
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .is_some()
                {
                    Self::Department
                } else {
                    Self::Common
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessCaseType {
    pub id: String,
    pub code: String,
    pub name: String,
    pub bpmn_xml: Option<String>,
    pub description: Option<String>,
    pub is_active: bool,
    #[serde(default)]
    pub visibility_scope: VisibilityScope,
    pub department_id: Option<String>,
    pub department_name_snapshot: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub ai_extraction_config: serde_json::Value,
    #[serde(default)]
    pub case_properties: serde_json::Value,
}

/// 业务事项追加条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessCaseAppendEntry {
    pub append_id: String,
    pub case_id: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_action_id: Option<String>,
    #[serde(default = "default_system")]
    pub submitted_by: String,
    pub submitted_operator_name: Option<String>,
    pub appended_at: DateTime<Utc>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// 航班业务事项 (用于触发业务工作流/通知)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessCaseTerminalMetadata {
    pub timestamp: DateTime<Utc>,
    pub operator: String,
    pub action: String,
    pub target_status: String,
    pub reason: Option<String>,
    pub workflow_run_id: Option<String>,
    pub workflow_outcome: Option<String>,
    pub receipt_group_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BusinessCaseWorkflowReceiptSummary {
    pub total_count: i64,
    pub pending_count: i64,
    pub acknowledged_count: i64,
    pub rejected_count: i64,
    pub latest_updated_at: Option<DateTime<Utc>>,
    pub remind_after_at: Option<DateTime<Utc>>,
    pub is_overdue: bool,
    pub overall_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BusinessCaseWorkflowReceiptItem {
    pub user_id: String,
    pub recipient_user_id: Option<String>,
    pub recipient_username: Option<String>,
    pub recipient_display_name: Option<String>,
    pub recipient_department: Option<String>,
    pub recipient_job_title: Option<String>,
    pub ack_status: String,
    pub ack_at: Option<DateTime<Utc>>,
    pub ack_note: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BusinessCaseWorkflowReceiptProjection {
    pub receipt_group_id: String,
    pub title: Option<String>,
    pub severity: Option<String>,
    pub origin_type: String,
    pub created_at: Option<DateTime<Utc>>,
    pub summary: BusinessCaseWorkflowReceiptSummary,
    #[serde(default)]
    pub items: Vec<BusinessCaseWorkflowReceiptItem>,
}

/// 航班业务事项 (用于触发业务工作流/通知)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightBusinessCase {
    pub case_id: String,
    #[serde(default = "default_case_type")]
    pub case_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case_type_name: Option<String>,
    pub flight_id: String,
    pub flight_no: String,
    pub created_at: DateTime<Utc>,
    #[serde(default = "default_system")]
    pub created_by: String,
    #[serde(default = "default_system")]
    pub updated_by: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_pending_status")]
    pub status: String,
    pub stand: Option<String>,
    pub gate: Option<String>,
    #[serde(default)]
    pub visibility_scope: VisibilityScope,
    pub department_id: Option<String>,
    pub department_name_snapshot: Option<String>,
    pub finished_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub log: Vec<serde_json::Value>,
    #[serde(default)]
    pub context: HashMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_receipt: Option<BusinessCaseWorkflowReceiptProjection>,
    #[serde(default)]
    pub terminal_metadata: Option<BusinessCaseTerminalMetadata>,
    #[serde(default)]
    pub append_count: i32,
    pub latest_append: Option<BusinessCaseAppendEntry>,
    #[serde(default)]
    pub append_entries: Vec<BusinessCaseAppendEntry>,
}

fn default_system() -> String {
    "system".to_string()
}
fn default_case_type() -> String {
    "generic_case".to_string()
}
fn default_pending_status() -> String {
    "PENDING".to_string()
}
