"""Unit tests for entity × tool governance explain chain.

Covers the three acceptance cases:
1. Tool missing from snapshot → deny / blocked_by=snapshot
2. denied_tools hit → deny / blocked_by=acl
3. Normal allow path
"""

from __future__ import annotations

from types import SimpleNamespace

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


def test_explain_denied_tools_hit():
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
    assert result["blocked_by"] == "acl"
    assert result["rule"] == "DENIED_TOOLS"
    assert "denied_tools" in result["detail"]
    steps = {c["step"]: c for c in result["checks"]}
    assert steps["acl"]["result"] == "deny"
    assert steps["snapshot"]["result"] == "missing"


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
        snapshot_tools=[],
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
    assert result["blocked_by"] == "acl"
    assert result["rule"] in ("TEMPLATE_DENIED_TOOLS", "TEMPLATE_CATEGORY_FILTER")
