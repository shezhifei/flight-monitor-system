"""Tests for the MQ authorization gate wired into :class:`ToolExecutor`.

The tests use in-memory fakes for the publisher, poller, and an
in-process event sequence recorder. They verify the public/protected
split, fail-closed behavior, heartbeat emission, and tool execution.
"""

from __future__ import annotations

import asyncio
import hashlib
from typing import Any

import pytest

from src.infrastructure.ai.context_envelope import (
    ContextEnvelope,
    EnvelopeContext,
    EnvelopeOntology,
    EnvelopeRequester,
    EnvelopeTask,
)
from src.infrastructure.ai.governance import (
    canonical_args_hash,
    tool_call_idempotency_key,
)
from src.infrastructure.ai.messaging import (
    AiRuntimeEventPublishError,
    PublishConfig,
    build_heartbeat,
    build_tool_call_requested,
    build_tool_result,
)
from src.infrastructure.ai.messaging.ai_runtime_event_publisher import (
    AiRuntimeEventPublisher,
)
from src.infrastructure.ai.messaging.command_dispatcher import ToolCommandWaiter
from src.infrastructure.ai.tools.mq_gate import (
    ToolAuthorizationTimeout,
    ToolMqGate,
    _summarize_arguments,
    make_tool_call_pk,
)
from src.infrastructure.ai.tools.tool_executor import (
    WRITE_ACTION_TOOLS,
    ToolExecutor,
)
from src.infrastructure.ai.tools.tool_registry_snapshot import ToolDefinition
from tests.sidecar.tool_executor_test_support import FakeReadOnlyBackend

# ---------------------------------------------------------------------------
# Fakes
# ---------------------------------------------------------------------------


class FakePublisher(AiRuntimeEventPublisher):
    """In-memory publisher that records every envelope it sees."""

    def __init__(self, fail_first_n: int = 0, fail_types: set[str] | None = None):
        self.published: list[dict[str, Any]] = []
        self._fail_first_n = fail_first_n
        self._fail_types = fail_types or set()
        config = PublishConfig(base_url="https://test.invalid", timeout=0.1)
        super().__init__(config, client=None)

    async def publish(self, event: dict[str, Any]) -> str:
        if self._fail_first_n > 0:
            self._fail_first_n -= 1
            raise AiRuntimeEventPublishError("AI_MQ_HTTP_503: simulated outage")
        if event.get("event_type") in self._fail_types:
            raise AiRuntimeEventPublishError("AI_MQ_HTTP_503: simulated outage")
        self.published.append(event)
        return f"msg-{len(self.published)}"


class FakePoller:
    """In-memory poller stub.

    The poller records every ``fetch_pending`` / ``mark_completed`` /
    ``mark_failed`` call. A pre-canned set of commands can be returned
    once and then drained.
    """

    def __init__(self, commands: list[dict[str, Any]] | None = None):
        self._queue: list[dict[str, Any]] = list(commands or [])
        self.fetch_calls: list[tuple[str | None, int | None]] = []
        self.completed: list[str] = []
        self.failed: list[tuple[str, str]] = []
        self.owner: str | None = None

    async def fetch_pending(self, owner: str | None = None, batch_size: int | None = None):
        self.fetch_calls.append((owner, batch_size))
        self.owner = owner
        if not self._queue:
            return []
        batch = self._queue[:batch_size] if batch_size else list(self._queue)
        self._queue = self._queue[batch_size:] if batch_size else []
        return batch

    async def mark_completed(self, command_id: str) -> None:
        self.completed.append(command_id)

    async def mark_failed(self, command_id: str, error: str) -> None:
        self.failed.append((command_id, error))

    def push(self, command: dict[str, Any]) -> None:
        self._queue.append(command)


def _tool_definition(
    name: str,
    *,
    source: str = "builtin",
    side_effect: bool = False,
    governance: dict[str, Any] | None = None,
) -> ToolDefinition:
    return ToolDefinition(
        name=name,
        display_name=name,
        description="d",
        parameters={},
        source=source,
        side_effect=side_effect,
        governance=governance,
    )


def _definition_lookup(tools: list[ToolDefinition]):
    by_name = {t.name: t for t in tools}
    return lambda name: by_name.get(name)


def _envelope(user_id: str = "user-1", roles: list[str] | None = None) -> ContextEnvelope:
    return ContextEnvelope(
        job_id="job-1",
        run_id="run-1",
        correlation_id="corr-1",
        requester=EnvelopeRequester(user_id=user_id, roles=roles or ["ai:chat"]),
        ontology=EnvelopeOntology(),
        context=EnvelopeContext(),
        task=EnvelopeTask(task_type="chat", user_message="hi"),
    )


def _decision_command(
    *,
    command_type: str = "tool_lease",
    run_id: str = "run-1",
    tool_call_pk: str | None = None,
    payload: dict[str, Any] | None = None,
) -> dict[str, Any]:
    return {
        "command_id": f"cmd-{command_type}",
        "run_id": run_id,
        "command_type": command_type,
        "command_sequence": 1,
        "tool_call_pk": tool_call_pk or make_tool_call_pk("run-1", "call-1"),
        "payload": payload or {},
        "status": "leased",
        "lease_owner": "python-sidecar",
    }


def _build_gate(
    *,
    tools: list[ToolDefinition] | None = None,
    commands: list[dict[str, Any]] | None = None,
    publisher: FakePublisher | None = None,
    poller: FakePoller | None = None,
    heartbeat_interval: float = 0.05,
    wait_poll_interval: float = 0.01,
    auth_margin: float = 0.1,
    run_owner: str = "python-sidecar",
) -> tuple[ToolMqGate, FakePublisher, FakePoller]:
    if publisher is None:
        publisher = FakePublisher()
    if poller is None:
        poller = FakePoller(commands=commands)
    lookup = _definition_lookup(tools or []) if tools is not None else None
    # Populate a ToolCommandWaiter with pre-canned commands so the
    # targeted wait path (the only path now) works in tests.
    command_waiter = None
    if poller._queue or commands:
        command_waiter = ToolCommandWaiter()
        for cmd in poller._queue or commands or []:
            tpk = cmd.get("tool_call_pk", "")
            if tpk:
                command_waiter.notify(tpk, cmd)
    gate = ToolMqGate(
        publisher=publisher,
        poller=poller,
        tool_definition_lookup=lookup,
        tool_type_resolver=lambda name: (
            "read_only"
            if name in {"flight_status_lookup", "search_flights_advanced"}
            else ("write_action" if name in WRITE_ACTION_TOOLS else "unknown")
        ),
        run_owner=run_owner,
        heartbeat_interval_seconds=heartbeat_interval,
        wait_poll_interval_seconds=wait_poll_interval,
        authorization_margin_seconds=auth_margin,
        command_waiter=command_waiter,
    )
    return gate, publisher, poller


# ---------------------------------------------------------------------------
# Gate unit tests
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_public_l0_tool_executes_locally_and_publishes_result_event() -> None:
    tools = [_tool_definition("flight_status_lookup")]
    publisher = FakePublisher()
    poller = FakePoller()
    gate, publisher, _poller = _build_gate(tools=tools, publisher=publisher, poller=poller)
    executor = ToolExecutor(mq_gate=gate, read_only_backend=FakeReadOnlyBackend())
    envelope = _envelope()
    tool_call = {
        "tool_call_id": "call-1",
        "tool_name": "flight_status_lookup",
        "arguments": {"flight_id": "CA1234"},
    }

    result = await executor.execute(
        tool_call,
        run_id="run-1",
        envelope=envelope,
        job_id="job-1",
        round_index=0,
    )

    assert result.success is True
    assert result.result["flight_id"] == "CA1234"

    event_types = [evt.get("event_type") for evt in publisher.published]
    assert event_types[0] == "tool_call_requested"
    assert event_types[-1] == "tool_result"
    last = publisher.published[-1]
    assert last["payload"]["status"] == "succeeded"
    assert last["payload"]["tool_call_id"] == "call-1"
    assert last["payload"]["tool_name"] == "flight_status_lookup"


@pytest.mark.asyncio
async def test_public_l0_tool_with_publish_failure_still_executes() -> None:
    tools = [_tool_definition("flight_status_lookup")]
    publisher = FakePublisher(fail_first_n=1)
    poller = FakePoller()
    gate, publisher, _poller = _build_gate(tools=tools, publisher=publisher, poller=poller)
    executor = ToolExecutor(mq_gate=gate, read_only_backend=FakeReadOnlyBackend())
    envelope = _envelope()
    tool_call = {
        "tool_call_id": "call-1",
        "tool_name": "flight_status_lookup",
        "arguments": {"flight_id": "CA1234"},
    }

    result = await executor.execute(
        tool_call,
        run_id="run-1",
        envelope=envelope,
        job_id="job-1",
    )

    assert result.success is True
    assert result.error is None
    event_types = [evt.get("event_type") for evt in publisher.published]
    assert "tool_call_requested" not in event_types
    assert "tool_result" in event_types


@pytest.mark.asyncio
async def test_protected_tool_publishes_requested_event_and_waits_for_lease() -> None:
    tools = [_tool_definition("add_flight_note", source="builtin", side_effect=True)]
    decision = _decision_command(command_type="tool_lease")
    poller = FakePoller(commands=[decision])
    publisher = FakePublisher()
    gate, publisher, poller = _build_gate(tools=tools, publisher=publisher, poller=poller)
    executor = ToolExecutor(mq_gate=gate)
    envelope = _envelope()
    tool_call = {
        "tool_call_id": "call-1",
        "tool_name": "add_flight_note",
        "arguments": {"flight_id": "CA1234", "note_content": "n"},
    }

    result = await executor.execute(
        tool_call,
        run_id="run-1",
        envelope=envelope,
        job_id="job-1",
    )

    assert result.success is True
    assert result.proposal is not None
    assert result.proposal["action_name"] == "add_note"
    event_types = [evt.get("event_type") for evt in publisher.published]
    assert "tool_call_requested" in event_types
    assert "tool_result" in event_types
    requested = publisher.published[0]
    assert requested["payload"]["authorization_mode"] == "rust_pdp"
    assert requested["payload"]["tool_call_id"] == "call-1"
    assert requested["payload"]["tool_name"] == "add_flight_note"
    assert requested["payload"]["args_hash"] == canonical_args_hash({"flight_id": "CA1234", "note_content": "n"})
    assert requested["payload"]["requester"]["user_id"] == "user-1"


@pytest.mark.asyncio
async def test_protected_tool_with_lease_executes_and_publishes_result() -> None:
    tools = [_tool_definition("add_flight_note", source="builtin", side_effect=True)]
    decision = _decision_command(command_type="tool_lease")
    poller = FakePoller(commands=[decision])
    publisher = FakePublisher()
    gate, publisher, poller = _build_gate(tools=tools, publisher=publisher, poller=poller)
    executor = ToolExecutor(mq_gate=gate)
    envelope = _envelope()
    tool_call = {
        "tool_call_id": "call-1",
        "tool_name": "add_flight_note",
        "arguments": {"flight_id": "CA1234", "note_content": "n"},
    }

    result = await executor.execute(
        tool_call,
        run_id="run-1",
        envelope=envelope,
        job_id="job-1",
    )

    assert result.success is True
    assert result.proposal is not None
    last = publisher.published[-1]
    assert last["payload"]["status"] == "succeeded"


@pytest.mark.asyncio
async def test_protected_tool_with_denied_command_raises_tool_denied() -> None:
    tools = [_tool_definition("add_flight_note", source="builtin", side_effect=True)]
    decision = _decision_command(
        command_type="tool_denied",
        payload={"code": "TOOL_ACTOR_PERMISSION_DENIED", "message": "lacks write permission"},
    )
    poller = FakePoller(commands=[decision])
    publisher = FakePublisher()
    gate, publisher, poller = _build_gate(tools=tools, publisher=publisher, poller=poller)
    executor = ToolExecutor(mq_gate=gate)
    envelope = _envelope()
    tool_call = {
        "tool_call_id": "call-1",
        "tool_name": "add_flight_note",
        "arguments": {"flight_id": "CA1234", "note_content": "n"},
    }

    result = await executor.execute(
        tool_call,
        run_id="run-1",
        envelope=envelope,
        job_id="job-1",
    )

    assert result.success is False
    assert "TOOL_DENIED" in result.error
    assert "TOOL_ACTOR_PERMISSION_DENIED" in result.error
    last = publisher.published[-1]
    assert last["payload"]["status"] == "denied"
    assert last["payload"]["error_code"] == "TOOL_ACTOR_PERMISSION_DENIED"


@pytest.mark.asyncio
async def test_protected_tool_with_proposal_only_command_skips_execution() -> None:
    """A protected read-only tool that Rust returns ``proposal_only`` for must be turned into a proposal."""
    tools = [
        _tool_definition(
            "search_flights_advanced",
            source="builtin",
            side_effect=False,
            governance={
                "preset": "read_only_query",
                "public": False,
                "required_account_permissions": ["ops:read"],
            },
        )
    ]
    decision = _decision_command(
        command_type="tool_proposal_only",
        tool_call_pk=make_tool_call_pk("run-1", "call-1"),
    )
    poller = FakePoller(commands=[decision])
    publisher = FakePublisher()
    gate, publisher, poller = _build_gate(tools=tools, publisher=publisher, poller=poller)
    executor = ToolExecutor(mq_gate=gate, read_only_backend=FakeReadOnlyBackend())
    envelope = _envelope()
    tool_call = {
        "tool_call_id": "call-1",
        "tool_name": "search_flights_advanced",
        "arguments": {"filters": {"airport": "PEK"}},
    }

    result = await executor.execute(
        tool_call,
        run_id="run-1",
        envelope=envelope,
        job_id="job-1",
    )

    assert result.success is True
    assert result.proposal is not None
    assert result.proposal["object_type"] == "ReadOnlyTool"
    last = publisher.published[-1]
    assert last["payload"]["status"] == "proposal_only"


@pytest.mark.asyncio
async def test_protected_tool_with_authorization_timeout_raises() -> None:
    tools = [_tool_definition("add_flight_note", source="builtin", side_effect=True)]
    poller = FakePoller(commands=[])
    publisher = FakePublisher()
    gate, _publisher, _poller = _build_gate(
        tools=tools,
        publisher=publisher,
        poller=poller,
        auth_margin=0.05,
        wait_poll_interval=0.01,
    )
    executor = ToolExecutor(mq_gate=gate)
    envelope = _envelope()
    tool_call = {
        "tool_call_id": "call-1",
        "tool_name": "add_flight_note",
        "arguments": {"flight_id": "CA1234", "note_content": "n"},
    }

    result = await executor.execute(
        tool_call,
        run_id="run-1",
        envelope=envelope,
        job_id="job-1",
    )

    assert result.success is False
    assert "TOOL_AUTHORIZATION_TIMEOUT" in result.error


@pytest.mark.asyncio
async def test_protected_tool_with_publish_failure_does_not_execute() -> None:
    tools = [_tool_definition("add_flight_note", source="builtin", side_effect=True)]
    publisher = FakePublisher(fail_types={"tool_call_requested"})
    poller = FakePoller()
    gate, _publisher, _poller = _build_gate(tools=tools, publisher=publisher, poller=poller)
    executor = ToolExecutor(mq_gate=gate)
    envelope = _envelope()
    tool_call = {
        "tool_call_id": "call-1",
        "tool_name": "add_flight_note",
        "arguments": {"flight_id": "CA1234", "note_content": "n"},
    }

    result = await executor.execute(
        tool_call,
        run_id="run-1",
        envelope=envelope,
        job_id="job-1",
    )

    assert result.success is False
    assert "TOOL_MQ_PUBLISH_FAILED" in result.error
    assert result.proposal is None


@pytest.mark.asyncio
async def test_protected_tool_with_missing_requester_raises_auth_context_required() -> None:
    tools = [_tool_definition("add_flight_note", source="builtin", side_effect=True)]
    poller = FakePoller(commands=[])
    publisher = FakePublisher()
    gate, _publisher, _poller = _build_gate(tools=tools, publisher=publisher, poller=poller)
    executor = ToolExecutor(mq_gate=gate)
    envelope = ContextEnvelope(
        job_id="job-1",
        run_id="run-1",
        correlation_id="corr-1",
        requester=EnvelopeRequester(user_id=""),
        ontology=EnvelopeOntology(),
        context=EnvelopeContext(),
        task=EnvelopeTask(task_type="chat", user_message="hi"),
    )
    tool_call = {
        "tool_call_id": "call-1",
        "tool_name": "add_flight_note",
        "arguments": {"flight_id": "CA1234", "note_content": "n"},
    }

    result = await executor.execute(
        tool_call,
        run_id="run-1",
        envelope=envelope,
        job_id="job-1",
    )

    assert result.success is False
    assert "TOOL_AUTH_CONTEXT_REQUIRED" in result.error


@pytest.mark.asyncio
async def test_long_running_tool_emits_heartbeat() -> None:
    """A tool that runs longer than ``timeout / 3`` should emit heartbeats."""
    slow_definition = ToolDefinition(
        name="slow_thing",
        display_name="slow_thing",
        description="d",
        parameters={},
        source="builtin",
        side_effect=True,
        governance={
            "preset": "read_only_query",
            "public": False,
            "required_account_permissions": ["ops:read"],
            "timeout_seconds": 60,
        },
    )
    poller = FakePoller()
    publisher = FakePublisher()
    gate, publisher, _poller = _build_gate(
        tools=[slow_definition],
        publisher=publisher,
        poller=poller,
        heartbeat_interval=0.05,
        wait_poll_interval=0.01,
    )

    from src.infrastructure.ai.tools.mq_gate import GateContext

    governance = gate._governance_resolver.resolve(slow_definition)
    context = GateContext(
        run_id="run-1",
        job_id="job-1",
        round_index=0,
        event_sequence=1,
        tool_call_pk=make_tool_call_pk("run-1", "call-slow"),
        tool_call_id="call-slow",
        tool_name="slow_thing",
        tool_type="read_only",
        arguments={"x": 1},
        args_summary={"x": 1},
        envelope=None,
        requester={"user_id": "user-1", "roles": [], "permissions": [], "department_id": None},
        entity_allowlist=[],
        object_decisions=[],
        governance=governance,
        idempotency_key="run-1:0:call-slow:slow_thing:abc",
        args_hash_value="abc",
    )

    task, stop_event = await gate.start_heartbeat(context=context)
    try:
        await asyncio.sleep(0.2)
    finally:
        await gate.stop_heartbeat(task, stop_event)

    heartbeats = [e for e in publisher.published if e.get("event_type") == "heartbeat"]
    assert heartbeats, "expected at least one heartbeat event during a long-running tool"
    heartbeat = heartbeats[0]
    assert heartbeat["payload"]["tool_call_pk"] == make_tool_call_pk("run-1", "call-slow")
    assert heartbeat["idempotency_key"].startswith("run-1:0:call-slow:slow_thing:")


@pytest.mark.asyncio
async def test_run_complete_emits_both_sse_and_mq_event() -> None:
    """The MQ ``run.complete`` event is published alongside the existing SSE event.

    This test exercises the helper rather than the runtime service
    end-to-end (which would require a full DI graph). The runtime
    service builds the same envelope via ``build_run_complete`` and
    publishes it after yielding the SSE ``run.complete`` event.
    """
    from src.infrastructure.ai.messaging import build_run_complete

    publisher = FakePublisher()
    envelope_dict = build_run_complete(
        run_id="run-1",
        job_id="job-1",
        round_index=3,
        event_sequence=42,
        idempotency_key="run-1:complete",
        output_raw={"answer": "all good"},
        proposal_ids=["p-1", "p-2"],
    )

    await publisher.publish(envelope_dict)

    assert publisher.published[0]["event_type"] == "run_complete"
    assert publisher.published[0]["payload"]["output_raw"] == {"answer": "all good"}
    assert publisher.published[0]["payload"]["proposal_ids"] == ["p-1", "p-2"]


# ---------------------------------------------------------------------------
# Idempotency key and payload contract
# ---------------------------------------------------------------------------


def test_idempotency_key_matches_rust_shape() -> None:
    args = {"flight_id": "CA1234"}
    key = tool_call_idempotency_key("run-42", 3, "call-99", "flight_status_lookup", args)
    expected_suffix = canonical_args_hash(args)
    assert key == f"run-42:3:call-99:flight_status_lookup:{expected_suffix}"


def test_idempotency_key_matches_locked_vector() -> None:
    locked_input = {
        "flight_id": "CA1234",
        "metadata": {"airport": "PEK", "gate": "B12"},
        "status": "ON_TIME",
        "tags": ["priority", "vip"],
    }
    key = tool_call_idempotency_key("run-1", 0, "call-1", "flight_status_lookup", locked_input)
    expected = "883af60772bce150610af6602572e27c45bd883acc01ee9bfc072901d6e972d3"
    assert key == f"run-1:0:call-1:flight_status_lookup:{expected}"


@pytest.mark.asyncio
async def test_payload_contract_includes_requester_entity_allowlist_governance_object_decisions() -> None:
    tools = [_tool_definition("add_flight_note", source="builtin", side_effect=True)]
    decision = _decision_command(command_type="tool_lease")
    poller = FakePoller(commands=[decision])
    publisher = FakePublisher()
    gate, publisher, _poller = _build_gate(
        tools=tools,
        publisher=publisher,
        poller=poller,
    )
    executor = ToolExecutor(mq_gate=gate)
    envelope = _envelope(roles=["ai:chat", "ops"])
    tool_call = {
        "tool_call_id": "call-1",
        "tool_name": "add_flight_note",
        "arguments": {"flight_id": "CA1234", "note_content": "n", "api_key": "sk-secret"},
    }

    await executor.execute(
        tool_call,
        run_id="run-1",
        envelope=envelope,
        job_id="job-1",
    )

    requested = next(e for e in publisher.published if e["event_type"] == "tool_call_requested")
    payload = requested["payload"]
    assert payload["requester"]["user_id"] == "user-1"
    assert payload["requester"]["roles"] == ["ai:chat", "ops"]
    assert payload["entity_allowlist"] == []
    assert payload["governance"]["tier"] == "L1_WORKSPACE_WRITE"
    assert payload["governance"]["authorization_mode"] == "rust_pdp"
    assert payload["object_decisions"] == []
    assert payload["args_summary"]["api_key"] == "<redacted>"


@pytest.mark.asyncio
async def test_publisher_uses_event_sequence_and_message_key() -> None:
    tools = [_tool_definition("flight_status_lookup")]
    publisher = FakePublisher()
    poller = FakePoller()
    gate, publisher, _poller = _build_gate(tools=tools, publisher=publisher, poller=poller)
    executor = ToolExecutor(mq_gate=gate)
    envelope = _envelope()
    tool_call = {
        "tool_call_id": "call-1",
        "tool_name": "flight_status_lookup",
        "arguments": {"flight_id": "CA1234"},
    }

    await executor.execute(tool_call, run_id="run-1", envelope=envelope, job_id="job-1")
    requested = publisher.published[0]
    assert requested["event_sequence"] == 1
    assert requested["run_id"] == "run-1"
    assert requested["job_id"] == "job-1"
    assert requested["round_index"] == 0


@pytest.mark.asyncio
async def test_per_run_event_sequence_increments_across_calls() -> None:
    tools = [_tool_definition("flight_status_lookup")]
    publisher = FakePublisher()
    poller = FakePoller()
    gate, publisher, _poller = _build_gate(tools=tools, publisher=publisher, poller=poller)
    executor = ToolExecutor(mq_gate=gate)
    envelope = _envelope()

    for index in range(3):
        await executor.execute(
            {
                "tool_call_id": f"call-{index}",
                "tool_name": "flight_status_lookup",
                "arguments": {"flight_id": "CA1234"},
            },
            run_id="run-1",
            envelope=envelope,
            job_id="job-1",
        )

    sequences = [evt["event_sequence"] for evt in publisher.published if evt["event_type"] == "tool_call_requested"]
    assert sequences == [1, 3, 5]
    result_sequences = [evt["event_sequence"] for evt in publisher.published if evt["event_type"] == "tool_result"]
    assert result_sequences == [2, 4, 6]


# ---------------------------------------------------------------------------
# Authorization and helper unit tests
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_request_authorization_publish_failure_for_protected_tool_raises_error() -> None:
    tools = [_tool_definition("add_flight_note", source="builtin", side_effect=True)]
    publisher = FakePublisher(fail_types={"tool_call_requested"})
    poller = FakePoller()
    gate, _publisher, _poller = _build_gate(tools=tools, publisher=publisher, poller=poller)

    with pytest.raises(Exception) as excinfo:
        await gate.request_authorization(
            tool_name="add_flight_note",
            tool_call_id="call-1",
            run_id="run-1",
            job_id="job-1",
            round_index=0,
            arguments={"flight_id": "CA1234", "note_content": "n"},
            envelope=_envelope(),
        )

    assert "TOOL_MQ_PUBLISH_FAILED" in str(excinfo.value)


@pytest.mark.asyncio
async def test_request_authorization_for_unknown_tool_is_protected_by_default() -> None:
    publisher = FakePublisher()
    poller = FakePoller(commands=[])
    gate, _publisher, _poller = _build_gate(publisher=publisher, poller=poller, auth_margin=0.05)

    with pytest.raises(ToolAuthorizationTimeout):
        await gate.request_authorization(
            tool_name="unknown_tool",
            tool_call_id="call-1",
            run_id="run-1",
            job_id="job-1",
            round_index=0,
            arguments={"x": 1},
            envelope=_envelope(),
        )


def test_summarize_arguments_redacts_secrets() -> None:
    args = {"api_key": "sk-1234", "nested": {"password": "p"}, "ok": "value"}
    summary = _summarize_arguments(args)
    assert summary["api_key"] == "<redacted>"
    assert summary["nested"]["password"] == "<redacted>"
    assert summary["ok"] == "value"


def test_summarize_arguments_truncates_long_strings() -> None:
    args = {"huge": "x" * 500}
    summary = _summarize_arguments(args)
    assert summary["huge"].endswith("...")
    assert len(summary["huge"]) <= 203


def test_make_tool_call_pk_is_deterministic() -> None:
    a = make_tool_call_pk("run-1", "call-1")
    b = make_tool_call_pk("run-1", "call-1")
    assert a == b
    assert len(a) == 32
    expected = hashlib.sha256(b"run-1:call-1").hexdigest()[:32]
    assert a == expected


# ---------------------------------------------------------------------------
# Public L0 local execution
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_public_l0_tool_executes_locally_without_gate() -> None:
    executor = ToolExecutor(read_only_backend=FakeReadOnlyBackend())
    tool_call = {
        "tool_call_id": "call-1",
        "tool_name": "flight_status_lookup",
        "arguments": {"flight_id": "CA1234"},
    }
    result = await executor.execute(tool_call, run_id="run-1")
    assert result.success is True
    assert result.result["flight_id"] == "CA1234"


@pytest.mark.asyncio
async def test_executor_batch_propagates_round_index_and_job_id() -> None:
    tools = [_tool_definition("flight_status_lookup")]
    publisher = FakePublisher()
    poller = FakePoller()
    gate, publisher, _poller = _build_gate(tools=tools, publisher=publisher, poller=poller)
    executor = ToolExecutor(mq_gate=gate)
    envelope = _envelope()
    tool_calls = [
        {
            "tool_call_id": "call-1",
            "tool_name": "flight_status_lookup",
            "arguments": {"flight_id": "CA1"},
        },
        {
            "tool_call_id": "call-2",
            "tool_name": "search_flights_advanced",
            "arguments": {"filters": {"airport": "PEK"}},
        },
    ]
    tool_definitions = [
        _tool_definition("search_flights_advanced"),
    ]
    gate, publisher, _poller = _build_gate(
        tools=tools + tool_definitions,
        publisher=publisher,
        poller=poller,
    )
    executor = ToolExecutor(mq_gate=gate, read_only_backend=FakeReadOnlyBackend())

    results = await executor.execute_batch(
        tool_calls,
        run_id="run-1",
        envelope=envelope,
        job_id="job-1",
        round_index=2,
    )

    assert all(r.success for r in results)
    assert all(evt["round_index"] == 2 for evt in publisher.published)
    assert all(evt["job_id"] == "job-1" for evt in publisher.published)


# ---------------------------------------------------------------------------
# Direct gate / envelope builder smoke checks
# ---------------------------------------------------------------------------


def test_build_tool_call_requested_envelope_shape_unchanged() -> None:
    envelope = build_tool_call_requested(
        run_id="run-1",
        job_id="job-1",
        round_index=0,
        event_sequence=1,
        idempotency_key="idem-1",
        tool_call_pk="tpc-1",
        tool_call_id="call-1",
        tool_name="t",
        tool_type="read_only",
        args_hash="abc",
        args_summary={"a": 1},
        authorization_mode="rust_pdp",
    )
    assert envelope["event_type"] == "tool_call_requested"
    assert envelope["idempotency_key"] == "idem-1"


def test_build_tool_result_envelope_shape_unchanged() -> None:
    envelope = build_tool_result(
        run_id="run-1",
        job_id="job-1",
        round_index=0,
        event_sequence=2,
        idempotency_key="idem-1",
        tool_call_pk="tpc-1",
        tool_call_id="call-1",
        tool_name="t",
        status="succeeded",
        duration_ms=100,
    )
    assert envelope["payload"]["status"] == "succeeded"


def test_build_heartbeat_envelope_shape_unchanged() -> None:
    envelope = build_heartbeat(
        run_id="run-1",
        job_id="job-1",
        round_index=0,
        event_sequence=3,
        idempotency_key="idem-1",
        tool_call_pk="tpc-1",
    )
    assert envelope["event_type"] == "heartbeat"
