//! 航班归档应用服务。

use std::sync::Arc;

use fms_domain::error::DomainError;
use fms_domain::ports::flight_archive_repository::FlightArchiveRepository;
use serde_json::Value;

pub struct FlightArchiveService {
    repo: Arc<dyn FlightArchiveRepository + Send + Sync>,
}

impl FlightArchiveService {
    pub fn new(repo: Arc<dyn FlightArchiveRepository + Send + Sync>) -> Self {
        Self { repo }
    }

    pub async fn find_archived_flights(&self, limit: i64, offset: i64) -> Result<Vec<Value>, DomainError> {
        self.repo.find_archived_flights(limit, offset).await
    }

    pub async fn find_archived_flight_by_id(&self, flight_id: &str) -> Result<Option<Value>, DomainError> {
        self.repo.find_archived_flight_by_id(flight_id).await
    }

    pub async fn get_archive_stats(&self) -> Result<Value, DomainError> {
        self.repo.get_archive_stats().await
    }

    pub async fn trigger_archive(
        &self,
        cutoff_date: Option<&str>,
        target_date: Option<&str>,
    ) -> Result<Value, DomainError> {
        self.repo.trigger_archive(cutoff_date, target_date).await
    }
}
