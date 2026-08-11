use crate::services::ai_runtime_service::AiRuntimeError;
use fms_domain::ports::ai_proposal_repository::AiProposalRepositoryError;
#[derive(Debug, Clone)]
pub enum AiActionProposalError {
    NotFound(String),
    Validation(String),
    Conflict(String),
    Execution(String),
    Repository(String),
    Forbidden(String),
}

impl AiActionProposalError {
    pub fn not_found(id: impl Into<String>) -> Self {
        Self::NotFound(id.into())
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }

    pub fn execution(message: impl Into<String>) -> Self {
        Self::Execution(message.into())
    }

    pub fn repository(message: impl Into<String>) -> Self {
        Self::Repository(message.into())
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::Forbidden(message.into())
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Repository(message.into())
    }
}

impl std::fmt::Display for AiActionProposalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "proposal not found: {}", id),
            Self::Validation(msg) => write!(f, "validation error: {}", msg),
            Self::Conflict(msg) => write!(f, "conflict: {}", msg),
            Self::Execution(msg) => write!(f, "execution error: {}", msg),
            Self::Repository(msg) => write!(f, "repository error: {}", msg),
            Self::Forbidden(msg) => write!(f, "forbidden: {}", msg),
        }
    }
}

impl std::error::Error for AiActionProposalError {}

impl From<AiProposalRepositoryError> for AiActionProposalError {
    fn from(err: AiProposalRepositoryError) -> Self {
        match err {
            AiProposalRepositoryError::NotFound(id) => Self::NotFound(id),
            AiProposalRepositoryError::Validation(msg) => Self::Validation(msg),
            AiProposalRepositoryError::Database(msg) => Self::Repository(msg),
        }
    }
}

impl From<AiRuntimeError> for AiActionProposalError {
    fn from(err: AiRuntimeError) -> Self {
        match err {
            AiRuntimeError::NotFound(id) => Self::NotFound(id),
            AiRuntimeError::Validation(msg) => Self::Validation(msg),
            AiRuntimeError::Conflict { code, message, .. } => Self::Conflict(format!("{}: {}", code, message)),
        }
    }
}
