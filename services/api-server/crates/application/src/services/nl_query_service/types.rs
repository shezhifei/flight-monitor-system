use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde_json::{json, Value};
use thiserror::Error;
use tokio::sync::mpsc;

#[derive(Debug, Error)]
pub enum NLQueryServiceError {
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Internal(String),
}

#[derive(Debug, Clone)]
pub struct NLQueryRuntimeContext {
    pub request_id: String,
    pub scene: String,
    pub event_sender: Option<mpsc::Sender<NLQueryStreamEvent>>,
}

#[derive(Debug, Clone)]
pub struct NLQueryStreamEvent {
    pub event: String,
    pub payload: Value,
}

impl NLQueryRuntimeContext {
    pub(super) fn emit(&self, event: &str, payload: Value) {
        if let Some(sender) = &self.event_sender {
            // Use try_send for bounded channel backpressure;
            // drop the event if the channel is full to prevent memory overflow
            let _ = sender.try_send(NLQueryStreamEvent {
                event: event.to_string(),
                payload,
            });
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ConversationMessage {
    pub(super) role: String,
    pub(super) content_raw: Value,
    pub(super) name: Option<String>,
    pub(super) tool_calls: Option<Vec<Value>>,
    pub(super) tool_call_id: Option<String>,
    pub(super) metadata: Option<Value>,
}

#[derive(Debug, Clone)]
pub(super) struct ConversationRecord {
    pub(super) conversation_id: String,
    pub(super) user_id: String,
    pub(super) title: Option<String>,
    pub(super) status: String,
    pub(super) model: Option<String>,
    pub(super) tags: Vec<String>,
    pub(super) messages: Vec<ConversationMessage>,
    pub(super) created_at: DateTime<Utc>,
    pub(super) updated_at: DateTime<Utc>,
    pub(super) last_activity_at: DateTime<Utc>,
    pub(super) ended_at: Option<DateTime<Utc>>,
}

#[derive(Default)]
pub(super) struct NLQueryState {
    pub(super) conversations: DashMap<String, ConversationRecord>,
}

pub(super) struct QueryAnalysis {
    pub(super) interpretation: String,
    pub(super) structured_data: Value,
    pub(super) visualization_hint: Option<String>,
    pub(super) summary: String,
    pub(super) tool_calls: Option<Vec<Value>>,
    pub(super) metadata: Option<Value>,
    pub(super) runtime_event: Option<RuntimeQueryEvent>,
}

#[derive(Debug, Clone)]
pub(super) struct RuntimeQueryEvent {
    pub(super) execution_id: String,
    pub(super) tool_call_id: String,
    pub(super) tool_name: String,
    pub(super) arguments: Value,
    pub(super) result: Value,
    pub(super) status: String,
    pub(super) duration_ms: Option<i64>,
}

impl RuntimeQueryEvent {
    pub(super) fn assistant_tool_calls(&self) -> Vec<Value> {
        vec![json!({
            "id": self.tool_call_id,
            "type": "function",
            "function": {
                "name": self.tool_name,
                "arguments": serde_json::to_string(&self.arguments).unwrap_or_else(|_| "{}".to_string()),
            }
        })]
    }
}
