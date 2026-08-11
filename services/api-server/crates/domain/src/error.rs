use thiserror::Error;

/// 领域层错误定义
#[derive(Debug, Error)]
pub enum DomainError {
    #[error("实体未找到: {entity_type} (id={id})")]
    NotFound { entity_type: &'static str, id: String },

    #[error("验证失败: {0}")]
    ValidationError(String),

    #[error("状态转换非法: 从 {from} 到 {to}")]
    InvalidStateTransition { from: String, to: String },

    #[error("业务规则违反: {0}")]
    BusinessRuleViolation(String),

    #[error("业务规则违反: {message}")]
    BusinessRuleViolationWithDetails {
        message: String,
        details: serde_json::Value,
    },

    #[error("权限不足: {0}")]
    PermissionDenied(String),

    #[error("认证失败: {0}")]
    Unauthorized(String),

    #[error("资源冲突: {0}")]
    Conflict(String),

    #[error("内部错误: {0}")]
    Internal(String),

    #[error("并发冲突: {0}")]
    ConcurrencyConflict(String),
}

impl DomainError {
    pub fn user_message(&self) -> &str {
        match self {
            Self::NotFound { .. } => "实体未找到",
            Self::ValidationError(message)
            | Self::BusinessRuleViolation(message)
            | Self::PermissionDenied(message)
            | Self::Unauthorized(message)
            | Self::Conflict(message)
            | Self::Internal(message)
            | Self::ConcurrencyConflict(message) => message,
            Self::BusinessRuleViolationWithDetails { message, .. } => message,
            Self::InvalidStateTransition { .. } => "状态转换非法",
        }
    }

    pub fn details(&self) -> Option<&serde_json::Value> {
        match self {
            Self::BusinessRuleViolationWithDetails { details, .. } => Some(details),
            _ => None,
        }
    }
}
