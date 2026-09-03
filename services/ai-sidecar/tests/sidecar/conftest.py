"""pytest configuration for ai-sidecar tests.

Adds services/ai-sidecar to sys.path so that `from src.infrastructure...`
imports resolve correctly regardless of the working directory (repo root,
services/ai-sidecar, or any other directory).
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

import pytest
import pytest_asyncio

# services/ai-sidecar is two levels up from this conftest.py
_SIDECAR_ROOT = str(Path(__file__).resolve().parents[2])
if _SIDECAR_ROOT not in sys.path:
    sys.path.insert(0, _SIDECAR_ROOT)

# The sidecar imports the shared ``config`` package from the repo root.
_REPO_ROOT = str(Path(__file__).resolve().parents[4])
if _REPO_ROOT not in sys.path:
    sys.path.insert(0, _REPO_ROOT)

# Also add the test directory itself so test-to-test imports resolve
_TEST_DIR = str(Path(__file__).resolve().parent)
if _TEST_DIR not in sys.path:
    sys.path.insert(0, _TEST_DIR)

# Migrations that create the sidecar-owned tables used by DB-backed tests.
# 123: ai_eval_jobs / ai_eval_spans / ai_eval_metrics_summary (Phase E3).
# Applied lazily and idempotently by the ``db_pool`` fixture.
_SIDECAR_MIGRATIONS_DIR = Path(_SIDECAR_ROOT) / "migrations"


def _default_test_database_url() -> str:
    host = os.environ.get("DB_HOST", "localhost")
    port = os.environ.get("DB_PORT", "5432")
    user = os.environ.get("DB_USER", "postgres")
    password = os.environ.get("DB_PASSWORD", "password")
    name = "flight_monitor_test"
    return f"postgresql://{user}:{password}@{host}:{port}/{name}"


@pytest_asyncio.fixture
async def db_pool():
    """Asyncpg pool against the FMS test database.

    Resolves ``TEST_DATABASE_URL`` (or ``DATABASE_URL``), falling back to
    ``flight_monitor_test`` on localhost with dev credentials. Applies the
    sidecar migrations (eval persistence) if the tables are missing, then
    yields an asyncpg pool. Tests that need a live Postgres are skipped
    (not failed) when no database is reachable — mirroring the Rust
    integration-test convention that requires ``TEST_DATABASE_URL``.
    """
    import asyncpg

    url = os.environ.get("TEST_DATABASE_URL") or os.environ.get("DATABASE_URL") or _default_test_database_url()
    try:
        pool = await asyncpg.create_pool(url, min_size=1, max_size=4)
    except Exception as exc:  # noqa: BLE001 - DB may be absent in CI
        pytest.skip(f"Postgres test database unavailable: {exc}")

    try:
        async with pool.acquire() as conn:
            exists = await conn.fetchval("SELECT to_regclass('public.ai_eval_jobs')")
            if exists is None:
                migration = _SIDECAR_MIGRATIONS_DIR / "123_ai_eval_jobs_persistent.sql"
                await conn.execute(migration.read_text(encoding="utf-8"))
            await conn.execute("TRUNCATE ai_eval_jobs, ai_eval_spans, ai_eval_metrics_summary RESTART IDENTITY CASCADE")
        yield pool
    finally:
        await pool.close()
