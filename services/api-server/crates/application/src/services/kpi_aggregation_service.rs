//! KPI 聚合服务。
//!
//! 对齐 Python `kpi_aggregation_service.py` 的主要读侧行为。

use chrono::{Datelike, Duration, NaiveDate, Utc};
use serde_json::{json, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tracing::warn;

use fms_domain::error::DomainError;
use fms_domain::ports::kpi_port::{KpiPort, KpiSnapshotMetrics};

const KPI_UPDATED_EVENT: &str = "kpi_updated";
// Match Python's refresh_cache ranges: today, this_week, this_month
const COMMON_CACHE_RANGES: &[&str] = &["today", "this_week", "this_month"];

pub trait KpiAggregationSsePublisher: Send + Sync {
    fn publish_kpi_updated<'a>(
        &'a self,
        payload: Value,
    ) -> Pin<Box<dyn Future<Output = Result<usize, DomainError>> + Send + 'a>>;
}

pub struct KpiAggregationService {
    repo: Arc<dyn KpiPort + Send + Sync>,
    sse_publisher: Option<Arc<dyn KpiAggregationSsePublisher + Send + Sync>>,
}

pub struct NoopSsePublisher;

impl KpiAggregationSsePublisher for NoopSsePublisher {
    fn publish_kpi_updated<'a>(
        &'a self,
        _payload: Value,
    ) -> Pin<Box<dyn Future<Output = Result<usize, DomainError>> + Send + 'a>> {
        Box::pin(async { Ok(0) })
    }
}

impl KpiAggregationService {
    pub fn new(repo: Arc<dyn KpiPort + Send + Sync>) -> Self {
        Self {
            repo,
            sse_publisher: None,
        }
    }
}

impl KpiAggregationService {
    pub fn with_sse_publisher(mut self, sse_publisher: Arc<dyn KpiAggregationSsePublisher + Send + Sync>) -> Self {
        self.sse_publisher = Some(sse_publisher);
        self
    }

    pub async fn get_kpi_snapshot(
        &self,
        time_range: &str,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> Result<Value, DomainError> {
        let (resolved_start, resolved_end) = resolve_date_range(time_range, start_date, end_date);
        let metrics = self.get_snapshot_metrics(resolved_start, resolved_end).await?;
        let trend = self.get_kpi_trend("on_time_rate", 7).await?;
        let distribution = self.get_turnaround_distribution(resolved_start, resolved_end).await?;
        let hourly = self.get_hourly_flight_volume(resolved_start, resolved_end).await?;
        let equipment_rate = self.get_equipment_utilization_rate().await?;

        Ok(json!({
            "calculated_at": Utc::now().to_rfc3339(),
            "time_range": time_range,
            "turnaround_time_avg_minutes": metric_f64(&metrics.avg_turnaround_minutes),
            "turnaround_time_p90_minutes": metric_f64(&metrics.p90_turnaround_minutes),
            "on_time_departure_rate": metric_f64(&metrics.on_time_departure_rate),
            "on_time_arrival_rate": metric_f64(&metrics.on_time_arrival_rate),
            "service_node_compliance_rate": metric_f64(&metrics.service_node_compliance_rate),
            "equipment_utilization_rate": equipment_rate,
            "abnormal_flight_ratio": metric_f64(&metrics.abnormal_ratio),
            "turnaround_distribution": distribution,
            "on_time_trend": trend,
            "hourly_flight_volume": hourly,
        }))
    }

    pub async fn get_kpi_trend(&self, metric: &str, days: i32) -> Result<Vec<Value>, DomainError> {
        let end_date = Utc::now().date_naive();
        let start_date = end_date - Duration::days((days.max(1) - 1) as i64);
        let metric_column = match metric {
            "turnaround" => "avg_turnaround_minutes",
            "abnormal_ratio" => "abnormal_ratio",
            _ => "on_time_departure_rate",
        };

        let rows = self.repo.fetch_kpi_trend(metric_column, start_date, end_date).await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                json!({
                    "date": row.flight_date.to_string(),
                    "value": row.metric_value,
                })
            })
            .collect())
    }

    pub async fn get_service_node_compliance(&self, target_date: NaiveDate) -> Result<Vec<Value>, DomainError> {
        let row = self.repo.fetch_service_node_compliance(target_date).await?;

        Ok(vec![
            json!({ "node": "cleaning", "rate": row.cleaning_rate.unwrap_or(0.0) }),
            json!({ "node": "loading", "rate": row.loading_rate.unwrap_or(0.0) }),
            json!({ "node": "boarding", "rate": row.boarding_rate.unwrap_or(0.0) }),
        ])
    }

    pub async fn compare_kpi(
        &self,
        base_range: (NaiveDate, NaiveDate),
        compare_range: (NaiveDate, NaiveDate),
    ) -> Result<Value, DomainError> {
        let base_metrics = self.get_snapshot_metrics(base_range.0, base_range.1).await?;
        let compare_metrics = self.get_snapshot_metrics(compare_range.0, compare_range.1).await?;

        let metric_keys = [
            (
                "avg_turnaround_minutes",
                base_metrics.avg_turnaround_minutes,
                compare_metrics.avg_turnaround_minutes,
            ),
            (
                "p90_turnaround_minutes",
                base_metrics.p90_turnaround_minutes,
                compare_metrics.p90_turnaround_minutes,
            ),
            (
                "on_time_departure_rate",
                base_metrics.on_time_departure_rate,
                compare_metrics.on_time_departure_rate,
            ),
            (
                "on_time_arrival_rate",
                base_metrics.on_time_arrival_rate,
                compare_metrics.on_time_arrival_rate,
            ),
            (
                "service_node_compliance_rate",
                base_metrics.service_node_compliance_rate,
                compare_metrics.service_node_compliance_rate,
            ),
            (
                "abnormal_ratio",
                base_metrics.abnormal_ratio,
                compare_metrics.abnormal_ratio,
            ),
        ];

        let mut metrics = serde_json::Map::new();
        for (key, base_opt, compare_opt) in metric_keys {
            let base_value = base_opt.unwrap_or(0.0);
            let compare_value = compare_opt.unwrap_or(0.0);
            let delta = compare_value - base_value;
            let change_rate = if base_value.abs() > f64::EPSILON {
                Some(delta / base_value)
            } else {
                None
            };
            metrics.insert(
                key.to_string(),
                json!({
                    "base": base_value,
                    "compare": compare_value,
                    "delta": delta,
                    "change_rate": change_rate,
                }),
            );
        }

        Ok(json!({
            "base_range": {
                "start_date": base_range.0.to_string(),
                "end_date": base_range.1.to_string(),
            },
            "compare_range": {
                "start_date": compare_range.0.to_string(),
                "end_date": compare_range.1.to_string(),
            },
            "metrics": metrics,
        }))
    }

    pub async fn get_trend_with_anomaly_overlay(&self, metric: &str, days: i32) -> Result<Value, DomainError> {
        let trend_items = self.get_kpi_trend(metric, days).await?;
        let end_date = Utc::now().date_naive();
        let start_date = end_date - Duration::days((days.max(1) - 1) as i64);

        let anomaly_rows = self.repo.fetch_anomaly_counts(start_date, end_date).await?;

        let anomaly_map = anomaly_rows
            .into_iter()
            .map(|row| (row.anomaly_date.to_string(), row.anomaly_count))
            .collect::<std::collections::HashMap<_, _>>();

        let mut anomaly_total = 0i64;
        let items = trend_items
            .into_iter()
            .map(|item| {
                let date = item.get("date").and_then(Value::as_str).unwrap_or_default().to_string();
                let anomaly_count = *anomaly_map.get(&date).unwrap_or(&0);
                anomaly_total += anomaly_count;
                json!({
                    "date": date,
                    "value": item.get("value").unwrap_or(&Value::Null),
                    "anomaly_count": anomaly_count,
                })
            })
            .collect::<Vec<_>>();

        Ok(json!({
            "metric": metric,
            "days": days,
            "items": items,
            "anomaly_total": anomaly_total,
        }))
    }

    pub async fn get_baseline_compare(
        &self,
        target_date: NaiveDate,
        weather_category: &str,
    ) -> Result<Value, DomainError> {
        let hourly_actual = self.get_hourly_flight_volume(target_date, target_date).await?;
        let actual_map = hourly_actual
            .into_iter()
            .filter_map(|item| {
                let hour = item.get("hour_label").and_then(Value::as_str)?.to_string();
                Some((hour, item))
            })
            .collect::<std::collections::HashMap<_, _>>();
        let baseline = baseline_profile(weather_category);

        let items = (0..24)
            .map(|hour| {
                let hour_str = format!("{hour:02}:00");
                let actual_item = actual_map.get(&hour_str);
                let actual_volume = actual_item
                    .and_then(|item| item.get("count"))
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                let actual_on_time_rate = actual_item
                    .and_then(|item| item.get("on_time_rate"))
                    .and_then(Value::as_f64)
                    .unwrap_or(baseline[hour].1);
                let (baseline_volume, baseline_rate, threshold_margin) = baseline[hour];
                json!({
                    "hour": hour_str,
                    "actual_volume": actual_volume,
                    "actual_on_time_rate": round_two(actual_on_time_rate),
                    "baseline_volume": baseline_volume,
                    "baseline_on_time_rate": baseline_rate,
                    "threshold_margin": threshold_margin,
                    "is_abnormal": actual_volume > 0 && actual_on_time_rate < (baseline_rate - threshold_margin),
                })
            })
            .collect::<Vec<_>>();

        Ok(json!({
            "target_date": target_date.to_string(),
            "weather_category": weather_category,
            "items": items,
        }))
    }

    pub async fn refresh_cache(&self) -> Result<Value, DomainError> {
        let materialized_view_refreshed = self.refresh_daily_kpi_materialized_view().await;
        let mut refreshed_range_count = 0usize;
        let mut failed_range_count = 0usize;
        let mut results = Vec::with_capacity(COMMON_CACHE_RANGES.len());

        for time_range in COMMON_CACHE_RANGES {
            let (start_date, end_date) = resolve_date_range(time_range, None, None);
            match self
                .get_kpi_snapshot(time_range, Some(start_date), Some(end_date))
                .await
            {
                Ok(snapshot) => {
                    refreshed_range_count += 1;
                    results.push(json!({
                        "time_range": time_range,
                        "status": "refreshed",
                        "start_date": start_date.to_string(),
                        "end_date": end_date.to_string(),
                        "calculated_at": snapshot.get("calculated_at").unwrap_or(&Value::Null),
                    }));
                }
                Err(error) => {
                    failed_range_count += 1;
                    warn!(time_range, error = %error, "failed to refresh KPI cache range");
                    results.push(json!({
                        "time_range": time_range,
                        "status": "failed",
                        "start_date": start_date.to_string(),
                        "end_date": end_date.to_string(),
                        "error": error.to_string(),
                    }));
                }
            }
        }

        let delivered_connections = self.broadcast_kpi_updated().await;

        Ok(json!({
            "timestamp": Utc::now().to_rfc3339(),
            "ranges": COMMON_CACHE_RANGES,
            "materialized_view_refreshed": materialized_view_refreshed,
            "refreshed_range_count": refreshed_range_count,
            "failed_range_count": failed_range_count,
            "results": results,
            "broadcast_topic": KPI_UPDATED_EVENT,
            "broadcast_event": KPI_UPDATED_EVENT,
            "delivered_connections": delivered_connections,
        }))
    }

    async fn get_snapshot_metrics(
        &self,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<KpiSnapshotMetrics, DomainError> {
        self.repo.fetch_snapshot_metrics(start_date, end_date).await
    }

    async fn get_turnaround_distribution(
        &self,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<Value>, DomainError> {
        let rows = self.repo.fetch_turnaround_distribution(start_date, end_date).await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                json!({
                    "bucket": row.bucket,
                    "count": row.count,
                })
            })
            .collect())
    }

    async fn get_hourly_flight_volume(
        &self,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<Value>, DomainError> {
        let rows = self.repo.fetch_hourly_flight_volume(start_date, end_date).await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                json!({
                    "hour": row.hour,
                    "hour_label": format!("{:02}:00", row.hour),
                    "count": row.count,
                })
            })
            .collect())
    }

    async fn get_equipment_utilization_rate(&self) -> Result<f64, DomainError> {
        match self.repo.fetch_equipment_utilization_rate().await {
            Ok(rate) => Ok(rate.unwrap_or(0.0)),
            Err(_) => Ok(0.0),
        }
    }

    async fn refresh_daily_kpi_materialized_view(&self) -> bool {
        match self.repo.refresh_daily_kpi_materialized_view().await {
            Ok(_) => true,
            Err(error) => {
                warn!(error = %error, "failed to refresh KPI materialized view");
                false
            }
        }
    }

    async fn broadcast_kpi_updated(&self) -> usize {
        let Some(sse_publisher) = self.sse_publisher.as_ref() else {
            return 0;
        };

        let payload = build_kpi_updated_payload(COMMON_CACHE_RANGES);
        match sse_publisher.publish_kpi_updated(payload).await {
            Ok(delivered) => delivered,
            Err(error) => {
                warn!(error = %error, "failed to broadcast KPI refresh event");
                0
            }
        }
    }
}

fn resolve_date_range(
    time_range: &str,
    start_date: Option<NaiveDate>,
    end_date: Option<NaiveDate>,
) -> (NaiveDate, NaiveDate) {
    let today = Utc::now().date_naive();
    match time_range {
        "yesterday" => {
            let yesterday = today - Duration::days(1);
            (yesterday, yesterday)
        }
        "custom" => (
            start_date.unwrap_or(today),
            end_date.unwrap_or(start_date.unwrap_or(today)),
        ),
        "this_week" => {
            let weekday = today.weekday().num_days_from_monday() as i64;
            let start = today - Duration::days(weekday);
            (start, today)
        }
        "this_month" => {
            let start = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today);
            (start, today)
        }
        _ => (today, today),
    }
}

fn build_kpi_updated_payload(ranges: &[&str]) -> Value {
    json!({
        "type": KPI_UPDATED_EVENT,
        "timestamp": Utc::now().to_rfc3339(),
        "ranges": ranges,
    })
}

fn metric_f64(value: &Option<f64>) -> f64 {
    value.unwrap_or(0.0)
}

fn baseline_profile(weather_category: &str) -> Vec<(i64, f64, f64)> {
    let base_curve = vec![
        (5, 0.95),
        (3, 0.95),
        (2, 0.95),
        (2, 0.95),
        (5, 0.95),
        (10, 0.92),
        (25, 0.90),
        (40, 0.88),
        (45, 0.85),
        (42, 0.85),
        (40, 0.86),
        (45, 0.85),
        (48, 0.82),
        (50, 0.80),
        (48, 0.82),
        (45, 0.82),
        (42, 0.84),
        (40, 0.85),
        (38, 0.86),
        (35, 0.88),
        (30, 0.90),
        (20, 0.92),
        (15, 0.95),
        (8, 0.95),
    ];

    let (volume_multiplier, rate_penalty, threshold_margin): (f64, f64, f64) = match weather_category {
        "rain" => (0.9, 0.15, 0.10),
        "storm" => (0.6, 0.35, 0.15),
        "snow" => (0.7, 0.25, 0.12),
        _ => (1.0, 0.0, 0.05),
    };

    base_curve
        .into_iter()
        .map(|(volume, rate)| {
            (
                ((volume as f64) * volume_multiplier).round() as i64,
                (rate - rate_penalty).clamp(0.0, 1.0),
                threshold_margin,
            )
        })
        .collect()
}

fn round_two(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::{build_kpi_updated_payload, resolve_date_range};
    use chrono::{Duration, Utc};
    use serde_json::json;

    #[test]
    fn resolve_date_range_supports_yesterday() {
        let today = Utc::now().date_naive();
        let expected = today - Duration::days(1);

        let (start_date, end_date) = resolve_date_range("yesterday", None, None);

        assert_eq!(start_date, expected);
        assert_eq!(end_date, expected);
    }

    #[test]
    fn kpi_updated_payload_matches_expected_contract() {
        let payload = build_kpi_updated_payload(&["today", "yesterday", "this_week", "this_month"]);

        assert_eq!(payload["type"], "kpi_updated");
        assert_eq!(
            payload["ranges"],
            json!(["today", "yesterday", "this_week", "this_month"])
        );
        assert!(payload["timestamp"].as_str().is_some());
    }
}
