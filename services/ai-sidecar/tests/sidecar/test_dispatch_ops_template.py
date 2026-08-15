"""Task A6 — dispatch_ops task template policy.

Asserts (docs/plans/2026-08-14-hybrid-agent-architecture.md, Task A6):

1. Template policy shape: advisory workflow prompt (read-only situational
   awareness → solver candidates → ranking/explanation → high-risk proposals
   awaiting approval), 16-round budget.
2. Applying a schedule is forbidden: reserved apply-schedule tool names are
   denied regardless of category; solver output can only become proposals.
3. Prompt assembly for ``task_type=dispatch_ops`` (helpers + graph mirror).
4. End to end: a dispatch_ops run hides apply-schedule tools, keeps the
   read-only + proposal-path face, and carries the policy block.
5. The three Phase-A templates register side by side without affecting each
   other's tool faces.
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
    DISPATCH_OPS_TEMPLATE,
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


def test_dispatch_ops_template_policy_shape() -> None:
    assert DISPATCH_OPS_TEMPLATE.task_type == "dispatch_ops"
    assert DISPATCH_OPS_TEMPLATE.default_max_tool_rounds == 16
    # Situational-awareness surface: dispatch read models, flight state,
    # aggregate queries, anomaly conflicts.
    assert {"dispatch_query", "flight", "query", "anomaly"} <= DISPATCH_OPS_TEMPLATE.allowed_tool_categories


def test_addendum_pins_the_approval_workflow() -> None:
    addendum = DISPATCH_OPS_TEMPLATE.system_prompt_addendum.lower()
    assert "never apply a schedule" in addendum
    assert "awaiting approval" in addendum or "waiting_for_approval" in addendum
    assert "proposal" in addendum


# ---------------------------------------------------------------------------
# 2. Apply-schedule is denied; proposal-path writes stay visible
# ---------------------------------------------------------------------------


def test_apply_schedule_tools_denied_regardless_of_category() -> None:
    for name in ("apply_schedule", "apply_dispatch_schedule"):
        assert template_allows_tool(DISPATCH_OPS_TEMPLATE, tool_name=name, tool_category="dispatch_query") is False
        assert template_allows_tool(DISPATCH_OPS_TEMPLATE, tool_name=name, tool_category=None) is False


def test_proposal_path_write_actions_stay_visible() -> None:
    """High-risk changes flow through proposals: write actions such as
    assign_gate remain in the face for the executor to convert."""
    assert DISPATCH_OPS_TEMPLATE.denied_tools.isdisjoint(WRITE_ACTION_TOOLS)
    assert template_allows_tool(DISPATCH_OPS_TEMPLATE, tool_name="assign_gate", tool_category="flight") is True


def test_situational_read_tools_pass_and_unknown_categories_fail_closed() -> None:
    allows = lambda name, category: template_allows_tool(  # noqa: E731
        DISPATCH_OPS_TEMPLATE, tool_name=name, tool_category=category
    )
    assert allows("list_dispatch_orders", "dispatch_query") is True
    assert allows("flight_status_lookup", "flight") is True
    assert allows("count_flights_by_status", "query") is True
    assert allows("list_anomalies", "anomaly") is True
    assert allows("search_advisor_knowledge", "advisor") is False, "advisor is not part of the dispatch face"
    assert allows("mcp.docserver.search", None) is False


# ---------------------------------------------------------------------------
# 3. Registry + prompt assembly
# ---------------------------------------------------------------------------


def test_registry_resolves_all_three_phase_a_templates() -> None:
    assert get_task_template("dispatch_ops") is DISPATCH_OPS_TEMPLATE
    assert get_task_template("anomaly_ops") is ANOMALY_OPS_TEMPLATE
    assert get_task_template("query_ops") is QUERY_OPS_TEMPLATE
    assert get_task_template("chat") is None


def test_build_system_prompt_appends_dispatch_ops_policy() -> None:
    prompt = build_system_prompt(_envelope("dispatch_ops"))
    assert DISPATCH_OPS_TEMPLATE.system_prompt_addendum in prompt
    assert prompt.startswith("You are the flight operations AI runtime assistant.")


def test_graph_mirror_prompt_stays_in_sync() -> None:
    envelope = _envelope("dispatch_ops")
    assert _graph_build_system_prompt(envelope) == build_system_prompt(envelope)


# ---------------------------------------------------------------------------
# 4. End-to-end streaming run
# ---------------------------------------------------------------------------


@dataclass
class _FakeResult:
    text: str = "当前有 3 个保障缺口。OR-Tools 候选方案 A/B 已生成，推荐方案 A（解释：……）。已提交高风险调整提案，等待审批。"
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


def test_dispatch_ops_run_hides_apply_schedule_and_keeps_proposal_face() -> None:
    config = FakeResolvedConfig()
    config.tools = [
        _FakeTool("list_dispatch_orders", "dispatch_query"),
        _FakeTool("assign_gate", "flight"),  # proposal-path write stays
        _FakeTool("apply_schedule", "dispatch_query"),  # forbidden local apply
        _FakeTool("search_advisor_knowledge", "advisor"),  # out of scope
    ]
    capture: dict[str, Any] = {}
    svc = RuntimeService(
        capability_resolver=FakeCapabilityResolver(resolved_config=config),
        llm_client=_ConfiguredLLM(),
    )
    with patch("src.infrastructure.ai.runtime_service.LLMStreamRunner", _make_capturing_runner(capture)):
        events = _collect(svc, _envelope("dispatch_ops"))

    names = [t["function"]["name"] for t in capture["runner"].tools]
    # C1: dispatch_ops is plan-first — the plan-board tools are injected too.
    assert names == [
        "list_dispatch_orders",
        "assign_gate",
        "update_plan",
        "complete_plan_step",
        "list_plan_steps",
    ]
    assert DISPATCH_OPS_TEMPLATE.system_prompt_addendum in capture["runner"].system_prompt
    assert any(e.get("event") == "run.complete" for e in events)
