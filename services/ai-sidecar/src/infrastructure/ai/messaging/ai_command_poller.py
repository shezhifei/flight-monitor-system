"""Postgres-as-queue poller for ``ai_runtime_commands``.

Phase 4 of the AI agent resilient tool architecture: Rust writes
authorization decisions (tool_lease, tool_denied, tool_proposal_only) and
control commands (start_run, cancel_run, retry_tool, resume_run) to the
``ai_runtime_commands`` table. The Python sidecar consumes the table
using the same ``FOR UPDATE SKIP LOCKED`` pattern as
``DomainEventRelayService``.

The poller is split into parts:

* :meth:`AiCommandPoller.fetch_pending` — atomically lease a batch of
  pending commands (sets ``status='leased'`` and ``lease_owner``).
* :meth:`AiCommandPoller.lease_pending_with_owner_check` — same as
  ``fetch_pending`` but skips rows whose run is owned by another live
  worker (``run_owner_lock``).
* :meth:`AiCommandPoller.run` — long-running loop that polls, claims run
  ownership, dispatches commands, and marks them completed or failed.

The same poller is also used by the tool authorization gate (see
:mod:`src.infrastructure.ai.tools.mq_gate`) to wait for the Rust
authorization decision for a specific ``tool_call_pk``.
"""

from __future__ import annotations

import asyncio
import contextlib
from collections.abc import Awaitable, Callable
from typing import Any, Protocol

from src.infrastructure.common.exceptions import POSTGRES_EXCEPTIONS
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


DEFAULT_POLL_INTERVAL_SECONDS: float = 0.2
DEFAULT_LEASE_TTL_SECONDS: int = 30
DEFAULT_FETCH_BATCH_SIZE: int = 10


class _AcquireCtx:
    def __init__(self, pool: Any) -> None:
        self._pool = pool

    async def __aenter__(self) -> Any:
        return await self._pool.acquire()

    async def __aexit__(self, exc_type: Any, exc: Any, tb: Any) -> None:
        if exc_type is not None:
            return None
        return None


class _AcquireFactory(Protocol):
    def __call__(self) -> Any: ...


class AiCommandPoller:
    """Postgres SKIP LOCKED poller for ``ai_runtime_commands``.

    The poller uses a connection pool with the same ``async with
    pool.acquire() as conn:`` contract as :class:`AsyncpgAIConfigStore`.
    Tests may inject a fake pool that returns a fake connection
    implementing ``fetch``/``execute``/``transaction``.
    """

    def __init__(
        self,
        pool: Any,
        owner: str,
        *,
        interval_seconds: float = DEFAULT_POLL_INTERVAL_SECONDS,
        batch_size: int = DEFAULT_FETCH_BATCH_SIZE,
        lease_ttl_seconds: int = DEFAULT_LEASE_TTL_SECONDS,
        dispatcher: Callable[[dict[str, Any]], Awaitable[None]] | None = None,
    ) -> None:
        self._pool = pool
        self._owner = owner
        self._interval_seconds = max(0.0, float(interval_seconds))
        self._batch_size = max(1, int(batch_size))
        self._lease_ttl_seconds = max(1, int(lease_ttl_seconds))
        self._dispatcher = dispatcher
        self._shutdown_event = asyncio.Event()
        self._lease_expires_clause = f"now() + interval '{int(self._lease_ttl_seconds)} seconds'"

    @property
    def owner(self) -> str:
        return self._owner

    @property
    def is_shutdown(self) -> bool:
        return self._shutdown_event.is_set()

    def request_shutdown(self) -> None:
        self._shutdown_event.set()

    async def _acquire(self) -> Any:
        acquire = getattr(self._pool, "acquire", None)
        if acquire is None:
            raise RuntimeError("AiCommandPoller requires a pool with an `acquire()` method")

        result = acquire()
        if hasattr(result, "__aenter__"):
            return await result.__aenter__()
        return await result

    def _release(self, conn: Any) -> None:
        release = getattr(conn, "release", None)
        if release is not None:
            release()
            return
        close = getattr(conn, "close", None)
        if close is not None:
            close()

    async def _acquire_conn(self) -> tuple[Any, bool]:
        """Acquire a connection.

        Returns ``(conn, owned)`` where ``owned=True`` means the poller
        has to release the connection back to the pool itself (raw
        asyncpg ``pool.acquire()`` returns a connection that must be
        released); ``owned=False`` means the connection came from an
        ``async with pool.acquire()`` context manager that will release
        it on exit.
        """
        acquire = getattr(self._pool, "acquire", None)
        if acquire is None:
            raise RuntimeError("AiCommandPoller requires a pool with an `acquire()` method")
        result = acquire()
        if hasattr(result, "__aenter__"):
            conn = await result.__aenter__()
            return conn, False
        if asyncio.iscoroutine(result):
            conn = await result
            return conn, True
        return result, True

    async def fetch_pending(self, owner: str | None = None, batch_size: int | None = None) -> list[dict[str, Any]]:
        """Atomically lease a batch of pending commands.

        Returns the leased rows. The same worker that leased a row is
        expected to mark it completed or failed.
        """
        owner = owner or self._owner
        limit = max(1, int(batch_size or self._batch_size))

        lease_expires = self._lease_expires_clause
        query = (
            "WITH leased AS ("
            "    SELECT command_id"
            "    FROM ai_runtime_commands"
            "    WHERE status = 'pending'"
            "      AND (lease_owner IS NULL OR lease_expires_at IS NULL OR lease_expires_at < now())"
            "    ORDER BY created_at"
            "    FOR UPDATE SKIP LOCKED"
            "    LIMIT $1"
            ") "
            "UPDATE ai_runtime_commands c "
            "SET status = 'leased',"
            "    lease_owner = $2,"
            "    lease_expires_at = " + lease_expires + " "
            "FROM leased "
            "WHERE c.command_id = leased.command_id "
            "RETURNING c.command_id, c.run_id, c.command_type, c.command_sequence, "
            "          c.tool_call_pk, c.payload, c.status, c.lease_owner, "
            "          c.lease_expires_at, c.created_at, c.processed_at"
        )

        conn, owned = await self._acquire_conn()
        try:
            fetch = getattr(conn, "fetch", None)
            if fetch is None:
                raise RuntimeError("Connection must implement fetch()")
            try:
                rows = await fetch(query, limit, owner)
            except POSTGRES_EXCEPTIONS as exc:
                logger.error("ai_command_poller_fetch_failed: %s", exc)
                raise
        finally:
            if owned:
                self._release(conn)

        return [_coerce_command_row(row) for row in rows or []]

    async def mark_completed(self, command_id: str) -> None:
        """Mark a leased command as completed."""
        query = (
            "UPDATE ai_runtime_commands "
            "SET status = 'completed', processed_at = now(), lease_expires_at = NULL "
            "WHERE command_id = $1 AND status IN ('leased', 'pending')"
        )
        conn, owned = await self._acquire_conn()
        try:
            execute = getattr(conn, "execute", None)
            if execute is None:
                raise RuntimeError("Connection must implement execute()")
            await execute(query, command_id)
        finally:
            if owned:
                self._release(conn)
        logger.info("ai_command_completed", extra={"command_id": command_id, "owner": self._owner})

    async def release_lease(self, command_id: str) -> None:
        """Release a previously leased command back to pending so another worker can claim it.

        Used by the fallback poller when it accidentally leases a command that belongs
        to a different tool_call / waiter. This avoids dropping the command on the floor
        (which would permanently lose the authorization decision for another waiter).
        """
        query = (
            "UPDATE ai_runtime_commands "
            "SET status = 'pending', lease_owner = NULL, lease_expires_at = NULL "
            "WHERE command_id = $1 AND status = 'leased' AND lease_owner = $2"
        )
        conn, owned = await self._acquire_conn()
        try:
            execute = getattr(conn, "execute", None)
            if execute is None:
                raise RuntimeError("Connection must implement execute()")
            await execute(query, command_id, self._owner)
        finally:
            if owned:
                self._release(conn)

    async def mark_failed(self, command_id: str, error: str) -> None:
        """Mark a leased command as failed and record the error message."""
        query = (
            "UPDATE ai_runtime_commands "
            "SET status = 'failed', processed_at = now(), lease_expires_at = NULL, "
            "    payload = payload || jsonb_build_object('error', $2::text) "
            "WHERE command_id = $1 AND status IN ('leased', 'pending')"
        )
        conn, owned = await self._acquire_conn()
        try:
            execute = getattr(conn, "execute", None)
            if execute is None:
                raise RuntimeError("Connection must implement execute()")
            await execute(query, command_id, error or "")
        finally:
            if owned:
                self._release(conn)
        logger.warning(
            "ai_command_failed",
            extra={"command_id": command_id, "owner": self._owner, "error": error},
        )

    async def mark_leased(self, command_id: str, *, ttl_seconds: int | None = None) -> None:
        """Refresh the lease expiry for a command that is being processed."""
        ttl = max(1, int(ttl_seconds or self._lease_ttl_seconds))
        query = (
            "UPDATE ai_runtime_commands "
            f"SET lease_expires_at = now() + interval '{ttl} seconds' "
            "WHERE command_id = $1 AND status = 'leased'"
        )
        conn, owned = await self._acquire_conn()
        try:
            execute = getattr(conn, "execute", None)
            if execute is None:
                raise RuntimeError("Connection must implement execute()")
            await execute(query, command_id)
        finally:
            if owned:
                self._release(conn)

    async def lease_pending_with_owner_check(
        self,
        owner: str | None = None,
        batch_size: int | None = None,
    ) -> list[dict[str, Any]]:
        """Lease pending commands, skipping runs owned by another live worker.

        A run is considered owned by another worker when any row for that
        ``run_id`` has ``run_owner_lock`` set to a different worker and a
        future ``lease_expires_at``. If the connection does not support the
        extended query, callers should fall back to :meth:`fetch_pending`
        plus a client-side owner check.
        """
        owner = owner or self._owner
        limit = max(1, int(batch_size or self._batch_size))
        lease_expires = f"now() + interval '{int(self._lease_ttl_seconds)} seconds'"

        query = (
            "WITH leased AS ("
            "    SELECT c.command_id"
            "    FROM ai_runtime_commands c"
            "    WHERE c.status = 'pending'"
            "      AND (c.lease_owner IS NULL OR c.lease_expires_at IS NULL OR c.lease_expires_at < now())"
            "      AND ("
            "          c.run_owner_lock IS NULL OR c.run_owner_lock = '' OR c.run_owner_lock = $2"
            "          OR NOT EXISTS ("
            "              SELECT 1 FROM ai_runtime_commands c2"
            "              WHERE c2.run_id = c.run_id"
            "                AND c2.run_owner_lock IS NOT NULL"
            "                AND c2.run_owner_lock != ''"
            "                AND c2.run_owner_lock != $2"
            "                AND (c2.lease_expires_at IS NULL OR c2.lease_expires_at > now())"
            "          )"
            "      )"
            "    ORDER BY c.created_at"
            "    FOR UPDATE SKIP LOCKED"
            "    LIMIT $1"
            ") "
            "UPDATE ai_runtime_commands c "
            "SET status = 'leased',"
            "    lease_owner = $2,"
            "    lease_expires_at = " + lease_expires + " "
            "FROM leased "
            "WHERE c.command_id = leased.command_id "
            "RETURNING c.command_id, c.run_id, c.command_type, c.command_sequence, "
            "          c.tool_call_pk, c.payload, c.status, c.lease_owner, "
            "          c.lease_expires_at, c.created_at, c.processed_at"
        )

        conn, owned = await self._acquire_conn()
        try:
            fetch = getattr(conn, "fetch", None)
            if fetch is None:
                raise RuntimeError("Connection must implement fetch()")
            try:
                rows = await fetch(query, limit, owner)
            except POSTGRES_EXCEPTIONS as exc:
                logger.error("ai_command_poller_lease_owner_check_failed: %s", exc)
                raise
        finally:
            if owned:
                self._release(conn)

        return [_coerce_command_row(row) for row in rows or []]

    async def run(
        self,
        dispatcher: Callable[[dict[str, Any]], Awaitable[None]] | None = None,
        owner_registry: Any | None = None,
    ) -> None:
        """Continuously poll and dispatch until :meth:`request_shutdown` is called."""
        active_dispatcher = dispatcher or self._dispatcher
        if active_dispatcher is None:
            raise RuntimeError(
                "AiCommandPoller.run() requires a dispatcher to be set at construction time or passed as an argument"
            )

        heartbeat_task: asyncio.Task[None] | None = None
        if owner_registry is not None:
            heartbeat_task = asyncio.create_task(
                self._owner_heartbeat_loop(owner_registry),
                name="ai-command-owner-heartbeat",
            )

        use_owner_check = owner_registry is not None

        try:
            while not self._shutdown_event.is_set():
                try:
                    if use_owner_check:
                        try:
                            commands = await self.lease_pending_with_owner_check(
                                owner=self._owner, batch_size=self._batch_size
                            )
                        except Exception:  # noqa: BLE001 - fall back to plain fetch
                            commands = await self.fetch_pending(owner=self._owner, batch_size=self._batch_size)
                    else:
                        commands = await self.fetch_pending(owner=self._owner, batch_size=self._batch_size)
                except POSTGRES_EXCEPTIONS as exc:
                    logger.error("ai_command_poller_fetch_error: %s", exc)
                    await self._sleep_or_shutdown(self._interval_seconds)
                    continue

                for cmd in commands:
                    if self._shutdown_event.is_set():
                        break
                    command_id = cmd.get("command_id", "")
                    run_id = cmd.get("run_id", "")

                    if owner_registry is not None:
                        try:
                            claimed = await owner_registry.claim(run_id, cmd)
                        except Exception as exc:
                            logger.error(
                                "ai_command_poller_claim_error",
                                extra={"command_id": command_id, "run_id": run_id},
                                exc_info=exc,
                            )
                            claimed = False
                        if not claimed:
                            try:
                                await self.release_lease(command_id)
                            except Exception as mark_exc:
                                logger.error(
                                    "ai_command_poller_skip_release_lease_error",
                                    extra={"command_id": command_id},
                                    exc_info=mark_exc,
                                )
                            continue

                    try:
                        await active_dispatcher(cmd)
                    except Exception as exc:
                        logger.error(
                            "ai_command_poller_dispatch_error",
                            extra={"command_id": command_id, "owner": self._owner},
                            exc_info=exc,
                        )
                        try:
                            await self.mark_failed(command_id, str(exc))
                        except Exception as mark_exc:
                            logger.error(
                                "ai_command_poller_mark_failed_error",
                                extra={"command_id": command_id, "owner": self._owner},
                                exc_info=mark_exc,
                            )
                        continue

                    try:
                        await self.mark_completed(command_id)
                    except Exception as exc:
                        logger.error(
                            "ai_command_poller_mark_completed_error",
                            extra={"command_id": command_id, "owner": self._owner},
                            exc_info=exc,
                        )

                if not commands and self._interval_seconds > 0:
                    await self._sleep_or_shutdown(self._interval_seconds)
        finally:
            if heartbeat_task is not None:
                heartbeat_task.cancel()
                with contextlib.suppress(asyncio.CancelledError, Exception):
                    await heartbeat_task
            if owner_registry is not None:
                cancel_all = getattr(active_dispatcher, "cancel_all_runs", None)
                if cancel_all is not None:
                    try:
                        await cancel_all()
                    except Exception as exc:  # noqa: BLE001
                        logger.warning("ai_command_poller_cancel_all_runs_failed", exc_info=exc)

    async def _owner_heartbeat_loop(self, owner_registry: Any) -> None:
        try:
            while not self._shutdown_event.is_set():
                try:
                    await owner_registry.heartbeat_all()
                except Exception as exc:  # noqa: BLE001
                    logger.warning("ai_command_poller_owner_heartbeat_failed: %s", exc)
                await self._sleep_or_shutdown(10.0)
        except asyncio.CancelledError:
            return

    async def _sleep_or_shutdown(self, seconds: float) -> None:
        if seconds <= 0:
            return
        try:
            await asyncio.wait_for(self._shutdown_event.wait(), timeout=seconds)
        except TimeoutError:
            return


def _coerce_command_row(row: Any) -> dict[str, Any]:
    """Normalize a DB row into a plain dict with the expected keys.

    asyncpg returns ``Record`` objects; some fakes return plain dicts.
    The ``payload`` column is JSONB and may be returned as a dict or a
    JSON string depending on the connection codec.
    """
    if isinstance(row, dict):
        data = dict(row)
    else:
        data = dict(row)
    payload = data.get("payload")
    if isinstance(payload, (str, bytes, bytearray)):
        import json

        try:
            data["payload"] = json.loads(payload)
        except (TypeError, ValueError):
            data["payload"] = {}
    elif payload is None:
        data["payload"] = {}
    return data


__all__ = [
    "DEFAULT_FETCH_BATCH_SIZE",
    "DEFAULT_LEASE_TTL_SECONDS",
    "DEFAULT_POLL_INTERVAL_SECONDS",
    "AiCommandPoller",
]
