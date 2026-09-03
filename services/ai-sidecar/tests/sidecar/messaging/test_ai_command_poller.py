"""Tests for the ``AiCommandPoller`` SKIP LOCKED consumer.

These tests use a fake asyncpg-style connection / pool. They verify the
poller: claims rows in ``created_at`` order, marks commands completed
and failed, runs the dispatcher loop until shutdown, and reacts to
poller fetch errors without aborting.
"""

from __future__ import annotations

import asyncio
import json

import pytest

from src.infrastructure.ai.messaging import AiCommandPoller
from src.infrastructure.common.exceptions import POSTGRES_EXCEPTIONS


class _FakeAcquireCtx:
    def __init__(self, conn):
        self._conn = conn

    async def __aenter__(self):
        return self._conn

    async def __aexit__(self, exc_type, exc, tb):
        return False


class _FakeConn:
    def __init__(self, *, fetch_rows=None, fetch_error=None, execute_log=None):
        self._fetch_rows = fetch_rows or []
        self._fetch_error = fetch_error
        self._execute_log = execute_log if execute_log is not None else []
        self.fetch_calls: list[tuple] = []
        self.execute_calls: list[tuple] = []

    async def fetch(self, query, *args):
        self.fetch_calls.append((query, args))
        if self._fetch_error is not None:
            raise self._fetch_error
        return self._fetch_rows

    async def execute(self, query, *args):
        self.execute_calls.append((query, args))
        self._execute_log.append((query, args))
        return "UPDATE 1"


class _FakePool:
    def __init__(self, conn):
        self._conn = conn

    def acquire(self):
        return _FakeAcquireCtx(self._conn)


def _cmd(
    command_id: str,
    *,
    run_id: str = "run-1",
    command_type: str = "tool_lease",
    tool_call_pk: str = "tpc-1",
    command_sequence: int = 1,
    payload: dict | None = None,
    status: str = "pending",
    lease_owner: str | None = None,
):
    return {
        "command_id": command_id,
        "run_id": run_id,
        "command_type": command_type,
        "command_sequence": command_sequence,
        "tool_call_pk": tool_call_pk,
        "payload": payload if payload is not None else {"code": "ok"},
        "status": status,
        "lease_owner": lease_owner,
        "lease_expires_at": None,
        "created_at": "2026-06-30T00:00:00Z",
        "processed_at": None,
    }


@pytest.mark.asyncio
async def test_fetch_pending_returns_ordered_by_created_at() -> None:
    rows = [
        _cmd("c-1", command_sequence=2),
        _cmd("c-2", command_sequence=3),
        _cmd("c-3", command_sequence=1),
    ]
    conn = _FakeConn(fetch_rows=rows)
    pool = _FakePool(conn)
    poller = AiCommandPoller(pool, owner="worker-1", batch_size=10)

    leased = await poller.fetch_pending()

    assert [r["command_id"] for r in leased] == ["c-1", "c-2", "c-3"]
    assert len(conn.fetch_calls) == 1
    query, args = conn.fetch_calls[0]
    assert "FOR UPDATE SKIP LOCKED" in query
    assert "ORDER BY created_at" in query
    assert args[0] == 10
    assert args[1] == "worker-1"
    assert "RETURNING" in query


@pytest.mark.asyncio
async def test_fetch_pending_decodes_jsonb_string_payload() -> None:
    rows = [
        {
            **_cmd("c-1"),
            "payload": json.dumps({"code": "TOOL_ACTOR_PERMISSION_DENIED", "message": "nope"}),
        }
    ]
    conn = _FakeConn(fetch_rows=rows)
    poller = AiCommandPoller(_FakePool(conn), owner="worker-1")

    leased = await poller.fetch_pending()
    assert leased[0]["payload"] == {
        "code": "TOOL_ACTOR_PERMISSION_DENIED",
        "message": "nope",
    }


@pytest.mark.asyncio
async def test_mark_completed_updates_status() -> None:
    conn = _FakeConn()
    poller = AiCommandPoller(_FakePool(conn), owner="worker-1")

    await poller.mark_completed("c-1")

    assert len(conn.execute_calls) == 1
    query, args = conn.execute_calls[0]
    assert "UPDATE ai_runtime_commands" in query
    assert "SET status = 'completed'" in query
    assert "processed_at = now()" in query
    assert "lease_expires_at = NULL" in query
    assert "WHERE command_id = $1" in query
    assert "AND status IN ('leased', 'pending')" in query
    assert args == ("c-1",)


@pytest.mark.asyncio
async def test_mark_failed_records_error_message() -> None:
    conn = _FakeConn()
    poller = AiCommandPoller(_FakePool(conn), owner="worker-1")

    await poller.mark_failed("c-1", "boom")

    assert len(conn.execute_calls) == 1
    query, args = conn.execute_calls[0]
    assert "SET status = 'failed'" in query
    assert "jsonb_build_object('error', $2::text)" in query
    assert args == ("c-1", "boom")


@pytest.mark.asyncio
async def test_mark_failed_handles_empty_error_string() -> None:
    conn = _FakeConn()
    poller = AiCommandPoller(_FakePool(conn), owner="worker-1")

    await poller.mark_failed("c-1", "")

    _query, args = conn.execute_calls[0]
    assert args == ("c-1", "")


@pytest.mark.asyncio
async def test_polling_loop_dispatches_and_continues() -> None:
    dispatched: list[str] = []
    rows = [_cmd("c-1"), _cmd("c-2"), _cmd("c-3")]
    remaining = list(rows)

    async def dispatcher(cmd):
        dispatched.append(cmd["command_id"])
        await asyncio.sleep(0)
        remaining[:] = [c for c in remaining if c.get("command_id") != cmd["command_id"]]

    async def fetch_rows(owner=None, batch_size=None):
        return list(remaining)

    conn = _FakeConn()
    poller = AiCommandPoller(
        _FakePool(conn),
        owner="worker-1",
        interval_seconds=0.01,
        dispatcher=dispatcher,
    )
    poller.fetch_pending = fetch_rows  # type: ignore[assignment]
    poller.mark_completed = lambda cid: asyncio.sleep(0)  # type: ignore[assignment]
    poller.mark_failed = lambda cid, err: asyncio.sleep(0)  # type: ignore[assignment]

    task = asyncio.create_task(poller.run())
    await asyncio.sleep(0.1)
    poller.request_shutdown()
    await asyncio.wait_for(task, timeout=2.0)

    assert sorted(dispatched) == ["c-1", "c-2", "c-3"]


@pytest.mark.asyncio
async def test_polling_loop_marks_completed_on_success_and_failed_on_dispatch_error() -> None:
    succeeded: list[str] = []
    failed: list[tuple[str, str]] = []
    remaining = [_cmd("c-1"), _cmd("c-2")]

    async def dispatcher(cmd):
        if cmd["command_id"] == "c-1":
            raise RuntimeError("kaboom")
        succeeded.append(cmd["command_id"])

    async def fetch_rows(owner=None, batch_size=None):
        return list(remaining)

    async def fake_mark_completed(cid):
        succeeded.append(f"completed:{cid}")
        remaining[:] = [c for c in remaining if c.get("command_id") != cid]

    async def fake_mark_failed(cid, err):
        failed.append((cid, err))
        remaining[:] = [c for c in remaining if c.get("command_id") != cid]

    conn = _FakeConn()
    poller = AiCommandPoller(
        _FakePool(conn),
        owner="worker-1",
        interval_seconds=0.01,
        dispatcher=dispatcher,
    )
    poller.fetch_pending = fetch_rows  # type: ignore[assignment]
    poller.mark_completed = fake_mark_completed  # type: ignore[assignment]
    poller.mark_failed = fake_mark_failed  # type: ignore[assignment]

    task = asyncio.create_task(poller.run())
    await asyncio.sleep(0.1)
    poller.request_shutdown()
    await asyncio.wait_for(task, timeout=2.0)

    assert ("c-1", "kaboom") in failed
    assert "completed:c-2" in succeeded


@pytest.mark.asyncio
async def test_polling_loop_stops_on_shutdown_signal() -> None:
    async def dispatcher(cmd):
        pass

    async def fetch_rows(owner=None, batch_size=None):
        return []

    poller = AiCommandPoller(
        _FakePool(_FakeConn()),
        owner="worker-1",
        interval_seconds=0.01,
        dispatcher=dispatcher,
    )
    poller.fetch_pending = fetch_rows  # type: ignore[assignment]

    task = asyncio.create_task(poller.run())
    await asyncio.sleep(0.02)
    assert not task.done()
    poller.request_shutdown()
    await asyncio.wait_for(task, timeout=1.0)
    assert poller.is_shutdown is True


@pytest.mark.asyncio
async def test_polling_loop_continues_after_fetch_error() -> None:
    attempts = {"count": 0}

    async def fetch_rows(owner=None, batch_size=None):
        attempts["count"] += 1
        if attempts["count"] == 1:
            raise ConnectionError("temporary")
        return []

    async def dispatcher(cmd):
        pass

    poller = AiCommandPoller(
        _FakePool(_FakeConn()),
        owner="worker-1",
        interval_seconds=0.01,
        dispatcher=dispatcher,
    )
    poller.fetch_pending = fetch_rows  # type: ignore[assignment]

    task = asyncio.create_task(poller.run())
    await asyncio.sleep(0.05)
    poller.request_shutdown()
    await asyncio.wait_for(task, timeout=1.0)
    assert attempts["count"] >= 2
    assert ConnectionError in POSTGRES_EXCEPTIONS


@pytest.mark.asyncio
async def test_run_requires_dispatcher() -> None:
    poller = AiCommandPoller(_FakePool(_FakeConn()), owner="worker-1")
    with pytest.raises(RuntimeError, match="dispatcher"):
        await poller.run()


def test_default_constants_are_stable() -> None:
    from src.infrastructure.ai.messaging.ai_command_poller import (
        DEFAULT_FETCH_BATCH_SIZE,
        DEFAULT_LEASE_TTL_SECONDS,
        DEFAULT_POLL_INTERVAL_SECONDS,
    )

    assert DEFAULT_POLL_INTERVAL_SECONDS == 0.2
    assert DEFAULT_FETCH_BATCH_SIZE == 10
    assert DEFAULT_LEASE_TTL_SECONDS == 30
