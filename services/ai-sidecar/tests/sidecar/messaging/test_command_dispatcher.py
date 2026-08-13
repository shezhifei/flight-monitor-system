"""Tests for the command dispatcher."""

from __future__ import annotations

import asyncio
from typing import Any

import pytest

from src.infrastructure.ai.context_envelope import (
    ContextEnvelope,
)
from src.infrastructure.ai.messaging.command_dispatcher import (
    CommandDispatcher,
    ToolCommandWaiter,
)


class _FakePoller:
    def __init__(self) -> None:
        self.mark_leased_calls: list[tuple[str, int | None]] = []
        self.mark_failed_calls: list[tuple[str, str]] = []

    async def mark_leased(self, command_id: str, *, ttl_seconds: int | None = None) -> None:
        self.mark_leased_calls.append((command_id, ttl_seconds))

    async def mark_failed(self, command_id: str, error: str) -> None:
        self.mark_failed_calls.append((command_id, error))


class _RecordingGate:
    def __init__(self) -> None:
        self.notifications: list[dict[str, Any]] = []

    def notify_command(self, command: dict[str, Any]) -> None:
        self.notifications.append(command)


def _envelope_data() -> dict[str, Any]:
    return {
        "job_id": "job-1",
        "run_id": "run-1",
        "correlation_id": "corr-1",
        "requester": {"user_id": "u1"},
        "ontology": {},
        "context": {},
        "task": {"task_type": "chat", "user_message": "hi"},
    }


@pytest.mark.asyncio
async def test_dispatch_routes_start_run_to_run_starter() -> None:
    started: list[ContextEnvelope] = []

    async def run_starter(envelope: ContextEnvelope) -> None:
        started.append(envelope)

    dispatcher = CommandDispatcher(
        worker_id="worker-1",
        poller=_FakePoller(),
        run_starter=run_starter,
        heartbeat_interval_seconds=0.01,
    )

    await dispatcher.dispatch(
        {
            "command_id": "c-1",
            "command_type": "start_run",
            "run_id": "run-1",
            "payload": {"envelope": _envelope_data()},
        }
    )

    assert len(started) == 1
    assert started[0].run_id == "run-1"


@pytest.mark.asyncio
async def test_dispatch_start_run_without_envelope_fails() -> None:
    dispatcher = CommandDispatcher(
        worker_id="worker-1",
        poller=_FakePoller(),
        run_starter=lambda env: None,
        heartbeat_interval_seconds=0.01,
    )

    with pytest.raises(ValueError, match="envelope"):
        await dispatcher.dispatch(
            {
                "command_id": "c-1",
                "command_type": "start_run",
                "run_id": "run-1",
                "payload": {},
            }
        )


@pytest.mark.asyncio
async def test_dispatch_start_run_without_handler_fails() -> None:
    dispatcher = CommandDispatcher(
        worker_id="worker-1",
        poller=_FakePoller(),
        heartbeat_interval_seconds=0.01,
    )

    with pytest.raises(RuntimeError, match="start_run handler not configured"):
        await dispatcher.dispatch(
            {
                "command_id": "c-1",
                "command_type": "start_run",
                "run_id": "run-1",
                "payload": {"envelope": _envelope_data()},
            }
        )


@pytest.mark.asyncio
async def test_dispatch_cancel_run_cancels_running_task() -> None:
    started = asyncio.Event()
    run_block = asyncio.Event()

    async def run_starter(envelope: ContextEnvelope) -> None:
        started.set()
        await run_block.wait()

    dispatcher = CommandDispatcher(
        worker_id="worker-1",
        poller=_FakePoller(),
        run_starter=run_starter,
        heartbeat_interval_seconds=0.01,
    )

    start_cmd = {
        "command_id": "c-1",
        "command_type": "start_run",
        "run_id": "run-1",
        "payload": {"envelope": _envelope_data()},
    }
    await dispatcher.dispatch(start_cmd)

    await asyncio.wait_for(started.wait(), timeout=1.0)
    assert "run-1" in dispatcher.running_runs

    cancel_cmd = {
        "command_id": "c-2",
        "command_type": "cancel_run",
        "run_id": "run-1",
        "payload": {},
    }
    await dispatcher.dispatch(cancel_cmd)

    assert "run-1" not in dispatcher.running_runs


@pytest.mark.asyncio
async def test_dispatch_tool_lease_wakes_gate_and_waiter() -> None:
    waiter = ToolCommandWaiter()
    gate = _RecordingGate()
    dispatcher = CommandDispatcher(
        worker_id="worker-1",
        poller=_FakePoller(),
        gate=gate,
        tool_command_waiter=waiter,
        heartbeat_interval_seconds=0.01,
    )

    received: dict[str, Any] | None = None

    async def _wait() -> None:
        nonlocal received
        received = await waiter.wait("tpk-1", timeout=1.0)

    wait_task = asyncio.create_task(_wait())
    await asyncio.sleep(0.05)

    await dispatcher.dispatch(
        {
            "command_id": "c-1",
            "command_type": "tool_lease",
            "run_id": "run-1",
            "tool_call_pk": "tpk-1",
            "payload": {"code": "ok"},
        }
    )

    await asyncio.wait_for(wait_task, timeout=1.0)
    assert received is not None
    assert received["tool_call_pk"] == "tpk-1"
    assert len(gate.notifications) == 1


@pytest.mark.asyncio
async def test_dispatch_unknown_command_type_logs_warning(caplog: Any) -> None:
    import logging

    dispatcher = CommandDispatcher(
        worker_id="worker-1",
        poller=_FakePoller(),
        heartbeat_interval_seconds=0.01,
    )

    with caplog.at_level(logging.WARNING):
        await dispatcher.dispatch(
            {
                "command_id": "c-1",
                "command_type": "weird_command",
                "run_id": "run-1",
                "payload": {},
            }
        )

    assert "command_dispatcher_unknown_type" in caplog.text


@pytest.mark.asyncio
async def test_dispatch_heartbeat_refreshes_command_lease() -> None:
    poller = _FakePoller()
    dispatcher = CommandDispatcher(
        worker_id="worker-1",
        poller=poller,
        run_starter=lambda env: None,
        heartbeat_interval_seconds=0.01,
    )

    await dispatcher.dispatch(
        {
            "command_id": "c-1",
            "command_type": "start_run",
            "run_id": "run-1",
            "payload": {"envelope": _envelope_data()},
        }
    )

    assert any(call[0] == "c-1" for call in poller.mark_leased_calls)


@pytest.mark.asyncio
async def test_cancel_all_runs_stops_every_running_task() -> None:
    blocks = {"run-1": asyncio.Event(), "run-2": asyncio.Event()}
    started = {rid: asyncio.Event() for rid in blocks}

    async def run_starter(envelope: ContextEnvelope) -> None:
        started[envelope.run_id].set()
        await blocks[envelope.run_id].wait()

    dispatcher = CommandDispatcher(
        worker_id="worker-1",
        poller=_FakePoller(),
        run_starter=run_starter,
        heartbeat_interval_seconds=0.01,
    )

    for rid in blocks:
        await dispatcher.dispatch(
            {
                "command_id": f"c-{rid}",
                "command_type": "start_run",
                "run_id": rid,
                "payload": {"envelope": {**_envelope_data(), "run_id": rid}},
            }
        )

    for evt in started.values():
        await asyncio.wait_for(evt.wait(), timeout=1.0)

    assert len(dispatcher.running_runs) == 2
    await dispatcher.cancel_all_runs()
    assert len(dispatcher.running_runs) == 0


@pytest.mark.asyncio
async def test_dispatch_retry_tool_calls_handler_and_enforces_max_attempts() -> None:
    retries: list[dict[str, Any]] = []

    async def retry_handler(command: dict[str, Any]) -> None:
        retries.append(command)

    dispatcher = CommandDispatcher(
        worker_id="worker-1",
        poller=_FakePoller(),
        tool_retry_handler=retry_handler,
        heartbeat_interval_seconds=0.01,
    )

    await dispatcher.dispatch(
        {
            "command_id": "c-1",
            "command_type": "retry_tool",
            "run_id": "run-1",
            "payload": {"attempt_count": 1, "max_attempts": 3},
        }
    )
    assert len(retries) == 1

    with pytest.raises(RuntimeError, match="max_attempts"):
        await dispatcher.dispatch(
            {
                "command_id": "c-2",
                "command_type": "retry_tool",
                "run_id": "run-1",
                "payload": {"attempt_count": 3, "max_attempts": 3},
            }
        )


@pytest.mark.asyncio
async def test_dispatch_resume_run_requires_checkpoint_id() -> None:
    dispatcher = CommandDispatcher(
        worker_id="worker-1",
        poller=_FakePoller(),
        run_resume_handler=lambda cmd: None,
        heartbeat_interval_seconds=0.01,
    )

    with pytest.raises(ValueError, match="checkpoint_id"):
        await dispatcher.dispatch(
            {
                "command_id": "c-1",
                "command_type": "resume_run",
                "run_id": "run-1",
                "payload": {},
            }
        )


@pytest.mark.asyncio
async def test_tool_command_waiter_timeout_returns_none() -> None:
    waiter = ToolCommandWaiter()
    result = await waiter.wait("tpk-missing", timeout=0.05)
    assert result is None


@pytest.mark.asyncio
async def test_tool_command_waiter_overwrites_stale_waiter() -> None:
    waiter = ToolCommandWaiter()
    first_cancelled = asyncio.Event()

    async def _wait() -> None:
        try:
            return await waiter.wait("tpk-1", timeout=1.0)
        except asyncio.CancelledError:
            first_cancelled.set()
            raise

    task1 = asyncio.create_task(_wait())
    await asyncio.sleep(0.05)
    task2 = asyncio.create_task(waiter.wait("tpk-1", timeout=1.0))
    await asyncio.sleep(0.05)

    await asyncio.wait_for(first_cancelled.wait(), timeout=1.0)
    waiter.notify("tpk-1", {"code": "ok"})

    result = await asyncio.wait_for(task2, timeout=1.0)
    assert result == {"code": "ok"}
    assert task1.cancelled()
