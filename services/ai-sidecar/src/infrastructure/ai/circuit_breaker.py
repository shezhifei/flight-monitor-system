import inspect
import threading
import time
from collections.abc import Callable
from enum import Enum
from typing import Any

from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class CircuitState(Enum):
    CLOSED = "closed"  # 正常状态
    OPEN = "open"  # 熔断状态
    HALF_OPEN = "half_open"  # 半开状态


class CircuitBreakerOpenError(Exception):
    """Raised when a circuit breaker rejects a call while open."""


class CircuitBreaker:
    def __init__(self, failure_threshold: int = 5, recovery_timeout: float = 30.0):
        self.failure_threshold = failure_threshold
        self.recovery_timeout = recovery_timeout
        self._lock = threading.RLock()
        self._failure_count = 0
        self._last_failure_time = 0.0
        self._state = CircuitState.CLOSED
        self._half_open_probe_in_flight = False

    @property
    def failure_count(self) -> int:
        with self._lock:
            return self._failure_count

    @failure_count.setter
    def failure_count(self, value: int) -> None:
        with self._lock:
            self._failure_count = value

    @property
    def last_failure_time(self) -> float:
        with self._lock:
            return self._last_failure_time

    @last_failure_time.setter
    def last_failure_time(self, value: float) -> None:
        with self._lock:
            self._last_failure_time = value

    @property
    def state(self) -> CircuitState:
        with self._lock:
            return self._state

    @state.setter
    def state(self, value: CircuitState) -> None:
        if not isinstance(value, CircuitState):
            raise ValueError("state must be a CircuitState")
        with self._lock:
            self._state = value
            self._half_open_probe_in_flight = False

    async def execute(self, func: Callable, *args, **kwargs) -> Any:
        half_open_probe = self._before_call()

        try:
            result = func(*args, **kwargs)
            if inspect.isawaitable(result):
                result = await result
            self._record_success(half_open_probe)
            return result
        except Exception as error:
            self._record_failure(half_open_probe)
            logger.warning("circuit_breaker_exec_failed", exc_info=error)
            raise

    def _before_call(self) -> bool:
        with self._lock:
            if self._state == CircuitState.OPEN:
                if time.time() - self._last_failure_time > self.recovery_timeout:
                    self._state = CircuitState.HALF_OPEN
                    self._half_open_probe_in_flight = True
                    return True
                raise CircuitBreakerOpenError("Circuit breaker is open")

            if self._state == CircuitState.HALF_OPEN:
                if not self._half_open_probe_in_flight:
                    self._half_open_probe_in_flight = True
                    return True
                raise CircuitBreakerOpenError("Circuit breaker is half-open")

            return False

    def _record_success(self, half_open_probe: bool) -> None:
        with self._lock:
            if half_open_probe or self._state == CircuitState.HALF_OPEN:
                self._state = CircuitState.CLOSED
                self._failure_count = 0
                self._half_open_probe_in_flight = False

    def _record_failure(self, half_open_probe: bool) -> None:
        with self._lock:
            self._failure_count += 1
            self._last_failure_time = time.time()
            if half_open_probe or self._failure_count >= self.failure_threshold:
                self._state = CircuitState.OPEN
                self._half_open_probe_in_flight = False
