//! Repository port for `ai_run_events` persistence.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::models::ai_job::AiRunEventRecord;

#[async_trait]
pub trait AiRunEventRepository: Send + Sync {
    async fn insert(
        &self,
        job_id: &str,
        run_id: &str,
        event_type: &str,
        payload: Option<Value>,
    ) -> Result<AiRunEventRecord, AiRunEventRepositoryError>;

    /// Insert without returning the row (audit recorder path).
    async fn insert_fire_and_forget(
        &self,
        job_id: &str,
        run_id: &str,
        event_type: &str,
        payload: Option<Value>,
    ) -> Result<(), AiRunEventRepositoryError>;

    async fn list_for_run(&self, run_id: &str, limit: i64) -> Result<Vec<AiRunEventRecord>, AiRunEventRepositoryError>;

    /// Count events for any of `job_ids` older than `older_than` (smoke dry-run).
    async fn count_by_job_ids_before(
        &self,
        job_ids: &[String],
        older_than: DateTime<Utc>,
    ) -> Result<i64, AiRunEventRepositoryError>;

    /// Delete events for any of `job_ids` older than `older_than` (smoke cleanup).
    ///
    /// Returns the number of rows deleted.
    async fn delete_by_job_ids_before(
        &self,
        job_ids: &[String],
        older_than: DateTime<Utc>,
    ) -> Result<u64, AiRunEventRepositoryError>;

    /// Count readiness-block events for smoke job id prefixes (rollout status).
    ///
    /// Equivalent to:
    /// `event_type = $1 AND (job_id LIKE 'smoke_job_%' OR job_id LIKE 'api_smoke_job_%')`.
    async fn count_smoke_readiness_blocks(&self, event_type: &str) -> Result<i64, AiRunEventRepositoryError>;
}

#[derive(Debug, Clone)]
pub enum AiRunEventRepositoryError {
    NotFound(String),
    Database(String),
    Validation(String),
}

impl AiRunEventRepositoryError {
    pub fn not_found(id: impl Into<String>) -> Self {
        Self::NotFound(id.into())
    }

    pub fn database(message: impl Into<String>) -> Self {
        Self::Database(message.into())
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }
}

impl std::fmt::Display for AiRunEventRepositoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "ai run event not found: {id}"),
            Self::Database(msg) => write!(f, "ai run event database error: {msg}"),
            Self::Validation(msg) => write!(f, "ai run event validation error: {msg}"),
        }
    }
}

impl std::error::Error for AiRunEventRepositoryError {}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn trait_is_object_safe() {
        fn assert_object_safe(_: &dyn AiRunEventRepository) {}

        struct Stub;
        #[async_trait]
        impl AiRunEventRepository for Stub {
            async fn insert(
                &self,
                job_id: &str,
                run_id: &str,
                event_type: &str,
                payload: Option<Value>,
            ) -> Result<AiRunEventRecord, AiRunEventRepositoryError> {
                Ok(AiRunEventRecord {
                    event_id: 1,
                    job_id: job_id.into(),
                    run_id: run_id.into(),
                    event_type: event_type.into(),
                    payload,
                    created_at: Utc::now(),
                })
            }
            async fn insert_fire_and_forget(
                &self,
                _job_id: &str,
                _run_id: &str,
                _event_type: &str,
                _payload: Option<Value>,
            ) -> Result<(), AiRunEventRepositoryError> {
                Ok(())
            }
            async fn list_for_run(
                &self,
                _run_id: &str,
                _limit: i64,
            ) -> Result<Vec<AiRunEventRecord>, AiRunEventRepositoryError> {
                Ok(vec![])
            }
            async fn count_by_job_ids_before(
                &self,
                _job_ids: &[String],
                _older_than: DateTime<Utc>,
            ) -> Result<i64, AiRunEventRepositoryError> {
                Ok(0)
            }
            async fn delete_by_job_ids_before(
                &self,
                _job_ids: &[String],
                _older_than: DateTime<Utc>,
            ) -> Result<u64, AiRunEventRepositoryError> {
                Ok(0)
            }
            async fn count_smoke_readiness_blocks(&self, _event_type: &str) -> Result<i64, AiRunEventRepositoryError> {
                Ok(0)
            }
        }

        assert_object_safe(&Stub);
    }
}
