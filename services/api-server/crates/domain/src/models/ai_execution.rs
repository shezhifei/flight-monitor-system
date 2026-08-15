//! Durable execution control plane records.
//!
//! Durable ledger for tool calls and the Rust → Python command queue.
//!
//! Two tables back this module:
//! * `ai_tool_calls` — one row per tool invocation. Replaces the
//!   implicit SSE-only "tool call" assumption with a stateful ledger
//!   (idempotency key, status, retries, heartbeat, MQ coordinates).
//! * `ai_runtime_commands` — Rust -> Python command queue (start /
//!   cancel / tool_lease / tool_denied / tool_proposal_only /
//!   retry_tool / resume_run). Skipped-locked consumption by
//!   `AiCommandConsumer`.
//!
//! The records here are the *domain* shape used by the application
//! service and the API read endpoints. Infrastructure adapters map
//! between these structs and the Postgres tables.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Tool call ledger
// ---------------------------------------------------------------------------

/// Status of a single `ai_tool_calls` row.
///
/// State machine (from the plan):
///
/// ```text
/// requested -> authorized -> running -> succeeded
///       |          |             |-> failed_retryable -> requested (via retry command)
///       |          |             |-> failed_terminal
///       |          |             |-> cancelled
///       |          |             |-> expired
///       |          |-> proposal_only
///       |-> denied
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiToolCallStatus {
    Requested,
    Authorized,
    Running,
    Succeeded,
    FailedRetryable,
    FailedTerminal,
    Cancelled,
    Expired,
    Denied,
    ProposalOnly,
}

impl AiToolCallStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Authorized => "authorized",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::FailedRetryable => "failed_retryable",
            Self::FailedTerminal => "failed_terminal",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
            Self::Denied => "denied",
            Self::ProposalOnly => "proposal_only",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "requested" => Some(Self::Requested),
            "authorized" => Some(Self::Authorized),
            "running" => Some(Self::Running),
            "succeeded" => Some(Self::Succeeded),
            "failed_retryable" => Some(Self::FailedRetryable),
            "failed_terminal" => Some(Self::FailedTerminal),
            "cancelled" => Some(Self::Cancelled),
            "expired" => Some(Self::Expired),
            "denied" => Some(Self::Denied),
            "proposal_only" => Some(Self::ProposalOnly),
            _ => None,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded
                | Self::FailedTerminal
                | Self::Cancelled
                | Self::Expired
                | Self::Denied
                | Self::ProposalOnly
        )
    }
}

impl std::fmt::Display for AiToolCallStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Origin / runtime family of a tool call. Matches the `tool_type`
/// column in `ai_tool_calls`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiToolCallType {
    Builtin,
    Mcp,
    Skill,
    Subagent,
}

impl AiToolCallType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::Mcp => "mcp",
            Self::Skill => "skill",
            Self::Subagent => "subagent",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "builtin" => Some(Self::Builtin),
            "mcp" => Some(Self::Mcp),
            "skill" => Some(Self::Skill),
            "subagent" => Some(Self::Subagent),
            _ => None,
        }
    }
}

impl std::fmt::Display for AiToolCallType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Persisted success result for a tool call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiToolCallResult {
    pub result_hash: Option<String>,
    pub result_summary: Option<Value>,
    pub proposal_ids: Vec<String>,
    pub duration_ms: u64,
}

/// Persisted failure for a tool call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiToolCallError {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub retryable: bool,
}

/// One row from `ai_tool_calls`. The shape mirrors the table column
/// set; infrastructure adapters populate all `Option<_>` columns from
/// `NULL`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiToolCallRecord {
    pub tool_call_pk: String,
    pub job_id: String,
    pub run_id: String,
    pub parent_tool_call_pk: Option<String>,
    pub root_tool_call_pk: Option<String>,
    pub depth: i32,
    pub round_index: i32,
    pub tool_call_id: String,
    pub tool_name: String,
    pub tool_type: AiToolCallType,
    pub status: AiToolCallStatus,
    pub args_hash: String,
    pub args_summary: Value,
    pub result_hash: Option<String>,
    pub result_summary: Option<Value>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub retry_count: i32,
    pub max_retries: i32,
    pub timeout_seconds: i32,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub idempotency_key: String,
    pub mq_message_id: Option<String>,
    pub mq_offset: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub metadata: Value,
}

// ---------------------------------------------------------------------------
// Runtime command queue
// ---------------------------------------------------------------------------

/// Command type discriminator. Strings match the `command_type` column
/// in `ai_runtime_commands`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiRuntimeCommandType {
    StartRun,
    CancelRun,
    ToolLease,
    ToolDenied,
    ToolProposalOnly,
    RetryTool,
    ResumeRun,
}

impl AiRuntimeCommandType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StartRun => "start_run",
            Self::CancelRun => "cancel_run",
            Self::ToolLease => "tool_lease",
            Self::ToolDenied => "tool_denied",
            Self::ToolProposalOnly => "tool_proposal_only",
            Self::RetryTool => "retry_tool",
            Self::ResumeRun => "resume_run",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "start_run" => Some(Self::StartRun),
            "cancel_run" => Some(Self::CancelRun),
            "tool_lease" => Some(Self::ToolLease),
            "tool_denied" => Some(Self::ToolDenied),
            "tool_proposal_only" => Some(Self::ToolProposalOnly),
            "retry_tool" => Some(Self::RetryTool),
            "resume_run" => Some(Self::ResumeRun),
            _ => None,
        }
    }
}

impl std::fmt::Display for AiRuntimeCommandType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Lifecycle status of a single command row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiRuntimeCommandStatus {
    Pending,
    Leased,
    Completed,
    Failed,
}

impl AiRuntimeCommandStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Leased => "leased",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "leased" => Some(Self::Leased),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

impl std::fmt::Display for AiRuntimeCommandStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One row from `ai_runtime_commands`. The full JSONB `payload` is
/// type-erased at the domain level; command-type-specific payload
/// structs live in the application service layer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiRuntimeCommandRecord {
    pub command_id: String,
    pub run_id: String,
    pub command_type: AiRuntimeCommandType,
    pub command_sequence: i64,
    pub tool_call_pk: Option<String>,
    pub payload: Value,
    pub status: AiRuntimeCommandStatus,
    pub run_owner: Option<String>,
    pub lease_owner: Option<String>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub processed_at: Option<DateTime<Utc>>,
    /// How many times this command has been leased (including
    /// renewals). Used to fail a command after `max_attempts`.
    pub attempt_count: i32,
    /// Maximum lease attempts before the command is considered
    /// permanently failed.
    pub max_attempts: i32,
    /// Last worker heartbeat timestamp for leased commands.
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    /// Sticky run owner assigned when `start_run` is first
    /// leased. Subsequent commands for the same run are only leased by
    /// the same owner unless the lease expires and another worker
    /// performs an explicit takeover.
    pub run_owner_lock: Option<String>,
}

// ---------------------------------------------------------------------------
// Run checkpoints
// ---------------------------------------------------------------------------

/// The 64 KB hard cap on the serialized checkpoint snapshot. The Rust
/// consumer rejects any [`CheckpointPayload`] whose `snapshot_size_bytes`
/// exceeds this budget — see `assert_checkpoint_size_within_budget` in
/// [`crate::ports::ai_execution_repository`]. The Postgres side stores
/// the JSONB as-is; the cap is enforced before insert.
pub const CHECKPOINT_SNAPSHOT_SIZE_BUDGET_BYTES: u32 = 64 * 1024;

/// Checkpoint categories. Strings match the `checkpoint_type` column in
/// `ai_run_checkpoints`. Mirrors the wire enum in
/// [`crate::ai_runtime_event::CheckpointType`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiRunCheckpointType {
    RunInput,
    RoundBeforeModel,
    BeforeTool,
    AfterTool,
    BeforeProposalIngest,
    BeforeDomainAction,
    AfterDomainAction,
    AfterCompletion,
}

impl AiRunCheckpointType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RunInput => "run_input",
            Self::RoundBeforeModel => "round_before_model",
            Self::BeforeTool => "before_tool",
            Self::AfterTool => "after_tool",
            Self::BeforeProposalIngest => "before_proposal_ingest",
            Self::BeforeDomainAction => "before_domain_action",
            Self::AfterDomainAction => "after_domain_action",
            Self::AfterCompletion => "after_completion",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "run_input" => Some(Self::RunInput),
            "round_before_model" => Some(Self::RoundBeforeModel),
            "before_tool" => Some(Self::BeforeTool),
            "after_tool" => Some(Self::AfterTool),
            "before_proposal_ingest" => Some(Self::BeforeProposalIngest),
            "before_domain_action" => Some(Self::BeforeDomainAction),
            "after_domain_action" => Some(Self::AfterDomainAction),
            "after_completion" => Some(Self::AfterCompletion),
            _ => None,
        }
    }
}

impl std::fmt::Display for AiRunCheckpointType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Whether a checkpoint row is the active recovery target, has been
/// replaced by a newer checkpoint, or has been consumed by a resume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiRunCheckpointStatus {
    Persisted,
    Superseded,
    Resumed,
}

impl AiRunCheckpointStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Persisted => "persisted",
            Self::Superseded => "superseded",
            Self::Resumed => "resumed",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "persisted" => Some(Self::Persisted),
            "superseded" => Some(Self::Superseded),
            "resumed" => Some(Self::Resumed),
            _ => None,
        }
    }
}

impl std::fmt::Display for AiRunCheckpointStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One row from `ai_run_checkpoints`. Mirrors the table column set
/// (plus the in-domain [`AiRunCheckpointStatus`] derived from the
/// `superseded_at` / `resumed_at` columns, or kept as `Persisted` until
/// [`AiRunCheckpointRepository::mark_superseded`] is invoked).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiRunCheckpointRecord {
    pub checkpoint_id: String,
    pub job_id: String,
    pub run_id: String,
    pub sequence_no: i64,
    pub checkpoint_type: AiRunCheckpointType,
    pub tool_call_pk: Option<String>,
    pub proposal_id: Option<String>,
    pub snapshot_hash: String,
    pub snapshot: Value,
    pub snapshot_size_bytes: i32,
    pub mq_message_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Compensation / rollback
// ---------------------------------------------------------------------------

/// Lifecycle status of an `ai_compensation_plans` row. The plan state
/// machine:
///
/// ```text
/// Planned -> Approved -> Executing -> Succeeded
///     |          |           |-> Failed (after 3 retries)
///     |          |-> Cancelled
///     |-> Cancelled
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiCompensationStatus {
    Planned,
    Approved,
    Executing,
    Succeeded,
    Failed,
    Cancelled,
}

impl AiCompensationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Approved => "approved",
            Self::Executing => "executing",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "planned" => Some(Self::Planned),
            "approved" => Some(Self::Approved),
            "executing" => Some(Self::Executing),
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

impl std::fmt::Display for AiCompensationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Compensation strategy. Strings match the `mode` column in
/// `ai_compensation_plans` and the in-domain taxonomy from the plan's
/// "Compensation Planner" subsection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiCompensationMode {
    /// Apply the registered inverse domain action (e.g. `Todo.reopen`
    /// in response to `Todo.complete`).
    InverseAction,
    /// Restore the object's field values back to the before snapshot
    /// (requires a matching object version).
    RestoreSnapshot,
    /// Create a corrective business case / todo when direct reversal
    /// is unsafe.
    FollowupAction,
    /// No compensation is possible (notification, external delivery,
    /// publish, etc.). The plan records this and refuses rollback.
    Irreversible,
}

impl AiCompensationMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InverseAction => "inverse_action",
            Self::RestoreSnapshot => "restore_snapshot",
            Self::FollowupAction => "followup_action",
            Self::Irreversible => "irreversible",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "inverse_action" => Some(Self::InverseAction),
            "restore_snapshot" => Some(Self::RestoreSnapshot),
            "followup_action" => Some(Self::FollowupAction),
            "irreversible" => Some(Self::Irreversible),
            _ => None,
        }
    }
}

impl std::fmt::Display for AiCompensationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One row from `ai_action_receipts`. The receipt is the durable proof
/// that a specific domain action ran with a specific set of arguments;
/// the compensation plan references it, and rollback creates a *new*
/// receipt rather than mutating this one.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiActionReceiptRecord {
    pub receipt_id: String,
    pub proposal_id: String,
    pub job_id: String,
    pub run_id: String,
    pub tool_call_pk: Option<String>,
    pub object_type: String,
    pub object_id: String,
    pub action_name: String,
    pub idempotency_key: String,
    pub before_checkpoint_id: Option<String>,
    pub after_checkpoint_id: Option<String>,
    pub outbox_event_id: Option<String>,
    pub execution_result: Value,
    pub executed_by: String,
    pub executed_at: DateTime<Utc>,
}

/// One row from `ai_compensation_plans`. The `plan` JSONB is mode
/// specific (see [`crate::models::ai_ontology::CompensationMetadata`]
/// for the planner output).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiCompensationPlanRecord {
    pub compensation_id: String,
    pub receipt_id: String,
    pub proposal_id: String,
    pub status: AiCompensationStatus,
    pub mode: AiCompensationMode,
    pub plan: Value,
    pub requires_approval: bool,
    pub approved_by: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
    pub executed_by: Option<String>,
    pub executed_at: Option<DateTime<Utc>>,
    pub execution_result: Option<Value>,
    pub execution_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tool_call_status_round_trips() {
        for value in [
            AiToolCallStatus::Requested,
            AiToolCallStatus::Authorized,
            AiToolCallStatus::Running,
            AiToolCallStatus::Succeeded,
            AiToolCallStatus::FailedRetryable,
            AiToolCallStatus::FailedTerminal,
            AiToolCallStatus::Cancelled,
            AiToolCallStatus::Expired,
            AiToolCallStatus::Denied,
            AiToolCallStatus::ProposalOnly,
        ] {
            let s = value.as_str();
            assert_eq!(AiToolCallStatus::from_str(s), Some(value));
        }
    }

    #[test]
    fn tool_call_status_serializes_as_snake_case() {
        let json = serde_json::to_string(&AiToolCallStatus::FailedRetryable).unwrap();
        assert_eq!(json, "\"failed_retryable\"");

        let parsed: AiToolCallStatus = serde_json::from_str("\"proposal_only\"").unwrap();
        assert_eq!(parsed, AiToolCallStatus::ProposalOnly);
    }

    #[test]
    fn tool_call_status_terminal_classification() {
        assert!(!AiToolCallStatus::Requested.is_terminal());
        assert!(!AiToolCallStatus::Authorized.is_terminal());
        assert!(!AiToolCallStatus::Running.is_terminal());
        assert!(AiToolCallStatus::Succeeded.is_terminal());
        assert!(AiToolCallStatus::FailedTerminal.is_terminal());
        assert!(AiToolCallStatus::Cancelled.is_terminal());
        assert!(AiToolCallStatus::Expired.is_terminal());
        assert!(AiToolCallStatus::Denied.is_terminal());
        assert!(AiToolCallStatus::ProposalOnly.is_terminal());
        assert!(!AiToolCallStatus::FailedRetryable.is_terminal());
    }

    #[test]
    fn tool_call_type_round_trips() {
        for value in [
            AiToolCallType::Builtin,
            AiToolCallType::Mcp,
            AiToolCallType::Skill,
            AiToolCallType::Subagent,
        ] {
            let s = value.as_str();
            assert_eq!(AiToolCallType::from_str(s), Some(value));
        }
    }

    #[test]
    fn command_type_round_trips() {
        for value in [
            AiRuntimeCommandType::StartRun,
            AiRuntimeCommandType::CancelRun,
            AiRuntimeCommandType::ToolLease,
            AiRuntimeCommandType::ToolDenied,
            AiRuntimeCommandType::ToolProposalOnly,
            AiRuntimeCommandType::RetryTool,
            AiRuntimeCommandType::ResumeRun,
        ] {
            let s = value.as_str();
            assert_eq!(AiRuntimeCommandType::from_str(s), Some(value));
        }
    }

    #[test]
    fn command_status_round_trips() {
        for value in [
            AiRuntimeCommandStatus::Pending,
            AiRuntimeCommandStatus::Leased,
            AiRuntimeCommandStatus::Completed,
            AiRuntimeCommandStatus::Failed,
        ] {
            let s = value.as_str();
            assert_eq!(AiRuntimeCommandStatus::from_str(s), Some(value));
        }
    }

    #[test]
    fn tool_call_record_serializes_with_all_fields() {
        let record = AiToolCallRecord {
            tool_call_pk: "tpc-1".into(),
            job_id: "job-1".into(),
            run_id: "run-1".into(),
            parent_tool_call_pk: None,
            root_tool_call_pk: None,
            depth: 0,
            round_index: 1,
            tool_call_id: "call-1".into(),
            tool_name: "flight_status_lookup".into(),
            tool_type: AiToolCallType::Builtin,
            status: AiToolCallStatus::Running,
            args_hash: "abc".into(),
            args_summary: json!({"flight_id": "CA1234"}),
            result_hash: None,
            result_summary: None,
            error_code: None,
            error_message: None,
            retry_count: 0,
            max_retries: 2,
            timeout_seconds: 30,
            last_heartbeat_at: None,
            idempotency_key: "run-1:1:call-1:flight_status_lookup:abc".into(),
            mq_message_id: Some("MQ-1".into()),
            mq_offset: Some(42),
            created_at: Utc::now(),
            started_at: None,
            finished_at: None,
            metadata: json!({}),
        };

        let serialized = serde_json::to_string(&record).unwrap();
        for needle in [
            "\"tool_call_pk\":\"tpc-1\"",
            "\"job_id\":\"job-1\"",
            "\"run_id\":\"run-1\"",
            "\"round_index\":1",
            "\"tool_type\":\"builtin\"",
            "\"status\":\"running\"",
            "\"args_hash\":\"abc\"",
            "\"max_retries\":2",
            "\"timeout_seconds\":30",
            "\"idempotency_key\":\"run-1:1:call-1:flight_status_lookup:abc\"",
            "\"mq_message_id\":\"MQ-1\"",
            "\"mq_offset\":42",
        ] {
            assert!(serialized.contains(needle), "missing {needle} in {serialized}");
        }

        let parsed: AiToolCallRecord = serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed, record);
    }

    #[test]
    fn command_record_serializes_with_all_fields() {
        let record = AiRuntimeCommandRecord {
            command_id: "cmd-1".into(),
            run_id: "run-1".into(),
            command_type: AiRuntimeCommandType::ToolLease,
            command_sequence: 7,
            tool_call_pk: Some("tpc-1".into()),
            payload: json!({"lease_seconds": 60}),
            status: AiRuntimeCommandStatus::Pending,
            run_owner: Some("worker-a".into()),
            lease_owner: None,
            lease_expires_at: None,
            created_at: Utc::now(),
            processed_at: None,
            attempt_count: 1,
            max_attempts: 3,
            last_heartbeat_at: None,
            run_owner_lock: Some("worker-a".into()),
        };

        let serialized = serde_json::to_string(&record).unwrap();
        for needle in [
            "\"command_id\":\"cmd-1\"",
            "\"run_id\":\"run-1\"",
            "\"command_type\":\"tool_lease\"",
            "\"command_sequence\":7",
            "\"tool_call_pk\":\"tpc-1\"",
            "\"status\":\"pending\"",
            "\"run_owner\":\"worker-a\"",
            "\"attempt_count\":1",
            "\"max_attempts\":3",
            "\"run_owner_lock\":\"worker-a\"",
        ] {
            assert!(serialized.contains(needle), "missing {needle} in {serialized}");
        }

        let parsed: AiRuntimeCommandRecord = serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed, record);
    }

    #[test]
    fn tool_call_error_default_retryable_is_false() {
        let err = AiToolCallError {
            code: "INTERNAL".into(),
            message: "boom".into(),
            retryable: false,
        };
        let serialized = serde_json::to_string(&err).unwrap();
        let parsed: AiToolCallError = serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed, err);
    }

    #[test]
    fn checkpoint_type_round_trips() {
        for value in [
            AiRunCheckpointType::RunInput,
            AiRunCheckpointType::RoundBeforeModel,
            AiRunCheckpointType::BeforeTool,
            AiRunCheckpointType::AfterTool,
            AiRunCheckpointType::BeforeProposalIngest,
            AiRunCheckpointType::BeforeDomainAction,
            AiRunCheckpointType::AfterDomainAction,
        ] {
            let s = value.as_str();
            let parsed = AiRunCheckpointType::from_str(s).unwrap();
            assert_eq!(parsed, value);
            let json = serde_json::to_string(&value).unwrap();
            let from_json: AiRunCheckpointType = serde_json::from_str(&json).unwrap();
            assert_eq!(from_json, value);
        }
    }

    #[test]
    fn checkpoint_type_serializes_as_snake_case() {
        let json = serde_json::to_string(&AiRunCheckpointType::BeforeDomainAction).unwrap();
        assert_eq!(json, "\"before_domain_action\"");
    }

    #[test]
    fn checkpoint_status_round_trips() {
        for value in [
            AiRunCheckpointStatus::Persisted,
            AiRunCheckpointStatus::Superseded,
            AiRunCheckpointStatus::Resumed,
        ] {
            let s = value.as_str();
            let parsed = AiRunCheckpointStatus::from_str(s).unwrap();
            assert_eq!(parsed, value);
        }
    }

    #[test]
    fn checkpoint_record_serializes_with_all_fields() {
        let record = AiRunCheckpointRecord {
            checkpoint_id: "cp-1".into(),
            job_id: "job-1".into(),
            run_id: "run-1".into(),
            sequence_no: 7,
            checkpoint_type: AiRunCheckpointType::BeforeTool,
            tool_call_pk: Some("tpc-1".into()),
            proposal_id: None,
            snapshot_hash: "hash-1".into(),
            snapshot: json!({"tool_name": "weather_at_airport"}),
            snapshot_size_bytes: 42,
            mq_message_id: Some("MQ-1".into()),
            created_at: Utc::now(),
        };
        let serialized = serde_json::to_string(&record).unwrap();
        for needle in [
            "\"checkpoint_id\":\"cp-1\"",
            "\"job_id\":\"job-1\"",
            "\"run_id\":\"run-1\"",
            "\"sequence_no\":7",
            "\"checkpoint_type\":\"before_tool\"",
            "\"tool_call_pk\":\"tpc-1\"",
            "\"snapshot_hash\":\"hash-1\"",
            "\"snapshot_size_bytes\":42",
            "\"mq_message_id\":\"MQ-1\"",
        ] {
            assert!(serialized.contains(needle), "missing {needle} in {serialized}");
        }
        let parsed: AiRunCheckpointRecord = serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed, record);
    }

    #[test]
    fn compensation_status_round_trips() {
        for value in [
            AiCompensationStatus::Planned,
            AiCompensationStatus::Approved,
            AiCompensationStatus::Executing,
            AiCompensationStatus::Succeeded,
            AiCompensationStatus::Failed,
            AiCompensationStatus::Cancelled,
        ] {
            let s = value.as_str();
            assert_eq!(AiCompensationStatus::from_str(s), Some(value));
        }
    }

    #[test]
    fn compensation_status_serializes_as_snake_case() {
        let json = serde_json::to_string(&AiCompensationStatus::Approved).unwrap();
        assert_eq!(json, "\"approved\"");
    }

    #[test]
    fn compensation_mode_round_trips() {
        for value in [
            AiCompensationMode::InverseAction,
            AiCompensationMode::RestoreSnapshot,
            AiCompensationMode::FollowupAction,
            AiCompensationMode::Irreversible,
        ] {
            let s = value.as_str();
            assert_eq!(AiCompensationMode::from_str(s), Some(value));
        }
    }

    #[test]
    fn compensation_mode_serializes_as_snake_case() {
        let json = serde_json::to_string(&AiCompensationMode::RestoreSnapshot).unwrap();
        assert_eq!(json, "\"restore_snapshot\"");
    }

    #[test]
    fn action_receipt_record_serializes_with_all_fields() {
        let record = AiActionReceiptRecord {
            receipt_id: "rcp-1".into(),
            proposal_id: "prop-1".into(),
            job_id: "job-1".into(),
            run_id: "run-1".into(),
            tool_call_pk: Some("tpc-1".into()),
            object_type: "Flight".into(),
            object_id: "flt-1".into(),
            action_name: "update_status".into(),
            idempotency_key: "idem-1".into(),
            before_checkpoint_id: Some("cp-before".into()),
            after_checkpoint_id: Some("cp-after".into()),
            outbox_event_id: Some("evt-1".into()),
            execution_result: json!({"status": "BOARDING"}),
            executed_by: "exec-1".into(),
            executed_at: Utc::now(),
        };
        let serialized = serde_json::to_string(&record).unwrap();
        for needle in [
            "\"receipt_id\":\"rcp-1\"",
            "\"proposal_id\":\"prop-1\"",
            "\"object_type\":\"Flight\"",
            "\"action_name\":\"update_status\"",
            "\"idempotency_key\":\"idem-1\"",
            "\"before_checkpoint_id\":\"cp-before\"",
            "\"after_checkpoint_id\":\"cp-after\"",
            "\"outbox_event_id\":\"evt-1\"",
            "\"executed_by\":\"exec-1\"",
        ] {
            assert!(serialized.contains(needle), "missing {needle} in {serialized}");
        }
        let parsed: AiActionReceiptRecord = serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed, record);
    }

    #[test]
    fn compensation_plan_record_serializes_with_all_fields() {
        let record = AiCompensationPlanRecord {
            compensation_id: "cmp-1".into(),
            receipt_id: "rcp-1".into(),
            proposal_id: "prop-1".into(),
            status: AiCompensationStatus::Planned,
            mode: AiCompensationMode::RestoreSnapshot,
            plan: json!({"object_type": "Flight", "object_id": "flt-1"}),
            requires_approval: true,
            approved_by: None,
            approved_at: None,
            executed_by: None,
            executed_at: None,
            execution_result: None,
            execution_error: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let serialized = serde_json::to_string(&record).unwrap();
        for needle in [
            "\"compensation_id\":\"cmp-1\"",
            "\"receipt_id\":\"rcp-1\"",
            "\"proposal_id\":\"prop-1\"",
            "\"status\":\"planned\"",
            "\"mode\":\"restore_snapshot\"",
            "\"requires_approval\":true",
        ] {
            assert!(serialized.contains(needle), "missing {needle} in {serialized}");
        }
        let parsed: AiCompensationPlanRecord = serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed, record);
    }
}
