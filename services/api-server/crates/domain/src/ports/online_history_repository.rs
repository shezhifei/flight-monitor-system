//! 在线历史记录仓储接口

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::error::DomainError;
use crate::models::online_history::OnlineHistoryRecord;

#[async_trait]
pub trait OnlineHistoryRepository {
    async fn record_login(
        &self,
        user_id: &str,
        session_id: &str,
        ip_address: Option<&str>,
        device_info: Option<&str>,
    ) -> Result<(), DomainError>;

    async fn record_logout(&self, user_id: &str, session_id: &str, forced: bool) -> Result<(), DomainError>;

    async fn list_history(
        &self,
        user_id: Option<&str>,
        start_date: Option<DateTime<Utc>>,
        end_date: Option<DateTime<Utc>>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<OnlineHistoryRecord>, DomainError>;

    async fn count_history(
        &self,
        user_id: Option<&str>,
        start_date: Option<DateTime<Utc>>,
        end_date: Option<DateTime<Utc>>,
    ) -> Result<i64, DomainError>;
}
