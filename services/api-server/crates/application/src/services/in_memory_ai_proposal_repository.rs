//! 进程内 AiActionProposal 仓储实现。
//!
//! 复刻 `PgAiProposalRepository` 的可观察语义（按 proposal_id upsert、
//! `search`/`count` 的过滤集合、`created_at DESC` 排序、缺失行更新静默成功），
//! 让以 proposal 为中心的服务逻辑与测试无需真实 Postgres 即可运行。

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{Map, Value};

use fms_domain::models::ai_proposal::{
    ActionProposalQuery, ActionProposalStats, ActionProposalStatus, AiActionProposal,
};
use fms_domain::ports::ai_proposal_repository::{
    AiProposalRepository, AiProposalRepositoryError, SmokeProposalRow, SmokeProposalSummary,
};

/// 进程内 [`AiProposalRepository`] 实现，可作为生产 Pg 仓储的测试替身。
#[derive(Debug, Default)]
pub struct InMemoryAiProposalRepository {
    proposals: Mutex<HashMap<String, AiActionProposal>>,
}

impl InMemoryAiProposalRepository {
    pub fn new() -> Self {
        Self::default()
    }

    /// 用一批初始 proposal 预填仓储（便于测试构造已有状态）。
    pub fn with_proposals(proposals: impl IntoIterator<Item = AiActionProposal>) -> Self {
        let map = proposals
            .into_iter()
            .map(|proposal| (proposal.proposal_id.clone(), proposal))
            .collect();
        Self {
            proposals: Mutex::new(map),
        }
    }

    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, AiActionProposal>> {
        self.proposals.lock().expect("in-memory proposal store poisoned")
    }

    /// 复刻 Pg `search` 的 WHERE 子句（注意：Pg 未按 requester_user_id 过滤，此处保持一致）。
    fn matches(query: &ActionProposalQuery, proposal: &AiActionProposal) -> bool {
        let opt_str = |filter: &Option<String>, value: &str| filter.as_ref().map(|f| f == value).unwrap_or(true);

        opt_str(&query.job_id, &proposal.job_id)
            && opt_str(&query.run_id, &proposal.run_id)
            && opt_str(&query.object_type, &proposal.object_type)
            && opt_str(&query.object_id, &proposal.object_id)
            && opt_str(&query.action_name, &proposal.action_name)
            && query.status.map(|s| proposal.status == s).unwrap_or(true)
            && query.risk_level.map(|r| proposal.risk_level == r).unwrap_or(true)
            && query
                .approval_policy
                .map(|a| proposal.approval_policy == a)
                .unwrap_or(true)
            && query
                .pending_action_id
                .as_ref()
                .map(|pid| proposal.pending_action_id.as_deref() == Some(pid.as_str()))
                .unwrap_or(true)
            && query
                .idempotency_key
                .as_ref()
                .map(|key| {
                    proposal
                        .metadata
                        .as_object()
                        .and_then(|m| m.get("idempotency_key"))
                        .and_then(|v| v.as_str())
                        == Some(key.as_str())
                })
                .unwrap_or(true)
            && query.created_after.map(|t| proposal.created_at >= t).unwrap_or(true)
            && query.created_before.map(|t| proposal.created_at <= t).unwrap_or(true)
    }

    /// 过滤并按 `created_at DESC`（proposal_id 二级排序保证确定性）排序的完整匹配集合。
    fn filtered_sorted(&self, query: &ActionProposalQuery) -> Vec<AiActionProposal> {
        let store = self.lock();
        let mut items: Vec<AiActionProposal> = store
            .values()
            .filter(|proposal| Self::matches(query, proposal))
            .cloned()
            .collect();
        items.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.proposal_id.cmp(&b.proposal_id))
        });
        items
    }
}

#[async_trait]
impl AiProposalRepository for InMemoryAiProposalRepository {
    async fn save(&self, proposal: &AiActionProposal) -> Result<(), AiProposalRepositoryError> {
        self.lock().insert(proposal.proposal_id.clone(), proposal.clone());
        Ok(())
    }

    async fn find_by_id(&self, proposal_id: &str) -> Result<Option<AiActionProposal>, AiProposalRepositoryError> {
        Ok(self.lock().get(proposal_id).cloned())
    }

    async fn find_by_pending_action_id(
        &self,
        pending_action_id: &str,
    ) -> Result<Option<AiActionProposal>, AiProposalRepositoryError> {
        Ok(self
            .lock()
            .values()
            .find(|proposal| proposal.pending_action_id.as_deref() == Some(pending_action_id))
            .cloned())
    }

    async fn find_by_job_id(&self, job_id: &str) -> Result<Vec<AiActionProposal>, AiProposalRepositoryError> {
        self.search(&ActionProposalQuery {
            job_id: Some(job_id.to_string()),
            ..Default::default()
        })
        .await
    }

    async fn find_by_run_id(&self, run_id: &str) -> Result<Vec<AiActionProposal>, AiProposalRepositoryError> {
        self.search(&ActionProposalQuery {
            run_id: Some(run_id.to_string()),
            ..Default::default()
        })
        .await
    }

    async fn find_by_object(
        &self,
        object_type: &str,
        object_id: &str,
    ) -> Result<Vec<AiActionProposal>, AiProposalRepositoryError> {
        self.search(&ActionProposalQuery {
            object_type: Some(object_type.to_string()),
            object_id: Some(object_id.to_string()),
            ..Default::default()
        })
        .await
    }

    async fn search(&self, query: &ActionProposalQuery) -> Result<Vec<AiActionProposal>, AiProposalRepositoryError> {
        let offset = query.offset.unwrap_or(0);
        let limit = query.limit.unwrap_or(50).min(200);
        Ok(self
            .filtered_sorted(query)
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect())
    }

    async fn find_pending(&self) -> Result<Vec<AiActionProposal>, AiProposalRepositoryError> {
        self.search(&ActionProposalQuery {
            status: Some(ActionProposalStatus::Pending),
            ..Default::default()
        })
        .await
    }

    async fn find_expired(&self) -> Result<Vec<AiActionProposal>, AiProposalRepositoryError> {
        let now = Utc::now();
        let mut items: Vec<AiActionProposal> = self
            .lock()
            .values()
            .filter(|proposal| proposal.expires_at.map(|expiry| expiry < now).unwrap_or(false))
            .cloned()
            .collect();
        items.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.proposal_id.cmp(&b.proposal_id))
        });
        Ok(items)
    }

    async fn count(&self, query: &ActionProposalQuery) -> Result<usize, AiProposalRepositoryError> {
        Ok(self
            .lock()
            .values()
            .filter(|proposal| Self::matches(query, proposal))
            .count())
    }

    async fn get_stats(&self) -> Result<ActionProposalStats, AiProposalRepositoryError> {
        let store = self.lock();
        let total = store.len();
        if total == 0 {
            return Ok(ActionProposalStats::default());
        }

        let mut by_status: Map<String, Value> = Map::new();
        let mut by_risk: Map<String, Value> = Map::new();
        let mut by_object: Map<String, Value> = Map::new();
        let mut confidence_sum = 0.0_f64;
        let (mut approved, mut rejected, mut executed, mut failed) = (0_u64, 0_u64, 0_u64, 0_u64);

        for proposal in store.values() {
            confidence_sum += proposal.confidence;
            bump(&mut by_status, proposal.status.label());
            bump(&mut by_risk, proposal.risk_level.label());
            bump(&mut by_object, &proposal.object_type);
            match proposal.status {
                ActionProposalStatus::Approved => approved += 1,
                ActionProposalStatus::Rejected => rejected += 1,
                ActionProposalStatus::Executed => executed += 1,
                ActionProposalStatus::Failed => failed += 1,
                _ => {}
            }
        }

        let terminal = approved + rejected;
        let execution_terminal = executed + failed;
        let ratio = |numerator: u64, denominator: u64| {
            if denominator > 0 {
                numerator as f64 / denominator as f64
            } else {
                0.0
            }
        };

        Ok(ActionProposalStats {
            total,
            by_status: Value::Object(by_status),
            by_risk_level: Value::Object(by_risk),
            by_object_type: Value::Object(by_object),
            avg_confidence: confidence_sum / total as f64,
            approval_rate: ratio(approved, terminal),
            rejection_rate: ratio(rejected, terminal),
            execution_success_rate: ratio(executed, execution_terminal),
            avg_execution_time_ms: None,
        })
    }

    async fn update_status(
        &self,
        proposal_id: &str,
        status: ActionProposalStatus,
    ) -> Result<(), AiProposalRepositoryError> {
        if let Some(proposal) = self.lock().get_mut(proposal_id) {
            proposal.status = status;
            proposal.updated_at = Utc::now();
        }
        Ok(())
    }

    async fn link_pending_action(
        &self,
        proposal_id: &str,
        pending_action_id: &str,
    ) -> Result<(), AiProposalRepositoryError> {
        if let Some(proposal) = self.lock().get_mut(proposal_id) {
            proposal.pending_action_id = Some(pending_action_id.to_string());
            proposal.updated_at = Utc::now();
        }
        Ok(())
    }

    async fn unlink_pending_action(&self, pending_action_id: &str) -> Result<(), AiProposalRepositoryError> {
        let now = Utc::now();
        for proposal in self.lock().values_mut() {
            if proposal.pending_action_id.as_deref() == Some(pending_action_id) {
                proposal.pending_action_id = None;
                proposal.updated_at = now;
            }
        }
        Ok(())
    }

    async fn delete(&self, proposal_id: &str) -> Result<(), AiProposalRepositoryError> {
        self.lock().remove(proposal_id);
        Ok(())
    }

    async fn count_pending_by_risk(&self) -> Result<Vec<(i16, i64)>, AiProposalRepositoryError> {
        let mut counts: std::collections::HashMap<i16, i64> = std::collections::HashMap::new();
        for proposal in self.lock().values() {
            if proposal.status == ActionProposalStatus::Pending {
                *counts.entry(proposal.risk_level.code() as i16).or_default() += 1;
            }
        }
        let mut result: Vec<(i16, i64)> = counts.into_iter().collect();
        result.sort_by_key(|(risk, _)| *risk);
        Ok(result)
    }

    async fn count_failed_since(&self, cutoff: DateTime<Utc>) -> Result<i64, AiProposalRepositoryError> {
        let count = self
            .lock()
            .values()
            .filter(|p| p.status == ActionProposalStatus::Failed && p.updated_at >= cutoff)
            .count();
        Ok(count as i64)
    }

    async fn count_executed_since(&self, cutoff: DateTime<Utc>) -> Result<i64, AiProposalRepositoryError> {
        let count = self
            .lock()
            .values()
            .filter(|p| p.status == ActionProposalStatus::Executed && p.updated_at >= cutoff)
            .count();
        Ok(count as i64)
    }

    async fn smoke_summary(&self) -> Result<Option<SmokeProposalSummary>, AiProposalRepositoryError> {
        let proposals: Vec<_> = self
            .lock()
            .values()
            .filter(|p| p.metadata.get("smoke").and_then(|v| v.as_str()) == Some("true"))
            .cloned()
            .collect();

        if proposals.is_empty() {
            return Ok(None);
        }

        let last_run_at = proposals.iter().map(|p| p.created_at).max();
        let total = proposals.len() as i64;
        let succeeded = proposals
            .iter()
            .filter(|p| p.status == ActionProposalStatus::Executed)
            .count() as i64;
        let failed = proposals
            .iter()
            .filter(|p| p.status == ActionProposalStatus::Failed)
            .count() as i64;

        Ok(Some(SmokeProposalSummary {
            last_run_at,
            total,
            succeeded,
            failed,
        }))
    }

    async fn find_smoke_older_than(
        &self,
        cutoff: DateTime<Utc>,
    ) -> Result<Vec<SmokeProposalRow>, AiProposalRepositoryError> {
        let rows: Vec<SmokeProposalRow> = self
            .lock()
            .values()
            .filter(|p| p.created_at < cutoff && p.metadata.get("smoke").and_then(|v| v.as_str()) == Some("true"))
            .map(|p| SmokeProposalRow {
                proposal_id: p.proposal_id.clone(),
                object_id: p.object_id.clone(),
                job_id: Some(p.job_id.clone()),
                run_id: Some(p.run_id.clone()),
            })
            .collect();
        Ok(rows)
    }

    async fn delete_smoke_older_than(
        &self,
        proposal_ids: &[String],
        cutoff: DateTime<Utc>,
    ) -> Result<u64, AiProposalRepositoryError> {
        let ids: std::collections::HashSet<&str> = proposal_ids.iter().map(|s| s.as_str()).collect();
        let mut store = self.lock();
        let before = store.len();
        store.retain(|_, p| {
            if ids.contains(p.proposal_id.as_str()) && p.created_at < cutoff {
                return false;
            }
            true
        });
        Ok((before - store.len()) as u64)
    }
}

fn bump(map: &mut Map<String, Value>, key: &str) {
    let next = map.get(key).and_then(Value::as_u64).unwrap_or(0) + 1;
    map.insert(key.to_string(), Value::from(next));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;

    fn proposal(id: &str, job: &str, status: ActionProposalStatus) -> AiActionProposal {
        let mut p = AiActionProposal::new(id, job, "run", "Flight", "FL1", "change_stand", json!({}));
        p.status = status;
        p.confidence = 0.5;
        p
    }

    #[tokio::test]
    async fn save_then_find_by_id_roundtrips() {
        let repo = InMemoryAiProposalRepository::new();
        assert!(repo.is_empty());
        repo.save(&proposal("p1", "j1", ActionProposalStatus::Draft))
            .await
            .unwrap();

        let found = repo.find_by_id("p1").await.unwrap().expect("present");
        assert_eq!(found.proposal_id, "p1");
        assert_eq!(repo.len(), 1);
        assert!(repo.find_by_id("missing").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn save_upserts_by_proposal_id() {
        let repo = InMemoryAiProposalRepository::new();
        repo.save(&proposal("p1", "j1", ActionProposalStatus::Draft))
            .await
            .unwrap();
        repo.save(&proposal("p1", "j1", ActionProposalStatus::Approved))
            .await
            .unwrap();
        assert_eq!(repo.len(), 1);
        assert_eq!(
            repo.find_by_id("p1").await.unwrap().unwrap().status,
            ActionProposalStatus::Approved
        );
    }

    #[tokio::test]
    async fn search_filters_by_status_and_respects_limit_offset_and_order() {
        let repo = InMemoryAiProposalRepository::new();
        // 三个 pending、一个 approved；created_at 递增以验证 DESC 排序
        for (i, id) in ["a", "b", "c"].iter().enumerate() {
            let mut p = proposal(id, "j1", ActionProposalStatus::Pending);
            p.created_at = Utc::now() + chrono::Duration::seconds(i as i64);
            repo.save(&p).await.unwrap();
        }
        repo.save(&proposal("d", "j1", ActionProposalStatus::Approved))
            .await
            .unwrap();

        let pending = repo
            .search(&ActionProposalQuery {
                status: Some(ActionProposalStatus::Pending),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(pending.len(), 3);
        // created_at DESC → c (latest) first
        assert_eq!(pending[0].proposal_id, "c");

        let page = repo
            .search(&ActionProposalQuery {
                status: Some(ActionProposalStatus::Pending),
                limit: Some(1),
                offset: Some(1),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].proposal_id, "b");

        assert_eq!(repo.find_pending().await.unwrap().len(), 3);
        assert_eq!(
            repo.count(&ActionProposalQuery {
                status: Some(ActionProposalStatus::Pending),
                ..Default::default()
            })
            .await
            .unwrap(),
            3
        );
    }

    #[tokio::test]
    async fn find_by_job_run_and_object() {
        let repo = InMemoryAiProposalRepository::new();
        let mut p = proposal("p1", "j1", ActionProposalStatus::Draft);
        p.object_id = "FL_X".to_string();
        repo.save(&p).await.unwrap();
        repo.save(&proposal("p2", "j2", ActionProposalStatus::Draft))
            .await
            .unwrap();

        assert_eq!(repo.find_by_job_id("j1").await.unwrap().len(), 1);
        assert_eq!(repo.find_by_run_id("run").await.unwrap().len(), 2);
        assert_eq!(repo.find_by_object("Flight", "FL_X").await.unwrap().len(), 1);
        assert_eq!(repo.find_by_object("Flight", "nope").await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn find_expired_only_returns_past_expiries() {
        let repo = InMemoryAiProposalRepository::new();
        let mut past = proposal("past", "j1", ActionProposalStatus::Pending);
        past.expires_at = Some(Utc::now() - chrono::Duration::hours(1));
        let mut future = proposal("future", "j1", ActionProposalStatus::Pending);
        future.expires_at = Some(Utc::now() + chrono::Duration::hours(1));
        repo.save(&past).await.unwrap();
        repo.save(&future).await.unwrap();
        repo.save(&proposal("noexp", "j1", ActionProposalStatus::Pending))
            .await
            .unwrap();

        let expired = repo.find_expired().await.unwrap();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].proposal_id, "past");
    }

    #[tokio::test]
    async fn update_status_and_pending_action_link_unlink() {
        let repo = InMemoryAiProposalRepository::new();
        repo.save(&proposal("p1", "j1", ActionProposalStatus::Pending))
            .await
            .unwrap();

        repo.update_status("p1", ActionProposalStatus::Approved).await.unwrap();
        assert_eq!(
            repo.find_by_id("p1").await.unwrap().unwrap().status,
            ActionProposalStatus::Approved
        );
        // 缺失行静默成功（与 Pg 一致）
        repo.update_status("missing", ActionProposalStatus::Failed)
            .await
            .unwrap();

        repo.link_pending_action("p1", "pa-9").await.unwrap();
        let by_pending = repo.find_by_pending_action_id("pa-9").await.unwrap().expect("linked");
        assert_eq!(by_pending.proposal_id, "p1");

        repo.unlink_pending_action("pa-9").await.unwrap();
        assert!(repo.find_by_pending_action_id("pa-9").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_removes_proposal() {
        let repo = InMemoryAiProposalRepository::new();
        repo.save(&proposal("p1", "j1", ActionProposalStatus::Draft))
            .await
            .unwrap();
        repo.delete("p1").await.unwrap();
        assert!(repo.is_empty());
        // 缺失行删除静默成功
        repo.delete("p1").await.unwrap();
    }

    #[tokio::test]
    async fn get_stats_aggregates_counts_and_rates() {
        let repo = InMemoryAiProposalRepository::with_proposals([
            proposal("a", "j1", ActionProposalStatus::Approved),
            proposal("b", "j1", ActionProposalStatus::Rejected),
            proposal("c", "j1", ActionProposalStatus::Executed),
            proposal("d", "j1", ActionProposalStatus::Failed),
            proposal("e", "j1", ActionProposalStatus::Pending),
        ]);

        let stats = repo.get_stats().await.unwrap();
        assert_eq!(stats.total, 5);
        assert!((stats.avg_confidence - 0.5).abs() < 1e-9);
        // approved/(approved+rejected) = 1/2
        assert!((stats.approval_rate - 0.5).abs() < 1e-9);
        assert!((stats.rejection_rate - 0.5).abs() < 1e-9);
        // executed/(executed+failed) = 1/2
        assert!((stats.execution_success_rate - 0.5).abs() < 1e-9);
        assert_eq!(stats.by_status["approved"], json!(1));
        assert_eq!(stats.by_object_type["Flight"], json!(5));
    }

    #[tokio::test]
    async fn get_stats_on_empty_repo_is_default() {
        let repo = InMemoryAiProposalRepository::new();
        let stats = repo.get_stats().await.unwrap();
        assert_eq!(stats.total, 0);
        assert_eq!(stats.approval_rate, 0.0);
    }

    /// 端到端示范：AiActionProposalService 经由内存仓储完成 generate→持久化→查询，
    /// 全程不依赖 Postgres——此前等价覆盖只能以 `#[ignore]` + TEST_DATABASE_URL 运行。
    #[tokio::test]
    async fn service_generate_persists_through_in_memory_repo_without_pool() {
        use crate::services::ai_action_proposal_service::{AiActionProposalService, GenerateProposalRequest};

        let repo = Arc::new(InMemoryAiProposalRepository::new());
        let service = AiActionProposalService::new().with_repository(repo.clone());

        let req = GenerateProposalRequest {
            job_id: "job-1".to_string(),
            run_id: "run-1".to_string(),
            ontology_version: Some("flight-ops.v1".to_string()),
            object_type: "Flight".to_string(),
            object_id: "FL123".to_string(),
            action_name: "change_stand".to_string(),
            arguments: json!({ "new_stand": "S02" }),
            reasoning: Some("test".to_string()),
            confidence: Some(0.9),
            requester_user_id: Some("generator".to_string()),
            requester_user_roles: vec!["flight:write".to_string()],
            requester_department_id: None,
            correlation_id: None,
            idempotency_key: None,
            expected_object_version: None,
            risk_level: None,
            approval_policy: None,
            // None → 服务依据 object_type/action_name 推断所需权限，由 flight:write 满足。
            required_permissions: None,
        };

        let proposal = service.generate_proposal(req).await.expect("generate succeeds");

        let stored = repo
            .find_by_id(&proposal.proposal_id)
            .await
            .unwrap()
            .expect("proposal persisted through the service");
        assert_eq!(stored.proposal_id, proposal.proposal_id);
        assert_eq!(stored.object_id, "FL123");
        assert_eq!(repo.len(), 1);
    }
}
