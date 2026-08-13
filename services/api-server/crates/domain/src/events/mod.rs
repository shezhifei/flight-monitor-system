//! 领域事件定义

use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct DomainEventOutboxRow {
    pub event_id: String,
    pub aggregate_type: Option<String>,
    pub aggregate_id: Option<String>,
    pub event_type: Option<String>,
    pub payload: Value,
    pub occurred_at: DateTime<Utc>,
    pub publish_attempts: i32,
    pub source_change_id: Option<String>,
}
