//! 异常监控仓储 trait

use crate::error::DomainError;
use crate::models::anomaly::{Anomaly, AnomalyRule, AnomalyStatus};
use async_trait::async_trait;

/// 异常仓储接口
#[async_trait]
pub trait AnomalyRepository {
    async fn find_by_id(&self, anomaly_id: &str) -> Result<Option<Anomaly>, DomainError>;
    async fn find_by_flight(&self, flight_id: &str) -> Result<Vec<Anomaly>, DomainError>;
    async fn find_by_status(&self, status: AnomalyStatus) -> Result<Vec<Anomaly>, DomainError>;
    async fn list_rules(&self, enabled_only: bool) -> Result<Vec<AnomalyRule>, DomainError>;
    async fn get_rule(&self, rule_id: &str) -> Result<Option<AnomalyRule>, DomainError>;
    async fn upsert_rule(&self, rule: &AnomalyRule) -> Result<AnomalyRule, DomainError>;
    async fn save(&self, anomaly: &Anomaly) -> Result<(), DomainError>;
    async fn update(&self, anomaly: &Anomaly) -> Result<bool, DomainError>;
    async fn acknowledge(&self, anomaly_id: &str) -> Result<bool, DomainError>;
    async fn resolve(&self, anomaly_id: &str) -> Result<bool, DomainError>;
    async fn escalate(&self, anomaly_id: &str) -> Result<bool, DomainError>;
}

#[async_trait]
pub trait AnomalyTransactionalRepository<Tx>: Send + Sync {
    async fn acknowledge_in_tx(&self, tx: &mut Tx, anomaly_id: &str) -> Result<bool, DomainError>;
    async fn escalate_in_tx(&self, tx: &mut Tx, anomaly_id: &str) -> Result<bool, DomainError>;
}
