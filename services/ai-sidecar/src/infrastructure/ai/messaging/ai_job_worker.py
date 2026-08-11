"""Async AI Job Worker (ADR-0004).

Consumes pending AI jobs from the Rust internal AI API, executes them
via ``RuntimeService.stream_run_with_tools``, and writes results back.

Architecture
------------

Rust receives AI requests at the edge, persists job records, and returns
``202 Accepted``. This worker leases pending jobs (``SKIP LOCKED`` on
the Rust side), executes them concurrently (bounded by a semaphore),
and reports completion/failure back to Rust via the internal AI API.

All requests carry an ``X-Service-Identity`` JWT (per-path, HS256) for
service-to-service authentication. The JWT ``path`` claim must exactly
match the request path checked by the Rust ``ServiceIdentity`` middleware.

Each leased job gets a dedicated heartbeat task that refreshes the lease
while execution is in progress. On shutdown, the worker stops leasing
new jobs and waits for active executions to drain (with a configurable
grace period). Unfinished runs are left for the Rust reaper to reclaim
when the lease expires.

See :doc:`docs/architecture/ADR-0004-python-ai-worker-extraction` for
the full design.
"""

from __future__ import annotations

import asyncio
import contextlib
from dataclasses import dataclass
from typing import Any, ClassVar

import httpx

from src.infrastructure.ai.service_identity_issuer import ServiceIdentityIssuer
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


# Default configuration constants.
DEFAULT_LEASE_SECONDS: int = 60
DEFAULT_HEARTBEAT_INTERVAL_SECONDS: float = 15.0  # < lease_seconds/2
DEFAULT_POLL_INTERVAL_SECONDS: float = 1.0
DEFAULT_REQUEST_TIMEOUT: float = 10.0
DEFAULT_MAX_RETRIES: int = 3
DEFAULT_BACKOFF_SECONDS: float = 0.25
DEFAULT_MAX_CONCURRENT_RUNS: int = 1
DEFAULT_SHUTDOWN_GRACE_SECONDS: float = 30.0


class AiJobWorkerError(RuntimeError):
    """Transport error from the Rust internal AI API.

    Attributes:
        status_code: HTTP status code when the server responded, or
            ``None`` for transport-level failures (timeout, connection).
    """

    def __init__(self, message: str, *, status_code: int | None = None) -> None:
        super().__init__(message)
        self.status_code = status_code


@dataclass(frozen=True)
class AiJobWorkerConfig:
    """Configuration for the AI job worker.

    All paths are relative to ``base_url`` and double as the JWT ``path``
    claim — they must exactly match the Rust ``req.path()`` for each
    request (no query string, no trailing slash).
    """

    base_url: str
    worker_id: str
    job_type_filter: str | None = None
    lease_seconds: int = DEFAULT_LEASE_SECONDS
    heartbeat_interval_seconds: float = DEFAULT_HEARTBEAT_INTERVAL_SECONDS
    poll_interval_seconds: float = DEFAULT_POLL_INTERVAL_SECONDS
    request_timeout: float = DEFAULT_REQUEST_TIMEOUT
    max_retries: int = DEFAULT_MAX_RETRIES
    backoff_seconds: float = DEFAULT_BACKOFF_SECONDS
    max_concurrent_runs: int = DEFAULT_MAX_CONCURRENT_RUNS
    shutdown_grace_seconds: float = DEFAULT_SHUTDOWN_GRACE_SECONDS

    # Internal AI API path constant (also used as JWT ``path`` claim).
    LEASE_PATH: ClassVar[str] = "/internal/ai/v1/jobs/lease"

    def heartbeat_path(self, job_id: str) -> str:
        return f"/internal/ai/v1/jobs/{job_id}/heartbeat"

    def runs_path(self, job_id: str) -> str:
        return f"/internal/ai/v1/jobs/{job_id}/runs"

    def events_path(self, run_id: str) -> str:
        return f"/internal/ai/v1/runs/{run_id}/events"

    def complete_path(self, run_id: str) -> str:
        return f"/internal/ai/v1/runs/{run_id}/complete"

    def fail_path(self, run_id: str) -> str:
        return f"/internal/ai/v1/runs/{run_id}/fail"

    def url(self, path: str) -> str:
        return f"{self.base_url.rstrip('/')}{path}"


def is_terminal_status(status: str) -> bool:
    """Return True for terminal run statuses."""
    return status in ("succeeded", "failed_terminal", "cancelled")


class AiJobWorker:
    """Async consumer of AI jobs from the Rust internal AI API.

    The worker runs a main lease loop, spawning execution tasks for each
    leased job. Each execution task runs ``RuntimeService.stream_run_with_tools``
    and forwards events to Rust. A heartbeat task refreshes the lease
    while execution is in progress.

    On shutdown, the worker stops leasing new jobs, waits for active
    executions to complete (with a grace period), then exits. Runs that
    are still in progress when the grace period expires are left for the
    Rust reaper to reclaim when the lease expires.
    """

    def __init__(
        self,
        config: AiJobWorkerConfig,
        issuer: ServiceIdentityIssuer,
        *,
        client: httpx.AsyncClient | None = None,
        runtime_service: Any = None,
    ) -> None:
        self._config = config
        self._issuer = issuer
        self._owns_client = client is None
        self._client = client or httpx.AsyncClient(timeout=config.request_timeout)
        # Optional injected RuntimeService for testing. In production,
        # get_runtime_service() is called lazily inside _execute_run so
        # the DI container is fully bootstrapped before use.
        self._runtime_service = runtime_service
        self._semaphore = asyncio.Semaphore(config.max_concurrent_runs)
        self._active_executions: set[asyncio.Task] = set()
        self._shutdown = asyncio.Event()

    async def run(self, shutdown_event: asyncio.Event | None = None) -> None:
        """Run the lease loop until ``shutdown_event`` is set."""
        if shutdown_event is not None:
            self._shutdown = shutdown_event
        logger.info(
            "ai_job_worker_started worker_id=%s base_url=%s",
            self._config.worker_id,
            self._config.base_url,
        )
        try:
            await self._lease_loop()
        finally:
            await self._drain_active_executions(self._config.shutdown_grace_seconds)
            logger.info("ai_job_worker_stopped")

    @property
    def config(self) -> AiJobWorkerConfig:
        """Return the worker configuration."""
        return self._config

    async def aclose(self) -> None:
        """Close the HTTP client if owned by this worker."""
        if self._owns_client:
            with contextlib.suppress(httpx.HTTPError):
                await self._client.aclose()

    async def __aenter__(self) -> AiJobWorker:
        return self

    async def __aexit__(self, *exc: object) -> None:
        await self.aclose()

    # --- main loop ---

    async def _lease_loop(self) -> None:
        """Lease jobs and spawn execution tasks until shutdown."""
        while not self._shutdown.is_set():
            try:
                job = await self._lease_one_job()
                if job is None:
                    await self._sleep_interruptible(self._config.poll_interval_seconds)
                    continue
                task = asyncio.create_task(self._execute_job_safely(job))
                self._active_executions.add(task)
                task.add_done_callback(self._active_executions.discard)
            except asyncio.CancelledError:
                raise
            except Exception:
                logger.exception("ai_job_worker_lease_error")
                await self._sleep_interruptible(self._config.poll_interval_seconds * 2)

    async def _execute_job_safely(self, job: dict[str, Any]) -> None:
        """Wrapper that catches all exceptions from job execution."""
        job_id = job.get("job_id", "<unknown>")
        try:
            async with self._semaphore:
                await self._execute_job(job)
        except asyncio.CancelledError:
            # Shutdown: the Rust reaper will reclaim the job when the
            # lease expires. Don't attempt best-effort fail_run here —
            # the HTTP call may itself be cancelled.
            logger.info("ai_job_worker_job_cancelled job_id=%s", job_id)
            raise
        except Exception as exc:
            logger.exception("ai_job_worker_execute_error job_id=%s", job_id)
            await self._fail_job_best_effort(job, "RUNTIME_ERROR", str(exc))

    async def _execute_job(self, job: dict[str, Any]) -> None:
        """Execute all runs for a leased job."""
        job_id = job["job_id"]
        logger.info("ai_job_worker_job_started job_id=%s", job_id)

        runs = await self._list_job_runs(job_id)
        if not runs:
            logger.warning("ai_job_worker_no_runs job_id=%s", job_id)
            return

        heartbeat_stop = asyncio.Event()
        heartbeat_task = asyncio.create_task(self._heartbeat_loop(job_id, heartbeat_stop))
        try:
            for run in runs:
                if self._shutdown.is_set():
                    break
                await self._execute_run(run)
        finally:
            heartbeat_stop.set()
            with contextlib.suppress(asyncio.CancelledError, asyncio.TimeoutError):
                await asyncio.wait_for(heartbeat_task, timeout=5.0)

    async def _execute_run(self, run: dict[str, Any]) -> None:
        """Stream events from RuntimeService, forwarding to Rust."""
        run_id = run["run_id"]
        input_envelope = run.get("input_envelope")
        if not input_envelope:
            await self._fail_run(run_id, "INVALID_INPUT", "Run has no input_envelope")
            return

        try:
            from src.infrastructure.ai.context_envelope import ContextEnvelope
            from src.infrastructure.ai.runtime_service import get_runtime_service

            envelope = ContextEnvelope(**input_envelope)
            service = self._runtime_service or get_runtime_service()
        except Exception as exc:  # noqa: BLE001 - envelope construction may fail through optional runtime adapters
            await self._fail_run(run_id, "ENVELOPE_ERROR", f"Failed to parse envelope: {exc}")
            return

        terminal_reached = False
        try:
            async for event in service.stream_run_with_tools(envelope):
                if self._shutdown.is_set():
                    break
                event_type = event.get("event", "")
                data = event.get("data") or {}

                # Forward every event to Rust (best-effort, non-blocking on failure).
                await self._forward_event(run_id, event_type, data)

                if event_type == "run.complete":
                    terminal_reached = True
                    await self._complete_run(run_id, data)
                elif event_type == "run.fail":
                    terminal_reached = True
                    error_code = str(data.get("error_code") or data.get("code") or "RUNTIME_FAILED")
                    error_message = str(data.get("error_message") or data.get("message") or "Run failed")
                    await self._fail_run(run_id, error_code, error_message)

            if not terminal_reached and not self._shutdown.is_set():
                logger.warning("ai_job_worker_no_terminal run_id=%s — completing with empty output", run_id)
                await self._complete_run(run_id, {})
        except asyncio.CancelledError:
            logger.info("ai_job_worker_run_cancelled run_id=%s — reaper will reclaim", run_id)
            raise
        except Exception as exc:
            logger.exception("ai_job_worker_run_error run_id=%s", run_id)
            await self._fail_run(run_id, "RUNTIME_ERROR", str(exc))

    # --- heartbeat ---

    async def _heartbeat_loop(self, job_id: str, stop: asyncio.Event) -> None:
        """Refresh the lease until ``stop`` or shutdown is set."""
        while not (stop.is_set() or self._shutdown.is_set()):
            try:
                renewed = await self._heartbeat_job(job_id)
                if not renewed:
                    logger.warning("ai_job_worker_heartbeat_not_renewed job_id=%s", job_id)
            except Exception:  # noqa: BLE001 - heartbeat loop must survive transient adapter failures
                logger.warning("ai_job_worker_heartbeat_failed job_id=%s", job_id, exc_info=True)
            await self._sleep_interruptible(self._config.heartbeat_interval_seconds, stop)

    # --- Rust API methods ---

    async def _lease_one_job(self) -> dict[str, Any] | None:
        body: dict[str, Any] = {
            "lease_owner": self._config.worker_id,
            "lease_seconds": self._config.lease_seconds,
        }
        if self._config.job_type_filter:
            body["job_type"] = self._config.job_type_filter
        response = await self._request("POST", self._config.LEASE_PATH, json=body)
        data = response.get("data")
        return data  # None when no pending job is available

    async def _heartbeat_job(self, job_id: str) -> bool:
        body = {
            "lease_owner": self._config.worker_id,
            "lease_seconds": self._config.lease_seconds,
        }
        response = await self._request("POST", self._config.heartbeat_path(job_id), json=body)
        return bool(response.get("data", {}).get("renewed", False))

    async def _list_job_runs(self, job_id: str) -> list[dict[str, Any]]:
        response = await self._request("GET", self._config.runs_path(job_id))
        data = response.get("data", [])
        return data if isinstance(data, list) else []

    async def _forward_event(self, run_id: str, event_type: str, data: dict[str, Any]) -> None:
        body = {"event_type": event_type, "payload": data}
        try:
            await self._request("POST", self._config.events_path(run_id), json=body)
        except AiJobWorkerError:
            logger.debug(
                "ai_job_worker_event_forward_failed run_id=%s event_type=%s",
                run_id,
                event_type,
                exc_info=True,
            )

    async def _complete_run(self, run_id: str, output: dict[str, Any]) -> None:
        body = {
            "output_raw": output,
            "output_validated": None,
            "token_usage": None,
        }
        try:
            await self._request("POST", self._config.complete_path(run_id), json=body)
        except AiJobWorkerError as exc:
            if exc.status_code == 409:
                logger.info("ai_job_worker_complete_409 run_id=%s (already terminal)", run_id)
                return
            raise

    async def _fail_run(self, run_id: str, error_code: str, error_message: str) -> None:
        body = {
            "error_code": error_code,
            "error_message": error_message,
            "output_raw": None,
        }
        try:
            await self._request("POST", self._config.fail_path(run_id), json=body)
        except AiJobWorkerError as exc:
            if exc.status_code == 409:
                logger.info("ai_job_worker_fail_409 run_id=%s (already terminal)", run_id)
                return
            logger.warning("ai_job_worker_fail_failed run_id=%s status=%s", run_id, exc.status_code)
        except Exception:  # noqa: BLE001 - failure reporting is best-effort and must not escape
            logger.warning("ai_job_worker_fail_failed run_id=%s", run_id, exc_info=True)

    async def _fail_job_best_effort(self, job: dict[str, Any], error_code: str, error_message: str) -> None:
        """Attempt to fail all non-terminal runs for a job. Best-effort."""
        job_id = job.get("job_id")
        if not job_id:
            return
        try:
            runs = await self._list_job_runs(job_id)
            for run in runs:
                if not is_terminal_status(run.get("status", "")):
                    await self._fail_run(run["run_id"], error_code, error_message)
        except Exception:  # noqa: BLE001 - best-effort job cleanup must absorb adapter failures
            logger.warning("ai_job_worker_fail_best_effort_failed job_id=%s", job_id, exc_info=True)

    # --- HTTP client ---

    async def _request(
        self,
        method: str,
        path: str,
        json: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """Make an authenticated request to the Rust internal AI API.

        Generates a per-path JWT, retries on 5xx/network errors, and
        raises :class:`AiJobWorkerError` on non-retryable failures.
        """
        headers = self._issuer.headers_for_path(path)
        url = self._config.url(path)
        attempts = self._config.max_retries + 1
        last_exc: Exception | None = None

        for attempt in range(1, attempts + 1):
            try:
                response = await self._client.request(
                    method,
                    url,
                    json=json,
                    headers=headers,
                    timeout=self._config.request_timeout,
                )
                if 500 <= response.status_code < 600 and attempt < attempts:
                    last_exc = AiJobWorkerError(
                        f"HTTP_{response.status_code}: {response.text}",
                        status_code=response.status_code,
                    )
                    await self._sleep_backoff(attempt)
                    continue
                if response.status_code >= 400:
                    raise AiJobWorkerError(
                        f"HTTP_{response.status_code}: {response.text}",
                        status_code=response.status_code,
                    )
                if not response.content:
                    return {}
                try:
                    return response.json()
                except ValueError:
                    return {}
            except httpx.HTTPError as exc:
                if attempt < attempts:
                    last_exc = exc
                    await self._sleep_backoff(attempt)
                    continue
                raise AiJobWorkerError(f"Transport error: {exc}") from exc

        raise AiJobWorkerError(f"Exhausted retries: {last_exc}")

    async def _sleep_backoff(self, attempt: int) -> None:
        delay = self._config.backoff_seconds * (2 ** (attempt - 1))
        await asyncio.sleep(delay)

    async def _sleep_interruptible(self, seconds: float, *extra_events: asyncio.Event) -> None:
        """Sleep for ``seconds``, interruptible by shutdown or any ``extra_events``."""
        events = (self._shutdown, *extra_events)
        tasks = [asyncio.create_task(e.wait()) for e in events]
        try:
            await asyncio.wait(tasks, timeout=seconds, return_when=asyncio.FIRST_COMPLETED)
        finally:
            for task in tasks:
                if not task.done():
                    task.cancel()
            await asyncio.gather(*tasks, return_exceptions=True)

    async def _drain_active_executions(self, timeout: float) -> None:
        """Wait for active executions to complete, then cancel stragglers."""
        if not self._active_executions:
            return
        tasks = list(self._active_executions)
        logger.info("ai_job_worker_draining tasks=%d", len(tasks))
        try:
            await asyncio.wait_for(
                asyncio.gather(*tasks, return_exceptions=True),
                timeout=timeout,
            )
        except TimeoutError:
            logger.warning("ai_job_worker_drain_timeout — cancelling remaining tasks")
            for task in tasks:
                if not task.done():
                    task.cancel()
            await asyncio.gather(*tasks, return_exceptions=True)


__all__ = [
    "AiJobWorker",
    "AiJobWorkerConfig",
    "AiJobWorkerError",
    "is_terminal_status",
]
