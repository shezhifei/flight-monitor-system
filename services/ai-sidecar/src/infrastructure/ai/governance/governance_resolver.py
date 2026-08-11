"""Tool governance resolver.

Builds canonical ``ResolvedToolGovernance`` dicts from a
:class:`ToolDefinition` snapshot. The defaults mirror
``services/api-server/crates/domain/src/models/tool_governance.rs``
exactly so the Rust consumer and the Python sidecar agree on every
field value.

Inference rules:

* explicit ``ToolDefinition.governance`` overrides inference (any missing
  keys are filled from the chosen preset);
* ``source="mcp"`` + ``side_effect=True`` -> ``external_side_effect``,
  ``authorization_mode="rust_pdp"``, ``execution_mode="proposal_only"``,
  ``reversibility="unknown"``, ``tier="L3_EXTERNAL_SIDE_EFFECT"``,
  ``public=False``;
* ``source="mcp"`` + ``side_effect=False`` -> ``read_only_query``;
* ``source="builtin"`` + ``side_effect=True`` -> ``internal_workspace_write``;
* ``source="builtin"`` + ``side_effect=False`` -> ``read_only_query``,
  ``public=True``, ``tier="L0_READ"``;
* ``source="skill"`` + ``side_effect=True`` -> ``irreversible_external``;
* ``source="skill"`` + ``side_effect=False`` -> ``read_only_query``,
  ``public=True``.
"""

from __future__ import annotations

from typing import Any, Literal, TypedDict, cast

from src.infrastructure.ai.governance.canonical_args import canonical_json_args
from src.infrastructure.ai.tools.tool_registry_snapshot import ToolDefinition

GovernanceTier = Literal[
    "L0_READ",
    "L1_WORKSPACE_WRITE",
    "L2_REVERSIBLE_WRITE",
    "L3_EXTERNAL_SIDE_EFFECT",
    "L4_IRREVERSIBLE",
]

ExecutionMode = Literal["direct", "proposal_only"]
Reversibility = Literal["none", "reversible", "irreversible", "unknown"]
RiskLevel = Literal["low", "medium", "high", "critical"]
AuthorizationMode = Literal["public_direct", "rust_pdp"]
LogPolicy = Literal["none", "summary", "full"]


class ResolvedToolGovernance(TypedDict, total=False):
    governance_version: str
    tool_name: str
    tier: GovernanceTier
    side_effect: bool
    execution_mode: ExecutionMode
    reversibility: Reversibility
    risk_level: RiskLevel
    public: bool
    required_account_permissions: list[str]
    authorization_mode: AuthorizationMode
    object_policy: dict[str, str | None]
    idempotency: dict[str, str | None]
    retry_policy: dict[str, str | int]
    checkpoint_policy: dict[str, str]
    approval_policy: dict[str, Any]
    compensation: dict[str, Any]
    timeout_seconds: int
    log_args: LogPolicy
    log_result: LogPolicy
    external_system: str | None
    extra: Any


ToolGovernancePresetId = Literal[
    "read_only_query",
    "internal_workspace_write",
    "internal_reversible_action",
    "external_side_effect",
    "irreversible_external",
]


def _read_only_query_defaults(tool_name: str) -> ResolvedToolGovernance:
    return ResolvedToolGovernance(
        governance_version="1.0",
        tool_name=tool_name,
        tier="L0_READ",
        side_effect=False,
        execution_mode="direct",
        reversibility="none",
        risk_level="low",
        public=True,
        required_account_permissions=[],
        authorization_mode="public_direct",
        object_policy={"object_type_arg": None, "object_id_arg": None, "permission": None},
        idempotency={"strategy": "run_tool_args_hash", "key_arg": None},
        retry_policy={"preset": "read_transient_default", "max_retries": 2},
        checkpoint_policy={"before": "none", "after": "summary"},
        approval_policy={"required": False, "min_approver_permissions": []},
        compensation={"mode": "none", "inverse_tool": None, "requires_approval": False},
        timeout_seconds=30,
        log_args="summary",
        log_result="summary",
        external_system=None,
        extra=None,
    )


def _internal_workspace_write_defaults(tool_name: str) -> ResolvedToolGovernance:
    return ResolvedToolGovernance(
        governance_version="1.0",
        tool_name=tool_name,
        tier="L1_WORKSPACE_WRITE",
        side_effect=True,
        execution_mode="proposal_only",
        reversibility="reversible",
        risk_level="medium",
        public=False,
        required_account_permissions=[],
        authorization_mode="rust_pdp",
        object_policy={"object_type_arg": None, "object_id_arg": None, "permission": None},
        idempotency={"strategy": "run_tool_args_hash", "key_arg": None},
        retry_policy={"preset": "workspace_write_default", "max_retries": 0},
        checkpoint_policy={"before": "summary", "after": "summary"},
        approval_policy={"required": True, "min_approver_permissions": []},
        compensation={"mode": "followup_action", "inverse_tool": None, "requires_approval": True},
        timeout_seconds=60,
        log_args="summary",
        log_result="summary",
        external_system=None,
        extra=None,
    )


def _internal_reversible_action_defaults(tool_name: str) -> ResolvedToolGovernance:
    return ResolvedToolGovernance(
        governance_version="1.0",
        tool_name=tool_name,
        tier="L2_REVERSIBLE_WRITE",
        side_effect=True,
        execution_mode="proposal_only",
        reversibility="reversible",
        risk_level="medium",
        public=False,
        required_account_permissions=[],
        authorization_mode="rust_pdp",
        object_policy={"object_type_arg": None, "object_id_arg": None, "permission": None},
        idempotency={"strategy": "domain_action_idempotency_key", "key_arg": None},
        retry_policy={"preset": "domain_action_default", "max_retries": 1},
        checkpoint_policy={"before": "full", "after": "full"},
        approval_policy={"required": True, "min_approver_permissions": []},
        compensation={"mode": "restore_snapshot", "inverse_tool": None, "requires_approval": True},
        timeout_seconds=60,
        log_args="summary",
        log_result="summary",
        external_system=None,
        extra=None,
    )


def _external_side_effect_defaults(tool_name: str) -> ResolvedToolGovernance:
    return ResolvedToolGovernance(
        governance_version="1.0",
        tool_name=tool_name,
        tier="L3_EXTERNAL_SIDE_EFFECT",
        side_effect=True,
        execution_mode="proposal_only",
        reversibility="unknown",
        risk_level="high",
        public=False,
        required_account_permissions=[],
        authorization_mode="rust_pdp",
        object_policy={"object_type_arg": None, "object_id_arg": None, "permission": None},
        idempotency={"strategy": "run_tool_args_hash", "key_arg": None},
        retry_policy={"preset": "external_transport_default", "max_retries": 1},
        checkpoint_policy={"before": "summary", "after": "summary"},
        approval_policy={"required": True, "min_approver_permissions": []},
        compensation={"mode": "none", "inverse_tool": None, "requires_approval": True},
        timeout_seconds=30,
        log_args="summary",
        log_result="summary",
        external_system=None,
        extra=None,
    )


def _irreversible_external_defaults(tool_name: str) -> ResolvedToolGovernance:
    return ResolvedToolGovernance(
        governance_version="1.0",
        tool_name=tool_name,
        tier="L4_IRREVERSIBLE",
        side_effect=True,
        execution_mode="proposal_only",
        reversibility="irreversible",
        risk_level="critical",
        public=False,
        required_account_permissions=[],
        authorization_mode="rust_pdp",
        object_policy={"object_type_arg": None, "object_id_arg": None, "permission": None},
        idempotency={"strategy": "run_tool_args_hash", "key_arg": None},
        retry_policy={"preset": "external_no_retry", "max_retries": 0},
        checkpoint_policy={"before": "summary", "after": "summary"},
        approval_policy={"required": True, "min_approver_permissions": []},
        compensation={"mode": "followup_action", "inverse_tool": None, "requires_approval": True},
        timeout_seconds=30,
        log_args="summary",
        log_result="summary",
        external_system=None,
        extra=None,
    )


_PRESET_BUILDERS = {
    "read_only_query": _read_only_query_defaults,
    "internal_workspace_write": _internal_workspace_write_defaults,
    "internal_reversible_action": _internal_reversible_action_defaults,
    "external_side_effect": _external_side_effect_defaults,
    "irreversible_external": _irreversible_external_defaults,
}


def _merge(base: ResolvedToolGovernance, override: dict[str, Any]) -> ResolvedToolGovernance:
    merged: dict[str, Any] = dict(base)
    for key, value in override.items():
        if value is None and key in {"external_system"}:
            merged[key] = None
        else:
            merged[key] = value
    result = ResolvedToolGovernance(**merged)  # type: ignore[typeddict-item]
    return cast(ResolvedToolGovernance, result)


def _infer_preset_id(tool: ToolDefinition) -> ToolGovernancePresetId:
    if tool.source == "mcp":
        return "external_side_effect" if tool.side_effect else "read_only_query"
    if tool.source == "builtin":
        return "internal_workspace_write" if tool.side_effect else "read_only_query"
    if tool.source == "skill":
        return "irreversible_external" if tool.side_effect else "read_only_query"
    return "read_only_query"


def _post_inference_overrides(
    preset_id: ToolGovernancePresetId,
    tool: ToolDefinition,
) -> dict[str, Any]:
    """Apply source-specific overrides that the preset defaults do not capture."""
    overrides: dict[str, Any] = {}

    if tool.source == "mcp" and tool.side_effect:
        overrides.update(
            {
                "tier": "L3_EXTERNAL_SIDE_EFFECT",
                "authorization_mode": "rust_pdp",
                "execution_mode": "proposal_only",
                "reversibility": "unknown",
                "public": False,
            }
        )
    elif tool.source == "builtin" and not tool.side_effect:
        overrides.update({"public": True, "tier": "L0_READ"})
    elif tool.source == "skill" and not tool.side_effect:
        overrides["public"] = True

    if tool.source == "skill" and tool.side_effect:
        overrides["tier"] = "L4_IRREVERSIBLE"

    return overrides


class ToolGovernanceResolver:
    """Resolve :class:`ToolDefinition` -> :class:`ResolvedToolGovernance`."""

    def resolve(self, tool: ToolDefinition) -> ResolvedToolGovernance:
        explicit = tool.governance or {}
        explicit_preset = explicit.get("preset")
        explicit_preset_valid = isinstance(explicit_preset, str) and explicit_preset in _PRESET_BUILDERS

        if explicit_preset_valid:
            preset_id: ToolGovernancePresetId = explicit_preset  # type: ignore[assignment]
            defaults = _PRESET_BUILDERS[preset_id](tool.name)
        else:
            if explicit_preset is not None and not explicit_preset_valid:
                raise ValueError(f"Unknown governance preset {explicit_preset!r} for tool {tool.name!r}")
            preset_id = _infer_preset_id(tool)
            defaults = _PRESET_BUILDERS[preset_id](tool.name)
            post_overrides = _post_inference_overrides(preset_id, tool)
            defaults = _merge(defaults, post_overrides)

        if explicit:
            override = {k: v for k, v in explicit.items() if k != "preset"}
            defaults = _merge(defaults, override)

        return defaults

    def canonical_args_hash(self, tool: ToolDefinition, args: dict[str, Any]) -> str:
        return canonical_json_args(args)


__all__ = [
    "AuthorizationMode",
    "ExecutionMode",
    "GovernanceTier",
    "LogPolicy",
    "ResolvedToolGovernance",
    "Reversibility",
    "RiskLevel",
    "ToolGovernancePresetId",
    "ToolGovernanceResolver",
    "is_public_l0_tool",
]


_PUBLIC_L0_EXPLICIT: frozenset[str] = frozenset(
    [
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
    ]
)

# --- L0 public tool classification (explicit allow-list only) ---
#
# Security invariant: NO pattern-based fallbacks.
# Only tools explicitly listed here bypass Rust authorization.
# See also: RustToolGovernanceResolver::is_known_public_l0


def is_public_l0_tool(tool_name: str) -> bool:
    """Classify a tool as PublicDirect L0 (safe to execute locally without Rust lease).

    Mirrors
    ``services/api-server/crates/domain/src/models/tool_governance.rs``
    ``RustToolGovernanceResolver::is_known_public_l0`` so that the Python
    sidecar and Rust PDP agree on which tools bypass authorization.

    **Security invariant**: only tools explicitly enumerated in
    ``_PUBLIC_L0_EXPLICIT`` are classified as public L0. There are NO
    pattern-based fallbacks (no ``get_``/``list_``/``*_status`` prefixes
    or suffixes). Every tool that is not in the explicit allow-list goes
    through the Rust PDP path.

    Unknown tools default to ``False`` (fail-closed / Rust PDP required).
    """
    name = tool_name.strip()
    return name in _PUBLIC_L0_EXPLICIT
