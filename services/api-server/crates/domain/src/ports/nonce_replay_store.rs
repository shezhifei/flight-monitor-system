use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NonceReplayDecision {
    FirstSeen,
    Replay,
}

#[derive(Debug, Error)]
pub enum NonceReplayStoreError {
    #[error("nonce replay store timeout")]
    Timeout,
    #[error("nonce replay store unavailable: {0}")]
    Unavailable(String),
}

#[async_trait]
pub trait NonceReplayStore: Send + Sync {
    async fn check_and_record(
        &self,
        session_hash: &str,
        timestamp: i64,
        nonce: &str,
    ) -> Result<NonceReplayDecision, NonceReplayStoreError>;
}
