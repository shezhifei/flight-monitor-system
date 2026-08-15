//! `ai.runtime.events` RocketMQ message schema.
//!
//! The Python sidecar publishes control events to the `ai.runtime.events`
//! topic using these envelopes; the Rust API consumes them to durably
//! record tool calls, checkpoints, and run lifecycle transitions.
//!
//! This is the **only** durable channel for tool call state — the SSE
//! fast path is reduced to ephemeral token/UI push and must never be
//! parsed for persistence.
//!
//! All events share a common envelope and dispatch on `event_type`:
//!
//! * `tool.call.requested` — Python is about to invoke a tool. Rust
//!   must authorize protected tools via `tool_lease` / `tool_denied` /
//!   `tool_proposal_only` commands.
//! * `tool.result` — Tool finished (succeeded / failed / cancelled).
//! * `checkpoint` — A run checkpoint at a deterministic boundary.
//! * `heartbeat` — Liveness ping for a long-running tool call.
//! * `run.complete` — Stream finished successfully; Rust finalizes the run.
//! * `run.fail` — Stream failed terminally; Rust fails the run.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// RocketMQ topic that carries all control events from the sidecar
/// to the Rust control plane. Same-run messages must use [`AiRuntimeEvent::run_id`]
/// as the MQ `Message Key` so they land on the same queue and are
/// processed serially.
pub const AI_RUNTIME_EVENTS_TOPIC: &str = "ai.runtime.events";

/// Event tag / type discriminator.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AiRuntimeEventType {
    ToolCallRequested,
    ToolResult,
    Checkpoint,
    Heartbeat,
    RunComplete,
    RunFail,
}

impl AiRuntimeEventType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ToolCallRequested => "tool.call.requested",
            Self::ToolResult => "tool.result",
            Self::Checkpoint => "checkpoint",
            Self::Heartbeat => "heartbeat",
            Self::RunComplete => "run.complete",
            Self::RunFail => "run.fail",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "tool.call.requested" => Some(Self::ToolCallRequested),
            "tool.result" => Some(Self::ToolResult),
            "checkpoint" => Some(Self::Checkpoint),
            "heartbeat" => Some(Self::Heartbeat),
            "run.complete" => Some(Self::RunComplete),
            "run.fail" => Some(Self::RunFail),
            _ => None,
        }
    }
}

/// Envelope shared by every event on the `ai.runtime.events` topic.
///
/// `event_sequence` is a per-run monotonically increasing counter. The
/// Rust consumer uses it to detect out-of-order delivery and reject
/// duplicate events together with [`AiRuntimeEventEnvelope::idempotency_key`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiRuntimeEventEnvelope {
    pub event_id: String,
    pub event_type: AiRuntimeEventType,
    pub run_id: String,
    pub job_id: String,
    pub round_index: u32,
    pub event_sequence: u64,
    pub idempotency_key: String,
    pub emitted_at: DateTime<Utc>,
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub payload: Value,
}

fn default_schema_version() -> u32 {
    1
}

impl AiRuntimeEventEnvelope {
    pub fn new(
        event_type: AiRuntimeEventType,
        run_id: impl Into<String>,
        job_id: impl Into<String>,
        round_index: u32,
        event_sequence: u64,
        idempotency_key: impl Into<String>,
        payload: Value,
    ) -> Self {
        Self {
            event_id: ulid::Ulid::new().to_string(),
            event_type,
            run_id: run_id.into(),
            job_id: job_id.into(),
            round_index,
            event_sequence,
            idempotency_key: idempotency_key.into(),
            emitted_at: Utc::now(),
            schema_version: 1,
            payload,
        }
    }
}

// ---- Tool call payloads -------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ToolAuthorizationMode {
    /// Tool is public L0 read; sidecar may execute locally and emit
    /// the result event after persistence. Rust still records the
    /// ledger row but does not gate execution.
    PublicDirect,
    /// Tool requires a Rust policy decision (PDP) before execution.
    /// The sidecar must wait for `tool_lease` / `tool_denied` /
    /// `tool_proposal_only` from the `ai_runtime_commands` queue.
    RustPdp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallRequestedPayload {
    pub tool_call_pk: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub tool_type: String,
    pub parent_tool_call_pk: Option<String>,
    pub depth: u32,
    pub args_hash: String,
    pub args_summary: Value,
    pub authorization_mode: ToolAuthorizationMode,
    pub max_retries: u32,
    pub timeout_seconds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ToolExecutionStatus {
    Succeeded,
    Failed,
    Cancelled,
    Expired,
    Denied,
    ProposalOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResultPayload {
    pub tool_call_pk: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub status: ToolExecutionStatus,
    pub result_hash: Option<String>,
    pub result_summary: Option<Value>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub retry_count: u32,
    pub proposal_ids: Vec<String>,
    pub duration_ms: u64,
}

// ---- Checkpoint payloads ------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointType {
    RunInput,
    RoundBeforeModel,
    BeforeTool,
    AfterTool,
    BeforeProposalIngest,
    BeforeDomainAction,
    AfterDomainAction,
    AfterCompletion,
}

impl CheckpointType {
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointPayload {
    pub checkpoint_id: String,
    pub sequence_no: u64,
    pub checkpoint_type: CheckpointType,
    pub tool_call_pk: Option<String>,
    pub proposal_id: Option<String>,
    pub snapshot_hash: String,
    pub snapshot: Value,
    pub snapshot_size_bytes: u32,
}

// ---- Heartbeat ---------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HeartbeatPayload {
    pub tool_call_pk: String,
    pub progress_pct: Option<u8>,
    pub note: Option<String>,
}

// ---- Run lifecycle -----------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunCompletePayload {
    pub output_raw: Value,
    pub token_usage: Option<Value>,
    pub proposal_ids: Vec<String>,
    pub terminal_event_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunFailPayload {
    pub error_code: String,
    pub error_message: String,
    pub terminal_event_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn event_type_round_trips() {
        for value in [
            AiRuntimeEventType::ToolCallRequested,
            AiRuntimeEventType::ToolResult,
            AiRuntimeEventType::Checkpoint,
            AiRuntimeEventType::Heartbeat,
            AiRuntimeEventType::RunComplete,
            AiRuntimeEventType::RunFail,
        ] {
            let s = value.as_str();
            assert_eq!(AiRuntimeEventType::from_str(s), Some(value));
        }
    }

    #[test]
    fn envelope_serializes_with_snake_case_event_type() {
        let env = AiRuntimeEventEnvelope::new(
            AiRuntimeEventType::ToolCallRequested,
            "run-1",
            "job-1",
            0,
            1,
            "idem-1",
            json!({"tool_name": "x"}),
        );
        let s = serde_json::to_string(&env).unwrap();
        assert!(s.contains("\"event_type\":\"tool_call_requested\""), "got: {s}");
        assert!(s.contains("\"schema_version\":1"));
    }

    #[test]
    fn tool_call_requested_payload_round_trips() {
        let payload = ToolCallRequestedPayload {
            tool_call_pk: "tpc-1".into(),
            tool_call_id: "call-1".into(),
            tool_name: "flight_status_lookup".into(),
            tool_type: "builtin".into(),
            parent_tool_call_pk: None,
            depth: 0,
            args_hash: "abc".into(),
            args_summary: json!({"flight_id": "CA1234"}),
            authorization_mode: ToolAuthorizationMode::RustPdp,
            max_retries: 2,
            timeout_seconds: 30,
        };
        let s = serde_json::to_string(&payload).unwrap();
        let parsed: ToolCallRequestedPayload = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed, payload);
    }

    #[test]
    fn tool_result_status_serializes_as_snake_case() {
        let payload = ToolResultPayload {
            tool_call_pk: "tpc-1".into(),
            tool_call_id: "call-1".into(),
            tool_name: "x".into(),
            status: ToolExecutionStatus::ProposalOnly,
            result_hash: None,
            result_summary: None,
            error_code: None,
            error_message: None,
            retry_count: 0,
            proposal_ids: vec!["p-1".into()],
            duration_ms: 42,
        };
        let s = serde_json::to_string(&payload).unwrap();
        assert!(s.contains("\"status\":\"proposal_only\""), "got: {s}");
    }

    #[test]
    fn checkpoint_type_round_trip() {
        for value in [
            CheckpointType::RunInput,
            CheckpointType::BeforeTool,
            CheckpointType::AfterTool,
            CheckpointType::BeforeDomainAction,
            CheckpointType::AfterDomainAction,
            CheckpointType::AfterCompletion,
        ] {
            let s = value.as_str();
            let parsed = serde_json::from_str::<CheckpointType>(&format!("\"{s}\"")).unwrap();
            assert_eq!(parsed, value);
        }
    }
}
