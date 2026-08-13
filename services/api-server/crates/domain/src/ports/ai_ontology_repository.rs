use async_trait::async_trait;

use crate::models::ai_ontology::OntologySchema;

#[async_trait]
pub trait AiOntologyRepository {
    async fn load_active_schema(&self) -> Result<Option<OntologySchema>, AiOntologyRepositoryError>;

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
