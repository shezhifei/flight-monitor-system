"""Composition root for the AI runtime MQ control plane.

Wires :class:`ToolMqGate` to :class:`AiRuntimeEventPublisher` and
:class:`AiCommandPoller`, plus :class:`CommandDispatcher` and
:class:`RunOwnerRegistry`, so the sidecar can consume ``start_run`` /
``cancel_run`` / ``retry_tool`` / ``resume_run`` commands and route
``tool_lease`` / ``tool_denied`` / ``tool_proposal_only`` to waiting
protected tool calls.

The module is the **single composition root** for the MQ control plane:
nothing else constructs :class:`ToolMqGate`,
:class:`AiRuntimeEventPublisher`, :class:`AiCommandPoller`,
:class:`CommandDispatcher` or :class:`RunOwnerRegistry` directly in
production. Tests and Wave-2-only runtimes may still construct them by
hand.

Design notes
------------

* **Explicit wiring, no import-time side effects.** The factories in
  this module are the only places where the publisher, poller, gate,
  dispatcher and owner registry are constructed. Callers (the FastAPI
  lifespan handler or the standalone worker entrypoint) decide when the
  consumer run loop starts and stops.
* **Degrade-closed.** When configuration or connectivity is missing
  the bootstrap returns a components object with ``is_wired=False`` and
  the sidecar continues without MQ.
* **One publisher, one poller, one gate, one dispatcher, one owner
  registry per process.** The bootstrap caches the constructed objects
  in module-level state so a second call returns the same instances.
  Resetting requires an explicit :func:`reset_mq_runtime_components`
  call (used in tests).
* **Consumer loop is a background task.** The poller's ``run()`` loop
  claims run ownership, dispatches commands and marks them completed or
  failed. On shutdown it cancels all running runs owned by this worker.
"""

from __future__ import annotations

import asyncio
import contextlib
import os
from collections.abc import Awaitable, Callable
from dataclasses import dataclass, field
from typing import Any

from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


DEFAULT_HEARTBEAT_INTERVAL_SECONDS: float = 10.0
DEFAULT_POLL_INTERVAL_SECONDS: float = 0.2
DEFAULT_LEASE_TTL_SECONDS: int = 30
DEFAULT_FETCH_BATCH_SIZE: int = 10
DEFAULT_PUBLISH_TIMEOUT_SECONDS: float = 5.0
DEFAULT_PUBLISH_MAX_RETRIES: int = 3
DEFAULT_PUBLISH_BACKOFF_SECONDS: float = 0.25


CommandHandler = Callable[[dict[str, Any]], Awaitable[None]]


def _resolve_mq_gateway_base_url() -> str | None:
    """Return the mq-gateway base URL from env, or ``None`` when not set."""
    for key in ("AI_MQ_GATEWAY_URL", "MQ_GATEWAY_URL", "AI_MQ_GATEWAY_BASE_URL"):
        value = os.environ.get(key, "").strip()
        if value:
            return value.rstrip("/")
    return None


def _resolve_mq_gateway_api_key() -> str | None:
    value = os.environ.get("AI_MQ_GATEWAY_API_KEY", "").strip()
    return value or None


def _resolve_worker_id() -> str:
    return os.environ.get("WORKER_ID", "").strip()


def _resolve_mq_poller_owner() -> str:
    return os.environ.get("AI_MQ_POLLER_OWNER", "python-sidecar").strip() or "python-sidecar"


def _resolve_heartbeat_interval() -> float:
    raw = os.environ.get("AI_MQ_HEARTBEAT_INTERVAL_SECONDS", "").strip()
    if not raw:
        return DEFAULT_HEARTBEAT_INTERVAL_SECONDS
    try:
        return max(0.1, float(raw))
    except ValueError:
        return DEFAULT_HEARTBEAT_INTERVAL_SECONDS


async def _default_start_run(envelope: Any) -> None:
    """Consume a streaming run to completion, discarding SSE events."""
    from src.infrastructure.ai.context_envelope import ContextEnvelope
    from src.infrastructure.ai.runtime_service import get_runtime_service

    if not isinstance(envelope, ContextEnvelope):
        envelope = ContextEnvelope(**envelope)
    service = get_runtime_service()
    async for _event in service.stream_run_with_tools(envelope):
        pass


@dataclass
class MqRuntimeComponents:
    """Bundle of MQ control-plane singletons owned by the composition root.

    Attributes:
        publisher: The :class:`AiRuntimeEventPublisher` (or ``None`` when
            the mq-gateway URL is not configured).
        poller: The :class:`AiCommandPoller` (or ``None`` when no
            database pool is available).
        gate: The :class:`ToolMqGate` wired to ``publisher`` and
            ``poller`` (or ``None`` when either is missing).
        dispatcher: The :class:`CommandDispatcher` (or ``None``).
        owner_registry: The :class:`RunOwnerRegistry` (or ``None``).
        run_loop_task: The background asyncio task running
            :meth:`AiCommandPoller.run` (only set after
            :meth:`start_poller_loop`).
    """

    publisher: Any | None
    poller: Any | None
    gate: Any | None
    dispatcher: Any | None = None
    owner_registry: Any | None = None
    run_loop_task: asyncio.Task[None] | None = None

    @property
    def is_wired(self) -> bool:
        return self.publisher is not None and self.poller is not None and self.gate is not None

    def start_poller_loop(self) -> asyncio.Task[None] | None:
        """Start :meth:`AiCommandPoller.run` as a background task.

        Returns the spawned task, or ``None`` if the poller is missing
        or the loop is already running.
        """
        if self.poller is None:
            return None
        if self.run_loop_task is not None and not self.run_loop_task.done():
            return self.run_loop_task
        loop = asyncio.get_event_loop()
        task = loop.create_task(
            self.poller.run(
                dispatcher=self.dispatcher,
                owner_registry=self.owner_registry,
            ),
            name="mq-command-consumer",
        )
        self.run_loop_task = task
        return task

    async def stop_poller_loop(self) -> None:
        """Request shutdown on the poller and await the background task."""
        if self.poller is not None:
            try:
                self.poller.request_shutdown()
            except Exception as exc:  # noqa: BLE001
                logger.warning("mq_poller_shutdown_request_failed: %s", exc)
        task = self.run_loop_task
        if task is None:
            return
        self.run_loop_task = None
        try:
            await asyncio.wait_for(task, timeout=5.0)
        except TimeoutError:
            task.cancel()
            with contextlib.suppress(asyncio.CancelledError, Exception):
                await task
        with contextlib.suppress(asyncio.CancelledError, Exception):
            pass

    async def aclose(self) -> None:
        """Tear down the publisher client. Poller pool is owned externally."""
        await self.stop_poller_loop()
        if self.publisher is not None:
            close = getattr(self.publisher, "aclose", None)
            if callable(close):
                try:
                    await close()
                except Exception as exc:  # noqa: BLE001
                    logger.warning("mq_publisher_close_failed: %s", exc)


_components: MqRuntimeComponents | None = None
_components_lock = asyncio.Lock()


@dataclass
class _HandlerConfig:
    """Optional overrides for command handlers."""

    run_starter: Callable[[Any], Awaitable[None]] | None = None
    tool_retry_handler: CommandHandler | None = None
    run_resume_handler: CommandHandler | None = None


_build_overrides: _HandlerConfig = field(default_factory=_HandlerConfig)  # type: ignore[var-annotated]


def set_mq_build_overrides(
    *,
    run_starter: Callable[[Any], Awaitable[None]] | None = None,
    tool_retry_handler: CommandHandler | None = None,
    run_resume_handler: CommandHandler | None = None,
) -> None:
    """Install handler overrides for :func:`build_mq_runtime_components`.

    Used by tests to inject fake run starters without touching the
    production wiring.
    """
    global _build_overrides
    _build_overrides = _HandlerConfig(
        run_starter=run_starter,
        tool_retry_handler=tool_retry_handler,
        run_resume_handler=run_resume_handler,
    )


def reset_mq_build_overrides() -> None:
    """Clear handler overrides."""
    global _build_overrides
    _build_overrides = _HandlerConfig()


async def build_mq_runtime_components(
    *,
    db_pool: Any | None = None,
) -> MqRuntimeComponents:
    """Construct the publisher, poller, gate, dispatcher and owner registry.

    Args:
        db_pool: Optional asyncpg pool used to build the
            :class:`AiCommandPoller` and :class:`RunOwnerRegistry`. When
            ``None`` the function tries to resolve a pool from the AI
            container's ``pg_shared_context_pool`` registration. When
            neither is available the returned components object has
            ``poller=None`` and ``gate=None``.

    Returns:
        An :class:`MqRuntimeComponents` bundle. ``is_wired`` is
        ``True`` only when publisher, poller, and gate were all
        constructed.
    """
    from src.infrastructure.ai.governance import ToolGovernanceResolver
    from src.infrastructure.ai.messaging import (
        AiCommandPoller,
        AiRuntimeEventPublisher,
        PublishConfig,
    )
    from src.infrastructure.ai.messaging.command_dispatcher import (
        CommandDispatcher,
        ToolCommandWaiter,
    )
    from src.infrastructure.ai.messaging.run_owner import RunOwnerRegistry
    from src.infrastructure.ai.messaging.worker_identity import WorkerIdentity
    from src.infrastructure.ai.tools.mq_gate import ToolMqGate

    worker_id = WorkerIdentity(_resolve_worker_id()).worker_id

    base_url = _resolve_mq_gateway_base_url()
    publisher: AiRuntimeEventPublisher | None = None
    if base_url:
        try:
            publisher = AiRuntimeEventPublisher(
                PublishConfig(
                    base_url=base_url,
                    api_key=_resolve_mq_gateway_api_key(),
                    timeout=DEFAULT_PUBLISH_TIMEOUT_SECONDS,
                    max_retries=DEFAULT_PUBLISH_MAX_RETRIES,
                    backoff_seconds=DEFAULT_PUBLISH_BACKOFF_SECONDS,
                )
            )
        except Exception as exc:  # noqa: BLE001 - composition root must not crash DI
            logger.warning("mq_publisher_construct_failed: %s", exc)
            publisher = None
    else:
        logger.warning(
            "mq_runtime_DEGRADED_no_gateway_url: AI_MQ_GATEWAY_URL not set; protected tools will be FAIL-CLOSED."
        )

    resolved_pool = db_pool
    if resolved_pool is None:
        try:
            from src.infrastructure.ai.ai_container import get_ai_container

            container = get_ai_container()
            resolved_pool = container.resolve("pg_shared_context_pool", None)
            if resolved_pool is None:
                resolved_pool = container.resolve("db_pool", None)
        except Exception as exc:  # noqa: BLE001
            logger.debug("mq_pool_resolution_failed: %s", exc)

    poller: AiCommandPoller | None = None
    owner_registry: RunOwnerRegistry | None = None
    if resolved_pool is not None:
        try:
            poller = AiCommandPoller(
                pool=resolved_pool,
                owner=worker_id,
                interval_seconds=DEFAULT_POLL_INTERVAL_SECONDS,
                batch_size=DEFAULT_FETCH_BATCH_SIZE,
                lease_ttl_seconds=DEFAULT_LEASE_TTL_SECONDS,
            )
            owner_registry = RunOwnerRegistry(
                worker_id=worker_id,
                pool=resolved_pool,
                lease_ttl_seconds=DEFAULT_LEASE_TTL_SECONDS,
            )
        except Exception as exc:  # noqa: BLE001
            logger.warning("mq_poller_construct_failed: %s", exc)
            poller = None
            owner_registry = None
    else:
        logger.warning(
            "mq_runtime_DEGRADED_no_db_pool: Postgres pool not available; protected tools will be FAIL-CLOSED."
        )

    dispatcher: CommandDispatcher | None = None
    gate: ToolMqGate | None = None
    if publisher is not None and poller is not None and owner_registry is not None:
        try:
            tool_command_waiter = ToolCommandWaiter()
            gate = ToolMqGate(
                publisher=publisher,
                poller=poller,
                governance_resolver=ToolGovernanceResolver(),
                heartbeat_interval_seconds=_resolve_heartbeat_interval(),
                run_owner=worker_id,
                command_waiter=tool_command_waiter,
            )
            dispatcher = CommandDispatcher(
                worker_id=worker_id,
                poller=poller,
                gate=gate,
                tool_command_waiter=tool_command_waiter,
                run_starter=_build_overrides.run_starter or _default_start_run,
                tool_retry_handler=_build_overrides.tool_retry_handler,
                run_resume_handler=_build_overrides.run_resume_handler,
                heartbeat_interval_seconds=_resolve_heartbeat_interval(),
                lease_ttl_seconds=DEFAULT_LEASE_TTL_SECONDS,
            )
        except Exception as exc:  # noqa: BLE001
            logger.warning("mq_consumer_construct_failed: %s", exc)
            gate = None
            dispatcher = None

    components = MqRuntimeComponents(
        publisher=publisher,
        poller=poller,
        gate=gate,
        dispatcher=dispatcher,
        owner_registry=owner_registry,
    )
    if components.is_wired:
        logger.info("mq_runtime_components_built worker_id=%s", worker_id)
    else:
        logger.warning(
            "mq_runtime_components_DEGRADED_FAIL_CLOSED worker_id=%s publisher=%s poller=%s gate=%s dispatcher=%s owner_registry=%s | "
            "Protected (non-L0) tools will be DENIED until the MQ control plane comes online. "
            "Verify RocketMQ gateway URL/API key and Postgres connectivity.",
            worker_id,
            publisher is not None,
            poller is not None,
            gate is not None,
            dispatcher is not None,
            owner_registry is not None,
        )
    return components


def set_mq_runtime_components(components: MqRuntimeComponents | None) -> None:
    """Install (or clear) the singleton returned by :func:`get_mq_runtime_components`."""
    global _components
    _components = components


def get_mq_runtime_components() -> MqRuntimeComponents | None:
    """Return the currently-installed :class:`MqRuntimeComponents` (or ``None``)."""
    return _components


def reset_mq_runtime_components() -> None:
    """Clear the module-level singleton (test helper)."""
    global _components
    _components = None


__all__ = [
    "DEFAULT_FETCH_BATCH_SIZE",
    "DEFAULT_HEARTBEAT_INTERVAL_SECONDS",
    "DEFAULT_LEASE_TTL_SECONDS",
    "DEFAULT_POLL_INTERVAL_SECONDS",
    "DEFAULT_PUBLISH_BACKOFF_SECONDS",
    "DEFAULT_PUBLISH_MAX_RETRIES",
    "DEFAULT_PUBLISH_TIMEOUT_SECONDS",
    "MqRuntimeComponents",
    "build_mq_runtime_components",
    "get_mq_runtime_components",
    "reset_mq_build_overrides",
    "reset_mq_runtime_components",
    "set_mq_build_overrides",
    "set_mq_runtime_components",
]
