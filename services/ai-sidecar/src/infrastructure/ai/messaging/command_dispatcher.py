"""Command dispatcher for the AI runtime command consumer.

Handles start_run, cancel_run, tool_lease, tool_denied,
tool_proposal_only, retry_tool and resume_run.
"""

from __future__ import annotations

import asyncio
import contextlib
import inspect
from collections.abc import Awaitable, Callable
from typing import Any

from src.infrastructure.ai.context_envelope import ContextEnvelope
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


DEFAULT_HEARTBEAT_INTERVAL_SECONDS: float = 10.0
DEFAULT_LEASE_TTL_SECONDS: int = 30


class ToolCommandWaiter:
    """In-memory waiter bus for tool authorization commands.

    The :class:`ToolMqGate` registers a future for a protected tool call.
    When the Rust authorization decision arrives via
    ``ai_runtime_commands``, the :class:`CommandDispatcher` resolves the
    future so the gate can proceed without waiting for its next DB poll.
    """

    def __init__(self) -> None:
        self._waiters: dict[str, asyncio.Future[dict[str, Any]]] = {}
        self._lock = asyncio.Lock()
        self._notify_tasks: set[asyncio.Task[None]] = set()

    async def wait(self, tool_call_pk: str, timeout: float) -> dict[str, Any] | None:
        async with self._lock:
            existing = self._waiters.get(tool_call_pk)
            if existing is not None and not existing.done():
                existing.cancel()
            future: asyncio.Future[dict[str, Any]] = asyncio.get_event_loop().create_future()
            self._waiters[tool_call_pk] = future
        try:
            return await asyncio.wait_for(future, timeout=timeout)
        except TimeoutError:
            return None
        finally:
            async with self._lock:
                if self._waiters.get(tool_call_pk) is future:
                    self._waiters.pop(tool_call_pk, None)

    def notify(self, tool_call_pk: str, command: dict[str, Any]) -> bool:
        future: asyncio.Future[dict[str, Any]] | None = None
        try:
            loop = asyncio.get_event_loop()
        except RuntimeError:
            return False

        async def _set() -> None:
            nonlocal future
            async with self._lock:
                future = self._waiters.get(tool_call_pk)
                if future is not None and not future.done():
                    future.set_result(command)

        if loop.is_running():
            task = asyncio.create_task(_set())
            self._notify_tasks.add(task)
            task.add_done_callback(self._notify_tasks.discard)
        else:
            loop.run_until_complete(_set())
        return future is not None and not future.done()


RunStarter = Callable[[ContextEnvelope], Awaitable[None]]
ToolRetryHandler = Callable[[dict[str, Any]], Awaitable[None]]
RunResumeHandler = Callable[[dict[str, Any]], Awaitable[None]]


class CommandDispatcher:
    """Dispatches ``ai_runtime_commands`` rows to the correct handler."""

    def __init__(
        self,
        *,
        worker_id: str,
        poller: Any,
        gate: Any | None = None,
        tool_command_waiter: ToolCommandWaiter | None = None,
        run_starter: RunStarter | None = None,
        tool_retry_handler: ToolRetryHandler | None = None,
        run_resume_handler: RunResumeHandler | None = None,
        heartbeat_interval_seconds: float = DEFAULT_HEARTBEAT_INTERVAL_SECONDS,
        lease_ttl_seconds: int = DEFAULT_LEASE_TTL_SECONDS,
    ) -> None:
        self._worker_id = worker_id
        self._poller = poller
        self._gate = gate
        self._tool_command_waiter = tool_command_waiter
        self._run_starter = run_starter
        self._tool_retry_handler = tool_retry_handler
        self._run_resume_handler = run_resume_handler
        self._heartbeat_interval_seconds = max(0.1, float(heartbeat_interval_seconds))
        self._lease_ttl_seconds = max(1, int(lease_ttl_seconds))
        self._cancelling_runs: set[str] = set()
        self._running_runs: dict[str, asyncio.Task[None]] = {}
        self._lock = asyncio.Lock()
        self._gate_notify_tasks: set[asyncio.Task[Any]] = set()

    @property
    def cancelling_runs(self) -> set[str]:
        return set(self._cancelling_runs)

    @property
    def running_runs(self) -> set[str]:
        return set(self._running_runs.keys())

    async def dispatch(self, command: dict[str, Any]) -> None:
        command_id = command.get("command_id", "")
        command_type = command.get("command_type", "")
        run_id = command.get("run_id", "")
        extra = {"run_id": run_id} if run_id else {}

        heartbeat_task = asyncio.create_task(
            self._heartbeat_loop(command_id),
            name=f"cmd-heartbeat-{command_id}",
        )
        await self._mark_leased(command_id)

        try:
            if command_type == "start_run":
                await self._handle_start_run(command)
            elif command_type == "cancel_run":
                await self._handle_cancel_run(command)
            elif command_type == "tool_lease":
                await self._handle_tool_lease(command)
            elif command_type == "tool_denied":
                await self._handle_tool_denied(command)
            elif command_type == "tool_proposal_only":
                await self._handle_tool_proposal_only(command)
            elif command_type == "retry_tool":
                await self._handle_retry_tool(command)
            elif command_type == "resume_run":
                await self._handle_resume_run(command)
            else:
                logger.warning(
                    "command_dispatcher_unknown_type",
                    extra={"command_type": command_type, **extra},
                )
        finally:
            heartbeat_task.cancel()
            with contextlib.suppress(asyncio.CancelledError, Exception):
                await heartbeat_task

    async def _heartbeat_loop(self, command_id: str) -> None:
        try:
            while True:
                await asyncio.sleep(self._heartbeat_interval_seconds)
                await self._mark_leased(command_id)
        except asyncio.CancelledError:
            return

    async def _mark_leased(self, command_id: str) -> None:
        if command_id is None or command_id == "":
            return
        mark_leased = getattr(self._poller, "mark_leased", None)
        if mark_leased is None:
            return
        try:
            await mark_leased(command_id, ttl_seconds=self._lease_ttl_seconds)
        except Exception as exc:  # noqa: BLE001 - heartbeat failures are non-fatal
            logger.warning("command_dispatcher_heartbeat_failed", extra={"command_id": command_id}, exc_info=exc)

    async def _mark_failed(self, command_id: str, error: str) -> None:
        try:
            await self._poller.mark_failed(command_id, error)
        except Exception as exc:
            logger.error("command_dispatcher_mark_failed_error", extra={"command_id": command_id}, exc_info=exc)

    async def _handle_start_run(self, command: dict[str, Any]) -> None:
        payload = command.get("payload") or {}
        envelope_data = payload.get("envelope")
        if envelope_data is None:
            raise ValueError("start_run payload missing envelope")

        envelope = self._build_envelope(envelope_data)
        run_id = envelope.run_id or command.get("run_id", "")

        async with self._lock:
            self._cancelling_runs.discard(run_id)

        if self._run_starter is None:
            raise RuntimeError("start_run handler not configured")

        async def _run() -> None:
            try:
                await self._run_starter(envelope)
            finally:
                async with self._lock:
                    self._running_runs.pop(run_id, None)
                    self._cancelling_runs.discard(run_id)

        task = asyncio.create_task(_run(), name=f"run-{run_id}")
        async with self._lock:
            self._running_runs[run_id] = task

    async def _handle_cancel_run(self, command: dict[str, Any]) -> None:
        run_id = command.get("run_id", "")
        async with self._lock:
            self._cancelling_runs.add(run_id)
            task = self._running_runs.get(run_id)

        if task is not None and not task.done():
            task.cancel()
            with contextlib.suppress(asyncio.CancelledError, Exception):
                await task

    async def _handle_tool_lease(self, command: dict[str, Any]) -> None:
        self._wake_gate(command)

    async def _handle_tool_denied(self, command: dict[str, Any]) -> None:
        self._wake_gate(command)

    async def _handle_tool_proposal_only(self, command: dict[str, Any]) -> None:
        self._wake_gate(command)

    def _wake_gate(self, command: dict[str, Any]) -> None:
        tool_call_pk = command.get("tool_call_pk")
        if not tool_call_pk:
            return
        if self._tool_command_waiter is not None:
            self._tool_command_waiter.notify(tool_call_pk, command)
        notify = getattr(self._gate, "notify_command", None)
        if notify is not None:
            try:
                if inspect.iscoroutinefunction(notify):
                    task = asyncio.create_task(notify(command))
                    self._gate_notify_tasks.add(task)
                    task.add_done_callback(self._gate_notify_tasks.discard)
                else:
                    notify(command)
            except Exception as exc:  # noqa: BLE001 - gate notify failures are non-fatal
                logger.warning("command_dispatcher_gate_notify_failed", exc_info=exc)

    async def _handle_retry_tool(self, command: dict[str, Any]) -> None:
        payload = command.get("payload") or {}
        attempt_count = int(payload.get("attempt_count", 0))
        max_attempts = int(payload.get("max_attempts", 0))
        if attempt_count >= max_attempts:
            raise RuntimeError(f"retry_tool exceeded max_attempts ({max_attempts})")
        if self._tool_retry_handler is None:
            raise RuntimeError("retry_tool handler not configured")
        await self._tool_retry_handler(command)

    async def _handle_resume_run(self, command: dict[str, Any]) -> None:
        payload = command.get("payload") or {}
        checkpoint_id = payload.get("checkpoint_id")
        if checkpoint_id is None:
            raise ValueError("resume_run payload missing checkpoint_id")
        if self._run_resume_handler is None:
            raise RuntimeError("resume_run handler not configured")
        await self._run_resume_handler(command)

    def _build_envelope(self, data: dict[str, Any]) -> ContextEnvelope:
        if isinstance(data, ContextEnvelope):
            return data
        return ContextEnvelope(**data)

    async def cancel_all_runs(self) -> None:
        tasks: list[asyncio.Task[None]] = []
        async with self._lock:
            for run_id in list(self._running_runs.keys()):
                self._cancelling_runs.add(run_id)
                task = self._running_runs.get(run_id)
                if task is not None and not task.done():
                    task.cancel()
                    tasks.append(task)
        for task in tasks:
            with contextlib.suppress(asyncio.CancelledError, Exception):
                await task


__all__ = [
    "CommandDispatcher",
    "ToolCommandWaiter",
]
