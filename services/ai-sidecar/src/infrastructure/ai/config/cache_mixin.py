"""缓存混入类，供 AI 配置存储后端复用缓存逻辑。"""

from __future__ import annotations

import os
import time
from typing import Any


class ConfigCacheMixin:
    """为配置存储提供内存缓存能力。

    用法::

        class MyStore(AIConfigStoreInterface, ConfigCacheMixin):
            def __init__(self, ...):
                self._init_cache()
                ...
    """

    def _init_cache(self, ttl: int | None = None) -> None:
        self._cache: dict[str, dict[str, Any]] = {}
        self._cache_timestamps: dict[str, float] = {}
        self._cache_ttl = ttl if ttl is not None else int(os.environ.get("AI_CONFIG_CACHE_TTL", "300"))

    def _cache_valid(self, entity_id: str) -> bool:
        if entity_id not in self._cache:
            return False
        ts = self._cache_timestamps.get(entity_id, 0.0)
        return (time.monotonic() - ts) < self._cache_ttl

    def _get_cached(self, entity_id: str) -> dict[str, Any] | None:
        return self._cache.get(entity_id)

    def _set_cached(self, entity_id: str, config: dict[str, Any]) -> None:
        self._cache[entity_id] = config
        self._cache_timestamps[entity_id] = time.monotonic()

    def _invalidate_cache(self, entity_id: str | None = None) -> None:
        if entity_id is not None:
            self._cache.pop(entity_id, None)
            self._cache_timestamps.pop(entity_id, None)
        else:
            self._cache.clear()
            self._cache_timestamps.clear()
