import asyncio
import threading
from concurrent.futures import ThreadPoolExecutor

import pytest

from src.infrastructure.database import redis_connection
from src.infrastructure.database.redis_connection import RedisConfig, RedisConnectionManager


@pytest.fixture(autouse=True)
def reset_redis_connection_manager():
    RedisConnectionManager._instance = None
    yield
    instance = RedisConnectionManager._instance
    if instance is not None:
        instance._initialized = False
        instance._client = None
        instance._pool = None
    RedisConnectionManager._instance = None


def test_concurrent_initialize_only_creates_one_pool(monkeypatch):
    workers = 8
    start = threading.Event()
    release_first_pool = threading.Event()
    first_pool_entered = threading.Event()
    call_lock = threading.Lock()

    class BlockingPool:
        calls = 0

        def __init__(self, **kwargs):
            with call_lock:
                type(self).calls += 1
                call_number = type(self).calls

            if call_number == 1:
                first_pool_entered.set()
                assert release_first_pool.wait(timeout=5)

            self.kwargs = kwargs

    class FakeRedis:
        calls = 0

        def __init__(self, connection_pool):
            type(self).calls += 1
            self.connection_pool = connection_pool

        async def close(self):
            return None

    monkeypatch.setattr(redis_connection, "AsyncConnectionPool", BlockingPool)
    monkeypatch.setattr(redis_connection.redis_async, "Redis", FakeRedis)

    manager = RedisConnectionManager()
    config = RedisConfig(host="redis-test")

    def initialize_in_thread():
        assert start.wait(timeout=5)
        asyncio.run(manager.initialize(config))

    with ThreadPoolExecutor(max_workers=workers) as executor:
        futures = [executor.submit(initialize_in_thread) for _ in range(workers)]
        start.set()
        assert first_pool_entered.wait(timeout=5)
        release_first_pool.set()
        for future in futures:
            future.result(timeout=5)

    assert BlockingPool.calls == 1
    assert FakeRedis.calls == 1
    assert manager.is_initialized is True


@pytest.mark.asyncio
async def test_initialize_failure_does_not_mark_initialized_and_can_retry(monkeypatch):
    class FlakyPool:
        calls = 0

        def __init__(self, **kwargs):
            type(self).calls += 1
            if type(self).calls == 1:
                raise RuntimeError("pool unavailable")
            self.kwargs = kwargs

    class FakeRedis:
        calls = 0

        def __init__(self, connection_pool):
            type(self).calls += 1
            self.connection_pool = connection_pool

    monkeypatch.setattr(redis_connection, "AsyncConnectionPool", FlakyPool)
    monkeypatch.setattr(redis_connection.redis_async, "Redis", FakeRedis)

    manager = RedisConnectionManager()
    config = RedisConfig(host="redis-test")

    with pytest.raises(RuntimeError, match="pool unavailable"):
        await manager.initialize(config)

    assert manager.is_initialized is False

    await manager.initialize(config)

    assert FlakyPool.calls == 2
    assert FakeRedis.calls == 1
    assert manager.is_initialized is True


@pytest.mark.asyncio
async def test_initialize_fast_path_does_not_recreate_pool(monkeypatch):
    class FakePool:
        calls = 0

        def __init__(self, **kwargs):
            type(self).calls += 1
            self.kwargs = kwargs

    class FakeRedis:
        calls = 0

        def __init__(self, connection_pool):
            type(self).calls += 1
            self.connection_pool = connection_pool

    monkeypatch.setattr(redis_connection, "AsyncConnectionPool", FakePool)
    monkeypatch.setattr(redis_connection.redis_async, "Redis", FakeRedis)

    manager = RedisConnectionManager()
    config = RedisConfig(host="redis-test")

    await manager.initialize(config)
    await manager.initialize(config)

    assert FakePool.calls == 1
    assert FakeRedis.calls == 1
    assert manager.is_initialized is True
