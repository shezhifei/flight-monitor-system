from __future__ import annotations

import asyncio
import threading
import time
from concurrent.futures import ThreadPoolExecutor

import pytest

from src.infrastructure.ai import circuit_breaker as circuit_breaker_module
from src.infrastructure.ai.circuit_breaker import CircuitBreaker, CircuitState


@pytest.mark.asyncio
async def test_open_circuit_raises_dedicated_exception() -> None:
    breaker = CircuitBreaker(failure_threshold=1, recovery_timeout=60.0)
    breaker.state = CircuitState.OPEN
    breaker.last_failure_time = time.time()

    assert hasattr(circuit_breaker_module, "CircuitBreakerOpenError")

    async def should_not_run() -> str:
        return "unexpected"

    with pytest.raises(circuit_breaker_module.CircuitBreakerOpenError):
        await breaker.execute(should_not_run)


def test_half_open_recovery_probe_is_single_flight_across_threads() -> None:
    breaker = CircuitBreaker(failure_threshold=1, recovery_timeout=0.01)
    breaker.state = CircuitState.OPEN
    breaker.last_failure_time = time.time() - 1.0

    worker_count = 16
    barrier = threading.Barrier(worker_count)
    started_probe_count = 0
    started_probe_count_lock = threading.Lock()

    async def probe() -> str:
        nonlocal started_probe_count
        with started_probe_count_lock:
            started_probe_count += 1
        await asyncio.sleep(0.2)
        return "ok"

    def call_breaker() -> object:
        barrier.wait(timeout=5)
        try:
            return asyncio.run(breaker.execute(probe))
        except Exception as exc:
            return exc

    with ThreadPoolExecutor(max_workers=worker_count) as executor:
        results = list(executor.map(lambda _: call_breaker(), range(worker_count)))

    open_error_type = getattr(circuit_breaker_module, "CircuitBreakerOpenError", None)

    assert started_probe_count == 1
    assert results.count("ok") == 1
    assert open_error_type is not None
    assert sum(isinstance(result, open_error_type) for result in results) == (worker_count - 1)
