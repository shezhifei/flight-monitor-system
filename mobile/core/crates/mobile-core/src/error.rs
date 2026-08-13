//! Unified error type for mobile-core.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("GET/HEAD requests must not carry a body")]
    BodyNotAllowed,

    #[error("network error: {0}")]
    Network(String),

    #[error("authentication required: {0}")]
    Auth(String),

    #[error("api error (request_id={request_id:?}): {message}")]
    Api {
        message: String,
        request_id: Option<String>,
    },

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("offline store error: {0}")]
    OfflineStore(String),
}

impl From<serde_json::Error> for CoreError {
    fn from(e: serde_json::Error) -> Self {
        CoreError::Serialization(e.to_string())
    }
}
