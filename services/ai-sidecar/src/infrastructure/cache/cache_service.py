"""缓存服务模块

提供多级缓存支持：本地内存缓存 + Redis 缓存
"""

import asyncio
import contextlib
import json
import time
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from functools import wraps
from typing import Any, Generic, TypeVar

from src.infrastructure.common.exceptions import JSON_EXCEPTIONS, REDIS_EXCEPTIONS
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)

T = TypeVar("T")


@dataclass
class CacheEntry(Generic[T]):
    """缓存条目"""

    value: T
    created_at: float = field(default_factory=time.time)
    expires_at: float = 0.0
    access_count: int = 0
    last_accessed_at: float = 0.0

    def __post_init__(self):
        if self.last_accessed_at == 0.0:
            self.last_accessed_at = self.created_at

    def is_expired(self) -> bool:
        """检查是否过期"""
        return time.time() > self.expires_at

    def access(self) -> T:
        """访问缓存条目"""
        self.access_count += 1
        self.last_accessed_at = time.time()
        return self.value


class CacheService(ABC):
    """缓存服务抽象基类"""

    @abstractmethod
    async def get(self, key: str) -> Any | None:
        """获取缓存值"""

    @abstractmethod
    async def set(self, key: str, value: Any, ttl: int | None = None) -> bool:
        """设置缓存值"""

    @abstractmethod
    async def delete(self, key: str) -> bool:
        """删除缓存"""

    @abstractmethod
    async def exists(self, key: str) -> bool:
        """检查缓存是否存在"""

    @abstractmethod
    async def clear(self) -> bool:
        """清空所有缓存"""

    @abstractmethod
    async def get_stats(self) -> dict[str, Any]:
        """获取缓存统计信息"""


class LocalCacheService(CacheService):
    """本地内存缓存服务（分片锁实现，减少并发争用）"""

    _DEFAULT_SHARDS = 16

    def __init__(
        self,
        max_size: int = 1000,
        default_ttl: int = 300,
        cleanup_interval: int = 60,
        num_shards: int | None = None,
    ):
        self._num_shards = num_shards or self._DEFAULT_SHARDS
        self._max_size = max_size
        self._shard_size = max(1, max_size // self._num_shards)
        self._default_ttl = default_ttl
        self._cleanup_interval = cleanup_interval
        self._stats = {
            "hits": 0,
            "misses": 0,
            "sets": 0,
            "deletes": 0,
            "evictions": 0,
        }
        self._cleanup_task: asyncio.Task | None = None

        self._shards: list[dict[str, CacheEntry]] = [{} for _ in range(self._num_shards)]
        self._locks: list[asyncio.Lock] = [asyncio.Lock() for _ in range(self._num_shards)]

    def _shard_index(self, key: str) -> int:
        return hash(key) % self._num_shards

    async def start(self) -> None:
        if self._cleanup_task is None or self._cleanup_task.done():
            self._cleanup_task = asyncio.create_task(self._cleanup_loop())

    async def stop(self) -> None:
        if self._cleanup_task and not self._cleanup_task.done():
            self._cleanup_task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await self._cleanup_task

    async def _cleanup_loop(self) -> None:
        while True:
            try:
                await asyncio.sleep(self._cleanup_interval)
                await self._cleanup_expired()
            except asyncio.CancelledError:
                break
            except Exception as e:  # noqa: BLE001 - background cleanup loop must not die on any error
                logger.error(f"Cache cleanup error: {e}")

    async def _cleanup_expired(self) -> None:
        for idx in range(self._num_shards):
            async with self._locks[idx]:
                expired = [k for k, v in self._shards[idx].items() if v.is_expired()]
                for key in expired:
                    del self._shards[idx][key]
                    self._stats["evictions"] += 1

    def _evict_if_needed(self, shard_idx: int) -> None:
        shard = self._shards[shard_idx]
        if len(shard) >= self._shard_size:
            oldest_key = min(shard.keys(), key=lambda k: shard[k].last_accessed_at)
            del shard[oldest_key]
            self._stats["evictions"] += 1

    async def get(self, key: str) -> Any | None:
        idx = self._shard_index(key)
        async with self._locks[idx]:
            entry = self._shards[idx].get(key)
            if entry is None:
                self._stats["misses"] += 1
                return None
            if entry.is_expired():
                del self._shards[idx][key]
                self._stats["misses"] += 1
                return None
            self._stats["hits"] += 1
            return entry.access()

    async def set(self, key: str, value: Any, ttl: int | None = None) -> bool:
        idx = self._shard_index(key)
        async with self._locks[idx]:
            self._evict_if_needed(idx)
            ttl = ttl or self._default_ttl
            entry = CacheEntry(
                value=value,
                expires_at=time.time() + ttl,
            )
            self._shards[idx][key] = entry
            self._stats["sets"] += 1
            return True

    async def delete(self, key: str) -> bool:
        idx = self._shard_index(key)
        async with self._locks[idx]:
            if key in self._shards[idx]:
                del self._shards[idx][key]
                self._stats["deletes"] += 1
                return True
            return False

    async def exists(self, key: str) -> bool:
        idx = self._shard_index(key)
        async with self._locks[idx]:
            entry = self._shards[idx].get(key)
            if entry is None:
                return False
            if entry.is_expired():
                del self._shards[idx][key]
                return False
            return True

    async def clear(self) -> bool:
        for idx in range(self._num_shards):
            async with self._locks[idx]:
                self._shards[idx].clear()
        return True

    async def get_stats(self) -> dict[str, Any]:
        total_size = sum(len(s) for s in self._shards)
        total_requests = self._stats["hits"] + self._stats["misses"]
        hit_rate = self._stats["hits"] / total_requests if total_requests > 0 else 0.0
        return {
            **self._stats,
            "size": total_size,
            "max_size": self._max_size,
            "hit_rate": hit_rate,
            "total_requests": total_requests,
        }


class RedisCacheService(CacheService):
    """Redis 缓存服务"""

    def __init__(
        self,
        redis_client: Any,
        key_prefix: str = "cache:",
        default_ttl: int = 300,
    ):
        """
        初始化 Redis 缓存服务

        Args:
            redis_client: Redis 客户端
            key_prefix: 键前缀
            default_ttl: 默认 TTL（秒）
        """
        self._redis = redis_client
        self._key_prefix = key_prefix
        self._default_ttl = default_ttl
        self._stats = {
            "hits": 0,
            "misses": 0,
            "sets": 0,
            "deletes": 0,
            "errors": 0,
        }

    def _make_key(self, key: str) -> str:
        """构建完整的 Redis 键"""
        return f"{self._key_prefix}{key}"

    async def get(self, key: str) -> Any | None:
        """获取缓存值"""
        try:
            full_key = self._make_key(key)
            value = await self._redis.get(full_key)
            if value is None:
                self._stats["misses"] += 1
                return None

            self._stats["hits"] += 1
            return json.loads(value)
        except REDIS_EXCEPTIONS + JSON_EXCEPTIONS as e:
            logger.error(f"Redis cache get error: {e}")
            self._stats["errors"] += 1
            return None

    async def set(self, key: str, value: Any, ttl: int | None = None) -> bool:
        """设置缓存值"""
        try:
            full_key = self._make_key(key)
            ttl = ttl or self._default_ttl
            serialized = json.dumps(value, ensure_ascii=False)
            await self._redis.setex(full_key, ttl, serialized)
            self._stats["sets"] += 1
            return True
        except REDIS_EXCEPTIONS + JSON_EXCEPTIONS as e:
            logger.error(f"Redis cache set error: {e}")
            self._stats["errors"] += 1
            return False

    async def delete(self, key: str) -> bool:
        """删除缓存"""
        try:
            full_key = self._make_key(key)
            result = await self._redis.delete(full_key)
            if result > 0:
                self._stats["deletes"] += 1
                return True
            return False
        except REDIS_EXCEPTIONS as e:
            logger.error(f"Redis cache delete error: {e}")
            self._stats["errors"] += 1
            return False

    async def exists(self, key: str) -> bool:
        """检查缓存是否存在"""
        try:
            full_key = self._make_key(key)
            return await self._redis.exists(full_key) > 0
        except REDIS_EXCEPTIONS as e:
            logger.error(f"Redis cache exists error: {e}")
            self._stats["errors"] += 1
            return False

    async def clear(self) -> bool:
        """清空所有缓存（使用前缀匹配）"""
        try:
            pattern = f"{self._key_prefix}*"
            keys = []
            async for key in self._redis.scan_iter(match=pattern, count=100):
                keys.append(key)
                if len(keys) >= 1000:
                    await self._redis.delete(*keys)
                    keys = []
            if keys:
                await self._redis.delete(*keys)
            return True
        except REDIS_EXCEPTIONS as e:
            logger.error(f"Redis cache clear error: {e}")
            self._stats["errors"] += 1
            return False

    async def get_stats(self) -> dict[str, Any]:
        """获取缓存统计信息"""
        total_requests = self._stats["hits"] + self._stats["misses"]
        hit_rate = self._stats["hits"] / total_requests if total_requests > 0 else 0.0
        return {
            **self._stats,
            "hit_rate": hit_rate,
            "total_requests": total_requests,
            "key_prefix": self._key_prefix,
        }


class MultiLevelCacheService(CacheService):
    """多级缓存服务（本地缓存 + Redis 缓存）"""

    def __init__(
        self,
        local_cache: LocalCacheService,
        redis_cache: RedisCacheService | None = None,
    ):
        """
        初始化多级缓存服务

        Args:
            local_cache: 本地缓存服务
            redis_cache: Redis 缓存服务（可选）
        """
        self._local = local_cache
        self._redis = redis_cache
        self._stats = {
            "local_hits": 0,
            "redis_hits": 0,
            "misses": 0,
        }

    async def start(self) -> None:
        """启动缓存服务"""
        await self._local.start()

    async def stop(self) -> None:
        """停止缓存服务"""
        await self._local.stop()

    async def get(self, key: str) -> Any | None:
        """获取缓存值（先本地，后 Redis）"""
        # 先尝试本地缓存
        value = await self._local.get(key)
        if value is not None:
            self._stats["local_hits"] += 1
            return value

        # 再尝试 Redis 缓存
        if self._redis:
            value = await self._redis.get(key)
            if value is not None:
                self._stats["redis_hits"] += 1
                # 回填本地缓存
                await self._local.set(key, value)
                return value

        self._stats["misses"] += 1
        return None

    async def set(self, key: str, value: Any, ttl: int | None = None) -> bool:
        """设置缓存值（同时写入本地和 Redis）"""
        local_result = await self._local.set(key, value, ttl)
        redis_result = True
        if self._redis:
            redis_result = await self._redis.set(key, value, ttl)
        return local_result and redis_result

    async def delete(self, key: str) -> bool:
        """删除缓存（同时删除本地和 Redis）"""
        local_result = await self._local.delete(key)
        redis_result = True
        if self._redis:
            redis_result = await self._redis.delete(key)
        return local_result or redis_result

    async def exists(self, key: str) -> bool:
        """检查缓存是否存在"""
        if await self._local.exists(key):
            return True
        if self._redis:
            return await self._redis.exists(key)
        return False

    async def clear(self) -> bool:
        """清空所有缓存"""
        local_result = await self._local.clear()
        redis_result = True
        if self._redis:
            redis_result = await self._redis.clear()
        return local_result and redis_result

    async def get_stats(self) -> dict[str, Any]:
        """获取缓存统计信息"""
        local_stats = await self._local.get_stats()
        redis_stats = await self._redis.get_stats() if self._redis else {}

        total_hits = self._stats["local_hits"] + self._stats["redis_hits"]
        total_requests = total_hits + self._stats["misses"]

        return {
            "multi_level": self._stats,
            "local": local_stats,
            "redis": redis_stats,
            "overall_hit_rate": total_hits / total_requests if total_requests > 0 else 0.0,
        }


def cache_key(*args, **kwargs) -> str:
    """
    生成缓存键

    Args:
        *args: 位置参数
        **kwargs: 关键字参数

    Returns:
        缓存键字符串
    """
    parts = [str(arg) for arg in args]
    parts.extend(f"{k}:{v}" for k, v in sorted(kwargs.items()))
    return ":".join(parts)


def cached(
    cache_service: CacheService,
    ttl: int | None = None,
    key_prefix: str = "",
):
    """
    缓存装饰器

    Args:
        cache_service: 缓存服务实例
        ttl: 缓存 TTL（秒）
        key_prefix: 键前缀
    """

    def decorator(func):
        @wraps(func)
        async def wrapper(*args, **kwargs):
            # 生成缓存键
            key = f"{key_prefix}{func.__name__}:{cache_key(*args, **kwargs)}"

            # 尝试从缓存获取
            result = await cache_service.get(key)
            if result is not None:
                return result

            # 执行函数
            result = await func(*args, **kwargs)

            # 写入缓存
            if result is not None:
                await cache_service.set(key, result, ttl)

            return result

        return wrapper

    return decorator
