#[derive(Debug, thiserror::Error)]
pub enum OntologyActionError {
    #[error("invalid arguments: {0}")]
    InvalidArguments(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("repository error: {0}")]
    Repository(String),
    #[error("internal error: {0}")]
    Internal(String),
}

pub fn repo_err(error: impl std::fmt::Display) -> OntologyActionError {
    OntologyActionError::Repository(error.to_string())
}
