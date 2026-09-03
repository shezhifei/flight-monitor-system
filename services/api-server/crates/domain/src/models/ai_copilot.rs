//! AI Copilot business-case draft models.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiCopilotBusinessCaseBatch {
    pub batch_id: String,
    pub entity_id: String,
    pub source_page: String,
    pub transcript_summary: String,
    pub transcript_text: String,
    pub draft_actions: serde_json::Value,
    pub status: AiCopilotBatchStatus,
    pub created_by: String,
    pub committed_case_ids: Vec<String>,
    pub idempotency_key: Option<String>,
    #[serde(default)]
    pub notification_groups: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_request: Option<serde_json::Value>,
    #[serde(default = "default_created_action_case_ids")]
    pub created_action_case_ids: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_error: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub commit_attempts: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_next_recovery_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub workflow_dispatch_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_dispatch_request: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_dispatch_error: Option<serde_json::Value>,
    #[serde(default)]
    pub workflow_dispatch_attempts: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_dispatch_next_retry_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_dispatched_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

fn default_created_action_case_ids() -> serde_json::Value {
    serde_json::json!({})
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiCopilotOperationalMetrics {
    pub generated_at: DateTime<Utc>,
    pub batch_status: AiCopilotBatchStatusMetrics,
    pub workflow_dispatch: AiCopilotWorkflowDispatchMetrics,
    pub recent_errors: Vec<AiCopilotOperationalError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AiCopilotBatchStatusMetrics {
    pub total: i64,
    pub draft: i64,
    pub committing: i64,
    pub committed: i64,
    pub failed: i64,
    pub failed_resolved: i64,
    pub expired: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AiCopilotWorkflowDispatchMetrics {
    pub not_required: i64,
    pub pending: i64,
    pub succeeded: i64,
    pub failed: i64,
    pub retry_due: i64,
    pub retry_exhausted: i64,
    pub max_attempts: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiCopilotOperationalError {
    pub batch_id: String,
    pub status: AiCopilotBatchStatus,
    pub workflow_dispatch_status: String,
    pub stage: Option<String>,
    pub message: Option<String>,
    pub attempts: i32,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AiCopilotBatchStatus {
    Draft,
    Committing,
    Committed,
    Failed,
    FailedResolved,
    Expired,
}

impl AiCopilotBatchStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Committing => "committing",
            Self::Committed => "committed",
            Self::Failed => "failed",
            Self::FailedResolved => "failed_resolved",
            Self::Expired => "expired",
        }
    }

    #[expect(clippy::should_implement_trait)] // Option 语义解析器；FromStr 要求 Result，改 trait 属 API 重设计
    pub fn from_str(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "committing" => Self::Committing,
            "committed" => Self::Committed,
            "failed" => Self::Failed,
            "failed_resolved" => Self::FailedResolved,
            "expired" => Self::Expired,
            _ => Self::Draft,
        }
    }
}
