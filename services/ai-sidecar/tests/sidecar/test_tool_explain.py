"""Unit tests for entity × tool governance explain chain.

Covers the acceptance cases:
1. Tool missing from snapshot → deny / blocked_by=snapshot
2. denied_tools already filtered from snapshot → snapshot wins over ACL
3. denied_tools still present in snapshot (inconsistent) → deny / blocked_by=acl
4. Template policy deny → deny / blocked_by=template
5. Normal allow path
"""

from __future__ import annotations

from types import SimpleNamespace

import pytest

from src.infrastructure.ai.tools.tool_explain import explain_from_snapshot, explain_tool_access


def test_explain_snapshot_missing_tool():
    result = explain_tool_access(
        entity_id="ent-1",
        tool_name="totally_unknown_tool",
        snapshot_tools=[{"name": "list_flights", "category": "read"}],
        tooling_config={},
    )
    assert result["decision"] == "deny"
    assert result["blocked_by"] == "snapshot"
    assert result["rule"] == "TOOL_NOT_IN_SNAPSHOT"
    assert result["entity_id"] == "ent-1"
    assert result["tool"] == "totally_unknown_tool"
    steps = {c["step"]: c for c in result["checks"]}
    assert steps["snapshot"]["result"] == "missing"


def test_explain_denied_tools_filtered_from_snapshot_is_snapshot():
    """Runtime first impression: denied_tools never appear in the snapshot."""
    result = explain_tool_access(
        entity_id="ent-1",
        tool_name="assign_gate",
        # Denied tools are filtered out of the snapshot by the resolver.
        snapshot_tools=[{"name": "list_flights", "category": "read"}],
        tooling_config={
            "denied_tools": ["assign_gate"],
            "allowed_tools": None,
        },
    )
    assert result["decision"] == "deny"
    assert result["blocked_by"] == "snapshot"
    assert result["rule"] == "TOOL_NOT_IN_SNAPSHOT"
    steps = {c["step"]: c for c in result["checks"]}
    assert steps["acl"]["result"] == "deny"
    assert steps["snapshot"]["result"] == "missing"


def test_explain_denied_tools_still_in_snapshot_is_acl():
    """ACL attribution only when the tool is still in the snapshot."""
    result = explain_tool_access(
        entity_id="ent-1",
        tool_name="assign_gate",
        snapshot_tools=[{"name": "assign_gate", "category": "write"}],
        tooling_config={
            "denied_tools": ["assign_gate"],
            "allowed_tools": None,
        },
    )
    assert result["decision"] == "deny"
    assert result["blocked_by"] == "acl"
    assert result["rule"] == "DENIED_TOOLS"
    assert "denied_tools" in result["detail"]
    steps = {c["step"]: c for c in result["checks"]}
    assert steps["snapshot"]["result"] == "present"
    assert steps["acl"]["result"] == "deny"


def test_explain_allow_when_present_and_not_denied():
    result = explain_tool_access(
        entity_id="ent-1",
        tool_name="list_flights",
        snapshot_tools=[
            {"name": "list_flights", "category": "read"},
            {"name": "get_flight_status", "category": "read"},
        ],
        tooling_config={
            "denied_tools": [],
            "allowed_tools": None,
        },
    )
    assert result["decision"] == "allow"
    assert result["blocked_by"] is None
    assert result["rule"] is None
    steps = {c["step"]: c for c in result["checks"]}
    assert steps["snapshot"]["result"] == "present"
    assert steps["acl"]["result"] == "allow"
    # list_flights is public L0 → lease not required
    assert steps["lease"]["result"] == "not_required"
    assert result["lease_required"] is False


def test_explain_from_snapshot_object():
    snapshot = SimpleNamespace(
        tools=[
            SimpleNamespace(name="get_flight_status", category="read"),
        ]
    )
    result = explain_from_snapshot(
        entity_id="ent-2",
        tool_name="get_flight_status",
        snapshot=snapshot,
        tooling_config={"denied_tools": []},
    )
    assert result["decision"] == "allow"
    assert result["tool"] == "get_flight_status"


def test_explain_not_in_allowed_tools_list():
    result = explain_tool_access(
        entity_id="ent-1",
        tool_name="assign_gate",
        snapshot_tools=[{"name": "assign_gate", "category": "write"}],
        tooling_config={
            "denied_tools": [],
            "allowed_tools": ["list_flights"],
        },
    )
    assert result["decision"] == "deny"
    assert result["blocked_by"] == "acl"
    assert result["rule"] == "NOT_IN_ALLOWED_TOOLS"


def test_explain_template_denies_write_for_query_ops():
    result = explain_tool_access(
        entity_id="ent-1",
        tool_name="assign_gate",
        snapshot_tools=[{"name": "assign_gate", "category": "write"}],
        tooling_config={"denied_tools": []},
        task_type="query_ops",
    )
    # query_ops template denies write tools / restricts categories
    assert result["decision"] == "deny"
    assert result["blocked_by"] == "template"
    assert result["rule"] in ("TEMPLATE_DENIED_TOOLS", "TEMPLATE_CATEGORY_FILTER")


@pytest.mark.asyncio
async def test_resolver_get_entity_tooling_config_uses_store_get():
    from src.infrastructure.ai.capability_resolver import CapabilityResolver

    class _Store:
        async def get(self, entity_id: str):
            assert entity_id == "ent-x"
            return {"tooling": {"denied_tools": ["assign_gate"], "allowed_tools": None}}

    resolver = CapabilityResolver(config_store=_Store())
    tooling = await resolver.get_entity_tooling_config("ent-x")
    assert tooling == {"denied_tools": ["assign_gate"], "allowed_tools": None}


@pytest.mark.asyncio
async def test_load_entity_tooling_config_uses_public_resolver_api():
    from src.infrastructure.ai.management_routes import _load_entity_tooling_config

    class _Resolver:
        def __init__(self) -> None:
            self._config_store = object()  # must not be probed

        async def get_entity_tooling_config(self, entity_id: str):
            assert entity_id == "ent-1"
            return {"denied_tools": ["x"]}

    tooling = await _load_entity_tooling_config(_Resolver(), "ent-1")
    assert tooling == {"denied_tools": ["x"]}
