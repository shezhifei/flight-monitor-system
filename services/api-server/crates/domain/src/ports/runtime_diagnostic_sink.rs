use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub trait RuntimeDiagnosticSink: Send + Sync {
    async fn insert(&self, topic: &str, event_type: &str, payload: Value, correlation_id: Option<String>);
}
