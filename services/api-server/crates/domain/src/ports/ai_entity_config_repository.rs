//! AI 实体配置仓储接口

use crate::error::DomainError;
use crate::models::ai_entity_config::AiEntityConfigRecord;
use async_trait::async_trait;

#[async_trait]
pub trait AiEntityConfigRepository {
    async fn find_all(&self) -> Result<Vec<AiEntityConfigRecord>, DomainError>;
    async fn find_by_id(&self, id: &str) -> Result<Option<AiEntityConfigRecord>, DomainError>;
    async fn save(&self, id: &str, config: &serde_json::Value) -> Result<AiEntityConfigRecord, DomainError>;
    async fn delete(&self, id: &str) -> Result<bool, DomainError>;
}
