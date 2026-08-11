"""
速率限制器

提供 RPM (Requests Per Minute) 和 TPM (Tokens Per Minute) 速率限制。
"""

import asyncio
import random
import threading
import time
from collections import deque
from dataclasses import dataclass

from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


@dataclass
class RateLimitConfig:
    """速率限制配置"""

    rpm: int = 60  # 每分钟请求数
    tpm: int = 100000  # 每分钟 Token 数
    burst_multiplier: float = 1.5  # 突发倍数
    window_seconds: int = 60  # 滑动窗口大小（秒）


@dataclass
class RateLimitStatus:
    """速率限制状态"""

    rpm_used: int
    rpm_limit: int
    tpm_used: int
    tpm_limit: int
    rpm_remaining: int
    tpm_remaining: int
    reset_at: float  # Unix timestamp

    @property
    def rpm_percentage(self) -> float:
        return (self.rpm_used / self.rpm_limit * 100) if self.rpm_limit > 0 else 0

    @property
    def tpm_percentage(self) -> float:
        return (self.tpm_used / self.tpm_limit * 100) if self.tpm_limit > 0 else 0

    def to_dict(self) -> dict:
        return {
            "rpm_used": self.rpm_used,
            "rpm_limit": self.rpm_limit,
            "tpm_used": self.tpm_used,
            "tpm_limit": self.tpm_limit,
            "rpm_remaining": self.rpm_remaining,
            "tpm_remaining": self.tpm_remaining,
            "rpm_percentage": round(self.rpm_percentage, 1),
            "tpm_percentage": round(self.tpm_percentage, 1),
            "reset_at": self.reset_at,
        }


class RateLimiter:
    """
    RPM/TPM 速率限制器

    使用滑动窗口算法控制 API 调用速率，支持 RPM 和 TPM 两种限制。

    Example:
        ```python
        limiter = RateLimiter(rpm=60, tpm=100000)

        # 在调用 API 前等待配额
        await limiter.acquire(estimated_tokens=1000)

        # 调用 API...

        # 记录实际使用的 token
        await limiter.record_tokens(actual_tokens=850)
        ```
    """

    def __init__(self, rpm: int = 60, tpm: int = 100000, window_seconds: int = 60):
        self.rpm = rpm
        self.tpm = tpm
        self.window_seconds = window_seconds

        # 使用 deque 存储时间戳和 token 使用
        self._request_times: deque = deque()
        self._token_records: deque = deque()  # (timestamp, tokens)
        self._pending_estimates: deque = deque()

        self._lock = asyncio.Lock()
        self._pending_tokens = 0  # 尚未确认的预估 token

        logger.info(f"RateLimiter initialized: RPM={rpm}, TPM={tpm}")

    def _cleanup_old_records(self, current_time: float) -> None:
        """清理过期的记录"""
        cutoff = current_time - self.window_seconds

        # 清理请求时间
        while self._request_times and self._request_times[0] < cutoff:
            self._request_times.popleft()

        # 清理 token 记录
        while self._token_records and self._token_records[0][0] < cutoff:
            self._token_records.popleft()

    def _get_current_usage(self) -> tuple[int, int]:
        """获取当前使用量 (requests, tokens)"""
        current_time = time.time()
        self._cleanup_old_records(current_time)

        request_count = len(self._request_times)
        token_count = sum(tokens for _, tokens in self._token_records)

        return request_count, token_count + self._pending_tokens

    async def acquire(self, estimated_tokens: int = 1000) -> None:
        """
        获取执行配额

        如果超出限制，会等待直到有足够配额。

        Args:
            estimated_tokens: 预估的 token 使用量
        """
        timeout_start = time.time()
        timeout_seconds = 60  # 最大等待时间

        while True:
            async with self._lock:
                # 检查超时
                if time.time() - timeout_start > timeout_seconds:
                    logger.error(f"Rate limiter acquire timed out after {timeout_seconds}s")
                    raise TimeoutError("Rate limit acquisition timed out")

                request_count, token_count = self._get_current_usage()

                # 检查是否可以执行
                can_proceed = True
                wait_time = 0.0

                now = time.time()

                if request_count >= self.rpm:
                    if self._request_times:
                        wait_time = max(
                            wait_time,
                            self._request_times[0] + self.window_seconds - now,
                        )
                    can_proceed = False

                if token_count + estimated_tokens > self.tpm:
                    if self._token_records:
                        tokens_needed = token_count + estimated_tokens - self.tpm
                        tokens_freed = 0
                        for ts, tokens in self._token_records:
                            tokens_freed += tokens
                            if tokens_freed >= tokens_needed:
                                wait_time = max(wait_time, ts + self.window_seconds - now)
                                break
                    can_proceed = False

                if can_proceed:
                    self._request_times.append(now)
                    self._pending_tokens += estimated_tokens
                    self._pending_estimates.append(estimated_tokens)
                    logger.debug(
                        f"Rate limit acquired: RPM {request_count + 1}/{self.rpm}, "
                        f"TPM ~{token_count + estimated_tokens}/{self.tpm}"
                    )
                    return

                wait_time = max(0.1, min(wait_time, 5.0))  # 最少0.1s，最多5s
                wait_time *= 0.8 + random.random() * 0.4  # ±20% 抖动，避免惊群效应

            logger.debug(f"Rate limit reached, waiting {wait_time:.2f}s")
            await asyncio.sleep(wait_time)

    async def record_tokens(self, actual_tokens: int, estimated_tokens: int | None = None) -> None:
        """
        记录实际使用的 token 数

        Args:
            actual_tokens: 实际使用的 token 数
            estimated_tokens: 对应请求的预估 token（可选）
        """
        async with self._lock:
            current_time = time.time()
            self._token_records.append((current_time, actual_tokens))

            estimate_to_release = 0
            if estimated_tokens is not None:
                candidate = max(0, int(estimated_tokens))
                try:
                    self._pending_estimates.remove(candidate)
                    estimate_to_release = candidate
                except ValueError:
                    if self._pending_estimates:
                        estimate_to_release = self._pending_estimates.popleft()
            elif self._pending_estimates:
                estimate_to_release = self._pending_estimates.popleft()

            self._pending_tokens = max(0, self._pending_tokens - estimate_to_release)

            logger.debug(
                f"Recorded {actual_tokens} tokens "
                f"(released estimate={estimate_to_release}, pending={self._pending_tokens})"
            )

    def get_status(self) -> RateLimitStatus:
        """获取当前速率限制状态"""
        request_count, token_count = self._get_current_usage()
        current_time = time.time()

        return RateLimitStatus(
            rpm_used=request_count,
            rpm_limit=self.rpm,
            tpm_used=token_count,
            tpm_limit=self.tpm,
            rpm_remaining=max(0, self.rpm - request_count),
            tpm_remaining=max(0, self.tpm - token_count),
            reset_at=current_time + self.window_seconds,
        )

    async def wait_for_capacity(self, min_tokens: int = 1000) -> None:
        """
        等待直到有足够的容量

        Args:
            min_tokens: 需要的最小 token 容量
        """
        while True:
            request_count, token_count = self._get_current_usage()

            if request_count < self.rpm and token_count + min_tokens <= self.tpm:
                return

            await asyncio.sleep(0.5)


class MultiEntityRateLimiter:
    """
    多实体速率限制器

    为每个 AI 实体维护独立的速率限制。
    """

    def __init__(self, default_config: RateLimitConfig | None = None, max_entities: int = 2048):
        self._limiters: dict[str, RateLimiter] = {}
        self._default_config = default_config or RateLimitConfig()
        self._registry_lock = asyncio.Lock()  # 保护 _limiters 字典的全局锁
        self._entity_locks: dict[str, asyncio.Lock] = {}  # per-entity 锁
        self._entity_locks_guard = threading.Lock()
        self._max_entities = max(64, int(max_entities))
        self._entity_last_access: dict[str, float] = {}

    def _get_entity_lock(self, entity_id: str) -> asyncio.Lock:
        """获取或创建 per-entity 锁。"""
        with self._entity_locks_guard:
            if entity_id not in self._entity_locks:
                self._entity_locks[entity_id] = asyncio.Lock()
            return self._entity_locks[entity_id]

    def _prune_if_needed_unlocked(self) -> None:
        if len(self._limiters) < self._max_entities:
            return

        if not self._entity_last_access:
            return

        stale_entity = min(
            self._entity_last_access.items(),
            key=lambda item: item[1],
        )[0]
        self._limiters.pop(stale_entity, None)
        self._entity_last_access.pop(stale_entity, None)
        with self._entity_locks_guard:
            self._entity_locks.pop(stale_entity, None)

    async def get_limiter(self, entity_id: str) -> RateLimiter:
        """获取指定实体的速率限制器"""
        async with self._registry_lock:
            if entity_id not in self._limiters:
                self._prune_if_needed_unlocked()
                self._limiters[entity_id] = RateLimiter(
                    rpm=self._default_config.rpm,
                    tpm=self._default_config.tpm,
                    window_seconds=self._default_config.window_seconds,
                )
            self._entity_last_access[entity_id] = time.time()
            entity_lock = self._get_entity_lock(entity_id)

        async with entity_lock:
            return self._limiters[entity_id]

    async def configure_entity(self, entity_id: str, rpm: int | None = None, tpm: int | None = None) -> None:
        """为特定实体配置速率限制"""
        config = RateLimitConfig(rpm=rpm or self._default_config.rpm, tpm=tpm or self._default_config.tpm)
        async with self._registry_lock:
            if entity_id not in self._limiters:
                self._prune_if_needed_unlocked()
            self._limiters[entity_id] = RateLimiter(
                rpm=config.rpm, tpm=config.tpm, window_seconds=config.window_seconds
            )
            self._entity_last_access[entity_id] = time.time()

    def get_all_status(self) -> dict[str, RateLimitStatus]:
        """获取所有实体的速率限制状态"""
        return {entity_id: limiter.get_status() for entity_id, limiter in self._limiters.items()}


# 全局默认限制器


def get_default_rate_limiter() -> RateLimiter:
    """获取默认速率限制器"""
    from src.infrastructure.runtime.providers import get_runtime_container

    container = get_runtime_container()
    if container is not None:
        _default_limiter = getattr(container, "default_limiter", None)
        if _default_limiter is not None:
            return _default_limiter
    _default_limiter = RateLimiter()
    if container is not None:
        container.default_limiter = _default_limiter
    return _default_limiter


def configure_default_rate_limiter(rpm: int = 60, tpm: int = 100000) -> RateLimiter:
    """配置默认速率限制器"""
    from src.infrastructure.runtime.providers import get_runtime_container

    _default_limiter = RateLimiter(rpm=rpm, tpm=tpm)
    container = get_runtime_container()
    if container is not None:
        container.default_limiter = _default_limiter
    return _default_limiter


__all__ = [
    "MultiEntityRateLimiter",
    "RateLimitConfig",
    "RateLimitStatus",
    "RateLimiter",
    "configure_default_rate_limiter",
    "get_default_rate_limiter",
]
