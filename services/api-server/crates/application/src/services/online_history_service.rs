//! 在线历史查询服务

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde_json::json;

use fms_domain::error::DomainError;
use fms_domain::ports::online_history_repository::OnlineHistoryRepository;

pub struct OnlineHistoryService {
    repo: Arc<dyn OnlineHistoryRepository + Send + Sync>,
}

impl OnlineHistoryService {
    pub fn new(repo: Arc<dyn OnlineHistoryRepository + Send + Sync>) -> Self {
        Self { repo }
    }

    pub async fn get_history(
        &self,
        user_id: Option<&str>,
        start_date: Option<DateTime<Utc>>,
        end_date: Option<DateTime<Utc>>,
        page: i64,
        page_size: i64,
    ) -> Result<serde_json::Value, DomainError> {
        let safe_page = page.max(1);
        let safe_page_size = page_size.clamp(1, 100);
        let offset = (safe_page - 1) * safe_page_size;
        let items = self
            .repo
            .list_history(user_id, start_date, end_date, safe_page_size, offset)
            .await?;
        let total = self.repo.count_history(user_id, start_date, end_date).await?;

        Ok(json!({
            "history": items,
            "pagination": {
                "page": safe_page,
                "page_size": safe_page_size,
                "total": total,
                "total_pages": if total <= 0 { 0 } else { (total + safe_page_size - 1) / safe_page_size },
            }
        }))
    }
}
