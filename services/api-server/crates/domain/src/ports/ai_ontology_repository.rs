use async_trait::async_trait;

use crate::ontology::governed::ActionOverlay;

#[async_trait]
pub trait AiOntologyRepository {
    /// DB 只能交回对代码 schema 已知动作键的覆盖，无法整份替换 schema。
    /// 要拿到 `OntologySchema` 必须经过 `load_governed_schema(&overlays)`。
    async fn load_action_overlays(&self) -> Result<Vec<ActionOverlay>, AiOntologyRepositoryError>;

    // Overlay 写（治理 G4 / PR6：定义页配置中心改启用/风险/审批）。
    // 只能覆盖代码 schema 已知的 (object, action) 键，无法新增对象/字段/动作清单。
    async fn save_action_overlay(&self, overlay: &ActionOverlay) -> Result<(), AiOntologyRepositoryError>;
    async fn delete_action_overlay(&self, object: &str, action: &str) -> Result<(), AiOntologyRepositoryError>;

    // Count methods (no raw SQL in application services)
    async fn count_active_objects(&self) -> Result<i64, AiOntologyRepositoryError>;
    async fn count_active_write_actions(&self) -> Result<i64, AiOntologyRepositoryError>;
}

#[derive(Debug, Clone)]
pub enum AiOntologyRepositoryError {
    Database(String),
    Validation(String),
}

impl std::fmt::Display for AiOntologyRepositoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(message) => write!(f, "database error: {}", message),
            Self::Validation(message) => write!(f, "validation error: {}", message),
        }
    }
}

impl std::error::Error for AiOntologyRepositoryError {}
