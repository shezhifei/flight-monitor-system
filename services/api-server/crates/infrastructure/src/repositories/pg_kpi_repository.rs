use async_trait::async_trait;
use chrono::NaiveDate;
use sqlx::{postgres::PgRow, PgPool, Row};

use fms_domain::error::DomainError;
use fms_domain::ports::kpi_port::{
    KpiAnomalyCount, KpiHourlyVolume, KpiPort, KpiServiceNodeCompliance, KpiSnapshotMetrics, KpiTrendPoint,
    KpiTurnaroundBucket,
};

pub struct PgKpiRepository {
    pool: PgPool,
}

impl PgKpiRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn fetch_snapshot_metrics_row(
        &self,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<PgRow, sqlx::Error> {
        let query = r#"
            WITH timeline AS (
                SELECT
                    flight_id,
                    MAX(occurred_at) FILTER (WHERE milestone_code = 'cleaning_start_time') AS cleaning_start_time,
                    MAX(occurred_at) FILTER (WHERE milestone_code = 'cleaning_end_time') AS cleaning_end_time,
                    MAX(occurred_at) FILTER (WHERE milestone_code = 'loading_complete_time') AS loading_complete_time,
                    MAX(occurred_at) FILTER (WHERE milestone_code = 'start_boarding_time') AS start_boarding_time,
                    MAX(occurred_at) FILTER (WHERE milestone_code = 'end_boarding_time') AS end_boarding_time
                FROM flight_dispatch_timeline_events
                GROUP BY flight_id
            ),
            open_anomaly AS (
                SELECT DISTINCT flight_id
                FROM anomalies
                WHERE status IN ('open', 'acknowledged')
            )
            SELECT
                COUNT(*) AS total_flights,
                COUNT(*) FILTER (WHERE f.actual_departure IS NOT NULL AND f.actual_arrival IS NOT NULL) AS completed_flights,
                AVG(EXTRACT(EPOCH FROM (f.actual_departure - f.actual_arrival)) / 60)
                    FILTER (WHERE f.actual_departure IS NOT NULL AND f.actual_arrival IS NOT NULL) AS avg_turnaround_minutes,
                PERCENTILE_CONT(0.9) WITHIN GROUP (
                    ORDER BY EXTRACT(EPOCH FROM (f.actual_departure - f.actual_arrival)) / 60
                ) FILTER (WHERE f.actual_departure IS NOT NULL AND f.actual_arrival IS NOT NULL) AS p90_turnaround_minutes,
                COUNT(*) FILTER (WHERE f.actual_departure <= f.scheduled_departure + INTERVAL '15 minutes')::FLOAT
                    / NULLIF(COUNT(*) FILTER (WHERE f.actual_departure IS NOT NULL), 0) AS on_time_departure_rate,
                COUNT(*) FILTER (WHERE f.actual_arrival <= f.scheduled_arrival + INTERVAL '15 minutes')::FLOAT
                    / NULLIF(COUNT(*) FILTER (WHERE f.actual_arrival IS NOT NULL), 0) AS on_time_arrival_rate,
                COUNT(*) FILTER (WHERE oa.flight_id IS NOT NULL)::FLOAT
                    / NULLIF(COUNT(*), 0) AS abnormal_ratio,
                (
                    (
                        COUNT(*) FILTER (
                            WHERE t.cleaning_start_time IS NOT NULL
                              AND t.cleaning_end_time IS NOT NULL
                              AND t.cleaning_end_time <= t.cleaning_start_time + INTERVAL '30 minutes'
                        )::FLOAT
                        / NULLIF(COUNT(*) FILTER (WHERE t.cleaning_start_time IS NOT NULL AND t.cleaning_end_time IS NOT NULL), 0)
                    )
                    +
                    (
                        COUNT(*) FILTER (
                            WHERE t.loading_complete_time IS NOT NULL
                              AND t.cleaning_end_time IS NOT NULL
                              AND t.loading_complete_time <= t.cleaning_end_time + INTERVAL '40 minutes'
                        )::FLOAT
                        / NULLIF(COUNT(*) FILTER (WHERE t.loading_complete_time IS NOT NULL AND t.cleaning_end_time IS NOT NULL), 0)
                    )
                    +
                    (
                        COUNT(*) FILTER (
                            WHERE t.end_boarding_time IS NOT NULL
                              AND t.start_boarding_time IS NOT NULL
                              AND t.end_boarding_time <= t.start_boarding_time + INTERVAL '45 minutes'
                        )::FLOAT
                        / NULLIF(COUNT(*) FILTER (WHERE t.end_boarding_time IS NOT NULL AND t.start_boarding_time IS NOT NULL), 0)
                    )
                ) / 3 AS service_node_compliance_rate
            FROM flights f
            LEFT JOIN timeline t ON t.flight_id = f.flight_id
            LEFT JOIN open_anomaly oa ON oa.flight_id = f.flight_id
            WHERE f.scheduled_departure IS NOT NULL
              AND f.scheduled_departure >= ($1::date::timestamp AT TIME ZONE 'Asia/Shanghai')
              AND f.scheduled_departure < (($2::date + INTERVAL '1 day')::timestamp AT TIME ZONE 'Asia/Shanghai')
        "#;

        sqlx::query(query)
            .bind(start_date)
            .bind(end_date)
            .fetch_one(&self.pool)
            .await
    }

    async fn fetch_kpi_trend_rows(
        &self,
        metric_column: &str,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<PgRow>, sqlx::Error> {
        let view_query = format!(
            r#"
            SELECT flight_date, {metric_column} AS metric_value
            FROM mv_daily_flight_kpi
            WHERE flight_date >= $1 AND flight_date <= $2
            ORDER BY flight_date ASC
            "#
        );

        match sqlx::query(&view_query)
            .bind(start_date)
            .bind(end_date)
            .fetch_all(&self.pool)
            .await
        {
            Ok(rows) => Ok(rows),
            Err(_) => {
                let fallback_query = r#"
                    WITH open_anomaly AS (
                        SELECT DISTINCT flight_id
                        FROM anomalies
                        WHERE status IN ('open', 'acknowledged')
                    )
                    SELECT
                        DATE(f.scheduled_departure AT TIME ZONE 'Asia/Shanghai') AS flight_date,
                        CASE
                            WHEN $1 = 'on_time_departure_rate' THEN
                                COUNT(*) FILTER (WHERE f.actual_departure <= f.scheduled_departure + INTERVAL '15 minutes')::FLOAT
                                / NULLIF(COUNT(*) FILTER (WHERE f.actual_departure IS NOT NULL), 0)
                            WHEN $1 = 'avg_turnaround_minutes' THEN
                                AVG(EXTRACT(EPOCH FROM (f.actual_departure - f.actual_arrival)) / 60)
                                FILTER (WHERE f.actual_departure IS NOT NULL AND f.actual_arrival IS NOT NULL)
                            ELSE
                                COUNT(*) FILTER (WHERE oa.flight_id IS NOT NULL)::FLOAT
                                / NULLIF(COUNT(*), 0)
                        END AS metric_value
                    FROM flights f
                    LEFT JOIN open_anomaly oa ON oa.flight_id = f.flight_id
                    WHERE f.scheduled_departure IS NOT NULL
                      AND f.scheduled_departure >= ($2::date::timestamp AT TIME ZONE 'Asia/Shanghai')
                      AND f.scheduled_departure < (($3::date + INTERVAL '1 day')::timestamp AT TIME ZONE 'Asia/Shanghai')
                    GROUP BY flight_date
                    ORDER BY flight_date ASC
                "#;
                sqlx::query(fallback_query)
                    .bind(metric_column)
                    .bind(start_date)
                    .bind(end_date)
                    .fetch_all(&self.pool)
                    .await
            }
        }
    }

    async fn fetch_service_node_compliance_row(&self, target_date: NaiveDate) -> Result<PgRow, sqlx::Error> {
        let query = r#"
            WITH timeline AS (
                SELECT
                    flight_id,
                    MAX(occurred_at) FILTER (WHERE milestone_code = 'cleaning_start_time') AS cleaning_start_time,
                    MAX(occurred_at) FILTER (WHERE milestone_code = 'cleaning_end_time') AS cleaning_end_time,
                    MAX(occurred_at) FILTER (WHERE milestone_code = 'loading_complete_time') AS loading_complete_time,
                    MAX(occurred_at) FILTER (WHERE milestone_code = 'start_boarding_time') AS start_boarding_time,
                    MAX(occurred_at) FILTER (WHERE milestone_code = 'end_boarding_time') AS end_boarding_time
                FROM flight_dispatch_timeline_events
                GROUP BY flight_id
            )
            SELECT
                COUNT(*) FILTER (
                    WHERE t.cleaning_start_time IS NOT NULL
                      AND t.cleaning_end_time IS NOT NULL
                      AND t.cleaning_end_time <= t.cleaning_start_time + INTERVAL '30 minutes'
                )::FLOAT
                / NULLIF(COUNT(*) FILTER (WHERE t.cleaning_start_time IS NOT NULL AND t.cleaning_end_time IS NOT NULL), 0)
                    AS cleaning_rate,
                COUNT(*) FILTER (
                    WHERE t.loading_complete_time IS NOT NULL
                      AND t.cleaning_end_time IS NOT NULL
                      AND t.loading_complete_time <= t.cleaning_end_time + INTERVAL '40 minutes'
                )::FLOAT
                / NULLIF(COUNT(*) FILTER (WHERE t.loading_complete_time IS NOT NULL AND t.cleaning_end_time IS NOT NULL), 0)
                    AS loading_rate,
                COUNT(*) FILTER (
                    WHERE t.end_boarding_time IS NOT NULL
                      AND t.start_boarding_time IS NOT NULL
                      AND t.end_boarding_time <= t.start_boarding_time + INTERVAL '45 minutes'
                )::FLOAT
                / NULLIF(COUNT(*) FILTER (WHERE t.end_boarding_time IS NOT NULL AND t.start_boarding_time IS NOT NULL), 0)
                    AS boarding_rate
            FROM flights f
            LEFT JOIN timeline t ON t.flight_id = f.flight_id
            WHERE f.scheduled_departure IS NOT NULL
              AND f.scheduled_departure >= ($1::date::timestamp AT TIME ZONE 'Asia/Shanghai')
              AND f.scheduled_departure < (($1::date + INTERVAL '1 day')::timestamp AT TIME ZONE 'Asia/Shanghai')
        "#;

        sqlx::query(query).bind(target_date).fetch_one(&self.pool).await
    }

    async fn fetch_turnaround_distribution_rows(
        &self,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<PgRow>, sqlx::Error> {
        let query = r#"
            WITH durations AS (
                SELECT EXTRACT(EPOCH FROM (actual_departure - actual_arrival)) / 60 AS duration_minutes
                FROM flights
                WHERE actual_departure IS NOT NULL
                  AND actual_arrival IS NOT NULL
                  AND scheduled_departure >= ($1::date::timestamp AT TIME ZONE 'Asia/Shanghai')
                  AND scheduled_departure < (($2::date + INTERVAL '1 day')::timestamp AT TIME ZONE 'Asia/Shanghai')
            )
            SELECT
                CASE
                    WHEN duration_minutes < 30 THEN '0-30'
                    WHEN duration_minutes < 60 THEN '30-60'
                    WHEN duration_minutes < 90 THEN '60-90'
                    ELSE '90+'
                END AS bucket,
                COUNT(*) AS count
            FROM durations
            GROUP BY bucket
            ORDER BY bucket ASC
        "#;

        sqlx::query(query)
            .bind(start_date)
            .bind(end_date)
            .fetch_all(&self.pool)
            .await
    }

    async fn fetch_hourly_flight_volume_rows(
        &self,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<PgRow>, sqlx::Error> {
        let query = r#"
            SELECT
                EXTRACT(HOUR FROM scheduled_departure AT TIME ZONE 'Asia/Shanghai')::INT AS hour,
                COUNT(*) AS count
            FROM flights
            WHERE scheduled_departure IS NOT NULL
              AND scheduled_departure >= ($1::date::timestamp AT TIME ZONE 'Asia/Shanghai')
              AND scheduled_departure < (($2::date + INTERVAL '1 day')::timestamp AT TIME ZONE 'Asia/Shanghai')
            GROUP BY hour
            ORDER BY hour ASC
        "#;

        sqlx::query(query)
            .bind(start_date)
            .bind(end_date)
            .fetch_all(&self.pool)
            .await
    }

    async fn fetch_equipment_utilization_rate_raw(&self) -> Result<Option<f64>, sqlx::Error> {
        let query = r#"
            WITH total AS (
                SELECT COUNT(*)::FLOAT AS total_count FROM equipment
            ),
            active AS (
                SELECT COUNT(DISTINCT doe.equipment_id)::FLOAT AS active_count
                FROM dispatch_order_equipment doe
                JOIN dispatch_orders d ON d.id = doe.dispatch_order_id
                WHERE doe.released_at IS NULL
                  AND d.status IN ('assigned', 'in_progress')
            )
            SELECT active.active_count / NULLIF(total.total_count, 0) AS rate
            FROM total, active
        "#;

        let row = sqlx::query(query).fetch_one(&self.pool).await?;
        row.try_get::<Option<f64>, _>("rate")
    }

    async fn fetch_anomaly_counts_rows(
        &self,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<PgRow>, sqlx::Error> {
        let query = r#"
            SELECT
                DATE(detected_at AT TIME ZONE 'Asia/Shanghai') AS anomaly_date,
                COUNT(*) AS anomaly_count
            FROM anomalies
            WHERE detected_at >= ($1::date::timestamp AT TIME ZONE 'Asia/Shanghai')
              AND detected_at < (($2::date + INTERVAL '1 day')::timestamp AT TIME ZONE 'Asia/Shanghai')
            GROUP BY anomaly_date
            ORDER BY anomaly_date ASC
        "#;

        sqlx::query(query)
            .bind(start_date)
            .bind(end_date)
            .fetch_all(&self.pool)
            .await
    }

    async fn refresh_daily_kpi_materialized_view_raw(&self) -> Result<(), sqlx::Error> {
        match sqlx::query("REFRESH MATERIALIZED VIEW CONCURRENTLY mv_daily_flight_kpi")
            .execute(&self.pool)
            .await
        {
            Ok(_) => Ok(()),
            Err(_) => {
                sqlx::query("REFRESH MATERIALIZED VIEW mv_daily_flight_kpi")
                    .execute(&self.pool)
                    .await?;
                Ok(())
            }
        }
    }
}

fn opt_f64(row: &PgRow, column: &str) -> Option<f64> {
    row.try_get::<Option<f64>, _>(column).ok().flatten()
}

fn map_sqlx(error: sqlx::Error) -> DomainError {
    DomainError::Internal(error.to_string())
}

#[async_trait]
impl KpiPort for PgKpiRepository {
    async fn fetch_snapshot_metrics(
        &self,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<KpiSnapshotMetrics, DomainError> {
        let row = self
            .fetch_snapshot_metrics_row(start_date, end_date)
            .await
            .map_err(map_sqlx)?;
        Ok(KpiSnapshotMetrics {
            avg_turnaround_minutes: opt_f64(&row, "avg_turnaround_minutes"),
            p90_turnaround_minutes: opt_f64(&row, "p90_turnaround_minutes"),
            on_time_departure_rate: opt_f64(&row, "on_time_departure_rate"),
            on_time_arrival_rate: opt_f64(&row, "on_time_arrival_rate"),
            service_node_compliance_rate: opt_f64(&row, "service_node_compliance_rate"),
            abnormal_ratio: opt_f64(&row, "abnormal_ratio"),
        })
    }

    async fn fetch_kpi_trend(
        &self,
        metric_column: &str,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<KpiTrendPoint>, DomainError> {
        let rows = self
            .fetch_kpi_trend_rows(metric_column, start_date, end_date)
            .await
            .map_err(map_sqlx)?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let flight_date = row.try_get::<NaiveDate, _>("flight_date").ok()?;
                let metric_value = row.try_get::<Option<f64>, _>("metric_value").ok().flatten();
                Some(KpiTrendPoint {
                    flight_date,
                    metric_value,
                })
            })
            .collect())
    }

    async fn fetch_service_node_compliance(
        &self,
        target_date: NaiveDate,
    ) -> Result<KpiServiceNodeCompliance, DomainError> {
        let row = self
            .fetch_service_node_compliance_row(target_date)
            .await
            .map_err(map_sqlx)?;
        Ok(KpiServiceNodeCompliance {
            cleaning_rate: opt_f64(&row, "cleaning_rate"),
            loading_rate: opt_f64(&row, "loading_rate"),
            boarding_rate: opt_f64(&row, "boarding_rate"),
        })
    }

    async fn fetch_turnaround_distribution(
        &self,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<KpiTurnaroundBucket>, DomainError> {
        let rows = self
            .fetch_turnaround_distribution_rows(start_date, end_date)
            .await
            .map_err(map_sqlx)?;
        Ok(rows
            .into_iter()
            .map(|row| KpiTurnaroundBucket {
                bucket: row.try_get::<String, _>("bucket").unwrap_or_default(),
                count: row.try_get::<i64, _>("count").unwrap_or(0),
            })
            .collect())
    }

    async fn fetch_hourly_flight_volume(
        &self,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<KpiHourlyVolume>, DomainError> {
        let rows = self
            .fetch_hourly_flight_volume_rows(start_date, end_date)
            .await
            .map_err(map_sqlx)?;
        Ok(rows
            .into_iter()
            .map(|row| KpiHourlyVolume {
                hour: row.try_get::<i32, _>("hour").unwrap_or(0),
                count: row.try_get::<i64, _>("count").unwrap_or(0),
            })
            .collect())
    }

    async fn fetch_equipment_utilization_rate(&self) -> Result<Option<f64>, DomainError> {
        self.fetch_equipment_utilization_rate_raw().await.map_err(map_sqlx)
    }

    async fn fetch_anomaly_counts(
        &self,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<KpiAnomalyCount>, DomainError> {
        let rows = self
            .fetch_anomaly_counts_rows(start_date, end_date)
            .await
            .map_err(map_sqlx)?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let anomaly_date = row.try_get::<NaiveDate, _>("anomaly_date").ok()?;
                let anomaly_count = row.try_get::<i64, _>("anomaly_count").ok()?;
                Some(KpiAnomalyCount {
                    anomaly_date,
                    anomaly_count,
                })
            })
            .collect())
    }

    async fn refresh_daily_kpi_materialized_view(&self) -> Result<(), DomainError> {
        self.refresh_daily_kpi_materialized_view_raw().await.map_err(map_sqlx)
    }
}
