use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use fms_domain::ports::ai_proposal_repository::AiProposalRepository;
use fms_domain::ports::ai_run_event_repository::AiRunEventRepository;
use fms_domain::ports::database_metadata_port::DatabaseMetadataPort;
use fms_domain::ports::domain_event_outbox_repository::DomainEventOutboxRepository;
use fms_domain::ports::todo_repository::{TodoRepository, TodoTransactionalRepository};

use crate::services::ai_execution_allowlist::ExecutionAllowlist;
use crate::services::ai_execution_metrics_service::AiExecutionMetricsService;
use crate::services::ai_execution_readiness_service::AiExecutionReadinessService;
use crate::sqlx_transactional_repositories::SqlxTodoTransactionalRepository;

#[derive(Debug, Clone, Serialize)]
pub struct RolloutStatusResponse {
    pub execution_enabled: bool,
    pub execution_mode: String,
    pub readiness_override: Option<String>,
    pub readiness: fms_domain::models::ai_execution_readiness::AiExecutionReadinessReport,
    pub metrics: MetricsSummary,
    pub recent_smoke: Option<SmokeSummary>,
    pub allowed_actions: Vec<String>,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsSummary {
    pub pending_proposals: i64,
    pub failed_proposals_24h: i64,
    pub executed_proposals_24h: i64,
    pub outbox_unprocessed: i64,
    pub outbox_oldest_age_seconds: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SmokeSummary {
    pub last_run_at: Option<DateTime<Utc>>,
    pub total_smoke_proposals: i64,
    pub succeeded: i64,
    pub failed: i64,
    pub blocked_by_readiness: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmokeCleanupResult {
    pub dry_run: bool,
    pub cutoff_at: DateTime<Utc>,
    pub database_name: String,
    pub smoke_proposals_matched: i64,
    pub audit_events_deleted: i64,
    pub outbox_events_deleted: i64,
    pub todos_soft_deleted: i64,
    pub proposals_deleted: i64,
}

pub struct AiRolloutStatusService {
    readiness_service: Arc<AiExecutionReadinessService>,
    metrics_service: Arc<AiExecutionMetricsService>,
    proposal_repo: Arc<dyn AiProposalRepository + Send + Sync>,
    todo_repo: Arc<dyn TodoRepository + Send + Sync>,
    todo_tx_repo: Arc<dyn SqlxTodoTransactionalRepository>,
    db_metadata_port: Arc<dyn DatabaseMetadataPort + Send + Sync>,
    pool: PgPool,
    outbox_repo: Arc<dyn DomainEventOutboxRepository + Send + Sync>,
    run_event_repo: Arc<dyn AiRunEventRepository + Send + Sync>,
}

impl AiRolloutStatusService {
    pub fn new(
        readiness_service: Arc<AiExecutionReadinessService>,
        metrics_service: Arc<AiExecutionMetricsService>,
        proposal_repo: Arc<dyn AiProposalRepository + Send + Sync>,
        todo_repo: Arc<dyn TodoRepository + Send + Sync>,
        todo_tx_repo: Arc<dyn SqlxTodoTransactionalRepository>,
        db_metadata_port: Arc<dyn DatabaseMetadataPort + Send + Sync>,
        pool: PgPool,
        outbox_repo: Arc<dyn DomainEventOutboxRepository + Send + Sync>,
        run_event_repo: Arc<dyn AiRunEventRepository + Send + Sync>,
    ) -> Self {
        Self {
            readiness_service,
            metrics_service,
            proposal_repo,
            todo_repo,
            todo_tx_repo,
            db_metadata_port,
            pool,
            outbox_repo,
            run_event_repo,
        }
    }

    pub async fn evaluate(&self) -> Result<RolloutStatusResponse, String> {
        let allowlist = ExecutionAllowlist::from_env();
        let metrics_snapshot = self.metrics_service.snapshot().await?;
        let pending_proposals = metrics_snapshot
            .pending_proposal_count_by_risk
            .iter()
            .map(|item| item.count)
            .sum();

        Ok(RolloutStatusResponse {
            execution_enabled: allowlist.is_execution_enabled(),
            execution_mode: allowlist.execution_mode().to_string(),
            readiness_override: std::env::var("FMS_AI_EXECUTION_READINESS_OVERRIDE").ok(),
            readiness: self.readiness_service.evaluate().await,
            metrics: MetricsSummary {
                pending_proposals,
                failed_proposals_24h: metrics_snapshot.failed_proposal_count_24h,
                executed_proposals_24h: metrics_snapshot.executed_proposal_count_24h,
                outbox_unprocessed: metrics_snapshot.outbox_unprocessed_count,
                outbox_oldest_age_seconds: metrics_snapshot.outbox_oldest_unprocessed_age_seconds,
            },
            recent_smoke: self.query_smoke_summary().await?,
            allowed_actions: allowlist.allowed_actions(),
            generated_at: Utc::now(),
        })
    }

    async fn query_smoke_summary(&self) -> Result<Option<SmokeSummary>, String> {
        let summary = self
            .proposal_repo
            .smoke_summary()
            .await
            .map_err(|e| format!("failed to query smoke proposal summary: {e}"))?;

        let Some(s) = summary else {
            return Ok(None);
        };

        let blocked_by_readiness = self
            .run_event_repo
            .count_smoke_readiness_blocks("proposal.execution_blocked_readiness")
            .await
            .map_err(|e| format!("failed to query smoke readiness blocks: {e}"))?;

        Ok(Some(SmokeSummary {
            last_run_at: s.last_run_at,
            total_smoke_proposals: s.total,
            succeeded: s.succeeded,
            failed: s.failed,
            blocked_by_readiness,
        }))
    }

    pub async fn cleanup_smoke_data(
        &self,
        older_than_hours: i64,
        dry_run: bool,
        confirm: bool,
    ) -> Result<SmokeCleanupResult, String> {
        if older_than_hours < 1 {
            return Err("older_than_hours must be at least 1".to_string());
        }

        if !dry_run && !confirm {
            return Err("confirm=true is required for execution".to_string());
        }

        let cleanup_enabled = std::env::var("FMS_AI_SMOKE_CLEANUP_ENABLED")
            .map(|val| val.trim() == "true")
            .unwrap_or(false);

        if !cleanup_enabled {
            return Err("FMS_AI_SMOKE_CLEANUP_ENABLED=true env variable is required".to_string());
        }

        // Get database name to verify it's test/staging and not production
        let db_name = self
            .db_metadata_port
            .current_database_name()
            .await
            .map_err(|e| format!("failed to query current database name: {e}"))?;

        let lower_db = db_name.to_lowercase();
        if lower_db.contains("prod") || lower_db.contains("production") {
            return Err(format!(
                "Cleanup rejected: database '{db_name}' contains production marker"
            ));
        }
        if !lower_db.contains("test") && !lower_db.contains("staging") {
            return Err(format!(
                "Cleanup rejected: database '{db_name}' does not contain 'test' or 'staging'"
            ));
        }

        let cutoff_at = Utc::now() - chrono::Duration::hours(older_than_hours);

        // First find matching smoke proposals via port
        let smoke_proposals = self
            .proposal_repo
            .find_smoke_older_than(cutoff_at)
            .await
            .map_err(|e| format!("failed to query old smoke proposals: {e}"))?;

        let smoke_proposals_matched = smoke_proposals.len() as i64;

        if smoke_proposals_matched == 0 {
            return Ok(SmokeCleanupResult {
                dry_run,
                cutoff_at,
                database_name: db_name,
                smoke_proposals_matched: 0,
                audit_events_deleted: 0,
                outbox_events_deleted: 0,
                todos_soft_deleted: 0,
                proposals_deleted: 0,
            });
        }

        let proposal_ids: Vec<String> = smoke_proposals.iter().map(|p| p.proposal_id.clone()).collect();
        let object_ids: Vec<String> = smoke_proposals.iter().map(|p| p.object_id.clone()).collect();
        let job_ids: Vec<String> = smoke_proposals.iter().filter_map(|p| p.job_id.clone()).collect();

        if dry_run {
            // Count matching audit events
            let audit_events_deleted = self
                .run_event_repo
                .count_by_job_ids_before(&job_ids, cutoff_at)
                .await
                .map_err(|e| format!("failed to count dry-run audit events: {e}"))?;

            // Count outbox events
            let outbox_events_deleted = self
                .outbox_repo
                .count_by_aggregates_and_type(&object_ids, "Todo.create", cutoff_at)
                .await
                .map_err(|e| format!("failed to count dry-run outbox events: {e}"))?;

            // Count todos to soft delete
            let todos_soft_deleted = self
                .todo_repo
                .count_by_source_ids("ai_action", &proposal_ids, cutoff_at)
                .await
                .map_err(|e| format!("failed to count dry-run todos: {e}"))?;

            return Ok(SmokeCleanupResult {
                dry_run: true,
                cutoff_at,
                database_name: db_name,
                smoke_proposals_matched,
                audit_events_deleted,
                outbox_events_deleted,
                todos_soft_deleted,
                proposals_deleted: smoke_proposals_matched,
            });
        }

        // Audit/outbox/proposal cleanup via ports (outside tx — ports have no in-tx delete).
        let audit_events_deleted = self
            .run_event_repo
            .delete_by_job_ids_before(&job_ids, cutoff_at)
            .await
            .map_err(|e| format!("failed to delete audit events: {e}"))? as i64;

        let outbox_events_deleted = self
            .outbox_repo
            .delete_by_aggregates_and_type(&object_ids, "Todo.create", cutoff_at)
            .await
            .map_err(|e| format!("failed to delete outbox events: {e}"))? as i64;

        let proposals_deleted = self
            .proposal_repo
            .delete_smoke_older_than(&proposal_ids, cutoff_at)
            .await
            .map_err(|e| format!("failed to delete proposals: {e}"))? as i64;

        // Execute remaining operations (todos) in a transaction
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| format!("failed to begin tx: {e}"))?;

        let todos_soft_deleted = self
            .todo_tx_repo
            .soft_delete_by_source_ids(&mut tx, "ai_action", &proposal_ids, cutoff_at)
            .await
            .map_err(|e| format!("failed to update todos: {e}"))? as i64;

        tx.commit().await.map_err(|e| format!("failed to commit tx: {e}"))?;

        Ok(SmokeCleanupResult {
            dry_run: false,
            cutoff_at,
            database_name: db_name,
            smoke_proposals_matched,
            audit_events_deleted,
            outbox_events_deleted,
            todos_soft_deleted,
            proposals_deleted,
        })
    }
}
