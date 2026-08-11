"""Tests for the tool governance resolver and ``ToolDefinition.governance``."""

from __future__ import annotations

import pytest

from src.infrastructure.ai.governance import (
    ToolGovernanceResolver,
    canonical_args_hash,
    canonical_json_args,
)
from src.infrastructure.ai.tools.tool_registry_snapshot import (
    ToolDefinition,
    ToolRegistrySnapshotBuilder,
)


def _builtin(name: str, *, side_effect: bool = False) -> ToolDefinition:
    return ToolDefinition(
        name=name,
        display_name=name,
        description="d",
        parameters={},
        source="builtin",
        side_effect=side_effect,
    )


def _mcp(name: str, *, side_effect: bool, server_id: str = "ops", original: str = "x") -> ToolDefinition:
    return ToolDefinition(
        name=name,
        display_name=name,
        description="d",
        parameters={},
        source="mcp",
        side_effect=side_effect,
        server_id=server_id,
        original_name=original,
    )


def _skill(name: str, *, side_effect: bool, slug: str = "s", original: str = "x") -> ToolDefinition:
    return ToolDefinition(
        name=name,
        display_name=name,
        description="d",
        parameters={},
        source="skill",
        side_effect=side_effect,
        skill_slug=slug,
        original_name=original,
    )


def test_builtin_read_only_inference() -> None:
    g = ToolGovernanceResolver().resolve(_builtin("flight_status_lookup", side_effect=False))
    assert g["tier"] == "L0_READ"
    assert g["public"] is True
    assert g["authorization_mode"] == "public_direct"
    assert g["execution_mode"] == "direct"
    assert g["reversibility"] == "none"
    assert g["side_effect"] is False
    assert g["required_account_permissions"] == []


def test_builtin_side_effect_inference() -> None:
    g = ToolGovernanceResolver().resolve(_builtin("todo_create", side_effect=True))
    assert g["tier"] == "L1_WORKSPACE_WRITE"
    assert g["public"] is False
    assert g["authorization_mode"] == "rust_pdp"
    assert g["execution_mode"] == "proposal_only"
    assert g["reversibility"] == "reversible"
    assert g["side_effect"] is True
    assert g["approval_policy"]["required"] is True
    assert g["compensation"]["mode"] == "followup_action"


def test_mcp_side_effect_inference() -> None:
    g = ToolGovernanceResolver().resolve(_mcp("mcp.ops.send", side_effect=True))
    assert g["tier"] == "L3_EXTERNAL_SIDE_EFFECT"
    assert g["public"] is False
    assert g["authorization_mode"] == "rust_pdp"
    assert g["execution_mode"] == "proposal_only"
    assert g["reversibility"] == "unknown"
    assert g["side_effect"] is True
    assert g["risk_level"] == "high"


def test_mcp_read_only_inference() -> None:
    g = ToolGovernanceResolver().resolve(_mcp("mcp.db.query", side_effect=False))
    assert g["tier"] == "L0_READ"
    assert g["public"] is True
    assert g["authorization_mode"] == "public_direct"
    assert g["execution_mode"] == "direct"
    assert g["reversibility"] == "none"
    assert g["side_effect"] is False


def test_skill_side_effect_inference() -> None:
    g = ToolGovernanceResolver().resolve(_skill("skill.weather.publish", side_effect=True))
    assert g["tier"] == "L4_IRREVERSIBLE"
    assert g["public"] is False
    assert g["authorization_mode"] == "rust_pdp"
    assert g["execution_mode"] == "proposal_only"
    assert g["reversibility"] == "irreversible"
    assert g["risk_level"] == "critical"
    assert g["retry_policy"]["max_retries"] == 0


def test_skill_read_only_inference() -> None:
    g = ToolGovernanceResolver().resolve(_skill("skill.helper.read", side_effect=False))
    assert g["tier"] == "L0_READ"
    assert g["public"] is True
    assert g["authorization_mode"] == "public_direct"
    assert g["execution_mode"] == "direct"


def test_explicit_governance_overrides_inference() -> None:
    tool = ToolDefinition(
        name="custom_tool",
        display_name="custom_tool",
        description="d",
        parameters={},
        source="builtin",
        side_effect=False,
        governance={
            "preset": "external_side_effect",
            "required_account_permissions": ["flight:read"],
            "risk_level": "high",
        },
    )
    g = ToolGovernanceResolver().resolve(tool)
    assert g["tier"] == "L3_EXTERNAL_SIDE_EFFECT"
    assert g["public"] is False
    assert g["authorization_mode"] == "rust_pdp"
    assert g["execution_mode"] == "proposal_only"
    assert g["required_account_permissions"] == ["flight:read"]
    assert g["risk_level"] == "high"


def test_explicit_governance_can_force_public_read() -> None:
    tool = ToolDefinition(
        name="public_lookup",
        display_name="public_lookup",
        description="d",
        parameters={},
        source="builtin",
        side_effect=False,
        governance={"preset": "read_only_query", "public": True},
    )
    g = ToolGovernanceResolver().resolve(tool)
    assert g["tier"] == "L0_READ"
    assert g["public"] is True
    assert g["authorization_mode"] == "public_direct"


def test_explicit_governance_fills_missing_keys_from_preset() -> None:
    tool = ToolDefinition(
        name="custom_tool",
        display_name="custom_tool",
        description="d",
        parameters={},
        source="builtin",
        side_effect=True,
        governance={"preset": "internal_workspace_write"},
    )
    g = ToolGovernanceResolver().resolve(tool)
    assert g["tier"] == "L1_WORKSPACE_WRITE"
    assert g["approval_policy"]["required"] is True
    assert g["compensation"]["mode"] == "followup_action"


def test_invalid_preset_raises() -> None:
    tool = ToolDefinition(
        name="bad",
        display_name="bad",
        description="d",
        parameters={},
        source="builtin",
        side_effect=False,
        governance={"preset": "not_a_real_preset"},
    )
    with pytest.raises(ValueError, match="Unknown governance preset"):
        ToolGovernanceResolver().resolve(tool)


def test_to_schema_does_not_leak_governance_metadata() -> None:
    tool = ToolDefinition(
        name="custom_tool",
        display_name="custom_tool",
        description="d",
        parameters={"type": "object", "properties": {"x": {"type": "string"}}},
        source="builtin",
        side_effect=False,
        governance={
            "preset": "external_side_effect",
            "required_account_permissions": ["flight:read"],
            "approval_policy": {"required": True},
        },
    )
    schema = tool.to_schema()
    assert "governance" not in schema
    assert "governance" not in schema["function"]
    assert "preset" not in str(schema)
    assert "external_side_effect" not in str(schema)
    assert "required_account_permissions" not in str(schema)


def test_to_dict_includes_governance() -> None:
    tool = ToolDefinition(
        name="custom_tool",
        display_name="custom_tool",
        description="d",
        parameters={},
        source="builtin",
        side_effect=False,
        governance={"preset": "read_only_query"},
    )
    payload = tool.to_dict()
    assert payload["governance"] == {"preset": "read_only_query"}


def test_canonical_args_hash_is_exposed_for_tool_call_idempotency() -> None:
    args = {"flight_id": "CA1234", "metadata": {"airport": "PEK"}}
    assert canonical_json_args(args) == (
        '{"flight_id":"CA1234","metadata":{"airport":"PEK"}}'
    )
    assert len(canonical_args_hash(args)) == 64


def test_resolver_exposes_canonical_hash_helper() -> None:
    tool = _builtin("flight_status_lookup")
    resolver = ToolGovernanceResolver()
    args = {"flight_id": "CA1234"}
    assert resolver.canonical_args_hash(tool, args) == canonical_json_args(args)


@pytest.mark.asyncio
async def test_builtin_builder_accepts_governance_override() -> None:
    builder = ToolRegistrySnapshotBuilder(
        builtin_tools=[{"name": "flight_status_lookup", "description": "d", "parameters": {}}]
    )
    snapshot = await builder.build(
        tooling_config={"enabled": True, "allowed_tool_sources": ["builtin"]}
    )
    assert len(snapshot.tools) == 1
    assert snapshot.tools[0].governance is None


@pytest.mark.asyncio
async def test_mcp_builder_accepts_governance_override() -> None:
    builder = ToolRegistrySnapshotBuilder(builtin_tools=[])
    snapshot = await builder.build(
        tooling_config={"enabled": True, "allowed_tool_sources": ["mcp"]},
        mcp_tools=[
            {
                "name": "query",
                "description": "d",
                "parameters": {},
                "server_id": "db",
                "side_effect": False,
            }
        ],
    )
    assert len(snapshot.tools) == 1
    assert snapshot.tools[0].source == "mcp"
    assert snapshot.tools[0].governance is None


@pytest.mark.asyncio
async def test_skill_builder_accepts_governance_override() -> None:
    builder = ToolRegistrySnapshotBuilder(builtin_tools=[])
    snapshot = await builder.build(
        tooling_config={"enabled": True, "allowed_tool_sources": ["skill"]},
        skill_tools=[
            {
                "name": "publish",
                "description": "d",
                "parameters": {},
                "skill_slug": "weather",
                "side_effect": True,
            }
        ],
    )
    assert len(snapshot.tools) == 1
    assert snapshot.tools[0].source == "skill"
    assert snapshot.tools[0].governance is None


def test_is_public_l0_tool_rejects_pattern_based_matches() -> None:
    """Regression: tools NOT in the explicit allow-list must NOT be
    classified as public L0, even if their names match read-like
    patterns (get_/list_/find_/*_status). This mirrors Rust's
    ``RustToolGovernanceResolver::is_known_public_l0``.

    The cost of false-denying a read tool (it goes through Rust PDP
    and is allowed) is far lower than the cost of false-allowing a
    tool that should require authorization.
    """
    from src.infrastructure.ai.governance.governance_resolver import is_public_l0_tool

    # Pattern-matched names that must NOT be public L0
    # (these are NOT in the explicit allow-list)
    for name in [
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
    ]:
        assert not is_public_l0_tool(name), (
            f"FAIL-CLOSED VIOLATION: {name} must NOT be public L0 "
            f"(not in explicit allow-list)"
        )

    # Explicit allow-list tools must still be public L0
    for name in [
        "weather_at_airport",
        "flight_status_lookup",
        "sql_query",
        "get_flight_status",
        "get_flight_details",
        "list_flights",
        "search_airports",
        "airline_lookup",
        "web_search",
        "web_fetch",
    ]:
        assert is_public_l0_tool(name), (
            f"{name} must be public L0 (in explicit allow-list)"
        )

    # Write-action names must NOT be public L0 (verify they are not in
    # the explicit list — they would also fail the write-action guard
    # if pattern-based fallbacks were still in place, but the explicit
    # check is the definitive guard now).
    for name in [
        "update_flight_status",
        "create_booking",
        "book_flight",
        "cancel_reservation",
        "delete_record",
        "assign_seat",
    ]:
        assert not is_public_l0_tool(name), (
            f"FAIL-CLOSED VIOLATION: {name} must NOT be public L0 "
            f"(not in explicit allow-list)"
        )


def test_python_public_l0_list_matches_rust_exactly() -> None:
    """Cross-language consistency test: Python ``_PUBLIC_L0_EXPLICIT``
    must be exactly equal to Rust ``RUST_PUBLIC_L0_TOOLS``.

    This prevents a critical authorization bypass where a tool is
    classified as public L0 by Python (allowing local execution during
    MQ gate unavailability) but as RustPdp by Rust (requiring
    authorization).

    The test parses the Rust source file directly to extract
    ``RUST_PUBLIC_L0_TOOLS`` at test time, rather than duplicating the
    list. If a new public L0 tool is added, it must be added to BOTH
    files and the tool must be confirmed to be genuinely read-only with
    no sensitive data access or side effects.
    """
    import re
    from pathlib import Path

    from src.infrastructure.ai.governance.governance_resolver import _PUBLIC_L0_EXPLICIT

    test_file = Path(__file__).resolve()
    repo_root = test_file.parents[5]
    rust_file = (
        repo_root
        / "services"
        / "api-server"
        / "crates"
        / "domain"
        / "src"
        / "models"
        / "tool_governance.rs"
    )

    rust_source = rust_file.read_text(encoding="utf-8")

    pattern = re.compile(
        r"const\s+RUST_PUBLIC_L0_TOOLS\s*:\s*&\[\s*&str\s*\]\s*=\s*&\[\s*(.*?)\s*\]\s*;",
        re.DOTALL,
    )
    match = pattern.search(rust_source)
    assert match is not None, "Could not find RUST_PUBLIC_L0_TOOLS in Rust source"

    tools_body = match.group(1)
    string_literals = re.findall(r'"([^"]+)"', tools_body)
    rust_tools = frozenset(string_literals)

    python_tools = frozenset(_PUBLIC_L0_EXPLICIT)

    python_only = python_tools - rust_tools
    rust_only = rust_tools - python_tools

    assert python_tools == rust_tools, (
        "Public L0 tool lists are out of sync between Python and Rust!\n"
        f"  Python-only (AUTH BYPASS RISK): {sorted(python_only)}\n"
        f"  Rust-only (harmless but inconsistent): {sorted(rust_only)}\n"
        "If you intended to add a new public L0 tool, you must add it to\n"
        "BOTH Rust (RUST_PUBLIC_L0_TOOLS in tool_governance.rs) and\n"
        "Python (_PUBLIC_L0_EXPLICIT in governance_resolver.py) after\n"
        "verifying the tool has no side effects and accesses no sensitive\n"
        "account/entity data that requires object-level authorization.\n"
        "Never add a tool to Python only."
    )

    assert len(python_tools) == 32, (
        f"Expected exactly 32 public L0 tools (current Rust count), "
        f"got {len(python_tools)}. If you intentionally added a new "
        f"public L0 tool after security review, update this assertion."
    )
