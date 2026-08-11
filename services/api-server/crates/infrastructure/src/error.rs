//! 基础设施层错误定义

use fms_domain::error::DomainError;
use std::error::Error as StdError;
use thiserror::Error;

pub type InfraErrorSource = Box<dyn StdError + Send + Sync + 'static>;

/// 基础设施错误
#[derive(Debug, Error)]
pub enum InfraError {
    #[error("数据库错误: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Redis 错误: {0}")]
    Redis(#[from] redis::RedisError),

    #[error("Redis 连接池错误: {0}")]
    RedisPool(#[source] bb8::RunError<redis::RedisError>),

    #[error("配置错误: {0}")]
    Config(String),

    #[error("认证错误: {0}")]
    Auth(String),

    #[error("外部服务错误({service}): {message}")]
    ExternalService {
        service: String,
        message: String,
        #[source]
        source: Option<InfraErrorSource>,
    },

    #[error("序列化错误: {0}")]
    Serialization(String),
}

impl InfraError {
    pub fn external_service(service: impl Into<String>, source: impl StdError + Send + Sync + 'static) -> Self {
        Self::ExternalService {
            service: service.into(),
            message: source.to_string(),
            source: Some(Box::new(source)),
        }
    }

    pub fn external_service_message(service: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ExternalService {
            service: service.into(),
            message: message.into(),
            source: None,
        }
    }
}

impl From<bb8::RunError<redis::RedisError>> for InfraError {
    fn from(error: bb8::RunError<redis::RedisError>) -> Self {
        Self::RedisPool(error)
    }
}

impl From<InfraError> for DomainError {
    fn from(error: InfraError) -> Self {
        match &error {
            InfraError::Auth(message) => {
                tracing::error!(error = %message, "authentication error in infrastructure layer");
                DomainError::Unauthorized(message.clone())
            }
            InfraError::Config(message) => {
                tracing::error!(error = %message, "configuration error in infrastructure layer");
                DomainError::Internal("基础设施配置错误".to_string())
            }
            InfraError::Database(err) => {
                tracing::error!(error = %err, "database error in infrastructure layer");
                DomainError::Internal("数据存储错误".to_string())
            }
            InfraError::Redis(err) => {
                tracing::error!(error = %err, "redis error in infrastructure layer");
                DomainError::Internal("缓存服务不可用".to_string())
            }
            InfraError::RedisPool(err) => {
                tracing::error!(error = %err, "redis pool error in infrastructure layer");
                DomainError::Internal("缓存服务不可用".to_string())
            }
            InfraError::ExternalService { service, message, .. } => {
                tracing::error!(service = %service, error = %message, "external service error in infrastructure layer");
                DomainError::Internal("外部服务不可用".to_string())
            }
            InfraError::Serialization(message) => {
                tracing::error!(error = %message, "serialization error in infrastructure layer");
                DomainError::Internal("数据序列化错误".to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::InfraError;
    use fms_domain::error::DomainError;
    use std::error::Error;
    use std::io;

    #[test]
    fn redis_error_preserves_original_source() {
        let redis_error = redis::RedisError::from((redis::ErrorKind::TypeError, "invalid cached payload"));

        let infra_error = InfraError::from(redis_error);

        assert!(
            infra_error.source().is_some(),
            "redis errors should retain the original error as source"
        );
    }

    #[test]
    fn external_service_error_preserves_original_source() {
        let source = io::Error::new(io::ErrorKind::ConnectionRefused, "connection refused by 10.0.0.42:8080");

        let infra_error = InfraError::external_service("flowable", source);

        assert!(
            infra_error.source().is_some(),
            "external service errors should retain the original error as source"
        );
    }

    #[test]
    fn infra_error_converts_to_sanitized_domain_error() {
        let source = io::Error::new(io::ErrorKind::ConnectionRefused, "connection refused by 10.0.0.42:8080");
        let infra_error = InfraError::external_service("flowable", source);

        let domain_error = DomainError::from(infra_error);

        match domain_error {
            DomainError::Internal(message) => {
                assert!(!message.contains("10.0.0.42"));
                assert!(!message.contains("connection refused"));
            }
            other => panic!("expected sanitized internal domain error, got {other:?}"),
        }
    }
}
