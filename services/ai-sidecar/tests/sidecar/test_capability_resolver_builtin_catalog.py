"""Task A3 — the capability resolver's builtin catalog is the single truth.

Asserts (docs/plans/2026-08-14-hybrid-agent-architecture.md, Task A3):

1. The resolver's ``builtin_tools`` come from the real source
   (``ai_runtime_bootstrap._builtin_tool_catalog``), not a mock list.
2. Every resolved builtin tool carries a non-empty name (the old
   OpenAI-wrapped ``READ_ONLY_TOOL_SCHEMAS`` shape produced empty names).
3. Every builtin tool routes to a non-mock implementation at execution:
   read-only tools execute through the wired backend, write actions produce
   proposals — no fabricated data anywhere.
4. ``ToolExecutor.get_available_tools()`` derives from the same catalog plus
   the write-action registry; no second hardcoded list.
5. Governance fields stay in the snapshot, not in the LLM function schema.
"""

from __future__ import annotations

import asyncio
from typing import Any

import pytest

from src.infrastructure.ai.ai_runtime_bootstrap import _builtin_tool_catalog
from src.infrastructure.ai.capability_resolver import CapabilityResolver
from src.infrastructure.ai.tools.read_only_tools import is_read_only_tool
from src.infrastructure.ai.tools.tool_executor import WRITE_ACTION_TOOLS, ToolExecutor

from tests.sidecar.tool_executor_test_support import (
    AuthorizedToolMqGate,
    FakeReadOnlyBackend,
)


def _run(coro):
    loop = asyncio.new_event_loop()
    try:
        return loop.run_until_complete(coro)
    finally:
        loop.close()


class _FakeConfigStore:
    """In-memory config store returning an entity doc that allows the catalog.

    Mirrors a query_ops entity: tooling explicitly allows the query category
    (the default normalizer document does not grant it — config narrows).
    """

    def __init__(self, doc: dict[str, Any] | None = None):
        self._doc = doc

    async def get(self, entity_id: str) -> dict[str, Any] | None:
        if self._doc is None:
            return None
        return {"entity_id": entity_id, **self._doc}


def _resolver() -> CapabilityResolver:
    return CapabilityResolver(
        config_store=_FakeConfigStore(
            {"tooling": {"allowed_tool_categories": ["flight", "query"]}}
        ),
        builtin_tools=_builtin_tool_catalog(),
    )


@pytest.fixture(autouse=True)
def _no_env_key(monkeypatch):
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)


def test_catalog_has_real_names_and_schema_shape() -> None:
    catalog = _builtin_tool_catalog()
    names = {tool["name"] for tool in catalog}
    assert "flight_status_lookup" in names
    assert "search_flights_advanced" in names
    assert "QUERY" in names
    for tool in catalog:
        assert tool["name"], "builtin tool must have a non-empty name"
        assert tool["description"], f"{tool['name']} must have a description"
        assert "parameters" in tool, f"{tool['name']} must carry an OpenAI parameters object"


def test_resolver_snapshot_contains_catalog_tools() -> None:
    resolver = _resolver()
    snapshot = _run(resolver.resolve("test-entity"))
    names = [t.name for t in snapshot.tools]
    assert "flight_status_lookup" in names
    assert "search_flights_advanced" in names
    assert "get_delayed_flights" in names
    assert all(name for name in names), "resolved tools must carry non-empty names"


def test_schema_payload_has_no_governance_fields() -> None:
    resolver = _resolver()
    snapshot = _run(resolver.resolve("test-entity"))
    for tool in snapshot.tools:
        schema = tool.to_schema()
        assert set(schema.keys()) == {"type", "function"}
        function = schema["function"]
        assert set(function.keys()) <= {"name", "description", "parameters"}
        assert "category" not in function
        assert "operation_level" not in function
        assert "risk_level" not in function


def test_every_builtin_tool_routes_to_non_mock_implementation() -> None:
    resolver = _resolver()
    snapshot = _run(resolver.resolve("test-entity"))
    assert snapshot.tools, "expected a non-empty resolved snapshot"

    executor = ToolExecutor(
        mq_gate=AuthorizedToolMqGate(),
        read_only_backend=FakeReadOnlyBackend(),
    )

    for tool in snapshot.tools:
        name = tool.name
        assert is_read_only_tool(name) or name in WRITE_ACTION_TOOLS, (
            f"builtin tool {name} routes to neither the read-only backend nor a write-action proposal"
        )
        result = _run(
            executor.execute(
                {"tool_call_id": "call-1", "tool_name": name, "arguments": {}},
                run_id="run-1",
            )
        )
        if name in WRITE_ACTION_TOOLS:
            assert result.success is True, f"{name} should produce a proposal"
            assert result.proposal is not None, f"{name} should produce a proposal"
        else:
            assert result.success is True, f"{name} should execute via the wired backend"
            assert result.result.get("source") == "test_fake", f"{name} must carry a source"


def test_get_available_tools_derives_from_catalog_not_a_second_list() -> None:
    catalog_names = {tool["name"] for tool in _builtin_tool_catalog()}
    available = set(ToolExecutor().get_available_tools())
    assert catalog_names <= available
    assert set(WRITE_ACTION_TOOLS.keys()) <= available
