//! DTOs for shift handover APIs.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use fms_domain::error::DomainError;

const MAX_SHIFT_CODE_LENGTH: usize = 32;
const MAX_USER_ID_LENGTH: usize = 26;
const MAX_SUMMARY_LENGTH: usize = 200;
const MAX_ITEM_TITLE_LENGTH: usize = 255;

#[derive(Debug, Clone, Deserialize)]
pub struct ShiftHandoverItemCreateRequest {
    #[serde(default = "default_other_item_type")]
    pub item_type: String,
    pub title: String,
    pub detail: Option<String>,
    pub owner_user_id: Option<String>,
    pub due_at: Option<DateTime<Utc>>,
    #[serde(default = "default_true")]
    pub is_mandatory: bool,
}

impl ShiftHandoverItemCreateRequest {
    pub fn validate(&self) -> Result<(), DomainError> {
        validate_item_type(&self.item_type)?;
        validate_required_max_length(&self.title, MAX_ITEM_TITLE_LENGTH, "handover item title")?;
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShiftHandoverCreateRequest {
    pub position_user_id: String,
    pub shift_date: NaiveDate,
    pub shift_code: String,
    pub from_user_id: Option<String>,
    pub to_user_id: String,
    pub summary: Option<String>,
    #[serde(default = "default_medium")]
    pub risk_level: String,
    #[serde(default)]
    pub items: Vec<ShiftHandoverItemCreateRequest>,
}

impl ShiftHandoverCreateRequest {
    pub fn validate(&self) -> Result<(), DomainError> {
        validate_required_max_length(&self.shift_code, MAX_SHIFT_CODE_LENGTH, "shift_code")?;
        validate_required_max_length(&self.position_user_id, MAX_USER_ID_LENGTH, "position_user_id")?;
        validate_required_max_length(&self.to_user_id, MAX_USER_ID_LENGTH, "to_user_id")?;
        validate_optional_max_length(self.summary.as_deref(), MAX_SUMMARY_LENGTH, "summary")?;
        for item in &self.items {
            item.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShiftHandoverItemAcknowledgeRequest {
    #[serde(default = "default_true")]
    pub acknowledged: bool,
}

/// 交接完成请求：核接班人密码后调 OccupySeat 切占用。
#[derive(Debug, Clone, Deserialize)]
pub struct ShiftHandoverCompleteRequest {
    pub password: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShiftHandoverItemResponse {
    pub item_id: String,
    pub handover_id: String,
    pub item_type: String,
    pub title: String,
    pub detail: Option<String>,
    pub owner_user_id: Option<String>,
    pub due_at: Option<DateTime<Utc>>,
    pub is_mandatory: bool,
    pub acknowledged: bool,
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub acknowledged_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShiftHandoverResponse {
    pub handover_id: String,
    pub shift_date: NaiveDate,
    pub shift_code: String,
    pub from_user_id: String,
    pub to_user_id: String,
    pub position_user_id: Option<String>,
    pub from_operator_name: Option<String>,
    pub from_operator_job_title: Option<String>,
    pub from_operator_label: Option<String>,
    pub to_operator_name: Option<String>,
    pub to_operator_job_title: Option<String>,
    pub to_operator_label: Option<String>,
    pub status: String,
    pub summary: Option<String>,
    pub risk_level: String,
    pub signed_at: Option<DateTime<Utc>>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub items: Vec<ShiftHandoverItemResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShiftHandoverCandidateResponse {
    pub user_id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub job_title: Option<String>,
    pub display_label: String,
}

fn default_true() -> bool {
    true
}

fn default_medium() -> String {
    "medium".to_string()
}

fn default_other_item_type() -> String {
    "other".to_string()
}

fn validate_item_type(value: &str) -> Result<(), DomainError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "pending_task" | "open_anomaly" | "risk_note" | "other" => Ok(()),
        _ => Err(DomainError::ValidationError("invalid item_type".to_string())),
    }
}

fn validate_required_max_length(value: &str, max_length: usize, field_name: &str) -> Result<(), DomainError> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(DomainError::ValidationError(format!("{field_name} is required")));
    }
    validate_max_length(normalized, max_length, field_name)
}

fn validate_optional_max_length(value: Option<&str>, max_length: usize, field_name: &str) -> Result<(), DomainError> {
    let Some(normalized) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    validate_max_length(normalized, max_length, field_name)
}

fn validate_max_length(value: &str, max_length: usize, field_name: &str) -> Result<(), DomainError> {
    if value.chars().count() <= max_length {
        Ok(())
    } else {
        Err(DomainError::ValidationError(format!(
            "{field_name} must be at most {max_length} characters"
        )))
    }
}
