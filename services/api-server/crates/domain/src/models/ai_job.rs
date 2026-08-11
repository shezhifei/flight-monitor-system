use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Durable AI job (conversation / work unit) record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiJobRecord {
    pub job_id: String,
    pub job_type: String,
    pub status: String,
    pub requester_user_id: Option<String>,
    pub ontology_version: Option<String>,
    pub context_policy: Option<Value>,
    pub risk_ceiling: Option<String>,
    pub correlation_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    /// Job-level timeout in milliseconds (reaper fallback when no lease).
    pub timeout_ms: Option<i64>,
    /// Current lease holder (worker id). None when job is not leased.
    pub lease_owner: Option<String>,
    /// When the current lease expires. Reaper scans this for stale jobs.
    pub lease_expires_at: Option<DateTime<Utc>>,
    /// Last heartbeat timestamp from the worker holding the lease.
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    /// Number of claim attempts (incremented on each lease/retry).
    pub attempt_count: i32,
    /// Maximum allowed attempts before the job is moved to dead_letter.
    pub max_attempts: i32,
    /// Absolute TTL for the job result. Reaper cleans up jobs past this time.
    pub expires_at: Option<DateTime<Utc>>,
}

/// Single LLM / runtime invocation under an AI job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRunRecord {
    pub run_id: String,
    pub job_id: String,
    pub runtime_engine: String,
    pub model_id: Option<String>,
    pub status: String,
    pub input_envelope: Option<Value>,
    pub output_raw: Option<Value>,
    pub output_validated: Option<Value>,
    pub token_usage: Option<Value>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Append-only event under an AI run (and job).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRunEventRecord {
    pub event_id: i64,
    pub job_id: String,
    pub run_id: String,
    pub event_type: String,
    pub payload: Option<Value>,
    pub created_at: DateTime<Utc>,
}

/// Per-status job count for rollup dashboards.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiJobStatusCount {
    pub status: String,
    pub count: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiJobStatus {
    Pending,
    Claimed,
    Running,
    Succeeded,
    FailedTerminal,
    TimedOut,
    Retrying,
    Cancelled,
    DeadLetter,
}

impl AiJobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Claimed => "claimed",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::FailedTerminal => "failed_terminal",
            Self::TimedOut => "timed_out",
            Self::Retrying => "retrying",
            Self::Cancelled => "cancelled",
            Self::DeadLetter => "dead_letter",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "claimed" => Some(Self::Claimed),
            "running" => Some(Self::Running),
            "succeeded" => Some(Self::Succeeded),
            "failed_terminal" => Some(Self::FailedTerminal),
            "timed_out" => Some(Self::TimedOut),
            "retrying" => Some(Self::Retrying),
            "cancelled" => Some(Self::Cancelled),
            "dead_letter" => Some(Self::DeadLetter),
            _ => None,
        }
    }

    pub fn can_transition_to(&self, target: &AiJobStatus) -> bool {
        match self {
            Self::Pending => matches!(target, Self::Claimed | Self::Cancelled | Self::DeadLetter),
            Self::Claimed => matches!(target, Self::Running | Self::Pending | Self::TimedOut | Self::Cancelled),
            Self::Running => matches!(
                target,
                Self::Succeeded | Self::FailedTerminal | Self::TimedOut | Self::Retrying | Self::Cancelled
            ),
            Self::Retrying => matches!(target, Self::Pending | Self::Cancelled | Self::DeadLetter),
            Self::TimedOut => matches!(target, Self::Retrying | Self::DeadLetter | Self::Cancelled),
            Self::Succeeded | Self::FailedTerminal | Self::Cancelled | Self::DeadLetter => false,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::FailedTerminal | Self::Cancelled | Self::DeadLetter
        )
    }
}

impl std::fmt::Display for AiJobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiRunStatus {
    Pending,
    Claimed,
    Running,
    Succeeded,
    FailedTerminal,
    FailedRecoverable,
    Stale,
    TimedOut,
    Cancelled,
}

impl AiRunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Claimed => "claimed",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::FailedTerminal => "failed_terminal",
            Self::FailedRecoverable => "failed_recoverable",
            Self::Stale => "stale",
            Self::TimedOut => "timed_out",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "claimed" => Some(Self::Claimed),
            "running" => Some(Self::Running),
            "succeeded" => Some(Self::Succeeded),
            "failed_terminal" => Some(Self::FailedTerminal),
            "failed_recoverable" => Some(Self::FailedRecoverable),
            "stale" => Some(Self::Stale),
            "timed_out" => Some(Self::TimedOut),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    pub fn can_transition_to(&self, target: &AiRunStatus) -> bool {
        match self {
            Self::Pending => matches!(target, Self::Claimed | Self::Cancelled),
            Self::Claimed => matches!(target, Self::Running | Self::Pending | Self::TimedOut | Self::Cancelled),
            Self::Running => matches!(
                target,
                Self::Succeeded
                    | Self::FailedTerminal
                    | Self::FailedRecoverable
                    | Self::Stale
                    | Self::TimedOut
                    | Self::Cancelled
            ),
            Self::FailedRecoverable => matches!(target, Self::Running | Self::Cancelled),
            Self::Stale => matches!(target, Self::Running | Self::FailedTerminal | Self::Cancelled),
            Self::TimedOut => matches!(target, Self::Cancelled),
            Self::Succeeded | Self::FailedTerminal | Self::Cancelled => false,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Succeeded | Self::FailedTerminal | Self::Cancelled)
    }
}

impl std::fmt::Display for AiRunStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_pending_can_transition_to_claimed() {
        assert!(AiJobStatus::Pending.can_transition_to(&AiJobStatus::Claimed));
    }

    #[test]
    fn job_pending_cannot_jump_to_succeeded() {
        assert!(!AiJobStatus::Pending.can_transition_to(&AiJobStatus::Succeeded));
    }

    #[test]
    fn job_claimed_can_transition_to_running() {
        assert!(AiJobStatus::Claimed.can_transition_to(&AiJobStatus::Running));
    }

    #[test]
    fn job_claimed_can_release_back_to_pending() {
        assert!(AiJobStatus::Claimed.can_transition_to(&AiJobStatus::Pending));
    }

    #[test]
    fn job_running_can_succeed() {
        assert!(AiJobStatus::Running.can_transition_to(&AiJobStatus::Succeeded));
    }

    #[test]
    fn job_running_can_retry() {
        assert!(AiJobStatus::Running.can_transition_to(&AiJobStatus::Retrying));
    }

    #[test]
    fn job_running_can_fail_terminal() {
        assert!(AiJobStatus::Running.can_transition_to(&AiJobStatus::FailedTerminal));
    }

    #[test]
    fn job_retrying_goes_back_to_pending() {
        assert!(AiJobStatus::Retrying.can_transition_to(&AiJobStatus::Pending));
    }

    #[test]
    fn job_retrying_can_dead_letter() {
        assert!(AiJobStatus::Retrying.can_transition_to(&AiJobStatus::DeadLetter));
    }

    #[test]
    fn job_timed_out_can_retry() {
        assert!(AiJobStatus::TimedOut.can_transition_to(&AiJobStatus::Retrying));
    }

    #[test]
    fn job_timed_out_can_dead_letter() {
        assert!(AiJobStatus::TimedOut.can_transition_to(&AiJobStatus::DeadLetter));
    }

    #[test]
    fn job_succeeded_is_terminal() {
        assert!(AiJobStatus::Succeeded.is_terminal());
        assert!(!AiJobStatus::Succeeded.can_transition_to(&AiJobStatus::Running));
    }

    #[test]
    fn job_failed_terminal_is_terminal() {
        assert!(AiJobStatus::FailedTerminal.is_terminal());
    }

    #[test]
    fn job_cancelled_is_terminal() {
        assert!(AiJobStatus::Cancelled.is_terminal());
    }

    #[test]
    fn job_dead_letter_is_terminal() {
        assert!(AiJobStatus::DeadLetter.is_terminal());
    }

    #[test]
    fn run_pending_can_transition_to_claimed() {
        assert!(AiRunStatus::Pending.can_transition_to(&AiRunStatus::Claimed));
    }

    #[test]
    fn run_running_can_succeed() {
        assert!(AiRunStatus::Running.can_transition_to(&AiRunStatus::Succeeded));
    }

    #[test]
    fn run_succeeded_is_terminal() {
        assert!(AiRunStatus::Succeeded.is_terminal());
    }

    #[test]
    fn job_status_display() {
        assert_eq!(AiJobStatus::FailedTerminal.to_string(), "failed_terminal");
        assert_eq!(AiJobStatus::DeadLetter.to_string(), "dead_letter");
    }

    #[test]
    fn run_status_display() {
        assert_eq!(AiRunStatus::FailedTerminal.to_string(), "failed_terminal");
    }

    #[test]
    fn job_status_from_str_roundtrip() {
        for status in [
            AiJobStatus::Pending,
            AiJobStatus::Claimed,
            AiJobStatus::Running,
            AiJobStatus::Succeeded,
            AiJobStatus::FailedTerminal,
            AiJobStatus::TimedOut,
            AiJobStatus::Retrying,
            AiJobStatus::Cancelled,
            AiJobStatus::DeadLetter,
        ] {
            assert_eq!(AiJobStatus::from_str(status.as_str()), Some(status));
        }
    }

    #[test]
    fn run_status_from_str_roundtrip() {
        for status in [
            AiRunStatus::Pending,
            AiRunStatus::Claimed,
            AiRunStatus::Running,
            AiRunStatus::Succeeded,
            AiRunStatus::FailedTerminal,
            AiRunStatus::FailedRecoverable,
            AiRunStatus::Stale,
            AiRunStatus::TimedOut,
            AiRunStatus::Cancelled,
        ] {
            assert_eq!(AiRunStatus::from_str(status.as_str()), Some(status));
        }
    }

    #[test]
    fn run_recoverable_can_transition_to_running() {
        assert!(AiRunStatus::FailedRecoverable.can_transition_to(&AiRunStatus::Running));
        assert!(AiRunStatus::FailedRecoverable.can_transition_to(&AiRunStatus::Cancelled));
    }

    #[test]
    fn run_stale_can_transition_to_running() {
        assert!(AiRunStatus::Stale.can_transition_to(&AiRunStatus::Running));
        assert!(AiRunStatus::Stale.can_transition_to(&AiRunStatus::FailedTerminal));
    }
}
