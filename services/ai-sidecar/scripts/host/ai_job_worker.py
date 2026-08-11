"""Standalone worker entrypoint for the async AI job consumer (ADR-0004).

Polls the Rust internal AI API for pending jobs, executes them via
``RuntimeService.stream_run_with_tools``, and writes results back. This
is the Python-side counterpart of the Rust async job path: Rust
receives AI requests at the edge, persists job records, and returns
``202 Accepted``; this worker leases and processes them.

Environment variables:

* ``WORKER_ID`` — optional stable worker identity. When omitted a
  unique identity is generated from hostname, PID and ULID.
* ``AI_INTERNAL_API_URL`` / ``RUST_API_BASE_URL`` / ``AI_API_BASE_URL``
  — Rust internal AI API base URL. Required; without it the worker
  degrades to idle (no jobs are consumed).
* ``JWT_SECRET`` / ``JWT_SECRET_KEY`` — shared HS256 secret for
  ``X-Service-Identity`` JWT. Required.
* ``AI_DATABASE_URL`` / ``DATABASE_URL`` / ``POSTGRES_*`` — Postgres DSN
  (used by the shared runtime bootstrap).
* ``AI_JOB_TYPE_FILTER`` — optional task-type filter for leasing.
* ``AI_JOB_LEASE_SECONDS`` — lease TTL (default 60).
* ``AI_JOB_HEARTBEAT_INTERVAL_SECONDS`` — heartbeat period (default 15).
* ``AI_JOB_POLL_INTERVAL_SECONDS`` — poll interval (default 1.0).
* ``AI_JOB_MAX_CONCURRENT_RUNS`` — concurrent run executions (default 1).
* ``AI_JOB_SHUTDOWN_GRACE_SECONDS`` — drain timeout (default 30.0).
"""

from __future__ import annotations

import asyncio
import contextlib
import os
import signal
import sys
from types import FrameType

from src.infrastructure.ai.ai_runtime_bootstrap import ai_runtime_lifespan
from src.infrastructure.ai.messaging.ai_job_worker_bootstrap import (
    build_ai_job_worker_from_env,
)
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


async def _run_job_worker(shutdown_event: asyncio.Event | None = None) -> None:
    """Run the job worker lifespan and block until a shutdown signal arrives.

    Args:
        shutdown_event: Optional event to trigger graceful shutdown. When
            omitted, an event is created and wired to ``SIGINT``/``SIGTERM``.
    """
    event = shutdown_event or asyncio.Event()

    if shutdown_event is None:

        def _signal_handler(signum: int, _frame: FrameType | None) -> None:
            logger.info("ai_job_worker_signal_received signum=%s", signum)
            event.set()

        for sig in (signal.SIGINT, signal.SIGTERM):
            signal.signal(sig, _signal_handler)

    async with ai_runtime_lifespan():
        logger.info(
            "ai_job_worker_starting worker_id=%s",
            os.environ.get("WORKER_ID", "<auto>"),
        )
        worker = build_ai_job_worker_from_env()
        if worker is None:
            logger.warning(
                "ai_job_worker_DEGRADED_not_configured — waiting for shutdown signal"
            )
            await event.wait()
            return

        worker_task = asyncio.create_task(worker.run(event))
        shutdown_wait = asyncio.ensure_future(event.wait())
        try:
            # Wait for either shutdown signal or worker to exit/crash.
            await asyncio.wait(
                [worker_task, shutdown_wait],
                return_when=asyncio.FIRST_COMPLETED,
            )
        finally:
            if not shutdown_wait.done():
                shutdown_wait.cancel()
                with contextlib.suppress(asyncio.CancelledError):
                    await shutdown_wait

            if not worker_task.done():
                # Worker is still running — wait for it to drain.
                try:
                    await asyncio.wait_for(
                        worker_task,
                        timeout=worker.config.shutdown_grace_seconds,
                    )
                except asyncio.TimeoutError:
                    logger.warning("ai_job_worker_drain_timeout — forcing exit")
                    worker_task.cancel()
                    with contextlib.suppress(asyncio.CancelledError):
                        await worker_task
            elif not worker_task.cancelled():
                # Worker exited before shutdown — re-raise any exception.
                worker_task.result()

            await worker.aclose()

    logger.info("ai_job_worker_stopped")


def main(argv: list[str] | None = None) -> int:
    """CLI entrypoint for the standalone async AI job worker."""
    try:
        asyncio.run(_run_job_worker())
    except KeyboardInterrupt:
        logger.info("ai_job_worker_keyboard_interrupt")
        return 0
    except Exception as exc:
        logger.exception("ai_job_worker_fatal: %s", exc)
        return 1
    return 0


if __name__ == "__main__":  # pragma: no cover - executed directly
    sys.exit(main())
