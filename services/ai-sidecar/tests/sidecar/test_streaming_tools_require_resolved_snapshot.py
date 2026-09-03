"""Task A2 — stream_run_with_tools must fail closed without a resolved tool snapshot.

Asserts (docs/plans/2026-08-14-hybrid-agent-architecture.md, Task A2 Step 1):

1. ``stream_run_with_tools`` with ``resolved_config is None`` (legacy env-only
   mode) fails the run with ``AI_TOOL_SNAPSHOT_MISSING`` instead of falling
   back to ``READ_ONLY_TOOL_SCHEMAS``.
2. A resolved snapshot whose ``tools == []`` fails closed the same way.
3. With a non-empty snapshot the tools handed to the runner are exactly the
   resolved ``to_schema()`` list — no mock fallback schemas leak in.
"""

from __future__ import annotations

import asyncio
from dataclasses import dataclass, field
from typing import Any
from unittest.mock import patch

import pytest
from test_skill_runtime_injection import (
    FakeCapabilityResolver,
    FakeEnvelope,
    FakeResolvedConfig,
)

from src.infrastructure.ai.runtime_service import RuntimeService


@dataclass
class _FakeResult:
    text: str = "ANSWER"
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
    """Minimal resolved-tool stand-in with the same surface as ResolvedToolConfig."""

    name: str
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

        async def stream_chat_with_tools(self, *, messages, tools, **kwargs):
            self.tools = tools
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

    loop = asyncio.new_event_loop()
    try:
        return loop.run_until_complete(_it())
    finally:
        loop.close()


@pytest.fixture(autouse=True)
def _no_env_key(monkeypatch):
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)


def _fail_events(events) -> list[dict]:
    return [e for e in events if e.get("event") == "run.fail"]


def test_missing_snapshot_fails_closed() -> None:
    svc = RuntimeService(
        capability_resolver=FakeCapabilityResolver(resolved_config=None),
        llm_client=_ConfiguredLLM(),
    )
    events = _collect(svc, FakeEnvelope())
    fails = _fail_events(events)
    assert len(fails) == 1, [e.get("event") for e in events]
    payload = fails[0]["data"]
    assert "AI_TOOL_SNAPSHOT_MISSING" in payload["answer"]
    assert payload["blocked_by"] == "snapshot"
    assert payload["rule"] == "AI_TOOL_SNAPSHOT_MISSING"
    assert payload["detail"] == "no resolved tool snapshot for this run"


def test_empty_tools_snapshot_fails_closed() -> None:
    config = FakeResolvedConfig()
    config.tools = []
    svc = RuntimeService(
        capability_resolver=FakeCapabilityResolver(resolved_config=config),
        llm_client=_ConfiguredLLM(),
    )
    events = _collect(svc, FakeEnvelope())
    fails = _fail_events(events)
    assert len(fails) == 1, [e.get("event") for e in events]
    payload = fails[0]["data"]
    assert "AI_TOOL_SNAPSHOT_MISSING" in payload["answer"]
    assert payload["blocked_by"] == "snapshot"
    assert payload["rule"] == "AI_TOOL_SNAPSHOT_MISSING"
    assert payload["detail"] == "no resolved tool snapshot for this run"


def test_snapshot_tools_are_passed_verbatim_without_fallback() -> None:
    config = FakeResolvedConfig()
    config.tools = [_FakeTool("search_flights_advanced"), _FakeTool("flight_status_lookup")]
    capture: dict[str, Any] = {}
    svc = RuntimeService(
        capability_resolver=FakeCapabilityResolver(resolved_config=config),
        llm_client=_ConfiguredLLM(),
    )
    with patch("src.infrastructure.ai.runtime_service.LLMStreamRunner", _make_capturing_runner(capture)):
        events = _collect(svc, FakeEnvelope())

    runner = capture["runner"]
    assert runner.tools is not None
    names = [t["function"]["name"] for t in runner.tools]
    assert names == ["search_flights_advanced", "flight_status_lookup"]
    # The heuristic/mock fallback would have produced a run.complete without a
    # runner; assert the stream actually completed through the runner.
    assert any(e.get("event") == "run.complete" for e in events)
