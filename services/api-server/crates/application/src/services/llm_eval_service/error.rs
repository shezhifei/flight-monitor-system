use thiserror::Error;

#[derive(Debug, Error)]
pub enum LLMEvalServiceError {
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Internal(String),
}
