use async_trait::async_trait;
use thiserror::Error;

use crate::api::{
    ReceivedMessage, ValidatedAckRequest, ValidatedPublishRequest, ValidatedReceiveRequest,
};

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("message queue backend unavailable: {0}")]
    Unavailable(String),
    #[error("message not found or already acknowledged: {0}")]
    UnknownReceipt(String),
    #[error("message queue backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait MessageTransport: Send + Sync {
    async fn publish(&self, request: ValidatedPublishRequest) -> Result<String, TransportError>;

    async fn receive(
        &self,
        request: ValidatedReceiveRequest,
    ) -> Result<Vec<ReceivedMessage>, TransportError>;

    async fn ack(&self, request: ValidatedAckRequest) -> Result<(), TransportError>;

    async fn health(&self) -> Result<(), TransportError>;
}
