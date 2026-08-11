use crate::error::DomainError;
use serde_json::Value;

#[async_trait::async_trait]
pub trait FlightSyncRepository: Send + Sync {
    async fn find_latest(&self, source_system: &str) -> Result<Option<Value>, DomainError>;

    async fn create_run(
        &self,
        run_id: &str,
        source_system: &str,
        trigger: &str,
        direction: &str,
        window_start: chrono::NaiveDate,
        window_end: chrono::NaiveDate,
        status: &str,
        started_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), DomainError>;

    async fn mark_completed(
        &self,
        run_id: &str,
        processed_count: i32,
        success_count: i32,
        failure_count: i32,
        created_count: i32,
        updated_count: i32,
        failure_samples: &[Value],
        error_summary: &[Value],
        completed_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), DomainError>;

    async fn mark_failed(
        &self,
        run_id: &str,
        failure_count: i32,
        failure_samples: &[Value],
        error_summary: &[Value],
        completed_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), DomainError>;

    async fn load_payload(&self, run_id: &str) -> Result<Value, DomainError>;
}
