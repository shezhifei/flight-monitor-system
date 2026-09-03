//! Resolved tool governance metadata.
//!
//! The governance model is a separate layer from the LLM-facing
//! function schema. It records what the runtime is *allowed* to do
//! with a tool (tier, reversibility, authorization mode, etc.) and is
//! resolved once at run start into an immutable snapshot.
//!
//! LLM only ever sees the function schema. Governance metadata is
//! out-of-band and consumed by:
//!
//! * the Python `ToolExecutor` to decide whether a tool can run
//!   locally or must wait for a Rust policy decision;
//! * the Rust `ToolAuthorizationService` to make that policy decision
//!   and to deny/proposal-only a protected tool call.
//!
//! Adding a new tool stays simple: a read-only tool can be declared
//! with `preset = "read_only_query"`. The [`ToolGovernancePreset`]
//! adapters build a sensible default governance record from existing
//! fields (`READ_ONLY_TOOL_SCHEMAS`, `BaseToolDefinition`,
//! `ToolDefinition.side_effect`, MCP annotations, MCP binding
//! `tool_defaults`).

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GovernanceTier {
    L0Read,
    L1WorkspaceWrite,
    L2ReversibleWrite,
    L3ExternalSideEffect,
    L4Irreversible,
}

impl GovernanceTier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::L0Read => "L0_READ",
            Self::L1WorkspaceWrite => "L1_WORKSPACE_WRITE",
            Self::L2ReversibleWrite => "L2_REVERSIBLE_WRITE",
            Self::L3ExternalSideEffect => "L3_EXTERNAL_SIDE_EFFECT",
            Self::L4Irreversible => "L4_IRREVERSIBLE",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    /// Tool may run locally on the sidecar as long as the snapshot
    /// permits it; ledger still records the call.
    Direct,
    /// Tool must always go through a proposal/approval flow.
    ProposalOnly,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Reversibility {
    None,
    Reversible,
    Irreversible,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizationMode {
    /// L0 read tool; sidecar may execute locally and emit the result
    /// event after persistence. Rust still records the ledger row
    /// but does not gate execution.
    PublicDirect,
    /// Tool requires a Rust policy decision (PDP) before execution.
    /// The sidecar must wait for `tool_lease` / `tool_denied` /
    /// `tool_proposal_only` from the `ai_runtime_commands` queue.
    RustPdp,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum LogPolicy {
    None,
    Summary,
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObjectPolicy {
    pub object_type_arg: Option<String>,
    pub object_id_arg: Option<String>,
    pub permission: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IdempotencyPolicy {
    pub strategy: String,
    pub key_arg: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RetryPolicy {
    pub preset: String,
    pub max_retries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointPolicy {
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApprovalPolicy {
    pub required: bool,
    pub min_approver_permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompensationPolicy {
    pub mode: String,
    pub inverse_tool: Option<String>,
    pub requires_approval: bool,
}

/// Resolved governance metadata for a single tool.
///
/// `ResolvedToolGovernance` is produced by `ToolGovernanceResolver` at
/// run start and is immutable for the run's lifetime. The LLM-facing
/// `to_schema()` must not include any of these fields.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResolvedToolGovernance {
    pub governance_version: String,
    pub tool_name: String,
    pub tier: GovernanceTier,
    pub side_effect: bool,
    pub execution_mode: ExecutionMode,
    pub reversibility: Reversibility,
    pub risk_level: RiskLevel,
    pub public: bool,
    pub required_account_permissions: Vec<String>,
    pub authorization_mode: AuthorizationMode,
    pub object_policy: ObjectPolicy,
    pub idempotency: IdempotencyPolicy,
    pub retry_policy: RetryPolicy,
    pub checkpoint_policy: CheckpointPolicy,
    pub approval_policy: ApprovalPolicy,
    pub compensation: CompensationPolicy,
    pub timeout_seconds: u32,
    pub log_args: LogPolicy,
    pub log_result: LogPolicy,
    pub external_system: Option<String>,
    pub extra: Value,
}

impl ResolvedToolGovernance {
    /// Returns true when the tool can be invoked by the sidecar
    /// without waiting for a Rust policy decision. Public L0 reads
    /// still emit ledger events.
    pub fn is_public_direct(&self) -> bool {
        self.authorization_mode == AuthorizationMode::PublicDirect
    }

    /// Returns true when the tool requires a proposal/approval flow
    /// before any side effect can occur.
    pub fn is_proposal_only(&self) -> bool {
        self.execution_mode == ExecutionMode::ProposalOnly
    }

    /// Returns true when the tool's effect is potentially reversible
    /// via a compensation plan. A compensation policy of `none` /
    /// missing does not on its own make a tool irreversible; that
    /// decision belongs to [`Reversibility`].
    pub fn has_compensation(&self) -> bool {
        self.compensation.mode != "none"
    }
}

/// Explicitly enumerated L0 public read-only tools.
///
/// Tools in this list are known to have no side effects and may
/// be executed by the sidecar without waiting for a Rust policy
/// decision. There are NO pattern-based fallbacks; only tools
/// explicitly listed here receive `PublicDirect` governance.
/// Unknown tools default to `RustPdp` (fail-closed).
const RUST_PUBLIC_L0_TOOLS: &[&str] = &[
    "weather_at_airport",
    "weather_at_airport_metar",
    "weather_at_airport_taf",
    "flight_status_lookup",
    "flight_info_lookup",
    "flight_search",
    "airport_info",
    "airport_lookup",
    "airline_lookup",
    "aircraft_lookup",
    "sql_query",
    "read_file",
    "list_directory",
    "web_search",
    "web_fetch",
    "get_current_time",
    "get_weather",
    "get_flight_status",
    "get_flight_details",
    "get_airport_status",
    "get_airport_info",
    "get_airport_weather",
    "get_flight_schedule",
    "get_flight_route",
    "list_flights",
    "list_airports",
    "list_airlines",
    "search_flights",
    "search_airports",
    "run_sql_query",
    "execute_read_query",
    "run_read_only_query",
];

/// Rust-side tool governance resolver (trust boundary).
///
/// This is the authority for determining whether a tool is L0
/// public_direct (sidecar may execute locally) or RustPdp (must
/// wait for a Rust policy decision). It does NOT trust any
/// `authorization_mode` field sent by the Python sidecar in MQ
/// payloads.
///
/// **Security invariant**: unknown tools default to `RustPdp`
/// (fail-closed). Only tools explicitly enumerated in
/// [`RUST_PUBLIC_L0_TOOLS`] receive `PublicDirect` governance.
/// Adding a new public read-only tool requires an explicit entry
/// here.
pub struct RustToolGovernanceResolver;

impl RustToolGovernanceResolver {
    /// Resolve governance for a tool by name.
    ///
    /// Returns `ReadOnlyQuery` preset governance for known L0
    /// public tools, and a conservative write-tier classification
    /// for all other tools. This is intentionally safe: a tool
    /// incorrectly classified as RustPdp will simply go through
    /// the PDP path (which may still allow it), while a tool
    /// incorrectly classified as PublicDirect would bypass
    /// authorization entirely.
    pub fn resolve(tool_name: &str) -> ResolvedToolGovernance {
        if Self::is_known_public_l0(tool_name) {
            ToolGovernancePreset::ReadOnlyQuery.default_governance(tool_name)
        } else {
            Self::classify_unknown_tool(tool_name)
        }
    }

    /// Check if a tool name is in the known L0 public set.
    ///
    /// **Security invariant**: only tools explicitly enumerated in
    /// [`RUST_PUBLIC_L0_TOOLS`] are classified as public L0. There
    /// are NO pattern-based fallbacks (no `get_`/`list_`/`*_status`
    /// prefixes or suffixes). Every tool that is not in the explicit
    /// allow-list goes through the Rust PDP path, where the
    /// entity-specific tool governance (including
    /// `required_account_permissions`) is loaded from the database
    /// and enforced by [`ToolAuthorizationService`].
    ///
    /// Adding a new public read-only tool requires an explicit entry
    /// in [`RUST_PUBLIC_L0_TOOLS`]. This is intentional: the cost of
    /// false-denying a read tool (it goes through PDP and is allowed)
    /// is far lower than the cost of false-allowing a tool that
    /// should require authorization.
    pub fn is_known_public_l0(tool_name: &str) -> bool {
        let name = tool_name.trim();
        RUST_PUBLIC_L0_TOOLS.contains(&name)
    }

    fn classify_unknown_tool(tool_name: &str) -> ResolvedToolGovernance {
        let name = tool_name.trim().to_lowercase();
        if name.contains("irreversible")
            || name.contains("delete")
            || name.contains("publish")
            || name.contains("purge")
        {
            ToolGovernancePreset::IrreversibleExternal.default_governance(tool_name)
        } else if name.starts_with("mcp.")
            || name.contains("send")
            || name.contains("notify")
            || name.contains("external")
        {
            ToolGovernancePreset::ExternalSideEffect.default_governance(tool_name)
        } else if name.contains("create")
            || name.contains("update")
            || name.contains("assign")
            || name.contains("delay")
            || name.contains("change")
            || name.contains("mark_")
            || name.contains("set_")
            || name.contains("add_")
            || name.contains("edit")
        {
            ToolGovernancePreset::InternalReversibleAction.default_governance(tool_name)
        } else {
            ToolGovernancePreset::InternalWorkspaceWrite.default_governance(tool_name)
        }
    }
}

/// Preset identifiers. The Python resolver and Rust resolver share
/// these strings; new presets must be added in both places.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ToolGovernancePreset {
    ReadOnlyQuery,
    InternalWorkspaceWrite,
    InternalReversibleAction,
    ExternalSideEffect,
    IrreversibleExternal,
}

impl ToolGovernancePreset {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnlyQuery => "read_only_query",
            Self::InternalWorkspaceWrite => "internal_workspace_write",
            Self::InternalReversibleAction => "internal_reversible_action",
            Self::ExternalSideEffect => "external_side_effect",
            Self::IrreversibleExternal => "irreversible_external",
        }
    }

    /// Default governance for the preset. The resolver layer is
    /// expected to overlay tool-specific fields (permissions,
    /// object policy, etc.) on top.
    pub fn default_governance(self, tool_name: impl Into<String>) -> ResolvedToolGovernance {
        let name = tool_name.into();
        match self {
            Self::ReadOnlyQuery => ResolvedToolGovernance {
                governance_version: "1.0".to_string(),
                tool_name: name,
                tier: GovernanceTier::L0Read,
                side_effect: false,
                execution_mode: ExecutionMode::Direct,
                reversibility: Reversibility::None,
                risk_level: RiskLevel::Low,
                public: true,
                required_account_permissions: Vec::new(),
                authorization_mode: AuthorizationMode::PublicDirect,
                object_policy: ObjectPolicy {
                    object_type_arg: None,
                    object_id_arg: None,
                    permission: None,
                },
                idempotency: IdempotencyPolicy {
                    strategy: "run_tool_args_hash".to_string(),
                    key_arg: None,
                },
                retry_policy: RetryPolicy {
                    preset: "read_transient_default".to_string(),
                    max_retries: 2,
                },
                checkpoint_policy: CheckpointPolicy {
                    before: "none".to_string(),
                    after: "summary".to_string(),
                },
                approval_policy: ApprovalPolicy {
                    required: false,
                    min_approver_permissions: Vec::new(),
                },
                compensation: CompensationPolicy {
                    mode: "none".to_string(),
                    inverse_tool: None,
                    requires_approval: false,
                },
                timeout_seconds: 30,
                log_args: LogPolicy::Summary,
                log_result: LogPolicy::Summary,
                external_system: None,
                extra: Value::Null,
            },
            Self::InternalWorkspaceWrite => ResolvedToolGovernance {
                governance_version: "1.0".to_string(),
                tool_name: name,
                tier: GovernanceTier::L1WorkspaceWrite,
                side_effect: true,
                execution_mode: ExecutionMode::ProposalOnly,
                reversibility: Reversibility::Reversible,
                risk_level: RiskLevel::Medium,
                public: false,
                required_account_permissions: Vec::new(),
                authorization_mode: AuthorizationMode::RustPdp,
                object_policy: ObjectPolicy {
                    object_type_arg: None,
                    object_id_arg: None,
                    permission: None,
                },
                idempotency: IdempotencyPolicy {
                    strategy: "run_tool_args_hash".to_string(),
                    key_arg: None,
                },
                retry_policy: RetryPolicy {
                    preset: "workspace_write_default".to_string(),
                    max_retries: 0,
                },
                checkpoint_policy: CheckpointPolicy {
                    before: "summary".to_string(),
                    after: "summary".to_string(),
                },
                approval_policy: ApprovalPolicy {
                    required: true,
                    min_approver_permissions: Vec::new(),
                },
                compensation: CompensationPolicy {
                    mode: "followup_action".to_string(),
                    inverse_tool: None,
                    requires_approval: true,
                },
                timeout_seconds: 60,
                log_args: LogPolicy::Summary,
                log_result: LogPolicy::Summary,
                external_system: None,
                extra: Value::Null,
            },
            Self::InternalReversibleAction => ResolvedToolGovernance {
                governance_version: "1.0".to_string(),
                tool_name: name,
                tier: GovernanceTier::L2ReversibleWrite,
                side_effect: true,
                execution_mode: ExecutionMode::ProposalOnly,
                reversibility: Reversibility::Reversible,
                risk_level: RiskLevel::Medium,
                public: false,
                required_account_permissions: Vec::new(),
                authorization_mode: AuthorizationMode::RustPdp,
                object_policy: ObjectPolicy {
                    object_type_arg: None,
                    object_id_arg: None,
                    permission: None,
                },
                idempotency: IdempotencyPolicy {
                    strategy: "domain_action_idempotency_key".to_string(),
                    key_arg: None,
                },
                retry_policy: RetryPolicy {
                    preset: "domain_action_default".to_string(),
                    max_retries: 1,
                },
                checkpoint_policy: CheckpointPolicy {
                    before: "full".to_string(),
                    after: "full".to_string(),
                },
                approval_policy: ApprovalPolicy {
                    required: true,
                    min_approver_permissions: Vec::new(),
                },
                compensation: CompensationPolicy {
                    mode: "restore_snapshot".to_string(),
                    inverse_tool: None,
                    requires_approval: true,
                },
                timeout_seconds: 60,
                log_args: LogPolicy::Summary,
                log_result: LogPolicy::Summary,
                external_system: None,
                extra: Value::Null,
            },
            Self::ExternalSideEffect => ResolvedToolGovernance {
                governance_version: "1.0".to_string(),
                tool_name: name,
                tier: GovernanceTier::L3ExternalSideEffect,
                side_effect: true,
                execution_mode: ExecutionMode::ProposalOnly,
                reversibility: Reversibility::Unknown,
                risk_level: RiskLevel::High,
                public: false,
                required_account_permissions: Vec::new(),
                authorization_mode: AuthorizationMode::RustPdp,
                object_policy: ObjectPolicy {
                    object_type_arg: None,
                    object_id_arg: None,
                    permission: None,
                },
                idempotency: IdempotencyPolicy {
                    strategy: "run_tool_args_hash".to_string(),
                    key_arg: None,
                },
                retry_policy: RetryPolicy {
                    preset: "external_transport_default".to_string(),
                    max_retries: 1,
                },
                checkpoint_policy: CheckpointPolicy {
                    before: "summary".to_string(),
                    after: "summary".to_string(),
                },
                approval_policy: ApprovalPolicy {
                    required: true,
                    min_approver_permissions: Vec::new(),
                },
                compensation: CompensationPolicy {
                    mode: "none".to_string(),
                    inverse_tool: None,
                    requires_approval: true,
                },
                timeout_seconds: 30,
                log_args: LogPolicy::Summary,
                log_result: LogPolicy::Summary,
                external_system: None,
                extra: Value::Null,
            },
            Self::IrreversibleExternal => ResolvedToolGovernance {
                governance_version: "1.0".to_string(),
                tool_name: name,
                tier: GovernanceTier::L4Irreversible,
                side_effect: true,
                execution_mode: ExecutionMode::ProposalOnly,
                reversibility: Reversibility::Irreversible,
                risk_level: RiskLevel::Critical,
                public: false,
                required_account_permissions: Vec::new(),
                authorization_mode: AuthorizationMode::RustPdp,
                object_policy: ObjectPolicy {
                    object_type_arg: None,
                    object_id_arg: None,
                    permission: None,
                },
                idempotency: IdempotencyPolicy {
                    strategy: "run_tool_args_hash".to_string(),
                    key_arg: None,
                },
                retry_policy: RetryPolicy {
                    preset: "external_no_retry".to_string(),
                    max_retries: 0,
                },
                checkpoint_policy: CheckpointPolicy {
                    before: "summary".to_string(),
                    after: "summary".to_string(),
                },
                approval_policy: ApprovalPolicy {
                    required: true,
                    min_approver_permissions: Vec::new(),
                },
                compensation: CompensationPolicy {
                    mode: "followup_action".to_string(),
                    inverse_tool: None,
                    requires_approval: true,
                },
                timeout_seconds: 30,
                log_args: LogPolicy::Summary,
                log_result: LogPolicy::Summary,
                external_system: None,
                extra: Value::Null,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_query_preset_is_public_and_low_risk() {
        let g = ToolGovernancePreset::ReadOnlyQuery.default_governance("weather_at_airport");
        assert_eq!(g.tier, GovernanceTier::L0Read);
        assert!(!g.side_effect);
        assert!(g.is_public_direct());
        assert!(!g.is_proposal_only());
        assert_eq!(g.execution_mode, ExecutionMode::Direct);
        assert!(!g.has_compensation());
    }

    #[test]
    fn internal_workspace_write_preset_requires_approval() {
        let g = ToolGovernancePreset::InternalWorkspaceWrite.default_governance("todo_create");
        assert_eq!(g.tier, GovernanceTier::L1WorkspaceWrite);
        assert!(g.side_effect);
        assert!(g.is_proposal_only());
        assert!(g.approval_policy.required);
        assert_eq!(g.authorization_mode, AuthorizationMode::RustPdp);
    }

    #[test]
    fn internal_reversible_action_supports_rollback() {
        let g = ToolGovernancePreset::InternalReversibleAction.default_governance("flight_update_status");
        assert_eq!(g.tier, GovernanceTier::L2ReversibleWrite);
        assert_eq!(g.compensation.mode, "restore_snapshot");
        assert!(g.has_compensation());
    }

    #[test]
    fn external_side_effect_is_proposal_only_by_default() {
        let g = ToolGovernancePreset::ExternalSideEffect.default_governance("mcp.ops.send");
        assert_eq!(g.tier, GovernanceTier::L3ExternalSideEffect);
        assert!(g.is_proposal_only());
        assert!(g.approval_policy.required);
    }

    #[test]
    fn irreversible_external_is_marked_irreversible() {
        let g = ToolGovernancePreset::IrreversibleExternal.default_governance("mcp.ops.publish");
        assert_eq!(g.tier, GovernanceTier::L4Irreversible);
        assert_eq!(g.reversibility, Reversibility::Irreversible);
        assert_eq!(g.risk_level, RiskLevel::Critical);
    }

    #[test]
    fn governance_serializes_with_screaming_snake_tier() {
        let g = ToolGovernancePreset::ReadOnlyQuery.default_governance("x");
        let s = serde_json::to_string(&g).unwrap();
        assert!(s.contains("\"tier\":\"L0_READ\""), "got: {s}");
        assert!(s.contains("\"authorization_mode\":\"public_direct\""), "got: {s}");
        assert!(s.contains("\"execution_mode\":\"direct\""), "got: {s}");
    }

    #[test]
    fn known_public_l0_tools_are_classified_public_direct() {
        for name in &[
            "weather_at_airport",
            "flight_status_lookup",
            "sql_query",
            "get_flight_status",
            "get_flight_details",
            "get_airport_status",
            "get_airport_info",
            "get_airport_weather",
            "get_flight_schedule",
            "get_flight_route",
            "list_flights",
            "list_airports",
            "list_airlines",
            "search_flights",
            "search_airports",
            "airline_lookup",
            "airport_lookup",
            "aircraft_lookup",
            "flight_search",
            "airport_info",
            "web_search",
            "web_fetch",
            "weather_at_airport_metar",
            "weather_at_airport_taf",
            "get_current_time",
            "get_weather",
            "read_file",
            "list_directory",
            "run_sql_query",
            "execute_read_query",
            "run_read_only_query",
        ] {
            assert!(
                RustToolGovernanceResolver::is_known_public_l0(name),
                "expected {name} to be public L0"
            );
            let g = RustToolGovernanceResolver::resolve(name);
            assert!(
                g.is_public_direct(),
                "expected {name} to resolve to public_direct, got tier={:?} auth={:?}",
                g.tier,
                g.authorization_mode
            );
        }
    }

    #[test]
    fn pattern_based_names_are_not_public_l0() {
        // Tools that match read-like patterns (get_/list_/find_/query_/etc.)
        // but are NOT in the explicit allow-list must go through Rust PDP.
        for name in &[
            "get_customer_secrets",
            "list_all_users",
            "lookup_credit_card",
            "search_customer_pii",
            "find_restricted_records",
            "query_financial_data",
            "check_audit_trail",
            "get_internal_config",
            "list_ssh_keys",
            "lookup_private_key",
            "search_employee_pii",
            "find_security_audit",
        ] {
            assert!(
                !RustToolGovernanceResolver::is_known_public_l0(name),
                "expected {name} to NOT be public L0 (not in explicit allow-list)"
            );
            let g = RustToolGovernanceResolver::resolve(name);
            assert!(
                !g.is_public_direct(),
                "expected {name} to require Rust PDP, got tier={:?} auth={:?}",
                g.tier,
                g.authorization_mode
            );
        }
    }

    #[test]
    fn write_action_tools_are_not_public_l0_even_with_status_suffix() {
        for name in &[
            "update_flight_status",
            "create_todo",
            "delete_booking",
            "assign_gate",
            "add_flight_note",
            "book_flight",
            "cancel_booking",
            "complete_todo",
            "send_notification",
            "publish_schedule",
            "purge_cache",
            "set_delay",
            "mark_completed",
            "edit_itinerary",
            "approve_request",
        ] {
            assert!(
                !RustToolGovernanceResolver::is_known_public_l0(name),
                "expected {name} to NOT be public L0 (write action)"
            );
            let g = RustToolGovernanceResolver::resolve(name);
            assert!(
                !g.is_public_direct(),
                "expected {name} to require Rust PDP, got tier={:?} auth={:?}",
                g.tier,
                g.authorization_mode
            );
        }
    }

    #[test]
    fn unknown_tool_defaults_to_workspace_write_rust_pdp() {
        let g = RustToolGovernanceResolver::resolve("completely_unknown_tool_xyz");
        assert_eq!(g.authorization_mode, AuthorizationMode::RustPdp);
        assert!(!g.is_public_direct());
    }
}
