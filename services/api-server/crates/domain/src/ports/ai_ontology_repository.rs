use async_trait::async_trait;

use crate::ontology::governed::ActionOverlay;

#[async_trait]
pub trait AiOntologyRepository {
    /// DB 只能交回对代码 schema 已知动作键的覆盖，无法整份替换 schema。
    /// 要拿到 `OntologySchema` 必须经过 `load_governed_schema(&overlays)`。
    async fn load_action_overlays(&self) -> Result<Vec<ActionOverlay>, AiOntologyRepositoryError>;

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
