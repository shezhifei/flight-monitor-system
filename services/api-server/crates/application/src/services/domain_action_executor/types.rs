use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DomainActionError {
    #[error("action not found: {0}")]
    NotFound(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("execution failed: {0}")]
    Execution(String),
    #[error("{0}")]
    Internal(String),
}

#[derive(Debug, Clone)]
pub struct DomainActionReceipt {
    pub action_name: String,
    pub object_type: String,
    pub object_id: String,
    pub result: Value,
    pub executed_at: chrono::DateTime<chrono::Utc>,
    pub executor_id: String,
}
