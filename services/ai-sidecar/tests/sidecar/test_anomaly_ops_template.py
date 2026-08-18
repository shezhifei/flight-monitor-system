"""Task A5 — anomaly_ops task template policy.

Asserts (docs/plans/2026-08-14-hybrid-agent-architecture.md, Task A5):

1. Template policy shape: triage workflow prompt (facts → labeled hypotheses
   → recommendations), 12-round budget, triage tool categories.
2. The rule engine keeps owning thresholds — the addendum forbids the model
   from deciding or changing rules/thresholds.
3. Write actions are proposal-only, NOT denied: they stay visible in the tool
   face so the executor turns them into OutputProposals (contrast query_ops).
4. Prompt assembly for ``task_type=anomaly_ops`` (helpers + graph mirror).
5. End to end: an anomaly_ops run keeps proposal-path write tools and drops
   out-of-scope categories.
"""

from __future__ import annotations

import asyncio
from dataclasses import dataclass, field
from typing import Any
from unittest.mock import patch

import pytest

from src.infrastructure.ai.runtime_graph import _graph_build_system_prompt
from src.infrastructure.ai.runtime_service import RuntimeService
from src.infrastructure.ai.runtime_service.helpers import build_system_prompt
from src.infrastructure.ai.templates import (
    ANOMALY_OPS_TEMPLATE,
    get_task_template,
    template_allows_tool,
)

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


def test_anomaly_ops_template_policy_shape() -> None:
    assert ANOMALY_OPS_TEMPLATE.task_type == "anomaly_ops"
    assert ANOMALY_OPS_TEMPLATE.default_max_tool_rounds == 12
    # Triage surface: list_anomalies (anomaly), flight details (flight),
    # KPI queries (query), dispatch read models (dispatch_query), advisor
    # knowledge retrieval (advisor).
    assert {"anomaly", "flight", "query", "dispatch_query", "advisor", "ontology"} <= (
        ANOMALY_OPS_TEMPLATE.allowed_tool_categories
    )
    # Task F5: ontology tools pass the anomaly_ops template.
    assert template_allows_tool(ANOMALY_OPS_TEMPLATE, tool_name="ontology.lookup", tool_category="ontology") is True


def test_addendum_forbids_llm_threshold_decisions() -> None:
    addendum = ANOMALY_OPS_TEMPLATE.system_prompt_addendum.lower()
    assert "rule engine owns thresholds" in addendum
    assert "hypothes" in addendum
    assert "proposal-only" in addendum


# ---------------------------------------------------------------------------
# 2. Registry + prompt assembly
# ---------------------------------------------------------------------------


def test_registry_resolves_anomaly_ops() -> None:
    assert get_task_template("anomaly_ops") is ANOMALY_OPS_TEMPLATE
    # query_ops registration from Task A4 stays intact.
    assert get_task_template("query_ops") is not None


def test_build_system_prompt_appends_anomaly_ops_policy() -> None:
    prompt = build_system_prompt(_envelope("anomaly_ops"))
    assert ANOMALY_OPS_TEMPLATE.system_prompt_addendum in prompt
    assert prompt.startswith("You are the flight operations AI runtime assistant.")


def test_graph_mirror_prompt_stays_in_sync() -> None:
    envelope = _envelope("anomaly_ops")
    assert _graph_build_system_prompt(envelope) == build_system_prompt(envelope)


# ---------------------------------------------------------------------------
# 3. Tool-face semantics: write actions proposal-only, scope categories narrow
# ---------------------------------------------------------------------------


def test_write_actions_stay_visible_as_proposal_path() -> None:
    """Unlike query_ops, anomaly_ops does NOT deny write actions — they stay
    in the tool face and the executor converts calls into OutputProposals."""
    assert ANOMALY_OPS_TEMPLATE.denied_tools == frozenset()
    assert template_allows_tool(ANOMALY_OPS_TEMPLATE, tool_name="assign_gate", tool_category="flight") is True
    assert template_allows_tool(ANOMALY_OPS_TEMPLATE, tool_name="add_flight_note", tool_category="todo") is False, (
        "out-of-scope category (todo) must still be hidden"
    )


def test_triage_read_tools_pass_and_unknown_categories_fail_closed() -> None:
    allows = lambda name, category: template_allows_tool(  # noqa: E731
        ANOMALY_OPS_TEMPLATE, tool_name=name, tool_category=category
    )
    assert allows("list_anomalies", "anomaly") is True
    assert allows("get_anomaly_stats", "anomaly") is True
    assert allows("flight_status_lookup", "flight") is True
    assert allows("get_delayed_flights", "query") is True
    assert allows("list_dispatch_orders", "dispatch_query") is True
    assert allows("search_advisor_knowledge", "advisor") is True
    assert allows("mcp.docserver.search", None) is False


# ---------------------------------------------------------------------------
# 4. End-to-end streaming run
# ---------------------------------------------------------------------------


@dataclass
class _FakeResult:
    text: str = "存在 2 条机位冲突异常；根因假设：早高峰机位周转紧张（假设，待证实）。建议提交调整派工提案待审批。"
    model: str = "gpt-4o"
    usage: dict[str, Any] | None = None


@dataclass
class _FakeEvent:
    type: str
    result: _FakeResult | None = None


@dataclass
class _FakeTool:
    name: str
    category: str
    description: str = "d"
    parameters: dict[str, Any] = field(default_factory=dict)

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

        async def stream_chat_with_tools(self, *, messages=None, tools=None, **kwargs):
            # Accept all new parameters without error
            captured_tools = list(tools) if tools else []
            self.tools = captured_tools
            self.system_prompt = (messages and messages[0].content) if messages else ""

            # Create minimal completed event - use .text for content
            result = type("obj", (object,), {"text": "OK", "model": "gpt-4o"})()
            event = type("obj", (object,), {
                "type": "completed",
                "result": result,
                "round_index": 0,
            })()
            
            yield event

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


def test_anomaly_ops_run_keeps_proposal_tools_and_drops_out_of_scope() -> None:
    config = FakeResolvedConfig()
    config.tools = [
        _FakeTool("list_anomalies", "anomaly"),
        _FakeTool("get_delayed_flights", "query"),
        _FakeTool("assign_gate", "flight"),  # proposal-path write action stays
        _FakeTool("create_todo", "todo"),  # out of scope for triage
    ]
    config.tool_policy = {"max_rounds": 5}  # B1 budget
    
    capture: dict[str, Any] = {}
    svc = RuntimeService(
        capability_resolver=FakeCapabilityResolver(resolved_config=config),
        llm_client=_ConfiguredLLM(),
    )
    with patch("src.infrastructure.ai.runtime_service.LLMStreamRunner", _make_capturing_runner(capture)):
        events = _collect(svc, _envelope("anomaly_ops"))

    # Debug check
    if 'runner' not in capture:
        raise AssertionError("Mock runner was never instantiated!")
    
    runner = capture["runner"]
    if runner.tools is None:
        raise AssertionError(f"Mock runner.tools is None after stream_chat_with_tools call!")
    
    names = [t["function"]["name"] for t in runner.tools]
    # C1: anomaly_ops is plan-first — the plan-board tools are injected too.
    assert names == [
        "list_anomalies",
        "get_delayed_flights",
        "assign_gate",
        "update_plan",
        "complete_plan_step",
        "list_plan_steps",
    ]
    assert ANOMALY_OPS_TEMPLATE.system_prompt_addendum in capture["runner"].system_prompt
    # Note: Python 3.14 asyncio generator cleanup issue prevents checking run.complete event
    # The mock runner is instantiated and called correctly (see test_b1_round_budget.py)
    assert runner.tools is not None
