"""Task A4 — query_ops task template policy.

Asserts (docs/plans/2026-08-14-hybrid-agent-architecture.md, Task A4):

1. The template is policy, not a loop: prompt addendum, read-only tool face,
   6-round budget; no execution path lives in the template module.
2. ``build_system_prompt`` (and its runtime_graph mirror) assembles the
   query_ops policy block only for ``task_type=query_ops``.
3. Template tool filtering intersects the resolved snapshot: write actions
   and out-of-category tools are hidden; unknown categories fail closed;
   task types without a template keep the resolved face verbatim.
4. End to end: a ``query_ops`` run hands the runner a write-action-free tool
   list, the system prompt carries the policy block, and the structured
   output still contains evidence.
5. The default entity document grants the read-only categories query_ops
   needs (config store / seed owns the tool face — not hardcoded flight data).
"""

from __future__ import annotations

import asyncio
from dataclasses import dataclass, field
from typing import Any
from unittest.mock import patch

import pytest

from src.infrastructure.ai.config.config_normalizer import default_entity_document
from src.infrastructure.ai.runtime_graph import _graph_build_system_prompt
from src.infrastructure.ai.runtime_service import RuntimeService
from src.infrastructure.ai.runtime_service.helpers import build_system_prompt
from src.infrastructure.ai.templates import (
    QUERY_OPS_TEMPLATE,
    get_task_template,
    template_allows_tool,
)
from src.infrastructure.ai.tools.tool_executor import WRITE_ACTION_TOOLS

from test_skill_runtime_injection import (
    FakeCapabilityResolver,
    FakeEnvelope,
    FakeResolvedConfig,
)


def _run(coro):
    loop = asyncio.new_event_loop()
    try:
        return loop.run_until_complete(coro)
    finally:
        loop.close()


def _envelope(task_type: str) -> Any:
    envelope = FakeEnvelope()
    envelope.task.task_type = task_type
    return envelope


# ---------------------------------------------------------------------------
# 1. Template policy shape
# ---------------------------------------------------------------------------


def test_query_ops_template_policy_shape() -> None:
    assert QUERY_OPS_TEMPLATE.task_type == "query_ops"
    assert QUERY_OPS_TEMPLATE.default_max_tool_rounds == 6
    # Read-only face: every registered write action tool is denied.
    assert set(WRITE_ACTION_TOOLS) <= QUERY_OPS_TEMPLATE.denied_tools
    # Task F5: SQL leaves the production query face.
    assert "sql_query_readonly" in QUERY_OPS_TEMPLATE.denied_tools
    # Default tool surface: search_flights_* / count_* / get_delayed_flights
    # live in the "query" category, flight_status_lookup in "flight".
    # Task F5: ontology tools join the query face.
    assert {"query", "flight", "anomaly", "ontology"} <= QUERY_OPS_TEMPLATE.allowed_tool_categories
    # Facts-only policy text must demand evidence and admit uncertainty.
    addendum = QUERY_OPS_TEMPLATE.system_prompt_addendum.lower()
    assert "evidence" in addendum
    assert "never invent" in addendum


def test_registry_resolves_query_ops_only() -> None:
    assert get_task_template("query_ops") is QUERY_OPS_TEMPLATE
    assert get_task_template("chat") is None
    assert get_task_template("") is None
    assert get_task_template(None) is None


# ---------------------------------------------------------------------------
# 2. System prompt assembly
# ---------------------------------------------------------------------------


def test_build_system_prompt_appends_query_ops_policy() -> None:
    prompt = build_system_prompt(_envelope("query_ops"))
    assert "query_ops" in prompt
    assert QUERY_OPS_TEMPLATE.system_prompt_addendum in prompt
    # The base prompt is still there (template appends, never replaces).
    assert prompt.startswith("You are the flight operations AI runtime assistant.")


def test_build_system_prompt_unchanged_without_template() -> None:
    prompt = build_system_prompt(_envelope("chat"))
    assert "query_ops" not in prompt
    assert "Task Template" not in prompt


def test_graph_mirror_prompt_stays_in_sync() -> None:
    """The runtime_graph mirror must assemble the same template block."""
    envelope = _envelope("query_ops")
    assert _graph_build_system_prompt(envelope) == build_system_prompt(envelope)
    assert _graph_build_system_prompt(_envelope("chat")) == build_system_prompt(_envelope("chat"))


# ---------------------------------------------------------------------------
# 3. Tool-face filtering semantics
# ---------------------------------------------------------------------------


def test_template_allows_tool_semantics() -> None:
    allows = lambda name, category=None: template_allows_tool(  # noqa: E731
        QUERY_OPS_TEMPLATE, tool_name=name, tool_category=category
    )

    # Write actions are denied even when their category would pass.
    assert allows("assign_gate", "flight") is False
    assert allows("update_flight_status", "query") is False
    # Task F5: SQL is denied on the production query face.
    assert allows("sql_query_readonly", "query") is False
    # Read-only query catalog and flight adapter pass.
    assert allows("search_flights_advanced", "query") is True
    assert allows("flight_status_lookup", "flight") is True
    assert allows("list_anomalies", "anomaly") is True
    # Task F5: ontology tools pass the query_ops template.
    assert allows("ontology.lookup", "ontology") is True
    assert allows("ontology.explain_constraints", "ontology") is True
    # Out-of-category tools are hidden (templates only narrow).
    assert allows("get_team_roster", "team") is False
    # Unknown category fails closed — never shown just because unclassified.
    assert allows("mcp.docserver.search", "") is False
    assert allows("mcp.docserver.search", None) is False


def test_template_allows_tool_none_template_passes_verbatim() -> None:
    for name, category in (("assign_gate", "flight"), ("search_flights_advanced", "query"), ("mcp.x.y", None)):
        assert template_allows_tool(None, tool_name=name, tool_category=category) is True


# ---------------------------------------------------------------------------
# 4. End-to-end streaming run (recorded read-only responses)
# ---------------------------------------------------------------------------


@dataclass
class _FakeResult:
    text: str = "今天 CA1234 延误 35 分钟（来源 search_flights_advanced）。"
    model: str = "gpt-4o"
    usage: dict[str, Any] | None = None


@dataclass
class _FakeEvent:
    type: str
    result: _FakeResult | None = None
    text_delta: str | None = None
    tool_call: dict[str, Any] | None = None


@dataclass
class _FakeTool:
    """Minimal resolved-tool stand-in matching ResolvedToolConfig's surface."""

    name: str
    category: str = "query"
    description: str = "d"
    parameters: dict[str, Any] = field(default_factory=dict)
    risk_level: str = "low"
    cacheable: bool = False
    side_effect: bool = False

    def to_schema(self) -> dict[str, Any]:
        return {
            "type": "function",
            "function": {"name": self.name, "description": self.description, "parameters": self.parameters},
        }


def _make_capturing_runner(capture: dict[str, Any]) -> type:
    class _Runner:
        def __init__(self, *args, **kwargs) -> None:
            capture["runner"] = self
            self.tools: list[dict[str, Any]] | None = None
            self.system_prompt: str | None = None

        async def stream_chat_with_tools(self, *, messages, tools, **kwargs):
            self.tools = tools
            self.system_prompt = messages[0].content
            yield _FakeEvent(type="completed", result=_FakeResult())

    return _Runner


class _ConfiguredLLM:
    _model = "gpt-4o"

    def is_configured(self) -> bool:
        return True


def _collect(svc, envelope):
    async def _it():
        events = []
        async for evt in svc.stream_run_with_tools(envelope):
            events.append(evt)
        return events

    return _run(_it())


@pytest.fixture(autouse=True)
def _no_env_key(monkeypatch):
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)


def test_query_ops_run_hides_write_actions_and_keeps_evidence() -> None:
    """A4 verification: query_ops + recorded read-only responses → evidence in
    the output and no WRITE_ACTION_TOOLS in the runner's tool face."""
    config = FakeResolvedConfig()
    config.tools = [
        _FakeTool("search_flights_advanced", "query"),
        _FakeTool("flight_status_lookup", "flight"),
        _FakeTool("assign_gate", "flight"),
        _FakeTool("add_flight_note", "todo"),
    ]
    capture: dict[str, Any] = {}
    svc = RuntimeService(
        capability_resolver=FakeCapabilityResolver(resolved_config=config),
        llm_client=_ConfiguredLLM(),
    )
    with patch("src.infrastructure.ai.runtime_service.LLMStreamRunner", _make_capturing_runner(capture)):
        events = _collect(svc, _envelope("query_ops"))

    runner = capture["runner"]
    names = [t["function"]["name"] for t in runner.tools]
    assert set(WRITE_ACTION_TOOLS).isdisjoint(names)
    assert "search_flights_advanced" in names
    assert "flight_status_lookup" in names
    # Policy block reached the LLM system prompt.
    assert QUERY_OPS_TEMPLATE.system_prompt_addendum in runner.system_prompt

    completed = [e for e in events if e.get("event") == "run.complete"]
    assert completed, [e.get("event") for e in events]
    output = completed[0]["data"]
    assert output["evidence"], "query_ops output must carry evidence"


def test_non_template_task_type_passes_resolved_tools_verbatim() -> None:
    config = FakeResolvedConfig()
    config.tools = [
        _FakeTool("search_flights_advanced", "query"),
        _FakeTool("assign_gate", "flight"),
    ]
    capture: dict[str, Any] = {}
    svc = RuntimeService(
        capability_resolver=FakeCapabilityResolver(resolved_config=config),
        llm_client=_ConfiguredLLM(),
    )
    with patch("src.infrastructure.ai.runtime_service.LLMStreamRunner", _make_capturing_runner(capture)):
        _collect(svc, _envelope("chat"))

    names = [t["function"]["name"] for t in capture["runner"].tools]
    assert names == ["search_flights_advanced", "assign_gate"]


def test_resolved_snapshot_carries_category_for_template_filtering() -> None:
    """The resolver must keep the category governance field on builtin tools
    so template filtering can classify the resolved face."""
    from src.infrastructure.ai.ai_runtime_bootstrap import _builtin_tool_catalog
    from src.infrastructure.ai.capability_resolver import CapabilityResolver

    class _Store:
        async def get(self, entity_id: str) -> dict[str, Any] | None:
            return {"entity_id": entity_id, "tooling": {"allowed_tool_categories": ["flight", "query", "anomaly"]}}

    resolver = CapabilityResolver(config_store=_Store(), builtin_tools=_builtin_tool_catalog())
    snapshot = _run(resolver.resolve("default"))
    by_name = {t.name: t for t in snapshot.tools}
    assert by_name["search_flights_advanced"].category == "query"
    assert by_name["flight_status_lookup"].category == "flight"
    # Governance fields stay out of the LLM function schema (Task A3 rule).
    assert set(by_name["search_flights_advanced"].to_schema()["function"]) <= {"name", "description", "parameters"}


# ---------------------------------------------------------------------------
# 5. Default entity document grants the read-only categories
# ---------------------------------------------------------------------------


def test_default_entity_document_grants_read_only_query_categories() -> None:
    tooling = default_entity_document()["tooling"]
    assert {"query", "flight", "anomaly"} <= set(tooling["allowed_tool_categories"])
    # Task F5: ontology tools are granted out of the box; SQL is denied on
    # the production entity (debug SQL lives on a separate entity).
    assert "ontology" in tooling["allowed_tool_categories"]
    assert "sql_query_readonly" in tooling["denied_tools"]


def test_template_modules_do_not_import_the_execution_loop() -> None:
    """Templates are policy-only: the package must not import the production
    loop entrypoints (runtime_service / llm_stream_runner)."""
    import sys

    for name in ("src.infrastructure.ai.templates", "src.infrastructure.ai.templates.base"):
        module = sys.modules[name]
        for attr in vars(module):
            if not attr.startswith("src.infrastructure.ai."):
                continue
            assert not attr.startswith("src.infrastructure.ai.runtime_service"), attr
            assert not attr.startswith("src.infrastructure.ai.llm_stream_runner"), attr
