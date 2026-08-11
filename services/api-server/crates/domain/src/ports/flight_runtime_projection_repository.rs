use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::error::DomainError;

#[derive(Debug, Clone)]
pub struct FlightRuntimeProjection {
    pub flight_id: String,
    pub timeline_snapshot: HashMap<String, DateTime<Utc>>,
    pub business_cases: Vec<Value>,
}

#[async_trait]
pub trait FlightRuntimeProjectionRepository: Send + Sync {
    async fn find_by_flight_ids(
        &self,
        flight_ids: &[String],
    ) -> Result<HashMap<String, FlightRuntimeProjection>, DomainError>;

    async fn rebuild_for_flight(&self, flight_id: &str) -> Result<(), DomainError>;

    async fn delete_for_flight(&self, flight_id: &str) -> Result<(), DomainError>;

    async fn invalidate_flight(&self, flight_id: &str);

    async fn rebuild_recent(&self, limit: i64) -> Result<usize, DomainError>;
}
