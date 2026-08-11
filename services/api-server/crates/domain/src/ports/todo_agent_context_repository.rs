//! Todo agent context 仓储接口。

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::DomainError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoAgentContext {
    pub todo_id: String,
    pub agent_entity_id: String,
    pub agent_run_id: Option<String>,
    pub agent_status: String,
    pub updated_by: String,
    pub updated_at: Option<DateTime<Utc>>,
    pub version: i32,
}

#[async_trait]
pub trait TodoAgentContextRepository {
    async fn get(&self, todo_id: &str) -> Result<Option<TodoAgentContext>, DomainError>;

    async fn batch_get(&self, todo_ids: &[String]) -> Result<HashMap<String, TodoAgentContext>, DomainError>;

    async fn upsert_partial(
        &self,
        todo_id: &str,
        agent_entity_id: Option<&str>,
        agent_run_id: Option<&str>,
        agent_status: Option<&str>,
        updated_by: &str,
    ) -> Result<TodoAgentContext, DomainError>;

    async fn find_todo_ids(
        &self,
        agent_status: Option<&str>,
        agent_entity_id: Option<&str>,
        agent_run_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<String>, DomainError>;

    fn get_metrics_snapshot(&self) -> HashMap<String, serde_json::Value>;
}
