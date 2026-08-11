//! `AiJobTimeoutReaperService` — scheduled scanner that reconciles
//! expired AI job leases. Modeled on `RecoveryOrchestrator`
//! (`ai_runtime_service::recovery_orchestrator`): `spawn_tracked` +
//! `tokio::time::interval` + `scan_expired_leases` → `take_over` or
//! `timeout_job`.
//!
//! ## Lifecycle
//!
//! 1. Every `tick_interval` (default 30 s), the reaper queries
//!    `ai_jobs` for rows in `claimed`/`running` whose
//!    `lease_expires_at < now` and `attempt_count < max_attempts`.
//! 2. For each expired job:
//!    - If `attempt_count < max_attempts`: `take_over_job` resets the
//!      lease so another worker (Rust or Python) can reclaim it.
//!    - If `attempt_count >= max_attempts`: `timeout_job` marks the
//!      job as `timed_out`, fails any active run, and writes an
//!      `ai_job.timed_out` outbox event for SSE fan-out.
//! 3. The loop exits on `stop()` or shutdown.
//!
//! ## Multi-instance safety
//!
//! `list_expired_leases` uses `FOR UPDATE SKIP LOCKED` in the
//! Postgres implementation, so concurrent reaper instances never
//! process the same job.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use fms_runtime::spawn_tracked::spawn_tracked;
use tokio::sync::Notify;
use tracing::{debug, info, warn};

use crate::services::ai_job_service::AiJobService;

/// Default tick interval: 30 s (same as `RecoveryOrchestrator`).
pub const DEFAULT_REAPER_TICK_SECONDS: u64 = 30;

/// Default lease duration granted on take-over: 60 s.
/// This is 2× the `ai_runtime_commands` lease (30 s) because AI jobs
/// are coarser-grained than individual tool commands.
pub const DEFAULT_REAPER_LEASE_SECONDS: i64 = 60;

/// Default scan batch size: how many expired leases to process per tick.
pub const DEFAULT_REAPER_SCAN_LIMIT: i64 = 1000;

/// Configuration for the reaper.
#[derive(Debug, Clone)]
pub struct ReaperConfig {
    pub tick_interval: Duration,
    pub lease_seconds: i64,
    pub scan_limit: i64,
}

impl Default for ReaperConfig {
    fn default() -> Self {
        Self {
            tick_interval: Duration::from_secs(DEFAULT_REAPER_TICK_SECONDS),
            lease_seconds: DEFAULT_REAPER_LEASE_SECONDS,
            scan_limit: DEFAULT_REAPER_SCAN_LIMIT,
        }
    }
}

/// Report returned by a single scan pass.
#[derive(Debug, Clone, Default)]
pub struct ReaperScanReport {
    pub expired_leases_found: u64,
    pub jobs_taken_over: u64,
    pub jobs_timed_out: u64,
    pub errors: u64,
}

impl ReaperScanReport {
    pub fn is_no_op(&self) -> bool {
        self.expired_leases_found == 0 && self.jobs_taken_over == 0 && self.jobs_timed_out == 0 && self.errors == 0
    }
}

/// Scheduled scanner that reconciles expired AI job leases.
pub struct AiJobTimeoutReaperService {
    job_service: Arc<AiJobService>,
    config: ReaperConfig,
    running: Arc<AtomicBool>,
    wake: Arc<Notify>,
}

impl std::fmt::Debug for AiJobTimeoutReaperService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiJobTimeoutReaperService")
            .field("config", &self.config)
            .field("running", &self.running.load(Ordering::Acquire))
            .finish_non_exhaustive()
    }
}

impl AiJobTimeoutReaperService {
    pub fn new(job_service: Arc<AiJobService>, config: ReaperConfig) -> Self {
        Self {
            job_service,
            config,
            running: Arc::new(AtomicBool::new(false)),
            wake: Arc::new(Notify::new()),
        }
    }

    pub fn config(&self) -> &ReaperConfig {
        &self.config
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// Spawn the reaper onto the current Tokio runtime. The reaper
    /// runs every `config.tick_interval` and exits when `stop()` is
    /// called. Safe to call multiple times — the second call is a no-op.
    pub fn start(self: Arc<Self>) {
        if self
            .running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let me = Arc::clone(&self);
        spawn_tracked("ai_job_timeout_reaper", async move {
            me.run_loop().await;
        });
    }

    /// Signal the running loop to exit after the current tick.
    pub fn stop(&self) {
        self.running.store(false, Ordering::Release);
        self.wake.notify_waiters();
    }

    /// Manually trigger a single scan pass (used by tests).
    pub async fn scan_once(&self) -> ReaperScanReport {
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

    fn record_report(&self, report: &ReaperScanReport) {
        if report.is_no_op() {
            debug!(target: "ai_job_timeout_reaper", "tick: no-op");
            return;
        }
        info!(
            target: "ai_job_timeout_reaper",
            expired_leases_found = report.expired_leases_found,
            jobs_taken_over = report.jobs_taken_over,
            jobs_timed_out = report.jobs_timed_out,
            errors = report.errors,
            "tick: reaper actions taken"
        );
    }

    /// Scan for expired leases and reconcile them.
    ///
    /// For each expired job:
    /// - If `attempt_count < max_attempts`: take over the job (reset lease
    ///   for another worker to reclaim).
    /// - If `attempt_count >= max_attempts`: mark the job as timed out,
    ///   fail any active run, and emit an `ai_job.timed_out` outbox event.
    async fn scan(&self) -> ReaperScanReport {
        let now = Utc::now();
        let mut report = ReaperScanReport::default();

        let expired = match self.job_service.list_expired_leases(self.config.scan_limit).await {
            Ok(jobs) => jobs,
            Err(error) => {
                warn!(
                    target: "ai_job_timeout_reaper",
                    error = %error,
                    "failed to list expired leases"
                );
                report.errors += 1;
                return report;
            }
        };

        report.expired_leases_found = expired.len() as u64;

        for job in expired {
            let job_id = job.job_id.clone();
            let should_timeout = job.attempt_count >= job.max_attempts;

            if should_timeout {
                let reason = format!(
                    "lease expired after {} attempt(s); max_attempts={}",
                    job.attempt_count, job.max_attempts
                );
                match self.job_service.timeout_job(&job_id, &reason).await {
                    Ok(_) => {
                        report.jobs_timed_out += 1;
                        debug!(
                            target: "ai_job_timeout_reaper",
                            job_id = %job_id,
                            attempt_count = job.attempt_count,
                            "job timed out"
                        );
                    }
                    Err(error) => {
                        report.errors += 1;
                        warn!(
                            target: "ai_job_timeout_reaper",
                            job_id = %job_id,
                            error = %error,
                            "failed to timeout job"
                        );
                    }
                }
            } else {
                let new_owner = "ai_job_timeout_reaper";
                match self
                    .job_service
                    .take_over_job(&job_id, new_owner, self.config.lease_seconds)
                    .await
                {
                    Ok(_) => {
                        report.jobs_taken_over += 1;
                        debug!(
                            target: "ai_job_timeout_reaper",
                            job_id = %job_id,
                            attempt_count = job.attempt_count,
                            "job lease taken over for retry"
                        );
                    }
                    Err(error) => {
                        report.errors += 1;
                        warn!(
                            target: "ai_job_timeout_reaper",
                            job_id = %job_id,
                            error = %error,
                            "failed to take over job"
                        );
                    }
                }
            }
        }

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_are_sane() {
        let cfg = ReaperConfig::default();
        assert_eq!(cfg.tick_interval, Duration::from_secs(30));
        assert_eq!(cfg.lease_seconds, 60);
        assert_eq!(cfg.scan_limit, 1000);
    }

    #[test]
    fn no_op_report_is_no_op() {
        assert!(ReaperScanReport::default().is_no_op());
        assert!(!ReaperScanReport {
            expired_leases_found: 1,
            ..Default::default()
        }
        .is_no_op());
    }
}
