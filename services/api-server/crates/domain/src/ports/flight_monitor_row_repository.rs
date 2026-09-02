use crate::error::DomainError;
use crate::models::flight_monitor_row::FlightMonitorRow;
use async_trait::async_trait;
use chrono::NaiveDate;

#[derive(Debug, Clone, Default)]
pub struct FlightMonitorRowQuery {
    pub workspace_date: Option<NaiveDate>,
    pub query: Option<String>,
    pub status: Option<String>,
    pub origin: Option<String>,
    pub destination: Option<String>,
    pub has_open_anomaly: Option<bool>,
}

#[async_trait]
pub trait FlightMonitorRowRepository: Send + Sync {
    async fn list(
        &self,
        workspace_date: Option<NaiveDate>,
        query: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<FlightMonitorRow>, DomainError>;
    async fn count(&self, workspace_date: Option<NaiveDate>, query: Option<&str>) -> Result<i64, DomainError>;
    async fn upsert(&self, row: &FlightMonitorRow) -> Result<(), DomainError>;

    /// Retire active projections that reference a deleted directional flight.
    /// Physical rows remain available for audit/history.
    async fn deactivate_flight(&self, _flight_id: &str) -> Result<(), DomainError> {
        Ok(())
    }

    /// Recompute the anomaly indicator from durable anomaly rows.
    async fn refresh_anomaly_flag(&self, _flight_id: &str) -> Result<(), DomainError> {
        Ok(())
    }

    /// Merge two single-sided rows into the stable inbound monitor row.
    async fn merge_turnaround(
        &self,
        _link_id: &str,
        _inbound_flight_id: &str,
        _outbound_flight_id: &str,
    ) -> Result<(), DomainError> {
        Ok(())
    }

    async fn search(
        &self,
        criteria: &FlightMonitorRowQuery,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<FlightMonitorRow>, DomainError> {
        self.list(criteria.workspace_date, criteria.query.as_deref(), limit, offset)
            .await
    }

    async fn count_filtered(&self, criteria: &FlightMonitorRowQuery) -> Result<i64, DomainError> {
        self.count(criteria.workspace_date, criteria.query.as_deref()).await
    }
}

/// 在调用方事务中写入监控宽行。
///
/// 该端口与航班事务仓储配对使用，确保航班主表、outbox 和宽表投影原子提交。
#[async_trait]
pub trait FlightMonitorRowTransactionalRepository<Tx>: Send + Sync {
    async fn upsert_in_tx(&self, tx: &mut Tx, row: &FlightMonitorRow) -> Result<(), DomainError>;

    /// Clear a directional reference in the same transaction as the flight
    /// delete; rows with no remaining side are soft-retired.
    async fn deactivate_flight_in_tx(&self, _tx: &mut Tx, _flight_id: &str) -> Result<(), DomainError> {
        Ok(())
    }

    /// Merge an inbound/outbound pair into the inbound monitor row without
    /// changing its stable `row_id`. The outbound single-sided row is
    /// soft-retired so historical keys remain non-destructive.
    async fn merge_turnaround_in_tx(
        &self,
        tx: &mut Tx,
        link_id: &str,
        inbound_flight_id: &str,
        outbound_flight_id: &str,
    ) -> Result<(), DomainError>;

    /// Break a turnaround link while retaining the inbound row id and
    /// reactivating/creating the outbound single-sided row.
    async fn break_turnaround_in_tx(
        &self,
        tx: &mut Tx,
        link_id: &str,
        inbound_flight_id: &str,
        outbound_flight_id: &str,
    ) -> Result<(), DomainError>;
}
