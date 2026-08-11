use async_trait::async_trait;
use serde_json::Value;

use crate::error::DomainError;

#[async_trait]
pub trait RuntimeDiagnosticRepository: Send + Sync + 'static {
    async fn fetch_recent(&self, topic: &str, limit: i64) -> Result<Vec<Value>, DomainError>;
    async fn count_by_topic(&self, topic: &str) -> Result<i64, DomainError>;
    async fn ping(&self) -> Result<bool, DomainError>;
}
