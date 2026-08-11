//! Request, response, and serde payload DTOs for the AI copilot service.
//!
//! All wire-facing types live here. The implementation file keeps only the
//! service impl and the test module; this keeps each file small and lets
//! reviewers scan the public schema surface in one place.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub struct AiCopilotDraftRequest {
    pub entity_id: String,
    pub transcript: String,
    #[serde(default)]
    pub source_page: Option<String>,
    #[serde(default)]
    pub context: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiCopilotCommitRequest {
    #[serde(default)]
    pub idempotency_key: Option<String>,
    #[serde(default)]
    pub actions: Vec<AiCopilotApprovedAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiCopilotApprovedAction {
    pub action_id: String,
    pub case_type: String,
    pub flight_id: String,
    pub flight_no: String,
    #[serde(default)]
    pub bound_leg_type: Option<String>,
    #[serde(default)]
    pub bound_flight_no: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub remarks: Option<String>,
    #[serde(default)]
    pub fields: Value,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiCopilotDraftResponse {
    pub batch_id: String,
    pub summary: String,
    pub transcript: String,
    pub actions: Vec<AiCopilotDraftAction>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiCopilotDraftDiagnosticResponse {
    pub ok: bool,
    pub entity_id: String,
    pub transcript_summary: String,
    pub candidate_case_types: Vec<AiCopilotCaseTypeDiagnostic>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_raw_preview: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parsed_payload: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_stage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiCopilotCaseTypeDiagnostic {
    pub code: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiCopilotDraftAction {
    pub action_id: String,
    pub case_type: String,
    pub case_type_name: Option<String>,
    pub flight_number_raw: String,
    pub leg_type_hint: String,
    pub description: String,
    pub remarks: String,
    #[serde(default)]
    pub fields: Value,
    pub confidence: f64,
    pub needs_review: bool,
    pub review_reason: Option<String>,
    pub matched_flight: Option<AiCopilotMatchedFlight>,
    #[serde(default)]
    pub candidates: Vec<AiCopilotMatchedFlight>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiCopilotMatchedFlight {
    pub flight_id: String,
    pub flight_no: String,
    pub leg_type: String,
    pub score: f64,
    pub scheduled_departure: Option<DateTime<Utc>>,
    pub estimated_departure: Option<DateTime<Utc>>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiCopilotCommitResponse {
    pub batch_id: String,
    pub case_ids: Vec<String>,
    pub notification_groups: Vec<AiCopilotNotificationGroup>,
    pub already_committed: bool,
    pub workflow_dispatch_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiCopilotBatchStatusResponse {
    pub batch_id: String,
    pub entity_id: String,
    pub source_page: String,
    pub transcript_summary: String,
    pub draft_actions: Value,
    pub status: fms_domain::models::ai_copilot::AiCopilotBatchStatus,
    pub created_by: String,
    pub committed_case_ids: Vec<String>,
    pub notification_groups: Value,
    pub commit_error: Option<Value>,
    pub committed_at: Option<DateTime<Utc>>,
    pub workflow_dispatch_status: String,
    pub workflow_dispatch_error: Option<Value>,
    pub workflow_dispatch_attempts: i32,
    pub workflow_dispatch_next_retry_at: Option<DateTime<Utc>>,
    pub workflow_dispatched_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiCopilotBatchListResponse {
    pub items: Vec<AiCopilotBatchStatusResponse>,
    pub limit: i64,
    pub offset: i64,
}

pub type AiCopilotOperationalMetricsResponse = fms_domain::models::ai_copilot::AiCopilotOperationalMetrics;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiCopilotWorkflowDispatchRetrySummary {
    pub scanned: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub skipped: usize,
    pub batch_ids: Vec<String>,
    pub errors: Vec<AiCopilotWorkflowDispatchRetryError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiCopilotWorkflowDispatchRetryError {
    pub batch_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiCopilotCommitRecoverySummary {
    pub scanned: usize,
    pub committed: usize,
    pub dispatched: usize,
    pub dispatch_failed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub legacy_missing_request: usize,
    pub batch_ids: Vec<String>,
    pub errors: Vec<AiCopilotCommitRecoveryError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiCopilotCommitRecoveryError {
    pub batch_id: String,
    pub stage: String,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AiCopilotFailedBatchResolutionRequest {
    pub action: AiCopilotFailedBatchResolutionAction,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AiCopilotFailedBatchResolutionAction {
    MarkResolved,
    ResetToDraft,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiCopilotNotificationGroup {
    pub group_id: String,
    pub case_type: String,
    pub case_ids: Vec<String>,
    pub title: String,
    pub body: String,
}

// ── Internal serde payloads (not part of the public API) ────────────────

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct StoredWorkflowDispatchRequest {
    #[serde(default)]
    pub items: Vec<StoredWorkflowDispatchItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct StoredWorkflowDispatchItem {
    #[serde(default)]
    pub template_code: String,
    #[serde(default)]
    pub case_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct LlmDraftPayload {
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub actions: Vec<LlmDraftAction>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct LlmDraftAction {
    #[serde(default)]
    pub case_type: String,
    #[serde(default)]
    pub case_type_name: Option<String>,
    #[serde(default)]
    pub flight_number_raw: String,
    #[serde(default)]
    pub leg_type_hint: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub remarks: String,
    #[serde(default)]
    pub fields: Value,
    #[serde(default)]
    pub confidence: Option<f64>,
}
