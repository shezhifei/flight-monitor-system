//! 会话运行时抽象

use crate::error::DomainError;
use crate::models::session_runtime::{OnlineSessionStatus, SessionEstablishResult, SessionRuntimeStatus};
use async_trait::async_trait;

#[async_trait]
pub trait SessionRuntimeRepository {
    async fn establish_session(
        &self,
        user_id: &str,
        client_ip: Option<&str>,
        refresh_token: Option<&str>,
    ) -> Result<SessionEstablishResult, DomainError>;

    async fn validate_refresh_token(&self, user_id: &str, refresh_token: &str) -> Result<bool, DomainError>;

    async fn revoke_refresh_tokens(&self, user_id: &str) -> Result<(), DomainError>;

    async fn revoke_session(&self, user_id: &str, reason: &str) -> Result<Option<OnlineSessionStatus>, DomainError>;

    async fn heartbeat(&self, user_id: &str) -> Result<Option<OnlineSessionStatus>, DomainError>;

    async fn get_online_users(&self) -> Result<Vec<String>, DomainError>;

    async fn get_online_status(&self, user_id: &str) -> Result<OnlineSessionStatus, DomainError>;

    async fn get_all_online_status(&self) -> Result<Vec<OnlineSessionStatus>, DomainError>;

    async fn get_runtime_status(&self) -> Result<SessionRuntimeStatus, DomainError>;

    async fn get_permission_version(&self, user_id: &str) -> Result<i64, DomainError>;
}
