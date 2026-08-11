//! 航班归档仓储接口。

use async_trait::async_trait;
use serde_json::Value;

use crate::error::DomainError;

#[async_trait]
pub trait FlightArchiveRepository {
    async fn find_archived_flights(&self, limit: i64, offset: i64) -> Result<Vec<Value>, DomainError>;

    async fn find_archived_flight_by_id(&self, flight_id: &str) -> Result<Option<Value>, DomainError>;

    async fn get_archive_stats(&self) -> Result<Value, DomainError>;

    async fn trigger_archive(&self, cutoff_date: Option<&str>, target_date: Option<&str>)
        -> Result<Value, DomainError>;
}
