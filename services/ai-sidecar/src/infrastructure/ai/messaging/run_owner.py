"""Run ownership registry for multi-worker Python sidecar command consumers."""

from __future__ import annotations

import asyncio
from datetime import UTC, datetime
from typing import Any

from src.infrastructure.common.exceptions import POSTGRES_EXCEPTIONS
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class RunOwnerRegistry:
    """Tracks which runs this worker owns and persists the claim to Postgres.

    The ownership record lives on ``ai_runtime_commands.run_owner_lock`` so
    that Rust recovery can tell which worker was executing a run when a
    command is stale. The registry also keeps an in-memory set for fast
    local checks.
    """

    def __init__(
        self,
        worker_id: str,
        pool: Any,
        *,
        lease_ttl_seconds: int = 30,
    ) -> None:
        self._worker_id = worker_id
        self._pool = pool
        self._lease_ttl_seconds = max(1, int(lease_ttl_seconds))
        self._owned: dict[str, dict[str, Any]] = {}
        self._lock = asyncio.Lock()

    @property
    def worker_id(self) -> str:
        return self._worker_id

    @property
    def owned_run_ids(self) -> set[str]:
        return set(self._owned.keys())

    async def claim(self, run_id: str, command: dict[str, Any] | None = None) -> bool:
        async with self._lock:
            if run_id in self._owned:
                return True
            claimed = await self._persist_claim(run_id)
            if claimed:
                self._owned[run_id] = {
                    "command": command,
                    "claimed_at": datetime.now(UTC),
                }
            return claimed

    async def _persist_claim(self, run_id: str) -> bool:
        query = (
            "UPDATE ai_runtime_commands "
            "SET run_owner_lock = $1, "
            f"    lease_expires_at = now() + interval '{self._lease_ttl_seconds} seconds' "
            "WHERE run_id = $2 "
            "  AND (run_owner_lock IS NULL OR run_owner_lock = '' OR run_owner_lock = $1) "
            "RETURNING command_id"
        )

        conn, owned = await self._acquire_conn()
        try:
            fetch = getattr(conn, "fetch", None)
            if fetch is None:
                raise RuntimeError("Connection must implement fetch()")
            try:
                rows = await fetch(query, self._worker_id, run_id)
            except POSTGRES_EXCEPTIONS as exc:
                logger.error("run_owner_claim_failed: %s", exc)
                return False
        finally:
            if owned:
                self._release(conn)

        if rows:
            return True

        verified = await self._verify_owner(run_id)
        return verified == self._worker_id

    async def _verify_owner(self, run_id: str) -> str | None:
        query = (
            "SELECT run_owner_lock FROM ai_runtime_commands "
            "WHERE run_id = $1 AND run_owner_lock IS NOT NULL AND run_owner_lock != '' "
            "LIMIT 1"
        )
        conn, owned = await self._acquire_conn()
        try:
            fetch = getattr(conn, "fetch", None)
            if fetch is None:
                return None
            try:
                rows = await fetch(query, run_id)
            except POSTGRES_EXCEPTIONS as exc:
                logger.error("run_owner_verify_failed: %s", exc)
                return None
        finally:
            if owned:
                self._release(conn)

        for row in rows or []:
            data = dict(row) if not isinstance(row, dict) else row
            return data.get("run_owner_lock")
        return None

    def is_owner(self, run_id: str) -> bool:
        return run_id in self._owned

    def release(self, run_id: str) -> None:
        self._owned.pop(run_id, None)

    async def heartbeat_all(self) -> int:
        owned = set()
        async with self._lock:
            owned = set(self._owned.keys())

        if not owned:
            return 0

        query = (
            "UPDATE ai_runtime_commands "
            f"SET lease_expires_at = now() + interval '{self._lease_ttl_seconds} seconds' "
            "WHERE run_id = ANY($1::text[]) "
            "  AND run_owner_lock = $2 "
            "  AND status IN ('leased', 'pending')"
        )

        conn, owned_conn = await self._acquire_conn()
        try:
            execute = getattr(conn, "execute", None)
            if execute is None:
                raise RuntimeError("Connection must implement execute()")
            try:
                await execute(query, list(owned), self._worker_id)
            except POSTGRES_EXCEPTIONS as exc:
                logger.error("run_owner_heartbeat_failed: %s", exc)
                return 0
        finally:
            if owned_conn:
                self._release(conn)

        active = 0
        async with self._lock:
            for run_id in owned:
                info = self._owned.get(run_id)
                if info is not None:
                    info["last_heartbeat_at"] = datetime.now(UTC)
                    active += 1
        return active

    async def _acquire_conn(self) -> tuple[Any, bool]:
        acquire = getattr(self._pool, "acquire", None)
        if acquire is None:
            raise RuntimeError("RunOwnerRegistry requires a pool with an acquire() method")
        result = acquire()
        if hasattr(result, "__aenter__"):
            conn = await result.__aenter__()
            return conn, False
        if asyncio.iscoroutine(result):
            conn = await result
            return conn, True
        return result, True

    def _release(self, conn: Any) -> None:
        release = getattr(conn, "release", None)
        if release is not None:
            release()
            return
        close = getattr(conn, "close", None)
        if close is not None:
            close()


__all__ = ["RunOwnerRegistry"]
