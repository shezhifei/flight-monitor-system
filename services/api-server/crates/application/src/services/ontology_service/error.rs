//! 本体 V1 服务错误

use std::fmt;

#[derive(Debug)]
pub enum OntologyError {
    Validation(String),
    NotFound(String),
    Conflict(String),
    Forbidden(String),
    Internal(String),
}

impl fmt::Display for OntologyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(message) => write!(f, "validation error: {message}"),
            Self::NotFound(message) => write!(f, "not found: {message}"),
            Self::Conflict(message) => write!(f, "conflict: {message}"),
            Self::Forbidden(message) => write!(f, "forbidden: {message}"),
            Self::Internal(message) => write!(f, "internal error: {message}"),
        }
    }
}

impl std::error::Error for OntologyError {}

impl OntologyError {
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::Forbidden(message.into())
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }
}

impl From<fms_domain::error::DomainError> for OntologyError {
    fn from(error: fms_domain::error::DomainError) -> Self {
        match error {
            fms_domain::error::DomainError::ValidationError(message)
            | fms_domain::error::DomainError::BusinessRuleViolation(message) => Self::Validation(message),
            fms_domain::error::DomainError::BusinessRuleViolationWithDetails { message, .. } => {
                Self::Validation(message)
            }
            fms_domain::error::DomainError::NotFound { entity_type, id } => {
                Self::NotFound(format!("{entity_type} {id} not found"))
            }
            fms_domain::error::DomainError::Conflict(message)
            | fms_domain::error::DomainError::ConcurrencyConflict(message) => Self::Conflict(message),
            fms_domain::error::DomainError::PermissionDenied(message) => Self::Forbidden(message),
            other => Self::Internal(other.to_string()),
        }
    }
}
