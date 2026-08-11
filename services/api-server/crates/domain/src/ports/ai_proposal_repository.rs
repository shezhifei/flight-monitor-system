//! AiActionProposal 仓储接口
//!
//! 定义 AiActionProposal 的持久化抽象，由 infrastructure 层实现。

use chrono::{DateTime, Utc};

use crate::models::ai_proposal::{ActionProposalQuery, ActionProposalStats, ActionProposalStatus, AiActionProposal};
use async_trait::async_trait;

#[async_trait]
pub trait AiProposalRepository {
    async fn save(&self, proposal: &AiActionProposal) -> Result<(), AiProposalRepositoryError>;
    async fn find_by_id(&self, proposal_id: &str) -> Result<Option<AiActionProposal>, AiProposalRepositoryError>;
    async fn find_by_pending_action_id(
        &self,
        pending_action_id: &str,
    ) -> Result<Option<AiActionProposal>, AiProposalRepositoryError>;
    async fn find_by_job_id(&self, job_id: &str) -> Result<Vec<AiActionProposal>, AiProposalRepositoryError>;
    async fn find_by_run_id(&self, run_id: &str) -> Result<Vec<AiActionProposal>, AiProposalRepositoryError>;
    async fn find_by_object(
        &self,
        object_type: &str,
        object_id: &str,
    ) -> Result<Vec<AiActionProposal>, AiProposalRepositoryError>;
    async fn search(&self, query: &ActionProposalQuery) -> Result<Vec<AiActionProposal>, AiProposalRepositoryError>;
    async fn find_pending(&self) -> Result<Vec<AiActionProposal>, AiProposalRepositoryError>;
    async fn find_expired(&self) -> Result<Vec<AiActionProposal>, AiProposalRepositoryError>;
    async fn count(&self, query: &ActionProposalQuery) -> Result<usize, AiProposalRepositoryError>;
    async fn get_stats(&self) -> Result<ActionProposalStats, AiProposalRepositoryError>;
    async fn update_status(
        &self,
        proposal_id: &str,
        status: ActionProposalStatus,
    ) -> Result<(), AiProposalRepositoryError>;
    async fn link_pending_action(
        &self,
        proposal_id: &str,
        pending_action_id: &str,
    ) -> Result<(), AiProposalRepositoryError>;
    async fn unlink_pending_action(&self, pending_action_id: &str) -> Result<(), AiProposalRepositoryError>;
    async fn delete(&self, proposal_id: &str) -> Result<(), AiProposalRepositoryError>;

    // Phase C: count/aggregation methods replacing raw SQL in application services
    async fn count_pending_by_risk(&self) -> Result<Vec<(i16, i64)>, AiProposalRepositoryError>;
    async fn count_failed_since(&self, cutoff: DateTime<Utc>) -> Result<i64, AiProposalRepositoryError>;
    async fn count_executed_since(&self, cutoff: DateTime<Utc>) -> Result<i64, AiProposalRepositoryError>;
    async fn smoke_summary(&self) -> Result<Option<SmokeProposalSummary>, AiProposalRepositoryError>;
    async fn find_smoke_older_than(
        &self,
        cutoff: DateTime<Utc>,
    ) -> Result<Vec<SmokeProposalRow>, AiProposalRepositoryError>;
    async fn delete_smoke_older_than(
        &self,
        proposal_ids: &[String],
        cutoff: DateTime<Utc>,
    ) -> Result<u64, AiProposalRepositoryError>;
}

#[derive(Debug, Clone)]
pub struct SmokeProposalSummary {
    pub last_run_at: Option<DateTime<Utc>>,
    pub total: i64,
    pub succeeded: i64,
    pub failed: i64,
}

#[derive(Debug, Clone)]
pub struct SmokeProposalRow {
    pub proposal_id: String,
    pub object_id: String,
    pub job_id: Option<String>,
    pub run_id: Option<String>,
}

#[derive(Debug, Clone)]
pub enum AiProposalRepositoryError {
    NotFound(String),
    Validation(String),
    Database(String),
}

impl AiProposalRepositoryError {
    pub fn not_found(id: impl Into<String>) -> Self {
        Self::NotFound(id.into())
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }

    pub fn database(message: impl Into<String>) -> Self {
        Self::Database(message.into())
    }
}

impl std::fmt::Display for AiProposalRepositoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "proposal not found: {}", id),
            Self::Validation(msg) => write!(f, "validation error: {}", msg),
            Self::Database(msg) => write!(f, "database error: {}", msg),
        }
    }
}

impl std::error::Error for AiProposalRepositoryError {}
