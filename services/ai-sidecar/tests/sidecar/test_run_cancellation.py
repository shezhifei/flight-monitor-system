"""Task D4 — cooperative cancellation: round-boundary stop + terminal event.

Asserts (docs/plans/2026-08-14-hybrid-agent-architecture.md, Task D4):

1. Flipping ``envelope.cancelled`` stops ``LLMStreamRunner`` at the next
   round boundary (no further LLM call, no ``completed`` event).
2. The runtime streaming path turns that stop into a terminal
   ``run.fail`` frame (SSE) AND a durable ``run_fail`` MQ event with
   ``error_code=RUN_CANCELLED`` — the event type the Rust consumer
   (``AiExecutionControlService.handle_run_fail``) expects.
"""

from __future__ import annotations

import asyncio
from types import SimpleNamespace
from typing import Any
from unittest.mock import AsyncMock, MagicMock, patch

import pytest
from test_skill_runtime_injection import (
    FakeCapabilityResolver,
    FakeEnvelope,
    FakeResolvedConfig,
)

from src.infrastructure.ai.llm_stream_runner import (
    LLMStreamRunner,
    StreamEvent,
)
from src.infrastructure.ai.openai_client import Message
from src.infrastructure.ai.runtime_service import RuntimeService


def _mock_executor():
    from src.infrastructure.ai.tools.tool_executor import ToolExecutor

    mock_executor = MagicMock(spec=ToolExecutor)
    mock_execution_result = MagicMock()
    mock_execution_result.error = None
    mock_execution_result.result = {"data": []}
    mock_execution_result.to_sse_payload.return_value = {"tool_name": "list_flights"}
    mock_execution_result.to_dict.return_value = {"result": {"data": []}}
    mock_executor.execute_batch = AsyncMock(return_value=[mock_execution_result])
    return mock_executor


class TestRoundBoundaryStop:
    """envelope.cancelled stops the loop at the round boundary."""

    @pytest.mark.asyncio
    async def test_cancel_flag_stops_next_round_and_suppresses_completed(self):
        envelope = SimpleNamespace(cancelled=False, job_id="job-1")
        runner = LLMStreamRunner(client=MagicMock())
        runner._tool_executor = _mock_executor()

        llm_calls = [0]

        async def mock_impl(*args, **kwargs):
            llm_calls[0] += 1
            run_result = kwargs.get("result")
            if run_result is not None:
                run_result.tool_calls = [{"id": "t1", "function": {"name": "list_flights", "arguments": "{}"}}]
                run_result.text = ""
            yield StreamEvent(type="tool_call", tool_call={})

        runner._stream_chat_impl = mock_impl

        events: list[StreamEvent] = []
        async for event in runner.stream_chat_with_tools(
            messages=[Message(role="user", content="hi")],
            model="test-model",
            run_id="run-cancel-1",
            envelope=envelope,
            on_child_event=None,
            max_tool_rounds=10,
        ):
            events.append(event)
            # The dispatcher flips the flag while round 0 tools execute.
            if llm_calls[0] == 1:
                envelope.cancelled = True

        assert llm_calls[0] == 1, "no second LLM call after cancellation"
        assert events[-1].type == "cancelled"
        assert events[-1].round_index == 0
        # The cancelled run never reaches the normal completion path.
        assert not any(e.type == "completed" and e.result is not None and e.result.text for e in events)

    @pytest.mark.asyncio
    async def test_cancel_flag_before_first_llm_call_stops_immediately(self):
        envelope = SimpleNamespace(cancelled=True, job_id="job-1")
        runner = LLMStreamRunner(client=MagicMock())

        async def mock_impl(*args, **kwargs):  # pragma: no cover - must not run
            raise AssertionError("LLM must not be called for a cancelled run")
            yield

        runner._stream_chat_impl = mock_impl

        events: list[StreamEvent] = []
        async for event in runner.stream_chat_with_tools(
            messages=[Message(role="user", content="hi")],
            model="test-model",
            run_id="run-cancel-0",
            envelope=envelope,
            max_tool_rounds=10,
        ):
            events.append(event)

        assert [e.type for e in events] == ["cancelled"]


# ---------------------------------------------------------------------------
# Streaming-level terminal event
# ---------------------------------------------------------------------------


class _FakeMqPublisher:
    def __init__(self) -> None:
        self.published: list[dict[str, Any]] = []

    async def publish(self, envelope):
        self.published.append(envelope)


class _CancelledRunner:
    """Runner stand-in: the run was cancelled at a round boundary."""

    def __init__(self, *args, **kwargs) -> None:
        pass

    async def stream_chat_with_tools(self, **kwargs):
        yield StreamEvent(type="cancelled", round_index=2)


def _collect(svc, envelope):
    async def _it():
        return [evt async for evt in svc.stream_run_with_tools(envelope)]

    loop = asyncio.new_event_loop()
    try:
        return loop.run_until_complete(_it())
    finally:
        loop.close()


def test_cancelled_run_emits_run_fail_over_sse_and_mq(monkeypatch) -> None:
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)

    publisher = _FakeMqPublisher()
    import src.infrastructure.ai.runtime_service._streaming_tools as streaming_tools

    monkeypatch.setattr(streaming_tools, "_resolve_mq_publisher", lambda: publisher)
    monkeypatch.setattr(streaming_tools, "_resolve_mq_gate", lambda: None)

    config = FakeResolvedConfig()
    config.tools = [
        SimpleNamespace(
            name="list_flights",
            category="read",
            to_schema=lambda: {
                "type": "function",
                "function": {"name": "list_flights", "description": "d", "parameters": {}},
            },
        )
    ]
    svc = RuntimeService(
        capability_resolver=FakeCapabilityResolver(resolved_config=config),
        llm_client=SimpleNamespace(is_configured=lambda: True, _model="gpt-4o"),
    )

    with patch("src.infrastructure.ai.runtime_service.LLMStreamRunner", _CancelledRunner):
        events = _collect(svc, FakeEnvelope())

    # SSE terminal frame
    fail_events = [e for e in events if e.get("event") == "run.fail"]
    assert len(fail_events) == 1, [e.get("event") for e in events]
    assert "RUN_CANCELLED" in fail_events[0]["data"]["answer"]
    assert not any(e.get("event") == "run.complete" for e in events)

    # Durable MQ terminal event — the type the Rust consumer expects.
    run_fail = [e for e in publisher.published if e.get("event_type") == "run_fail"]
    assert len(run_fail) == 1
    assert run_fail[0]["payload"]["error_code"] == "RUN_CANCELLED"
    assert run_fail[0]["run_id"] == "test-run"
    assert run_fail[0]["job_id"] == "test-job"
