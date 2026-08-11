use async_trait::async_trait;
use chrono::{DateTime, Utc};
use fms_domain::models::ai_proposal::{
    ActionProposalQuery, ActionProposalStats, ActionProposalStatus, AiActionProposal,
};
use fms_domain::ports::ai_proposal_repository::{
    AiProposalRepository, AiProposalRepositoryError, SmokeProposalRow, SmokeProposalSummary,
};
#[derive(Debug, Clone)]
pub struct NoopAiProposalRepository;

#[async_trait]
impl AiProposalRepository for NoopAiProposalRepository {
    async fn save(&self, _proposal: &AiActionProposal) -> Result<(), AiProposalRepositoryError> {
        Ok(())
    }
    async fn find_by_id(&self, _id: &str) -> Result<Option<AiActionProposal>, AiProposalRepositoryError> {
        Ok(None)
    }
    async fn find_by_pending_action_id(
        &self,
        _pending_action_id: &str,
    ) -> Result<Option<AiActionProposal>, AiProposalRepositoryError> {
        Ok(None)
    }
    async fn find_by_job_id(&self, _job_id: &str) -> Result<Vec<AiActionProposal>, AiProposalRepositoryError> {
        Ok(vec![])
    }
    async fn find_by_run_id(&self, _run_id: &str) -> Result<Vec<AiActionProposal>, AiProposalRepositoryError> {
        Ok(vec![])
    }
    async fn find_by_object(
        &self,
        _object_type: &str,
        _object_id: &str,
    ) -> Result<Vec<AiActionProposal>, AiProposalRepositoryError> {
        Ok(vec![])
    }
    async fn search(&self, _query: &ActionProposalQuery) -> Result<Vec<AiActionProposal>, AiProposalRepositoryError> {
        Ok(vec![])
    }
    async fn find_pending(&self) -> Result<Vec<AiActionProposal>, AiProposalRepositoryError> {
        Ok(vec![])
    }
    async fn find_expired(&self) -> Result<Vec<AiActionProposal>, AiProposalRepositoryError> {
        Ok(vec![])
    }
    async fn count(&self, _query: &ActionProposalQuery) -> Result<usize, AiProposalRepositoryError> {
        Ok(0)
    }
    async fn get_stats(&self) -> Result<ActionProposalStats, AiProposalRepositoryError> {
        Ok(ActionProposalStats::default())
    }
    async fn update_status(
        &self,
        _proposal_id: &str,
        _status: ActionProposalStatus,
    ) -> Result<(), AiProposalRepositoryError> {
        Ok(())
    }
    async fn link_pending_action(
        &self,
        _proposal_id: &str,
        _pending_action_id: &str,
    ) -> Result<(), AiProposalRepositoryError> {
        Ok(())
    }
    async fn unlink_pending_action(&self, _pending_action_id: &str) -> Result<(), AiProposalRepositoryError> {
        Ok(())
    }
    async fn delete(&self, _proposal_id: &str) -> Result<(), AiProposalRepositoryError> {
        Ok(())
    }
    async fn count_pending_by_risk(&self) -> Result<Vec<(i16, i64)>, AiProposalRepositoryError> {
        Ok(vec![])
    }
    async fn count_failed_since(&self, _cutoff: DateTime<Utc>) -> Result<i64, AiProposalRepositoryError> {
        Ok(0)
    }
    async fn count_executed_since(&self, _cutoff: DateTime<Utc>) -> Result<i64, AiProposalRepositoryError> {
        Ok(0)
    }
    async fn smoke_summary(&self) -> Result<Option<SmokeProposalSummary>, AiProposalRepositoryError> {
        Ok(None)
    }
    async fn find_smoke_older_than(
        &self,
        _cutoff: DateTime<Utc>,
    ) -> Result<Vec<SmokeProposalRow>, AiProposalRepositoryError> {
        Ok(vec![])
    }
    async fn delete_smoke_older_than(
        &self,
        _proposal_ids: &[String],
        _cutoff: DateTime<Utc>,
    ) -> Result<u64, AiProposalRepositoryError> {
        Ok(0)
    }
}
