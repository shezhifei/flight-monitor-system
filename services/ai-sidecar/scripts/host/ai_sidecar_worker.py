"""Standalone worker entrypoint for the Python AI sidecar command consumer.

Runs the AI runtime bootstrap and the MQ command consumer loop without
starting the FastAPI HTTP server. Multiple instances of this process can
run concurrently; commands are distributed via Postgres
``FOR UPDATE SKIP LOCKED`` and run ownership prevents duplicate execution.

Environment variables:

* ``WORKER_ID`` — optional stable worker identity. When omitted a unique
  identity is generated from hostname, PID and ULID.
* ``AI_DATABASE_URL`` / ``DATABASE_URL`` / ``POSTGRES_*`` — Postgres DSN.
* ``AI_MQ_GATEWAY_URL`` / ``MQ_GATEWAY_URL`` — MQ gateway base URL.
* ``AI_MQ_GATEWAY_API_KEY`` — optional API key for the MQ gateway.
* ``AI_MQ_HEARTBEAT_INTERVAL_SECONDS`` — command heartbeat interval.
"""

from __future__ import annotations

import asyncio
import os
import signal
import sys
from types import FrameType

from src.infrastructure.ai.ai_runtime_bootstrap import ai_runtime_lifespan
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


async def _run_worker(shutdown_event: asyncio.Event | None = None) -> None:
    """Run the worker lifespan and block until a shutdown signal arrives.

    Args:
        shutdown_event: Optional event to trigger graceful shutdown. When
            omitted, an event is created and wired to ``SIGINT``/``SIGTERM``.
    """
    event = shutdown_event or asyncio.Event()

    if shutdown_event is None:

        def _signal_handler(signum: int, _frame: FrameType | None) -> None:
            logger.info("ai_sidecar_worker_signal_received signum=%s", signum)
            event.set()

        for sig in (signal.SIGINT, signal.SIGTERM):
            signal.signal(sig, _signal_handler)

    async with ai_runtime_lifespan():
        logger.info("ai_sidecar_worker_started worker_id=%s", os.environ.get("WORKER_ID", "<auto>"))
        await event.wait()

    logger.info("ai_sidecar_worker_stopped")


def main(argv: list[str] | None = None) -> int:
    """CLI entrypoint for the standalone command consumer worker."""
    try:
        asyncio.run(_run_worker())
    except KeyboardInterrupt:
        logger.info("ai_sidecar_worker_keyboard_interrupt")
        return 0
    except Exception as exc:
        logger.exception("ai_sidecar_worker_fatal: %s", exc)
        return 1
    return 0


if __name__ == "__main__":  # pragma: no cover - executed directly
    sys.exit(main())
