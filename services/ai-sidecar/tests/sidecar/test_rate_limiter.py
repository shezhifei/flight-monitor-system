"""Tests for AI rate limiter behavior."""

from __future__ import annotations

import asyncio
import threading
import time
from concurrent.futures import ThreadPoolExecutor

import pytest

from src.infrastructure.ai import rate_limiter as rate_limiter_module
from src.infrastructure.ai.rate_limiter import MultiEntityRateLimiter


def test_get_entity_lock_creates_one_lock_when_threads_race(monkeypatch):
    """Concurrent callers for the same entity must share one lock object."""
    limiter = MultiEntityRateLimiter()
    created_locks = []
    creation_guard = threading.Lock()

    def slow_lock_factory():
        lock = object()
        with creation_guard:
            created_locks.append(lock)
        time.sleep(0.05)
        return lock

    monkeypatch.setattr(rate_limiter_module.asyncio, "Lock", slow_lock_factory)

    with ThreadPoolExecutor(max_workers=8) as executor:
        locks = list(executor.map(lambda _: limiter._get_entity_lock("entity-a"), range(8)))

    assert len(created_locks) == 1
    assert len({id(lock) for lock in locks}) == 1
    assert limiter._entity_locks["entity-a"] is locks[0]


@pytest.mark.asyncio
async def test_get_entity_lock_reuses_lock_for_concurrent_coroutines():
    """Coroutine callers for the same entity must also observe one shared lock."""
    limiter = MultiEntityRateLimiter()

    async def get_lock():
        await asyncio.sleep(0)
        return limiter._get_entity_lock("entity-a")

    locks = await asyncio.gather(*(get_lock() for _ in range(8)))

    assert len({id(lock) for lock in locks}) == 1


@pytest.mark.asyncio
async def test_get_limiter_reuses_existing_limiter_behavior():
    """Existing get_limiter behavior remains unchanged for repeated entity access."""
    limiter = MultiEntityRateLimiter()

    first = await limiter.get_limiter("entity-a")
    second = await limiter.get_limiter("entity-a")

    assert first is second
