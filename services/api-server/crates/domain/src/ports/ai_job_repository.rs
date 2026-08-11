//! Repository port for `ai_jobs` persistence.
//!
//! Covers CRUD, concurrent claim (`FOR UPDATE SKIP LOCKED`), status
//! transitions, and status aggregates. Claim semantics must stay
//! identical to the historical application-layer SQL.

use async_trait::async_trait;

use crate::models::ai_job::{AiJobRecord, AiJobStatusCount};

#[async_trait]
pub trait AiJobRepository: Send + Sync {
    async fn insert(
        &self,
        job_id: &str,
        job_type: &str,
        requester_user_id: Option<&str>,
        correlation_id: Option<&str>,
        ontology_version: Option<&str>,
        risk_ceiling: Option<&str>,
    ) -> Result<AiJobRecord, AiJobRepositoryError>;

    async fn find_by_id(&self, job_id: &str) -> Result<Option<AiJobRecord>, AiJobRepositoryError>;

    async fn list(
        &self,
        status_filter: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AiJobRecord>, AiJobRepositoryError>;

    /// Update status and lifecycle timestamps (started/finished/cancelled).
    async fn update_status(&self, job_id: &str, new_status: &str) -> Result<AiJobRecord, AiJobRepositoryError>;

    async fn set_error_message(&self, job_id: &str, error_message: &str) -> Result<(), AiJobRepositoryError>;

    /// Atomically claim the oldest pending job (optionally filtered by type).
    ///
    /// Implementations MUST use `FOR UPDATE SKIP LOCKED` so concurrent
    /// workers never claim the same row.
    async fn claim_pending(&self, job_type: Option<&str>) -> Result<Option<AiJobRecord>, AiJobRepositoryError>;

    /// Claim a pending job and set a lease with heartbeat semantics.
    ///
    /// Like `claim_pending` but additionally sets `lease_owner`,
    /// `lease_expires_at = now() + lease_seconds`, `last_heartbeat_at = now()`,
    /// and increments `attempt_count`. Returns the leased job or `None` if
    /// no pending job matched.
    async fn lease_pending(
        &self,
        job_type: Option<&str>,
        lease_owner: &str,
        lease_seconds: i64,
    ) -> Result<Option<AiJobRecord>, AiJobRepositoryError>;

    /// List jobs whose lease has expired (reaper query).
    ///
    /// Returns claimed/running jobs where `lease_expires_at < now` and
    /// `attempt_count < max_attempts`. Ordered by `lease_expires_at ASC`.
    async fn list_expired_leases(
        &self,
        now: chrono::DateTime<chrono::Utc>,
        limit: i64,
    ) -> Result<Vec<AiJobRecord>, AiJobRepositoryError>;

    /// Extend the lease on a job owned by `lease_owner`.
    ///
    /// Updates `lease_expires_at = now() + lease_seconds` and
    /// `last_heartbeat_at = now()`. Returns `false` if the job is not
    /// currently leased by `lease_owner` (e.g. lease already expired and
    /// was taken over).
    async fn heartbeat(
        &self,
        job_id: &str,
        lease_owner: &str,
        lease_seconds: i64,
    ) -> Result<bool, AiJobRepositoryError>;

    /// Take over a job whose lease has expired.
    ///
    /// Atomically transitions the job from its current owner to
    /// `new_owner` with a fresh lease. Returns the job if takeover
    /// succeeded, or `None` if the job is no longer in a claimable state.
    async fn take_over(
        &self,
        job_id: &str,
        new_owner: &str,
        lease_seconds: i64,
    ) -> Result<Option<AiJobRecord>, AiJobRepositoryError>;

    /// Status → count aggregates for dashboards (`GROUP BY status`).
    async fn count_by_status(&self) -> Result<Vec<AiJobStatusCount>, AiJobRepositoryError>;
}

#[derive(Debug, Clone)]
pub enum AiJobRepositoryError {
    NotFound(String),
    Database(String),
    Validation(String),
}

impl AiJobRepositoryError {
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

impl std::fmt::Display for AiJobRepositoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "ai job not found: {id}"),
            Self::Database(msg) => write!(f, "ai job database error: {msg}"),
            Self::Validation(msg) => write!(f, "ai job validation error: {msg}"),
        }
    }
}

impl std::error::Error for AiJobRepositoryError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ai_job::AiJobRecord;
    use chrono::Utc;

    #[test]
    fn trait_is_object_safe() {
        fn assert_object_safe(_: &dyn AiJobRepository) {}

        struct Stub;
        #[async_trait]
        impl AiJobRepository for Stub {
            async fn insert(
                &self,
                job_id: &str,
                job_type: &str,
                _requester_user_id: Option<&str>,
                _correlation_id: Option<&str>,
                _ontology_version: Option<&str>,
                _risk_ceiling: Option<&str>,
            ) -> Result<AiJobRecord, AiJobRepositoryError> {
                Ok(AiJobRecord {
                    job_id: job_id.into(),
                    job_type: job_type.into(),
                    status: "pending".into(),
                    requester_user_id: None,
                    ontology_version: None,
                    context_policy: None,
                    risk_ceiling: None,
                    correlation_id: None,
                    created_at: Utc::now(),
                    started_at: None,
                    finished_at: None,
                    cancelled_at: None,
                    error_code: None,
                    error_message: None,
                    timeout_ms: None,
                    lease_owner: None,
                    lease_expires_at: None,
                    last_heartbeat_at: None,
                    attempt_count: 0,
                    max_attempts: 1,
                    expires_at: None,
                })
            }
            async fn find_by_id(&self, _job_id: &str) -> Result<Option<AiJobRecord>, AiJobRepositoryError> {
                Ok(None)
            }
            async fn list(
                &self,
                _status_filter: Option<&str>,
                _limit: i64,
                _offset: i64,
            ) -> Result<Vec<AiJobRecord>, AiJobRepositoryError> {
                Ok(vec![])
            }
            async fn update_status(&self, job_id: &str, new_status: &str) -> Result<AiJobRecord, AiJobRepositoryError> {
                Ok(AiJobRecord {
                    job_id: job_id.into(),
                    job_type: "t".into(),
                    status: new_status.into(),
                    requester_user_id: None,
                    ontology_version: None,
                    context_policy: None,
                    risk_ceiling: None,
                    correlation_id: None,
                    created_at: Utc::now(),
                    started_at: None,
                    finished_at: None,
                    cancelled_at: None,
                    error_code: None,
                    error_message: None,
                    timeout_ms: None,
                    lease_owner: None,
                    lease_expires_at: None,
                    last_heartbeat_at: None,
                    attempt_count: 0,
                    max_attempts: 1,
                    expires_at: None,
                })
            }
            async fn set_error_message(&self, _job_id: &str, _error_message: &str) -> Result<(), AiJobRepositoryError> {
                Ok(())
            }
            async fn claim_pending(
                &self,
                _job_type: Option<&str>,
            ) -> Result<Option<AiJobRecord>, AiJobRepositoryError> {
                Ok(None)
            }
            async fn lease_pending(
                &self,
                _job_type: Option<&str>,
                _lease_owner: &str,
                _lease_seconds: i64,
            ) -> Result<Option<AiJobRecord>, AiJobRepositoryError> {
                Ok(None)
            }
            async fn list_expired_leases(
                &self,
                _now: chrono::DateTime<chrono::Utc>,
                _limit: i64,
            ) -> Result<Vec<AiJobRecord>, AiJobRepositoryError> {
                Ok(vec![])
            }
            async fn heartbeat(
                &self,
                _job_id: &str,
                _lease_owner: &str,
                _lease_seconds: i64,
            ) -> Result<bool, AiJobRepositoryError> {
                Ok(false)
            }
            async fn take_over(
                &self,
                _job_id: &str,
                _new_owner: &str,
                _lease_seconds: i64,
            ) -> Result<Option<AiJobRecord>, AiJobRepositoryError> {
                Ok(None)
            }
            async fn count_by_status(&self) -> Result<Vec<AiJobStatusCount>, AiJobRepositoryError> {
                Ok(vec![])
            }
        }

        assert_object_safe(&Stub);
    }
}
