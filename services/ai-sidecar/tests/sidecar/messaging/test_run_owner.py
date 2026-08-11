"""Tests for the multi-worker run ownership registry."""

from __future__ import annotations

import pytest

from src.infrastructure.ai.messaging.run_owner import RunOwnerRegistry


class _FakeAcquireCtx:
    def __init__(self, conn):
        self._conn = conn

    async def __aenter__(self):
        return self._conn

    async def __aexit__(self, exc_type, exc, tb):
        return False


class _FakeConn:
    def __init__(self, *, fetch_rows=None, execute_result="UPDATE 1"):
        self._fetch_rows = fetch_rows or []
        self._execute_result = execute_result
        self.fetch_calls: list[tuple] = []
        self.execute_calls: list[tuple] = []

    async def fetch(self, query, *args):
        self.fetch_calls.append((query, args))
        return list(self._fetch_rows)

    async def execute(self, query, *args):
        self.execute_calls.append((query, args))
        return self._execute_result


class _FakePool:
    def __init__(self, conn):
        self._conn = conn

    def acquire(self):
        return _FakeAcquireCtx(self._conn)


@pytest.mark.asyncio
async def test_claim_returns_true_when_row_updated() -> None:
    conn = _FakeConn(fetch_rows=[{"command_id": "c-1"}])
    registry = RunOwnerRegistry("worker-1", _FakePool(conn), lease_ttl_seconds=30)

    claimed = await registry.claim("run-1")

    assert claimed is True
    assert registry.is_owner("run-1")
    assert "run-1" in registry.owned_run_ids
    assert len(conn.fetch_calls) == 1
    query, args = conn.fetch_calls[0]
    assert "UPDATE ai_runtime_commands" in query
    assert "run_owner_lock = $1" in query
    assert args[0] == "worker-1"
    assert args[1] == "run-1"


@pytest.mark.asyncio
async def test_claim_returns_true_when_already_owned_in_memory() -> None:
    conn = _FakeConn(fetch_rows=[{"command_id": "c-1"}])
    registry = RunOwnerRegistry("worker-1", _FakePool(conn), lease_ttl_seconds=30)

    assert await registry.claim("run-1") is True
    assert await registry.claim("run-1") is True
    assert len(conn.fetch_calls) == 1


@pytest.mark.asyncio
async def test_claim_returns_false_when_owned_by_other_worker() -> None:
    conn = _FakeConn(fetch_rows=[])
    verify_conn = _FakeConn(fetch_rows=[{"run_owner_lock": "worker-2"}])

    class _SplitPool:
        def __init__(self, claim_conn, verify_conn):
            self._claim_conn = claim_conn
            self._verify_conn = verify_conn
            self._index = 0

        def acquire(self):
            conn = self._claim_conn if self._index == 0 else self._verify_conn
            self._index += 1
            return _FakeAcquireCtx(conn)

    registry = RunOwnerRegistry("worker-1", _SplitPool(conn, verify_conn), lease_ttl_seconds=30)

    claimed = await registry.claim("run-1")

    assert claimed is False
    assert not registry.is_owner("run-1")


@pytest.mark.asyncio
async def test_release_removes_in_memory_ownership() -> None:
    conn = _FakeConn(fetch_rows=[{"command_id": "c-1"}])
    registry = RunOwnerRegistry("worker-1", _FakePool(conn), lease_ttl_seconds=30)

    await registry.claim("run-1")
    registry.release("run-1")
    assert not registry.is_owner("run-1")


@pytest.mark.asyncio
async def test_heartbeat_all_refreshes_leases_for_owned_runs() -> None:
    conn = _FakeConn(fetch_rows=[{"command_id": "c-1"}])
    registry = RunOwnerRegistry("worker-1", _FakePool(conn), lease_ttl_seconds=30)

    await registry.claim("run-1")
    await registry.claim("run-2")

    heartbeat_conn = _FakeConn()

    class _SplitPool:
        def __init__(self, claim_conn, heartbeat_conn):
            self._claim_conn = claim_conn
            self._heartbeat_conn = heartbeat_conn
            self._index = 0

        def acquire(self):
            conn = self._claim_conn if self._index < 2 else self._heartbeat_conn
            self._index += 1
            return _FakeAcquireCtx(conn)

    registry = RunOwnerRegistry("worker-1", _SplitPool(conn, heartbeat_conn), lease_ttl_seconds=30)
    await registry.claim("run-1")
    await registry.claim("run-2")

    active = await registry.heartbeat_all()

    assert active == 2
    assert len(heartbeat_conn.execute_calls) == 1
    query, args = heartbeat_conn.execute_calls[0]
    assert "UPDATE ai_runtime_commands" in query
    assert "run_id = ANY($1::text[])" in query
    assert set(args[0]) == {"run-1", "run-2"}
    assert args[1] == "worker-1"


@pytest.mark.asyncio
async def test_heartbeat_all_returns_zero_when_no_owned_runs() -> None:
    registry = RunOwnerRegistry("worker-1", _FakePool(_FakeConn()), lease_ttl_seconds=30)
    assert await registry.heartbeat_all() == 0


@pytest.mark.asyncio
async def test_worker_id_exposed() -> None:
    registry = RunOwnerRegistry("worker-42", _FakePool(_FakeConn()), lease_ttl_seconds=30)
    assert registry.worker_id == "worker-42"
