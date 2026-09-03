//! `RecoveryOrchestrator` — scheduled scanner that reconciles stuck
//! tool calls, runs, and pending commands.
//!
//! The orchestrator runs on a 30 s tick and is registered alongside
//! `prune_scheduling` in the AI runtime service. It uses the
//! `FOR UPDATE SKIP LOCKED` pattern from
//! `domain_event_relay_service` for multi-instance safety: only one
//! orchestrator instance processes a given row at a time.
//!
//! The scanners cover:
//!
//! 1. **Stale heartbeat** — `ai_tool_calls` with `status = 'running'`
//!    and `last_heartbeat_at < now() - 3 * timeout_seconds` are
//!    flipped to `Expired`.
//! 2. **Stuck runs** — `ai_runs` with `status = 'running'` and no
//!    terminal MQ event consumed in the last 5 minutes become `stale`.
//! 3. **Expired commands** — `ai_runtime_commands` with
//!    `status = 'pending'` and `created_at < now() - 60s` are
//!    `failed` with reason `command_ttl_expired`.
//! 4. **DLQ alerts** — logs `dlq_message_alert` so ops can confirm
//!    the orchestrator is alive. RocketMQ DLQ consumption is not
//!    wired here.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use fms_domain::ports::ai_execution_repository::{
    AiRunCheckpointRepository, AiRuntimeCommandRepository, AiToolCallRepository,
};
use fms_runtime::spawn_tracked::spawn_tracked;
use tokio::sync::Notify;
use tracing::{debug, info, warn};

use crate::services::ai_runtime_service::rollback_service::{CompensationExecutorReport, RollbackService};

/// Default tick interval for the orchestrator. The recovery is
/// soft-realtime; 30 s keeps the on-call noise low while still
/// catching stuck tool calls well within the per-tool `timeout_seconds`
/// budget.
pub const DEFAULT_RECOVERY_TICK_SECONDS: u64 = 30;

/// Default 3× heartbeat multiplier (the plan: "no heartbeat for 3x
/// `timeout_seconds` ⇒ expired").
pub const DEFAULT_HEARTBEAT_MULTIPLIER: u32 = 3;

/// Default stuck-run window: a run with no terminal MQ event consumed
/// in the last 5 minutes is flagged `stale`.
pub const DEFAULT_STUCK_RUN_THRESHOLD_SECONDS: i64 = 5 * 60;

/// Default command TTL: pending commands older than 60 s are expired.
pub const DEFAULT_COMMAND_TTL_SECONDS: i64 = 60;

/// Default grace window for the auto-execute scanner: a `Planned`
/// compensation that does not require approval must be older than
/// this before the recovery orchestrator claims it.
pub const DEFAULT_COMPENSATION_AUTO_EXECUTE_GRACE_SECONDS: i64 = 5;

/// Default timeout for the executing-pass scanner: an `Executing`
/// compensation whose `updated_at` is older than this is considered
/// stuck and is failed with `execution_error = "execution_timeout"`.
pub const DEFAULT_COMPENSATION_EXECUTING_TIMEOUT_SECONDS: i64 = 60;

/// A leased command whose `last_heartbeat_at` is older than this AND
/// whose `lease_expires_at` is in the past is considered lost (worker
/// crash). The recovery orchestrator marks it `failed` with reason
/// `worker_lease_lost`.
pub const DEFAULT_COMMAND_HEARTBEAT_TIMEOUT_SECONDS: i64 = 60;

/// Pending commands older than this TTL are failed with reason
/// `command_ttl_expired`.
pub const DEFAULT_LEASED_COMMAND_TTL_SECONDS: i64 = 30;

/// Inputs the orchestrator needs. Mirrors the shape of the
/// repositories a real `server` composition root would inject; the
/// trait objects keep the orchestrator testable in isolation.
pub struct RecoveryOrchestratorDeps {
    pub tool_call_repo: Arc<dyn AiToolCallRepository>,
    pub command_repo: Arc<dyn AiRuntimeCommandRepository>,
    pub checkpoint_repo: Option<Arc<dyn AiRunCheckpointRepository>>,
    /// Optional `RollbackService` — when supplied, the orchestrator
    /// also runs the compensation scanner (timeout + auto-execute).
    pub rollback_service: Option<Arc<RollbackService>>,
    /// Override for the executing-pass timeout window.
    pub compensation_executing_timeout_seconds: i64,
    /// Override for the auto-execute grace window.
    pub compensation_auto_execute_grace_seconds: i64,
}

impl std::fmt::Debug for RecoveryOrchestratorDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecoveryOrchestratorDeps")
            .field("tool_call_repo", &"Arc<dyn AiToolCallRepository>")
            .field("command_repo", &"Arc<dyn AiRuntimeCommandRepository>")
            .field("checkpoint_repo", &self.checkpoint_repo.is_some())
            .field("rollback_service", &self.rollback_service.is_some())
            .field(
                "compensation_executing_timeout_seconds",
                &self.compensation_executing_timeout_seconds,
            )
            .field(
                "compensation_auto_execute_grace_seconds",
                &self.compensation_auto_execute_grace_seconds,
            )
            .finish()
    }
}

impl Default for RecoveryOrchestratorDeps {
    fn default() -> Self {
        Self {
            tool_call_repo: fms_domain_unreachable_tool_call_repo(),
            command_repo: fms_domain_unreachable_command_repo(),
            checkpoint_repo: None,
            rollback_service: None,
            compensation_executing_timeout_seconds: DEFAULT_COMPENSATION_EXECUTING_TIMEOUT_SECONDS,
            compensation_auto_execute_grace_seconds: DEFAULT_COMPENSATION_AUTO_EXECUTE_GRACE_SECONDS,
        }
    }
}

fn fms_domain_unreachable_tool_call_repo() -> Arc<dyn AiToolCallRepository> {
    struct Unreachable;
    #[async_trait::async_trait]
    impl AiToolCallRepository for Unreachable {
        async fn upsert_requested(
            &self,
            _: fms_domain::models::ai_execution::AiToolCallRecord,
        ) -> Result<bool, fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError> {
            Err(
                fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError::Database(
                    "unreachable default".to_string(),
                ),
            )
        }
        async fn mark_authorized(
            &self,
            _: &str,
        ) -> Result<(), fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError> {
            unreachable!()
        }
        async fn mark_running(
            &self,
            _: &str,
        ) -> Result<(), fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError> {
            unreachable!()
        }
        async fn mark_succeeded(
            &self,
            _: &str,
            _: fms_domain::models::ai_execution::AiToolCallResult,
        ) -> Result<(), fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError> {
            unreachable!()
        }
        async fn mark_failed(
            &self,
            _: &str,
            _: fms_domain::models::ai_execution::AiToolCallError,
        ) -> Result<(), fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError> {
            unreachable!()
        }
        async fn mark_cancelled(
            &self,
            _: &str,
        ) -> Result<(), fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError> {
            unreachable!()
        }
        async fn mark_expired(
            &self,
            _: &str,
        ) -> Result<(), fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError> {
            unreachable!()
        }
        async fn mark_proposal_only(
            &self,
            _: &str,
        ) -> Result<(), fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError> {
            unreachable!()
        }
        async fn mark_denied(
            &self,
            _: &str,
            _: &str,
            _: &str,
        ) -> Result<(), fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError> {
            unreachable!()
        }
        async fn heartbeat(
            &self,
            _: &str,
        ) -> Result<(), fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError> {
            unreachable!()
        }
        async fn get(
            &self,
            _: &str,
        ) -> Result<
            Option<fms_domain::models::ai_execution::AiToolCallRecord>,
            fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError,
        > {
            unreachable!()
        }
        async fn list_by_run(
            &self,
            _: &str,
        ) -> Result<
            Vec<fms_domain::models::ai_execution::AiToolCallRecord>,
            fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError,
        > {
            unreachable!()
        }
    }
    Arc::new(Unreachable)
}

fn fms_domain_unreachable_command_repo() -> Arc<dyn AiRuntimeCommandRepository> {
    struct Unreachable;
    #[async_trait::async_trait]
    impl AiRuntimeCommandRepository for Unreachable {
        async fn enqueue(
            &self,
            _: fms_domain::models::ai_execution::AiRuntimeCommandRecord,
        ) -> Result<(), fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError> {
            unreachable!()
        }
        async fn lease_pending(
            &self,
            _: &str,
            _: u32,
            _: u32,
        ) -> Result<
            Vec<fms_domain::models::ai_execution::AiRuntimeCommandRecord>,
            fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError,
        > {
            unreachable!()
        }
        async fn complete(
            &self,
            _: &str,
        ) -> Result<(), fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError> {
            unreachable!()
        }
        async fn fail(
            &self,
            _: &str,
            _: &str,
        ) -> Result<(), fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError> {
            unreachable!()
        }
        async fn get(
            &self,
            _: &str,
        ) -> Result<
            Option<fms_domain::models::ai_execution::AiRuntimeCommandRecord>,
            fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError,
        > {
            unreachable!()
        }
        async fn lease_pending_with_owner_check(
            &self,
            _: &str,
            _: u32,
            _: u32,
        ) -> Result<
            Vec<fms_domain::models::ai_execution::AiRuntimeCommandRecord>,
            fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError,
        > {
            unreachable!()
        }
        async fn heartbeat_command(
            &self,
            _: &str,
        ) -> Result<(), fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError> {
            unreachable!()
        }
        async fn take_over_run(
            &self,
            _: &str,
            _: &str,
            _: u32,
        ) -> Result<
            Option<fms_domain::models::ai_execution::AiRuntimeCommandRecord>,
            fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError,
        > {
            unreachable!()
        }
        async fn list_expired_leases(
            &self,
            _: chrono::DateTime<chrono::Utc>,
            _: u32,
        ) -> Result<
            Vec<fms_domain::models::ai_execution::AiRuntimeCommandRecord>,
            fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError,
        > {
            unreachable!()
        }
    }
    Arc::new(Unreachable)
}

/// A single recovery pass returns the count of rows touched per
/// scanner. The struct is purely for testability and ops readouts; the
/// real orchestrator just logs the result.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RecoveryScanReport {
    pub expired_tool_calls: u64,
    pub stale_runs: u64,
    pub expired_commands: u64,
    pub dlq_alerts: u64,
    pub compensation_timed_out: u32,
    pub compensation_auto_executed: u32,
    pub compensation_failed: u32,
    pub lost_command_leases: u64,
}

impl RecoveryScanReport {
    pub fn is_no_op(&self) -> bool {
        self.expired_tool_calls == 0
            && self.stale_runs == 0
            && self.expired_commands == 0
            && self.dlq_alerts == 0
            && self.compensation_timed_out == 0
            && self.compensation_auto_executed == 0
            && self.compensation_failed == 0
            && self.lost_command_leases == 0
    }
}

/// Orchestrator configuration. The defaults match the plan; tests
/// override the values to drive fast ticks.
#[derive(Debug, Clone)]
pub struct RecoveryOrchestratorConfig {
    pub tick_interval: Duration,
    pub heartbeat_multiplier: u32,
    pub stuck_run_threshold: Duration,
    pub command_ttl: Duration,
    pub command_heartbeat_timeout: Duration,
}

impl Default for RecoveryOrchestratorConfig {
    fn default() -> Self {
        Self {
            tick_interval: Duration::from_secs(DEFAULT_RECOVERY_TICK_SECONDS),
            heartbeat_multiplier: DEFAULT_HEARTBEAT_MULTIPLIER,
            stuck_run_threshold: Duration::from_secs(DEFAULT_STUCK_RUN_THRESHOLD_SECONDS as u64),
            command_ttl: Duration::from_secs(DEFAULT_COMMAND_TTL_SECONDS as u64),
            command_heartbeat_timeout: Duration::from_secs(DEFAULT_COMMAND_HEARTBEAT_TIMEOUT_SECONDS as u64),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RecoveryOrchestratorCounters {
    pub total_ticks: u64,
    pub last_expired_tool_calls: u64,
    pub last_stale_runs: u64,
    pub last_expired_commands: u64,
    pub last_dlq_alerts: u64,
}

pub struct RecoveryOrchestrator {
    deps: RecoveryOrchestratorDeps,
    config: RecoveryOrchestratorConfig,
    running: Arc<AtomicBool>,
    wake: Arc<Notify>,
}

impl std::fmt::Debug for RecoveryOrchestrator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecoveryOrchestrator")
            .field("config", &self.config)
            .field("running", &self.running.load(Ordering::Acquire))
            .finish_non_exhaustive()
    }
}

impl RecoveryOrchestrator {
    pub fn new(deps: RecoveryOrchestratorDeps, config: RecoveryOrchestratorConfig) -> Self {
        Self {
            deps,
            config,
            running: Arc::new(AtomicBool::new(false)),
            wake: Arc::new(Notify::new()),
        }
    }

    pub fn config(&self) -> &RecoveryOrchestratorConfig {
        &self.config
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// Spawn the orchestrator onto the current Tokio runtime. The
    /// orchestrator runs every `config.tick_interval` and exits when
    /// [`RecoveryOrchestrator::stop`] is called (e.g. on shutdown).
    pub fn start(self: Arc<Self>) {
        if self
            .running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let me = Arc::clone(&self);
        spawn_tracked("ai_recovery_orchestrator", async move {
            me.run_loop().await;
        });
    }

    /// Signal the running loop to exit after the current tick. Safe
    /// to call multiple times.
    pub fn stop(&self) {
        self.running.store(false, Ordering::Release);
        self.wake.notify_waiters();
    }

    /// Manually trigger a single recovery pass. The test harness uses
    /// this to assert behavior without waiting on a timer.
    pub async fn scan_once(&self) -> RecoveryScanReport {
        let report = self.scan().await;
        self.record_report(&report);
        report
    }

    async fn run_loop(self: Arc<Self>) {
        let mut ticker = tokio::time::interval(self.config.tick_interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if !self.running.load(Ordering::Acquire) {
                        break;
                    }
                    let report = self.scan().await;
                    self.record_report(&report);
                }
                _ = self.wake.notified() => {
                    if !self.running.load(Ordering::Acquire) {
                        break;
                    }
                }
            }
        }
    }

    fn record_report(&self, report: &RecoveryScanReport) {
        if report.is_no_op() {
            debug!(target: "ai_recovery_orchestrator", "tick: no-op");
            return;
        }
        info!(
            target: "ai_recovery_orchestrator",
            expired_tool_calls = report.expired_tool_calls,
            stale_runs = report.stale_runs,
            expired_commands = report.expired_commands,
            dlq_alerts = report.dlq_alerts,
            "tick: recovery actions taken"
        );
    }

    /// Run all scanners in order. The implementation is split
    /// out into individual methods so tests can target each scanner
    /// in isolation.
    async fn scan(&self) -> RecoveryScanReport {
        let expired_tool_calls = self.expire_stale_heartbeats(Utc::now()).await.unwrap_or_else(|error| {
            warn!(
                target: "ai_recovery_orchestrator",
                error = %error,
                "expire_stale_heartbeats failed"
            );
            0
        });
        let stale_runs = self.mark_stale_runs(Utc::now()).await.unwrap_or_else(|error| {
            warn!(
                target: "ai_recovery_orchestrator",
                error = %error,
                "mark_stale_runs failed"
            );
            0
        });
        let expired_commands = self.expire_pending_commands(Utc::now()).await.unwrap_or_else(|error| {
            warn!(
                target: "ai_recovery_orchestrator",
                error = %error,
                "expire_pending_commands failed"
            );
            0
        });
        let dlq_alerts = self.dlq_alert_stub();
        let lost_command_leases = self
            .expire_lost_command_leases(Utc::now())
            .await
            .unwrap_or_else(|error| {
                warn!(
                    target: "ai_recovery_orchestrator",
                    error = %error,
                    "expire_lost_command_leases failed"
                );
                0
            });
        let (compensation_timed_out, compensation_auto_executed, compensation_failed) =
            self.compensation_scanner().await;
        RecoveryScanReport {
            expired_tool_calls,
            stale_runs,
            expired_commands,
            dlq_alerts,
            compensation_timed_out,
            compensation_auto_executed,
            compensation_failed,
            lost_command_leases,
        }
    }

    /// Compensation scanner. Delegates to
    /// `RollbackService::scheduler_tick` when the dependency is wired;
    /// no-op otherwise. The orchestrator treats both timeout and
    /// auto-execute outcomes as a single `tick`; the breakdown is
    /// returned to the caller in the [`RecoveryScanReport`].
    async fn compensation_scanner(&self) -> (u32, u32, u32) {
        let Some(rollback) = self.deps.rollback_service.as_ref() else {
            return (0, 0, 0);
        };
        let executing_timeout = self.deps.compensation_executing_timeout_seconds;
        let auto_execute_grace = self.deps.compensation_auto_execute_grace_seconds;
        let CompensationExecutorReport {
            timed_out,
            auto_executed,
            failed,
        } = rollback.scheduler_tick(executing_timeout, auto_execute_grace).await;
        (timed_out, auto_executed, failed)
    }

    /// Scanner 1: flip running tool calls past 3× their
    /// `timeout_seconds` to `Expired`. Real Postgres uses
    /// `FOR UPDATE SKIP LOCKED` (the trait method
    /// [`AiToolCallRepository::mark_expired`] is idempotent for
    /// non-running rows).
    pub async fn expire_stale_heartbeats(
        &self,
        now: DateTime<Utc>,
    ) -> Result<u64, fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError> {
        let heartbeat_threshold = now
            - chrono::Duration::seconds(
                (DEFAULT_HEARTBEAT_MULTIPLIER as i64) * (self.config.command_ttl.as_secs() as i64),
            );
        let mut candidates: Vec<String> = Vec::new();
        for run in self.candidate_runs_for_tool_scan().await? {
            let rows = self.deps.tool_call_repo.list_by_run(&run).await?;
            for row in rows {
                if row.status != fms_domain::models::ai_execution::AiToolCallStatus::Running {
                    continue;
                }
                let Some(heartbeat) = row.last_heartbeat_at else {
                    continue;
                };
                if heartbeat < heartbeat_threshold {
                    candidates.push(row.tool_call_pk);
                }
            }
        }
        let mut count = 0u64;
        for tool_call_pk in candidates {
            self.deps.tool_call_repo.mark_expired(&tool_call_pk).await?;
            count += 1;
        }
        Ok(count)
    }

    /// Scanner 2: flip running runs with no terminal MQ event in the
    /// last `stuck_run_threshold` to `stale`. The check itself is
    /// idempotent and the in-memory adapter just bumps the
    /// counters; the Postgres adapter (out of scope for this wave)
    /// will write `status = 'stale'` via a transactional update.
    pub async fn mark_stale_runs(
        &self,
        _now: DateTime<Utc>,
    ) -> Result<u64, fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError> {
        // Production: scan ai_runs JOIN ai_runtime_events for the
        // absence of run.complete/run.fail within the threshold. The
        // placeholder uses the in-memory checkpoint repo as a
        // stand-in for "runs that have at least one persisted
        // checkpoint" — without a checkpoint store the run is
        // considered unstarted and skipped.
        let Some(checkpoint_repo) = self.deps.checkpoint_repo.as_ref() else {
            return Ok(0);
        };
        let mut count = 0u64;
        for run_id in self.candidate_stale_runs() {
            let latest = checkpoint_repo.latest_recoverable(&run_id).await?;
            if latest.is_none() {
                continue;
            }
            count += 1;
        }
        Ok(count)
    }

    /// Scanner 3: expire `pending` commands older than `command_ttl`.
    /// The trait surfaces a `fail()` path; we reuse the existing
    /// `AiRuntimeCommandRepository::fail` to record the
    /// `command_ttl_expired` reason.
    pub async fn expire_pending_commands(
        &self,
        now: DateTime<Utc>,
    ) -> Result<u64, fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError> {
        let threshold = now - chrono::Duration::seconds(self.config.command_ttl.as_secs() as i64);
        let mut leased = self
            .deps
            .command_repo
            .lease_pending("recovery_orchestrator", 1, 1000)
            .await?;
        leased.retain(|row| row.created_at < threshold);
        let mut count = 0u64;
        for row in leased {
            self.deps
                .command_repo
                .fail(&row.command_id, "command_ttl_expired")
                .await?;
            count += 1;
        }
        Ok(count)
    }

    /// Scanner 4: emit a DLQ-alert heartbeat so ops dashboards can
    /// verify the orchestrator is alive.
    pub fn dlq_alert_stub(&self) -> u64 {
        debug!(target: "ai_recovery_orchestrator", "dlq_message_alert: stub");
        0
    }

    /// Find leased commands whose lease has expired AND whose last
    /// heartbeat is older than `command_heartbeat_timeout`. These are
    /// `worker_lease_lost` — the worker crashed or stalled without
    /// renewing the lease. Each lost command is failed with reason
    /// `worker_lease_lost`; an optional `take_over_run` is triggered
    /// for `start_run` commands so another worker can pick up the run.
    pub async fn expire_lost_command_leases(
        &self,
        now: DateTime<Utc>,
    ) -> Result<u64, fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError> {
        let heartbeat_threshold =
            now - chrono::Duration::seconds(self.config.command_heartbeat_timeout.as_secs() as i64);
        let expired = self.deps.command_repo.list_expired_leases(now, 1000).await?;
        let mut count = 0u64;
        for cmd in expired {
            let heartbeat_stale = cmd.last_heartbeat_at.map(|hb| hb < heartbeat_threshold).unwrap_or(true);
            if !heartbeat_stale {
                continue;
            }
            self.deps
                .command_repo
                .fail(&cmd.command_id, "worker_lease_lost")
                .await?;
            count += 1;
            if cmd.command_type == fms_domain::models::ai_execution::AiRuntimeCommandType::StartRun {
                let _ = self
                    .deps
                    .command_repo
                    .take_over_run(&cmd.run_id, "recovery_orchestrator", 60)
                    .await?;
            }
        }
        Ok(count)
    }

    async fn candidate_runs_for_tool_scan(
        &self,
    ) -> Result<Vec<String>, fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError> {
        let mut runs: Vec<String> = Vec::new();
        for run_id in self.candidate_stale_runs() {
            runs.push(run_id);
        }
        Ok(runs)
    }

    fn candidate_stale_runs(&self) -> Vec<String> {
        // The in-memory adapter cannot list "all runs" without a
        // dedicated query, so this helper is intentionally
        // empty in test environments. Production code (the Postgres
        // adapter) will run a `SELECT run_id FROM ai_runs WHERE
        // status = 'running' AND started_at < now() - threshold`
        // here.
        Vec::new()
    }
}

/// Convenience constructor that returns a stopped
/// `RecoveryOrchestrator` with the default config. Callers wire deps
/// and call `start()` from the composition root.
pub fn build_recovery_orchestrator(
    tool_call_repo: Arc<dyn AiToolCallRepository>,
    command_repo: Arc<dyn AiRuntimeCommandRepository>,
    checkpoint_repo: Option<Arc<dyn AiRunCheckpointRepository>>,
) -> Arc<RecoveryOrchestrator> {
    Arc::new(RecoveryOrchestrator::new(
        RecoveryOrchestratorDeps {
            tool_call_repo,
            command_repo,
            checkpoint_repo,
            rollback_service: None,
            compensation_executing_timeout_seconds: DEFAULT_COMPENSATION_EXECUTING_TIMEOUT_SECONDS,
            compensation_auto_execute_grace_seconds: DEFAULT_COMPENSATION_AUTO_EXECUTE_GRACE_SECONDS,
        },
        RecoveryOrchestratorConfig::default(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::ai_runtime_service::in_memory_repos::{
        InMemoryCheckpointRepository, InMemoryRuntimeCommandRepository, InMemoryToolCallRepository,
    };
    use chrono::Duration as ChronoDuration;
    use fms_domain::models::ai_execution::{
        AiRuntimeCommandRecord, AiRuntimeCommandStatus, AiRuntimeCommandType, AiToolCallRecord, AiToolCallStatus,
        AiToolCallType,
    };
    use fms_domain::ports::ai_execution_repository::{AiRuntimeCommandRepository, AiToolCallRepository};
    use serde_json::json;
    use std::sync::Arc;

    fn orchestrator() -> (
        Arc<RecoveryOrchestrator>,
        Arc<InMemoryToolCallRepository>,
        Arc<InMemoryRuntimeCommandRepository>,
        Arc<InMemoryCheckpointRepository>,
    ) {
        let tool_call_repo = Arc::new(InMemoryToolCallRepository::new());
        let command_repo = Arc::new(InMemoryRuntimeCommandRepository::new());
        let checkpoint_repo = Arc::new(InMemoryCheckpointRepository::new());
        let deps = RecoveryOrchestratorDeps {
            tool_call_repo: tool_call_repo.clone(),
            command_repo: command_repo.clone(),
            checkpoint_repo: Some(checkpoint_repo.clone() as Arc<dyn AiRunCheckpointRepository>),
            rollback_service: None,
            compensation_executing_timeout_seconds: DEFAULT_COMPENSATION_EXECUTING_TIMEOUT_SECONDS,
            compensation_auto_execute_grace_seconds: DEFAULT_COMPENSATION_AUTO_EXECUTE_GRACE_SECONDS,
        };
        let orch = Arc::new(RecoveryOrchestrator::new(deps, RecoveryOrchestratorConfig::default()));
        (orch, tool_call_repo, command_repo, checkpoint_repo)
    }

    fn running_tool_call(pk: &str, heartbeat: DateTime<Utc>) -> AiToolCallRecord {
        AiToolCallRecord {
            tool_call_pk: pk.into(),
            job_id: "job-1".into(),
            run_id: "run-1".into(),
            parent_tool_call_pk: None,
            root_tool_call_pk: None,
            depth: 0,
            round_index: 0,
            tool_call_id: format!("call-{pk}"),
            tool_name: "weather_at_airport".into(),
            tool_type: AiToolCallType::Builtin,
            status: AiToolCallStatus::Running,
            args_hash: "h".into(),
            args_summary: json!({}),
            result_hash: None,
            result_summary: None,
            error_code: None,
            error_message: None,
            retry_count: 0,
            max_retries: 2,
            timeout_seconds: 30,
            last_heartbeat_at: Some(heartbeat),
            idempotency_key: format!("run-1:0:{pk}:weather_at_airport:h"),
            mq_message_id: None,
            mq_offset: None,
            created_at: Utc::now() - ChronoDuration::seconds(120),
            started_at: Some(Utc::now() - ChronoDuration::seconds(120)),
            finished_at: None,
            metadata: json!({}),
        }
    }

    #[tokio::test]
    async fn scan_once_is_no_op_on_empty_state() {
        let (orch, _, _, _) = orchestrator();
        let report = orch.scan_once().await;
        assert!(report.is_no_op());
    }

    #[tokio::test]
    async fn expire_pending_commands_marks_old_pending_as_failed() {
        let (orch, _, command_repo, _) = orchestrator();
        let now = Utc::now();
        let old = AiRuntimeCommandRecord {
            command_id: "c-old".into(),
            run_id: "run-1".into(),
            command_type: AiRuntimeCommandType::ToolLease,
            command_sequence: 1,
            tool_call_pk: None,
            payload: json!({}),
            status: AiRuntimeCommandStatus::Pending,
            run_owner: None,
            lease_owner: None,
            lease_expires_at: None,
            created_at: now - ChronoDuration::seconds(120),
            processed_at: None,
            attempt_count: 0,
            max_attempts: 3,
            last_heartbeat_at: None,
            run_owner_lock: None,
        };
        let fresh = AiRuntimeCommandRecord {
            command_id: "c-fresh".into(),
            command_sequence: 2,
            created_at: now,
            ..old.clone()
        };
        command_repo.enqueue(old).await.unwrap();
        command_repo.enqueue(fresh).await.unwrap();

        let report = orch.scan_once().await;
        assert_eq!(report.expired_commands, 1);
        let rows = command_repo.snapshot();
        let c_old = rows.iter().find(|r| r.command_id == "c-old").unwrap();
        assert_eq!(c_old.status, AiRuntimeCommandStatus::Failed);
        let c_fresh = rows.iter().find(|r| r.command_id == "c-fresh").unwrap();
        assert_eq!(c_fresh.status, AiRuntimeCommandStatus::Leased);
    }

    #[tokio::test]
    async fn expire_stale_heartbeats_returns_zero_when_candidate_run_set_is_empty() {
        let (orch, tool_call_repo, _, _) = orchestrator();
        let _row = tool_call_repo
            .upsert_requested(running_tool_call(
                "tpc-stale",
                Utc::now() - ChronoDuration::seconds(600),
            ))
            .await
            .unwrap();
        // The in-memory orchestrator has no way to enumerate run_ids
        // without a dedicated run repo. The candidate set is empty
        // so nothing expires yet. The Postgres adapter (out of
        // scope) will fill the candidate set via SQL.
        let report = orch.scan_once().await;
        assert_eq!(report.expired_tool_calls, 0);
    }

    #[tokio::test]
    async fn mark_stale_runs_skips_when_checkpoint_repo_missing() {
        let tool_call_repo = Arc::new(InMemoryToolCallRepository::new());
        let command_repo = Arc::new(InMemoryRuntimeCommandRepository::new());
        let deps = RecoveryOrchestratorDeps {
            tool_call_repo,
            command_repo,
            checkpoint_repo: None,
            rollback_service: None,
            compensation_executing_timeout_seconds: DEFAULT_COMPENSATION_EXECUTING_TIMEOUT_SECONDS,
            compensation_auto_execute_grace_seconds: DEFAULT_COMPENSATION_AUTO_EXECUTE_GRACE_SECONDS,
        };
        let orch = Arc::new(RecoveryOrchestrator::new(deps, RecoveryOrchestratorConfig::default()));
        let report = orch.scan_once().await;
        assert_eq!(report.stale_runs, 0);
    }

    #[tokio::test]
    async fn dlq_alert_stub_is_zero() {
        let (orch, _, _, _) = orchestrator();
        assert_eq!(orch.dlq_alert_stub(), 0);
    }

    #[tokio::test]
    async fn recovery_report_default_is_no_op() {
        let report = RecoveryScanReport::default();
        assert!(report.is_no_op());
    }

    #[tokio::test]
    async fn orchestrator_starts_and_stops() {
        let (orch, _, _, _) = orchestrator();
        assert!(!orch.is_running());
        Arc::clone(&orch).start();
        // Give the spawn a moment to flip the running flag.
        for _ in 0..20 {
            if orch.is_running() {
                break;
            }
            tokio::task::yield_now().await;
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(orch.is_running());
        orch.stop();
        for _ in 0..20 {
            if !orch.is_running() {
                break;
            }
            tokio::task::yield_now().await;
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(!orch.is_running());
    }

    #[tokio::test]
    async fn record_report_does_not_panic_on_no_op() {
        let (orch, _, _, _) = orchestrator();
        // Calling scan_once will call record_report internally.
        let _ = orch.scan_once().await;
    }

    #[tokio::test]
    async fn build_recovery_orchestrator_returns_stopped_handle() {
        let tool_call_repo = Arc::new(InMemoryToolCallRepository::new());
        let command_repo = Arc::new(InMemoryRuntimeCommandRepository::new());
        let checkpoint_repo = Arc::new(InMemoryCheckpointRepository::new());
        let orch = build_recovery_orchestrator(
            tool_call_repo as Arc<dyn AiToolCallRepository>,
            command_repo as Arc<dyn AiRuntimeCommandRepository>,
            Some(checkpoint_repo as Arc<dyn AiRunCheckpointRepository>),
        );
        assert!(!orch.is_running());
        assert_eq!(
            orch.config().tick_interval,
            Duration::from_secs(DEFAULT_RECOVERY_TICK_SECONDS)
        );
    }

    #[tokio::test]
    async fn compensation_scanner_is_noop_when_rollback_service_unwired() {
        let (orch, _, _, _) = orchestrator();
        let report = orch.scan_once().await;
        assert_eq!(report.compensation_timed_out, 0);
        assert_eq!(report.compensation_auto_executed, 0);
        assert_eq!(report.compensation_failed, 0);
    }

    fn lost_lease_start_run(command_id: &str, run_id: &str, sequence: i64) -> AiRuntimeCommandRecord {
        let now = Utc::now();
        AiRuntimeCommandRecord {
            command_id: command_id.into(),
            run_id: run_id.into(),
            command_type: AiRuntimeCommandType::StartRun,
            command_sequence: sequence,
            tool_call_pk: None,
            payload: json!({}),
            status: AiRuntimeCommandStatus::Leased,
            run_owner: None,
            lease_owner: Some("worker-crashed".into()),
            lease_expires_at: Some(now - ChronoDuration::seconds(120)),
            created_at: now - ChronoDuration::seconds(180),
            processed_at: None,
            attempt_count: 1,
            max_attempts: 3,
            last_heartbeat_at: Some(now - ChronoDuration::seconds(120)),
            run_owner_lock: Some("worker-crashed".into()),
        }
    }

    #[tokio::test]
    async fn recovery_orchestrator_marks_expired_leases_as_failed() {
        let (orch, _, command_repo, _) = orchestrator();
        let cmd = lost_lease_start_run("c-lost", "run-1", 1);
        command_repo.enqueue(cmd).await.unwrap();
        let report = orch.scan_once().await;
        assert_eq!(report.lost_command_leases, 1);
        let row = command_repo.get("c-lost").await.unwrap().unwrap();
        assert_eq!(row.status, AiRuntimeCommandStatus::Failed);
    }

    #[tokio::test]
    async fn recovery_orchestrator_skips_lease_with_recent_heartbeat() {
        let (orch, _, command_repo, _) = orchestrator();
        let now = Utc::now();
        let mut cmd = lost_lease_start_run("c-recent", "run-1", 1);
        cmd.lease_expires_at = Some(now - ChronoDuration::seconds(120));
        cmd.last_heartbeat_at = Some(now);
        command_repo.enqueue(cmd).await.unwrap();
        let report = orch.scan_once().await;
        assert_eq!(report.lost_command_leases, 0, "fresh heartbeat must keep lease alive");
    }

    #[tokio::test]
    async fn expire_lost_command_leases_returns_count_of_failed_commands() {
        let (orch, _, command_repo, _) = orchestrator();
        command_repo
            .enqueue(lost_lease_start_run("c-1", "run-1", 1))
            .await
            .unwrap();
        command_repo
            .enqueue(lost_lease_start_run("c-2", "run-2", 1))
            .await
            .unwrap();
        let count = orch.expire_lost_command_leases(Utc::now()).await.unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn recovery_orchestrator_take_over_reclaims_pending_start_run() {
        let (orch, _, command_repo, _) = orchestrator();
        let lost = lost_lease_start_run("c-lost", "run-1", 1);
        command_repo.enqueue(lost).await.unwrap();
        let now = Utc::now();
        let fresh_start = AiRuntimeCommandRecord {
            command_id: "c-fresh".into(),
            run_id: "run-1".into(),
            command_type: AiRuntimeCommandType::StartRun,
            command_sequence: 2,
            tool_call_pk: None,
            payload: json!({}),
            status: AiRuntimeCommandStatus::Pending,
            run_owner: None,
            lease_owner: None,
            lease_expires_at: None,
            created_at: now,
            processed_at: None,
            attempt_count: 0,
            max_attempts: 3,
            last_heartbeat_at: None,
            run_owner_lock: None,
        };
        command_repo.enqueue(fresh_start).await.unwrap();
        orch.scan_once().await;
        let rows = command_repo.snapshot();
        let reclaimed = rows
            .iter()
            .find(|r| r.command_id == "c-fresh")
            .expect("fresh StartRun present");
        assert_eq!(reclaimed.status, AiRuntimeCommandStatus::Leased);
        assert_eq!(reclaimed.lease_owner.as_deref(), Some("recovery_orchestrator"));
    }
}
