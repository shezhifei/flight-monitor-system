"""缓存模块

提供多级缓存支持：本地内存缓存 + Redis 缓存
"""

from .cache_service import (
    CacheEntry,
    CacheService,
    LocalCacheService,
    MultiLevelCacheService,
    RedisCacheService,
    cache_key,
    cached,
)

__all__ = [
    "CacheEntry",
    "CacheService",
    "LocalCacheService",
    "MultiLevelCacheService",
    "RedisCacheService",
    "cache_key",
    "cached",
]
