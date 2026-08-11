//! PostgreSQL 机位间旅途时间统计仓储实现

use async_trait::async_trait;
use sqlx::{PgPool, Row};

use fms_domain::error::DomainError;
use fms_domain::ports::dispatch_repository::DispatchTravelStatsRepository;

pub struct PgDispatchTravelStatsRepository {
    pool: PgPool,
}

impl PgDispatchTravelStatsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DispatchTravelStatsRepository for PgDispatchTravelStatsRepository {
    async fn record_travel(
        &self,
        from_stand_id: &str,
        to_stand_id: &str,
        travel_minutes: f64,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO dispatch_stand_travel_stats (
                from_stand_id, to_stand_id, travel_minutes, sample_count, avg_minutes, recorded_at
            ) VALUES ($1, $2, $3, 1, $3, NOW())
            ON CONFLICT (from_stand_id, to_stand_id) DO UPDATE SET
                sample_count = dispatch_stand_travel_stats.sample_count + 1,
                avg_minutes  = (
                    dispatch_stand_travel_stats.avg_minutes * dispatch_stand_travel_stats.sample_count
                    + EXCLUDED.travel_minutes
                ) / (dispatch_stand_travel_stats.sample_count + 1),
                travel_minutes = EXCLUDED.travel_minutes,
                recorded_at = NOW()
            "#,
        )
        .bind(from_stand_id)
        .bind(to_stand_id)
        .bind(travel_minutes)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn get_average_travel(&self, from_stand_id: &str, to_stand_id: &str) -> Result<Option<f64>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT avg_minutes
            FROM dispatch_stand_travel_stats
            WHERE from_stand_id = $1 AND to_stand_id = $2
            "#,
        )
        .bind(from_stand_id)
        .bind(to_stand_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        match row {
            Some(r) => {
                let avg: f64 = r
                    .try_get("avg_minutes")
                    .map_err(|e| DomainError::Internal(e.to_string()))?;
                Ok(Some(avg))
            }
            None => Ok(None),
        }
    }
}
