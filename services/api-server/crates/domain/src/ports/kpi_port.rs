//! KPI aggregation data-access port.
//!
//! Wraps the read-side metrics previously exposed only via
//! `PgKpiRepository` so application services depend on a domain trait.

use async_trait::async_trait;
use chrono::NaiveDate;

use crate::error::DomainError;

/// Snapshot row for a KPI date-range aggregation.
#[derive(Debug, Clone, Default)]
pub struct KpiSnapshotMetrics {
    pub avg_turnaround_minutes: Option<f64>,
    pub p90_turnaround_minutes: Option<f64>,
    pub on_time_departure_rate: Option<f64>,
    pub on_time_arrival_rate: Option<f64>,
    pub service_node_compliance_rate: Option<f64>,
    pub abnormal_ratio: Option<f64>,
}

/// One point on a KPI trend series.
#[derive(Debug, Clone)]
pub struct KpiTrendPoint {
    pub flight_date: NaiveDate,
    pub metric_value: Option<f64>,
}

/// Service-node compliance rates for a single day.
#[derive(Debug, Clone, Default)]
pub struct KpiServiceNodeCompliance {
    pub cleaning_rate: Option<f64>,
    pub loading_rate: Option<f64>,
    pub boarding_rate: Option<f64>,
}

/// Turnaround duration histogram bucket.
#[derive(Debug, Clone)]
pub struct KpiTurnaroundBucket {
    pub bucket: String,
    pub count: i64,
}

/// Hourly flight volume bucket.
#[derive(Debug, Clone)]
pub struct KpiHourlyVolume {
    pub hour: i32,
    pub count: i64,
}

/// Daily anomaly count.
#[derive(Debug, Clone)]
pub struct KpiAnomalyCount {
    pub anomaly_date: NaiveDate,
    pub anomaly_count: i64,
}

/// Port for KPI aggregation queries.
#[async_trait]
pub trait KpiPort: Send + Sync {
    async fn fetch_snapshot_metrics(
        &self,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<KpiSnapshotMetrics, DomainError>;

    async fn fetch_kpi_trend(
        &self,
        metric_column: &str,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<KpiTrendPoint>, DomainError>;

    async fn fetch_service_node_compliance(
        &self,
        target_date: NaiveDate,
    ) -> Result<KpiServiceNodeCompliance, DomainError>;

    async fn fetch_turnaround_distribution(
        &self,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<KpiTurnaroundBucket>, DomainError>;

    async fn fetch_hourly_flight_volume(
        &self,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<KpiHourlyVolume>, DomainError>;

    async fn fetch_equipment_utilization_rate(&self) -> Result<Option<f64>, DomainError>;

    async fn fetch_anomaly_counts(
        &self,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<KpiAnomalyCount>, DomainError>;

    async fn refresh_daily_kpi_materialized_view(&self) -> Result<(), DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_is_object_safe() {
        fn assert_object_safe(_: &dyn KpiPort) {}

        struct Stub;
        #[async_trait]
        impl KpiPort for Stub {
            async fn fetch_snapshot_metrics(
                &self,
                _: NaiveDate,
                _: NaiveDate,
            ) -> Result<KpiSnapshotMetrics, DomainError> {
                Ok(KpiSnapshotMetrics::default())
            }
            async fn fetch_kpi_trend(
                &self,
                _: &str,
                _: NaiveDate,
                _: NaiveDate,
            ) -> Result<Vec<KpiTrendPoint>, DomainError> {
                Ok(vec![])
            }
            async fn fetch_service_node_compliance(
                &self,
                _: NaiveDate,
            ) -> Result<KpiServiceNodeCompliance, DomainError> {
                Ok(KpiServiceNodeCompliance::default())
            }
            async fn fetch_turnaround_distribution(
                &self,
                _: NaiveDate,
                _: NaiveDate,
            ) -> Result<Vec<KpiTurnaroundBucket>, DomainError> {
                Ok(vec![])
            }
            async fn fetch_hourly_flight_volume(
                &self,
                _: NaiveDate,
                _: NaiveDate,
            ) -> Result<Vec<KpiHourlyVolume>, DomainError> {
                Ok(vec![])
            }
            async fn fetch_equipment_utilization_rate(&self) -> Result<Option<f64>, DomainError> {
                Ok(None)
            }
            async fn fetch_anomaly_counts(
                &self,
                _: NaiveDate,
                _: NaiveDate,
            ) -> Result<Vec<KpiAnomalyCount>, DomainError> {
                Ok(vec![])
            }
            async fn refresh_daily_kpi_materialized_view(&self) -> Result<(), DomainError> {
                Ok(())
            }
        }

        assert_object_safe(&Stub);
    }
}
