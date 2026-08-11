"""Wave 2.5 tests — composition root, EnvelopeRequester permissions, run terminal events, round_index.

These tests cover the four tasks in the Wave 2.5 scope:

* :class:`EnvelopeRequester` accepts a JSON payload without ``permissions``
  (backward compat) and surfaces ``permissions`` to the gate.
* The MQ runtime composition root constructs a
  :class:`ToolMqGate` from a publisher + poller and exposes it through
  the AI container so the runtime service picks it up.
* The streaming tool path publishes a ``run.complete`` / ``run.fail``
  MQ event alongside the SSE event.
* The LLM tool-call loop increments ``round_index`` for every
  model → tool → model cycle.
"""

from __future__ import annotations

import asyncio
from typing import Any

import pytest

from src.infrastructure.ai.ai_container import get_ai_container
from src.infrastructure.ai.context_envelope import (
    ContextEnvelope,
    EnvelopeContext,
    EnvelopeOntology,
    EnvelopeRequester,
    EnvelopeTask,
)
from src.infrastructure.ai.messaging.ai_command_poller import AiCommandPoller
from src.infrastructure.ai.messaging.ai_runtime_event_publisher import (
    AiRuntimeEventPublisher,
    PublishConfig,
)
from src.infrastructure.ai.messaging.mq_runtime_bootstrap import (
    MqRuntimeComponents,
    build_mq_runtime_components,
    get_mq_runtime_components,
    reset_mq_runtime_components,
    set_mq_runtime_components,
)
from src.infrastructure.ai.tools.mq_gate import (
    ToolMqGate,
    _read_requester,
)
from src.infrastructure.ai.tools.tool_executor import ToolExecutor

# ---------------------------------------------------------------------------
# Fakes
# ---------------------------------------------------------------------------


class FakePublisher(AiRuntimeEventPublisher):
    """In-memory publisher that records every envelope it sees."""

    def __init__(self) -> None:
        self.published: list[dict[str, Any]] = []
        config = PublishConfig(base_url="https://test.invalid", timeout=0.1)
        super().__init__(config, client=None)

    async def publish(self, event: dict[str, Any]) -> str:
        self.published.append(event)
        return f"msg-{len(self.published)}"


class FakePoller:
    """Minimal poller stub for composition-root tests."""

    def __init__(self) -> None:
        self.fetch_pending_calls: list[tuple[str | None, int | None]] = []
        self._shutdown_event = asyncio.Event()

    @property
    def is_shutdown(self) -> bool:
        return self._shutdown_event.is_set()

    def request_shutdown(self) -> None:
        self._shutdown_event.set()

    async def fetch_pending(self, owner: str | None = None, batch_size: int | None = None):
        self.fetch_pending_calls.append((owner, batch_size))
        return []

    async def mark_completed(self, command_id: str) -> None:
        return None

    async def mark_failed(self, command_id: str, error: str) -> None:
        return None


# ---------------------------------------------------------------------------
# Task 1 — EnvelopeRequester carries permissions
# ---------------------------------------------------------------------------


def test_envelope_requester_deserializes_without_permissions_field() -> None:
    """Backward compat: a JSON payload without ``permissions`` deserializes to ``[]``."""
    payload = {
        "user_id": "user-1",
        "roles": ["ai:chat"],
        "department_id": "dept-1",
        "permission_version": "v1",
    }
    requester = EnvelopeRequester.model_validate(payload)
    assert requester.user_id == "user-1"
    assert requester.roles == ["ai:chat"]
    assert requester.permissions == []
    assert requester.department_id == "dept-1"
    assert requester.permission_version == "v1"


def test_envelope_requester_accepts_permissions_field() -> None:
    requester = EnvelopeRequester(
        user_id="user-1",
        roles=["ai:chat"],
        permissions=["ops:read", "ops:write"],
    )
    assert requester.permissions == ["ops:read", "ops:write"]


def test_context_envelope_propagates_permissions_through_requester() -> None:
    envelope = ContextEnvelope(
        job_id="job-1",
        run_id="run-1",
        correlation_id="corr-1",
        requester=EnvelopeRequester(user_id="u1", permissions=["ops:read"]),
        ontology=EnvelopeOntology(),
        context=EnvelopeContext(),
        task=EnvelopeTask(task_type="chat", user_message="hi"),
    )
    assert envelope.requester.permissions == ["ops:read"]


def test_read_requester_includes_permissions() -> None:
    """The gate helper surfaces ``permissions`` on the requester dict."""
    envelope = ContextEnvelope(
        job_id="job-1",
        run_id="run-1",
        correlation_id="corr-1",
        requester=EnvelopeRequester(
            user_id="u1",
            roles=["ai:chat"],
            permissions=["ops:read", "ops:write"],
        ),
        ontology=EnvelopeOntology(),
        context=EnvelopeContext(),
        task=EnvelopeTask(task_type="chat", user_message="hi"),
    )
    requester = _read_requester(envelope)
    assert requester is not None
    assert requester["user_id"] == "u1"
    assert requester["permissions"] == ["ops:read", "ops:write"]


def test_read_requester_returns_empty_permissions_when_field_absent() -> None:
    envelope = ContextEnvelope(
        job_id="job-1",
        run_id="run-1",
        correlation_id="corr-1",
        requester=EnvelopeRequester(user_id="u1"),
        ontology=EnvelopeOntology(),
        context=EnvelopeContext(),
        task=EnvelopeTask(task_type="chat", user_message="hi"),
    )
    requester = _read_requester(envelope)
    assert requester is not None
    assert requester["permissions"] == []


# ---------------------------------------------------------------------------
# Task 2 — Composition root builds publisher + poller + gate
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_build_mq_runtime_components_with_explicit_deps() -> None:
    publisher = FakePublisher()
    poller = FakePoller()
    gate = ToolMqGate(publisher=publisher, poller=poller)

    components = MqRuntimeComponents(
        publisher=publisher,
        poller=poller,
        gate=gate,
    )
    assert components.is_wired is True
    assert components.publisher is publisher
    assert components.poller is poller
    assert components.gate is gate
    assert components.run_loop_task is None


@pytest.mark.asyncio
async def test_build_mq_runtime_components_async_constructs_gate() -> None:
    """When called with no overrides the function returns a (possibly degraded) bundle."""
    components = await build_mq_runtime_components(db_pool=None)
    # Without a DB pool the poller is None; without an mq-gateway URL the
    # publisher is None. The bundle must still be safe to introspect.
    assert components.publisher is None or isinstance(components.publisher, AiRuntimeEventPublisher)
    assert components.poller is None or isinstance(components.poller, AiCommandPoller)
    assert components.gate is None or isinstance(components.gate, ToolMqGate)
    assert components.is_wired is False


@pytest.mark.asyncio
async def test_set_and_get_mq_runtime_components_singleton() -> None:
    publisher = FakePublisher()
    poller = FakePoller()
    gate = ToolMqGate(publisher=publisher, poller=poller)
    components = MqRuntimeComponents(publisher=publisher, poller=poller, gate=gate)

    set_mq_runtime_components(components)
    try:
        assert get_mq_runtime_components() is components
    finally:
        reset_mq_runtime_components()
    assert get_mq_runtime_components() is None


@pytest.mark.asyncio
async def test_register_mq_components_in_ai_container() -> None:
    """The composition root registers the gate so the runtime service picks it up."""
    from src.infrastructure.ai.ai_runtime_bootstrap import _register_mq_components

    publisher = FakePublisher()
    poller = FakePoller()
    gate = ToolMqGate(publisher=publisher, poller=poller)
    components = MqRuntimeComponents(publisher=publisher, poller=poller, gate=gate)

    container = get_ai_container()
    _register_mq_components(components)
    try:
        assert container.resolve("tool_mq_gate", None) is gate
        assert container.resolve("mq_event_publisher", None) is publisher
        assert container.resolve("mq_command_poller", None) is poller
        registered_executor = container.resolve("tool_executor", None)
        assert isinstance(registered_executor, ToolExecutor)
        assert registered_executor.mq_gate is gate
    finally:
        container.clear()
        reset_mq_runtime_components()


# ---------------------------------------------------------------------------
# Task 3 — run.complete / run.fail MQ publishes alongside SSE
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_publish_run_complete_emits_envelope_to_publisher() -> None:
    from src.infrastructure.ai.runtime_service._streaming_tools import (
        _publish_run_complete_mq,
    )

    publisher = FakePublisher()
    await _publish_run_complete_mq(
        publisher,
        None,
        run_id="run-1",
        job_id="job-1",
        round_index=2,
        event_sequence=5,
        output={"status": "succeeded", "answer": "hi"},
        proposal_ids=["p-1"],
    )
    assert len(publisher.published) == 1
    event = publisher.published[0]
    assert event["event_type"] == "run_complete"
    assert event["run_id"] == "run-1"
    assert event["job_id"] == "job-1"
    assert event["round_index"] == 2
    assert event["event_sequence"] == 5
    assert event["payload"]["output_raw"] == {"status": "succeeded", "answer": "hi"}
    assert event["payload"]["proposal_ids"] == ["p-1"]


@pytest.mark.asyncio
async def test_publish_run_fail_emits_envelope_to_publisher() -> None:
    from src.infrastructure.ai.runtime_service._streaming_tools import (
        _publish_run_fail_mq,
    )

    publisher = FakePublisher()
    await _publish_run_fail_mq(
        publisher,
        run_id="run-2",
        job_id="job-2",
        round_index=1,
        event_sequence=3,
        error_code="AI_RUNTIME_PROCESSING_ERROR",
        error_message="boom",
    )
    assert len(publisher.published) == 1
    event = publisher.published[0]
    assert event["event_type"] == "run_fail"
    assert event["payload"]["error_code"] == "AI_RUNTIME_PROCESSING_ERROR"
    assert event["payload"]["error_message"] == "boom"


@pytest.mark.asyncio
async def test_publish_run_complete_is_noop_without_publisher() -> None:
    from src.infrastructure.ai.runtime_service._streaming_tools import (
        _publish_run_complete_mq,
    )

    # Must not raise; best-effort semantics.
    await _publish_run_complete_mq(
        None,
        None,
        run_id="run-1",
        job_id="job-1",
        round_index=0,
        event_sequence=1,
        output={},
    )


@pytest.mark.asyncio
async def test_publish_run_fail_is_noop_without_publisher() -> None:
    from src.infrastructure.ai.runtime_service._streaming_tools import (
        _publish_run_fail_mq,
    )

    await _publish_run_fail_mq(
        None,
        run_id="run-1",
        job_id="job-1",
        round_index=0,
        event_sequence=1,
        error_code="X",
        error_message="y",
    )


# ---------------------------------------------------------------------------
# Task 4 — round_index increments per model → tool → model cycle
# ---------------------------------------------------------------------------


def test_stream_runner_round_index_zero_for_no_tool_call() -> None:
    """When the LLM does not request a tool, the completed event reports round 0."""
    from src.infrastructure.ai.llm_stream_runner import (
        LLMStreamRunner,
    )
    from src.infrastructure.ai.openai_client import ChatCompletionChunk

    class _StubClient:
        config: Any = None

        async def chat_completion(self, *args, **kwargs):
            async def _iter():
                yield ChatCompletionChunk(
                    id="x",
                    object="chat.completion.chunk",
                    created=0,
                    model="m",
                    choices=[{"index": 0, "delta": {"content": "hi"}, "finish_reason": "stop"}],
                )

            return _iter()

    runner = LLMStreamRunner(client=_StubClient(), tool_executor=ToolExecutor())
    msgs: list[Any] = []

    async def _collect() -> tuple[list[int], set[int]]:
        rounds_tool_call: list[int] = []
        completed_rounds: list[int] = []
        async for evt in runner.stream_chat_with_tools(
            messages=msgs,
            model="m",
            tools=None,
            run_id="run-1",
            envelope=None,
        ):
            if evt.type == "tool_call":
                rounds_tool_call.append(evt.round_index)
            if evt.type == "completed":
                completed_rounds.append(evt.round_index)
        return rounds_tool_call, set(completed_rounds)

    tool_call_rounds, completed_rounds = asyncio.run(_collect())
    assert tool_call_rounds == []
    assert completed_rounds == {0}


def test_stream_runner_round_index_increments_per_round() -> None:
    """Each model → tool → model cycle must report an incrementing round_index."""
    from src.infrastructure.ai.llm_stream_runner import (
        LLMStreamRunner,
    )
    from src.infrastructure.ai.openai_client import ChatCompletionChunk

    class _StubClient:
        config: Any = None

        def __init__(self) -> None:
            self.calls = 0

        async def chat_completion(self, *args, **kwargs):
            self.calls += 1
            call_id = self.calls

            async def _iter():
                if call_id == 1:
                    yield ChatCompletionChunk(
                        id="x",
                        object="chat.completion.chunk",
                        created=0,
                        model="m",
                        choices=[
                            {
                                "index": 0,
                                "delta": {
                                    "tool_calls": [
                                        {
                                            "index": 0,
                                            "id": "call-1",
                                            "type": "function",
                                            "function": {
                                                "name": "flight_status_lookup",
                                                "arguments": '{"flight_id": "CA1234"}',
                                            },
                                        }
                                    ]
                                },
                                "finish_reason": "tool_calls",
                            }
                        ],
                    )
                else:
                    yield ChatCompletionChunk(
                        id="x",
                        object="chat.completion.chunk",
                        created=0,
                        model="m",
                        choices=[
                            {
                                "index": 0,
                                "delta": {"content": "final"},
                                "finish_reason": "stop",
                            }
                        ],
                    )

            return _iter()

    captured: list[dict[str, Any]] = []

    class _CapturingExecutor(ToolExecutor):
        async def execute_batch(self, tool_calls, run_id, envelope=None, **kwargs):  # type: ignore[override]
            captured.append(
                {
                    "round_index": kwargs.get("round_index"),
                    "tool_call_id": tool_calls[0]["tool_call_id"],
                }
            )
            from src.infrastructure.ai.tools.tool_executor import ToolExecutionResult

            return [
                ToolExecutionResult(
                    tool_call_id=tc["tool_call_id"],
                    tool_name=tc["tool_name"],
                    success=True,
                    result={"flight_id": "CA1234"},
                )
                for tc in tool_calls
            ]

    runner = LLMStreamRunner(
        client=_StubClient(),
        tool_executor=_CapturingExecutor(),
    )

    async def _run() -> set[int]:
        rounds: list[int] = []
        async for evt in runner.stream_chat_with_tools(
            messages=[],
            model="m",
            tools=[{"function": {"name": "flight_status_lookup"}}],
            run_id="run-1",
            envelope=None,
        ):
            if evt.type == "completed":
                rounds.append(evt.round_index)
        return set(rounds)

    completed_rounds = asyncio.run(_run())
    assert completed_rounds == {0, 1}
    assert [c["round_index"] for c in captured] == [0]


# ---------------------------------------------------------------------------
# Backward compatibility — gate-less path still works
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_executor_without_gate_keeps_legacy_behavior() -> None:
    """The existing ``mq_gate=None`` fallback is preserved for tests / degraded mode."""
    executor = ToolExecutor()
    tool_call = {
        "tool_call_id": "call-1",
        "tool_name": "flight_status_lookup",
        "arguments": {"flight_id": "CA1234"},
    }
    result = await executor.execute(tool_call, run_id="run-1")
    assert result.success is True
    assert result.result["flight_id"] == "CA1234"
