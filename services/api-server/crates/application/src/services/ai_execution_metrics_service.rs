use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;

use fms_domain::ports::ai_proposal_repository::AiProposalRepository;
use fms_domain::ports::domain_event_outbox_repository::DomainEventOutboxRepository;

#[derive(Debug, Clone, Serialize)]
pub struct ExecutionMetricsSnapshot {
    pub pending_proposal_count_by_risk: Vec<RiskLevelCount>,
    pub failed_proposal_count_24h: i64,
    pub executed_proposal_count_24h: i64,
    pub outbox_unprocessed_count: i64,
    pub outbox_oldest_unprocessed_age_seconds: Option<i64>,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RiskLevelCount {
    pub risk_level: i32,
    pub count: i64,
}

pub struct AiExecutionMetricsService {
    proposal_repo: Arc<dyn AiProposalRepository + Send + Sync>,
    outbox_repo: Arc<dyn DomainEventOutboxRepository + Send + Sync>,
}

impl AiExecutionMetricsService {
    pub fn new(
        proposal_repo: Arc<dyn AiProposalRepository + Send + Sync>,
        outbox_repo: Arc<dyn DomainEventOutboxRepository + Send + Sync>,
    ) -> Self {
        Self {
            proposal_repo,
            outbox_repo,
        }
    }

    pub async fn snapshot(&self) -> Result<ExecutionMetricsSnapshot, String> {
        let pending_proposal_count_by_risk = self.query_pending_by_risk().await?;
        let failed_proposal_count_24h = self.query_failed_count_24h().await?;
        let executed_proposal_count_24h = self.query_executed_count_24h().await?;
        let outbox_unprocessed_count = self.query_outbox_unprocessed_count().await?;
        let outbox_oldest_unprocessed_age_seconds = self.query_outbox_oldest_age().await?;

        Ok(ExecutionMetricsSnapshot {
            pending_proposal_count_by_risk,
            failed_proposal_count_24h,
            executed_proposal_count_24h,
            outbox_unprocessed_count,
            outbox_oldest_unprocessed_age_seconds,
            generated_at: Utc::now(),
        })
    }

    async fn query_pending_by_risk(&self) -> Result<Vec<RiskLevelCount>, String> {
        let rows = self
            .proposal_repo
            .count_pending_by_risk()
            .await
            .map_err(|e| format!("failed to query pending proposals by risk: {e}"))?;

        Ok(rows
            .into_iter()
            .map(|(risk_level, count)| RiskLevelCount {
                risk_level: risk_level as i32,
                count,
            })
            .collect())
    }

    async fn query_failed_count_24h(&self) -> Result<i64, String> {
        let cutoff = Utc::now() - chrono::Duration::hours(24);
        self.proposal_repo
            .count_failed_since(cutoff)
            .await
            .map_err(|e| format!("failed to query failed proposals: {e}"))
    }

    async fn query_executed_count_24h(&self) -> Result<i64, String> {
        let cutoff = Utc::now() - chrono::Duration::hours(24);
        self.proposal_repo
            .count_executed_since(cutoff)
            .await
            .map_err(|e| format!("failed to query executed proposals: {e}"))
    }

    async fn query_outbox_unprocessed_count(&self) -> Result<i64, String> {
        self.outbox_repo
            .count_unpublished()
            .await
            .map_err(|e| format!("failed to query outbox unprocessed count: {e}"))
    }

    async fn query_outbox_oldest_age(&self) -> Result<Option<i64>, String> {
        let oldest = self
            .outbox_repo
            .oldest_unpublished()
            .await
            .map_err(|e| format!("failed to query oldest outbox event: {e}"))?;

        Ok(oldest.map(|occurred_at| {
            let now = Utc::now();
            (now - occurred_at).num_seconds()
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use fms_domain::error::DomainError;
    use fms_domain::events::DomainEventOutboxRow;
    use fms_domain::ports::ai_proposal_repository::{
        AiProposalRepository, AiProposalRepositoryError, SmokeProposalRow, SmokeProposalSummary,
    };

    struct StubOutbox {
        count: i64,
        oldest: Option<DateTime<Utc>>,
    }

    #[async_trait]
    impl DomainEventOutboxRepository for StubOutbox {
        async fn claim_pending_for_relay(&self, _limit: i64) -> Result<Vec<DomainEventOutboxRow>, DomainError> {
            Ok(vec![])
        }
        async fn count_unpublished(&self) -> Result<i64, DomainError> {
            Ok(self.count)
        }
        async fn oldest_unpublished(&self) -> Result<Option<DateTime<Utc>>, DomainError> {
            Ok(self.oldest)
        }
        async fn delete_by_aggregate_and_type(
            &self,
            _aggregate_id: &str,
            _event_type: &str,
            _older_than: DateTime<Utc>,
        ) -> Result<u64, DomainError> {
            Ok(0)
        }
        async fn count_by_aggregates_and_type(
            &self,
            _aggregate_ids: &[String],
            _event_type: &str,
            _older_than: DateTime<Utc>,
        ) -> Result<i64, DomainError> {
            Ok(0)
        }
        async fn delete_by_aggregates_and_type(
            &self,
            _aggregate_ids: &[String],
            _event_type: &str,
            _older_than: DateTime<Utc>,
        ) -> Result<u64, DomainError> {
            Ok(0)
        }
        async fn insert_event(
            &self,
            _aggregate_type: &str,
            _aggregate_id: &str,
            _event_type: &str,
            _payload: serde_json::Value,
            _source_change_id: &str,
        ) -> Result<String, DomainError> {
            Ok("stub_event".to_string())
        }
    }

    struct StubProposalRepo;

    #[async_trait]
    impl AiProposalRepository for StubProposalRepo {
        async fn save(
            &self,
            _: &fms_domain::models::ai_proposal::AiActionProposal,
        ) -> Result<(), AiProposalRepositoryError> {
            Ok(())
        }
        async fn find_by_id(
            &self,
            _: &str,
        ) -> Result<Option<fms_domain::models::ai_proposal::AiActionProposal>, AiProposalRepositoryError> {
            Ok(None)
        }
        async fn find_by_pending_action_id(
            &self,
            _: &str,
        ) -> Result<Option<fms_domain::models::ai_proposal::AiActionProposal>, AiProposalRepositoryError> {
            Ok(None)
        }
        async fn find_by_job_id(
            &self,
            _: &str,
        ) -> Result<Vec<fms_domain::models::ai_proposal::AiActionProposal>, AiProposalRepositoryError> {
            Ok(vec![])
        }
        async fn find_by_run_id(
            &self,
            _: &str,
        ) -> Result<Vec<fms_domain::models::ai_proposal::AiActionProposal>, AiProposalRepositoryError> {
            Ok(vec![])
        }
        async fn find_by_object(
            &self,
            _: &str,
            _: &str,
        ) -> Result<Vec<fms_domain::models::ai_proposal::AiActionProposal>, AiProposalRepositoryError> {
            Ok(vec![])
        }
        async fn search(
            &self,
            _: &fms_domain::models::ai_proposal::ActionProposalQuery,
        ) -> Result<Vec<fms_domain::models::ai_proposal::AiActionProposal>, AiProposalRepositoryError> {
            Ok(vec![])
        }
        async fn find_pending(
            &self,
        ) -> Result<Vec<fms_domain::models::ai_proposal::AiActionProposal>, AiProposalRepositoryError> {
            Ok(vec![])
        }
        async fn find_expired(
            &self,
        ) -> Result<Vec<fms_domain::models::ai_proposal::AiActionProposal>, AiProposalRepositoryError> {
            Ok(vec![])
        }
        async fn count(
            &self,
            _: &fms_domain::models::ai_proposal::ActionProposalQuery,
        ) -> Result<usize, AiProposalRepositoryError> {
            Ok(0)
        }
        async fn get_stats(
            &self,
        ) -> Result<fms_domain::models::ai_proposal::ActionProposalStats, AiProposalRepositoryError> {
            Ok(fms_domain::models::ai_proposal::ActionProposalStats::default())
        }
        async fn update_status(
            &self,
            _: &str,
            _: fms_domain::models::ai_proposal::ActionProposalStatus,
        ) -> Result<(), AiProposalRepositoryError> {
            Ok(())
        }
        async fn link_pending_action(&self, _: &str, _: &str) -> Result<(), AiProposalRepositoryError> {
            Ok(())
        }
        async fn unlink_pending_action(&self, _: &str) -> Result<(), AiProposalRepositoryError> {
            Ok(())
        }
        async fn delete(&self, _: &str) -> Result<(), AiProposalRepositoryError> {
            Ok(())
        }
        async fn count_pending_by_risk(&self) -> Result<Vec<(i16, i64)>, AiProposalRepositoryError> {
            Ok(vec![])
        }
        async fn count_failed_since(&self, _: DateTime<Utc>) -> Result<i64, AiProposalRepositoryError> {
            Ok(0)
        }
        async fn count_executed_since(&self, _: DateTime<Utc>) -> Result<i64, AiProposalRepositoryError> {
            Ok(0)
        }
        async fn smoke_summary(&self) -> Result<Option<SmokeProposalSummary>, AiProposalRepositoryError> {
            Ok(None)
        }
        async fn find_smoke_older_than(
            &self,
            _: DateTime<Utc>,
        ) -> Result<Vec<SmokeProposalRow>, AiProposalRepositoryError> {
            Ok(vec![])
        }
        async fn delete_smoke_older_than(
            &self,
            _: &[String],
            _: DateTime<Utc>,
        ) -> Result<u64, AiProposalRepositoryError> {
            Ok(0)
        }
    }

    #[tokio::test]
    async fn metrics_snapshot_returns_zero_defaults_with_stub_repo() {
        let proposal_repo = Arc::new(StubProposalRepo);
        let outbox = Arc::new(StubOutbox { count: 0, oldest: None });
        let service = AiExecutionMetricsService::new(proposal_repo, outbox);
        let snapshot = service.snapshot().await.expect("snapshot");

        assert!(snapshot.outbox_unprocessed_count >= 0);
        assert_eq!(snapshot.failed_proposal_count_24h, 0);
        assert_eq!(snapshot.executed_proposal_count_24h, 0);
        assert!(snapshot.generated_at <= chrono::Utc::now());
    }
}
