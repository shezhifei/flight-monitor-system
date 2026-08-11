"""In-memory cache for AI configuration payloads."""

import hashlib
import pickle
import threading
import time
from collections import OrderedDict
from collections.abc import Callable
from typing import Any


class AIConfigCache:
    """Thread-safe LRU-like cache with TTL support."""

    def __init__(self, max_size: int = 1000, default_ttl: int = 300):
        """Create cache with max item count and default TTL."""
        self.max_size = max_size
        self.default_ttl = default_ttl
        self._cache: OrderedDict[str, dict[str, Any]] = OrderedDict()
        self._lock = threading.RLock()
        self._hits = 0
        self._misses = 0

    def get(self, key: str, loader: Callable[[], Any] | None = None) -> Any:
        """Get value by key, optionally loading on miss."""
        with self._lock:
            if key in self._cache:
                entry = self._cache[key]

                if entry["expires_at"] < time.time():
                    del self._cache[key]
                    self._misses += 1
                else:
                    self._cache.move_to_end(key)
                    self._hits += 1
                    return entry["value"]
            else:
                self._misses += 1

            if loader is not None:
                value = loader()
                self.set(key, value)
                return value

            return None

    def set(self, key: str, value: Any, ttl: int | None = None) -> None:
        """Set value by key with optional TTL override."""
        with self._lock:
            if len(self._cache) >= self.max_size:
                self._cache.popitem(last=False)

            ttl = ttl or self.default_ttl
            expires_at = time.time() + ttl

            self._cache[key] = {"value": value, "expires_at": expires_at, "created_at": time.time()}

            self._cache.move_to_end(key)

    def delete(self, key: str) -> bool:
        """Delete one cache key."""
        with self._lock:
            if key in self._cache:
                del self._cache[key]
                return True
            return False

    def clear(self) -> None:
        """Clear all cached items."""
        with self._lock:
            self._cache.clear()

    def get_stats(self) -> dict[str, Any]:
        """Return cache hit/miss and size metrics."""
        with self._lock:
            total = self._hits + self._misses
            hit_rate = self._hits / total if total > 0 else 0

            return {
                "size": len(self._cache),
                "max_size": self.max_size,
                "hits": self._hits,
                "misses": self._misses,
                "hit_rate": hit_rate,
                "total_requests": total,
            }

    def cleanup_expired(self) -> int:
        """
        清理过期缓存

        Returns:
            int: 清理的条目数
        """
        with self._lock:
            current_time = time.time()
            expired_keys = []

            for key, entry in self._cache.items():
                if entry["expires_at"] < current_time:
                    expired_keys.append(key)

            for key in expired_keys:
                del self._cache[key]

            return len(expired_keys)


class AIConfigCacheManager:
    """AI配置缓存管理器，支持多级缓存"""

    def __init__(self, memory_cache_size: int = 1000):
        """
        初始化缓存管理器

        Args:
            memory_cache_size: 内存缓存大小
        """
        self.memory_cache = AIConfigCache(max_size=memory_cache_size)
        self._providers_cache: dict[str, Any] = {}
        self._models_cache: dict[str, Any] = {}
        self._lock = threading.RLock()

    def get_provider_config(self, provider_name: str, loader: Callable[[], Any]) -> Any:
        """
        获取提供商配置（带缓存）

        Args:
            provider_name: 提供商名称
            loader: 配置加载函数

        Returns:
            Any: 提供商配置
        """
        cache_key = f"provider:{provider_name}"

        # 尝试从内存缓存获取
        cached = self.memory_cache.get(cache_key)
        if cached is not None:
            return cached

        # 使用加载器获取
        config = loader()

        # 缓存结果
        self.memory_cache.set(cache_key, config, ttl=60)  # 提供商配置缓存1分钟
        return config

    def get_model_config(self, model_name: str, loader: Callable[[], Any]) -> Any:
        """
        获取模型配置（带缓存）

        Args:
            model_name: 模型名称
            loader: 配置加载函数

        Returns:
            Any: 模型配置
        """
        cache_key = f"model:{model_name}"

        # 尝试从内存缓存获取
        cached = self.memory_cache.get(cache_key)
        if cached is not None:
            return cached

        # 使用加载器获取
        config = loader()

        # 缓存结果
        self.memory_cache.set(cache_key, config, ttl=300)  # 模型配置缓存5分钟
        return config

    def invalidate_provider_config(self, provider_name: str) -> None:
        """使提供商配置缓存失效"""
        cache_key = f"provider:{provider_name}"
        self.memory_cache.delete(cache_key)

    def invalidate_model_config(self, model_name: str) -> None:
        """使模型配置缓存失效"""
        cache_key = f"model:{model_name}"
        self.memory_cache.delete(cache_key)

    def clear_all(self) -> None:
        """清除所有缓存"""
        self.memory_cache.clear()
        with self._lock:
            self._providers_cache.clear()
            self._models_cache.clear()

    def get_cache_stats(self) -> dict[str, Any]:
        """
        获取缓存统计

        Returns:
            Dict[str, Any]: 缓存统计
        """
        memory_stats = self.memory_cache.get_stats()

        with self._lock:
            return {
                "memory_cache": memory_stats,
                "providers_cache_size": len(self._providers_cache),
                "models_cache_size": len(self._models_cache),
            }


class ConfigHashCache:
    """配置哈希缓存，用于检测配置变更"""

    def __init__(self):
        """初始化配置哈希缓存"""
        self._config_hashes: dict[str, str] = {}
        self._lock = threading.RLock()

    def compute_hash(self, config_data: Any) -> str:
        """
        计算配置数据的哈希

        Args:
            config_data: 配置数据

        Returns:
            str: 配置哈希
        """
        # 将配置数据序列化为字节
        config_bytes = pickle.dumps(config_data)

        # 计算SHA-256哈希（用于缓存键，非密码学安全目的）
        return hashlib.sha256(config_bytes).hexdigest()

    def has_changed(self, key: str, config_data: Any) -> bool:
        """
        检查配置是否变更

        Args:
            key: 配置键
            config_data: 配置数据

        Returns:
            bool: 是否发生变更
        """
        with self._lock:
            new_hash = self.compute_hash(config_data)

            if key not in self._config_hashes:
                self._config_hashes[key] = new_hash
                return True  # 新配置，视为有变更

            old_hash = self._config_hashes[key]
            if new_hash != old_hash:
                self._config_hashes[key] = new_hash
                return True  # 哈希不同，有变更

            return False  # 哈希相同，无变更

    def update_hash(self, key: str, config_data: Any) -> None:
        """
        更新配置哈希

        Args:
            key: 配置键
            config_data: 配置数据
        """
        with self._lock:
            new_hash = self.compute_hash(config_data)
            self._config_hashes[key] = new_hash

    def clear(self) -> None:
        """清除所有哈希"""
        with self._lock:
            self._config_hashes.clear()
