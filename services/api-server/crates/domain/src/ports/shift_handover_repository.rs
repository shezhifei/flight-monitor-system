//! 交接班仓储 trait

use crate::error::DomainError;
use crate::models::shift_handover::{ShiftHandover, ShiftHandoverItem};
use async_trait::async_trait;
use chrono::NaiveDate;

#[async_trait]
pub trait ShiftHandoverRepository {
    async fn create(&self, handover: &ShiftHandover) -> Result<ShiftHandover, DomainError>;
    async fn find_by_id(&self, handover_id: &str) -> Result<Option<ShiftHandover>, DomainError>;
    async fn find_all(
        &self,
        shift_date: Option<NaiveDate>,
        shift_code: Option<&str>,
        status: Option<&str>,
        from_user_id: Option<&str>,
        to_user_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ShiftHandover>, DomainError>;
    async fn submit(&self, handover_id: &str) -> Result<Option<ShiftHandover>, DomainError>;
    async fn acknowledge_item(
        &self,
        handover_id: &str,
        item_id: &str,
        acknowledged_by: &str,
        acknowledged: bool,
    ) -> Result<Option<ShiftHandoverItem>, DomainError>;
    async fn list_unacked_mandatory_titles(&self, handover_id: &str) -> Result<Vec<String>, DomainError>;
    async fn complete(
        &self,
        handover_id: &str,
        to_operator_name: Option<&str>,
        to_operator_job_title: Option<&str>,
    ) -> Result<Option<ShiftHandover>, DomainError>;
}
