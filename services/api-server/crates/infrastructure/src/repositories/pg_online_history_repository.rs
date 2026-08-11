//! PostgreSQL 在线历史记录仓储实现

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

use fms_domain::error::DomainError;
use fms_domain::models::online_history::OnlineHistoryRecord;
use fms_domain::ports::online_history_repository::OnlineHistoryRepository;

pub struct PgOnlineHistoryRepository {
    pool: PgPool,
}

impl PgOnlineHistoryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn row_to_record(row: &sqlx::postgres::PgRow) -> OnlineHistoryRecord {
        OnlineHistoryRecord {
            id: row.get::<uuid::Uuid, _>("id").to_string(),
            user_id: row.get("user_id"),
            username: row.get("username"),
            login_time: row.get("login_time"),
            logout_time: row.get("logout_time"),
            duration_seconds: row.get("duration_seconds"),
            ip_address: row.get::<Option<String>, _>("ip_address"),
            device_info: row.get("device_info"),
            forced_logout: row.get("forced_logout"),
        }
    }
}

#[async_trait]
impl OnlineHistoryRepository for PgOnlineHistoryRepository {
    async fn record_login(
        &self,
        user_id: &str,
        session_id: &str,
        ip_address: Option<&str>,
        device_info: Option<&str>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO online_history
                (user_id, session_id, login_time, ip_address, device_info)
            VALUES
                ($1, $2, CURRENT_TIMESTAMP, $3::inet, $4)
            "#,
        )
        .bind(user_id)
        .bind(session_id)
        .bind(ip_address)
        .bind(device_info)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(())
    }

    async fn record_logout(&self, user_id: &str, session_id: &str, forced: bool) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            UPDATE online_history
               SET logout_time = CURRENT_TIMESTAMP,
                   duration_seconds = EXTRACT(EPOCH FROM (CURRENT_TIMESTAMP - login_time)),
                   forced_logout = $3
             WHERE user_id = $1
               AND session_id = $2
               AND logout_time IS NULL
            "#,
        )
        .bind(user_id)
        .bind(session_id)
        .bind(forced)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(())
    }

    async fn list_history(
        &self,
        user_id: Option<&str>,
        start_date: Option<DateTime<Utc>>,
        end_date: Option<DateTime<Utc>>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<OnlineHistoryRecord>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT
                oh.id,
                oh.user_id,
                COALESCE(u.username, '') AS username,
                oh.login_time,
                oh.logout_time,
                oh.duration_seconds,
                host(oh.ip_address) AS ip_address,
                oh.device_info,
                oh.forced_logout
            FROM online_history oh
            LEFT JOIN users u ON u.id = oh.user_id
            WHERE ($1::text IS NULL OR oh.user_id = $1)
              AND ($2::timestamptz IS NULL OR oh.login_time >= $2)
              AND ($3::timestamptz IS NULL OR oh.login_time <= $3)
            ORDER BY oh.login_time DESC
            LIMIT $4 OFFSET $5
            "#,
        )
        .bind(user_id)
        .bind(start_date)
        .bind(end_date)
        .bind(limit.max(1))
        .bind(offset.max(0))
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(rows.iter().map(Self::row_to_record).collect())
    }

    async fn count_history(
        &self,
        user_id: Option<&str>,
        start_date: Option<DateTime<Utc>>,
        end_date: Option<DateTime<Utc>>,
    ) -> Result<i64, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) AS count
            FROM online_history oh
            WHERE ($1::text IS NULL OR oh.user_id = $1)
              AND ($2::timestamptz IS NULL OR oh.login_time >= $2)
              AND ($3::timestamptz IS NULL OR oh.login_time <= $3)
            "#,
        )
        .bind(user_id)
        .bind(start_date)
        .bind(end_date)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(row.get("count"))
    }
}
