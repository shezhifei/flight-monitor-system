use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightImportSourceFileSchema {
    pub filename: String,
    pub size: usize,
    pub checksum_sha256: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FlightImportSummarySchema {
    pub total_rows: usize,
    pub valid_rows: usize,
    pub invalid_rows: usize,
    pub create_count: usize,
    pub update_count: usize,
    pub skip_count: usize,
    #[serde(default)]
    pub failed_count: usize,
    pub warning_count: usize,
    pub error_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightImportTimelineEventSchema {
    pub milestone_code: String,
    pub occurred_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leg_type: Option<String>,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightImportPreviewRowSchema {
    pub source_row_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_strategy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_flight_id: Option<String>,
    pub action: String,
    #[serde(default)]
    pub normalized_flight: Value,
    #[serde(default)]
    pub timeline_events: Vec<FlightImportTimelineEventSchema>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightImportPreviewDataSchema {
    pub preview_id: String,
    pub airport_context: Value,
    pub source_file: FlightImportSourceFileSchema,
    pub summary: FlightImportSummarySchema,
    #[serde(default)]
    pub rows: Vec<FlightImportPreviewRowSchema>,
    #[serde(default)]
    pub errors: Vec<String>,
    pub mapping_version: String,
    pub status: String,
    #[serde(default)]
    pub field_mapping: Value,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub source_system: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightImportCommitResultSchema {
    pub preview_id: String,
    pub airport_context: Value,
    pub source_file: FlightImportSourceFileSchema,
    pub summary: FlightImportSummarySchema,
    #[serde(default)]
    pub rows: Vec<FlightImportPreviewRowSchema>,
    #[serde(default)]
    pub errors: Vec<String>,
    pub mapping_version: String,
    pub status: String,
    #[serde(default)]
    pub flight_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub committed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub field_mapping: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_system: Option<String>,
}
