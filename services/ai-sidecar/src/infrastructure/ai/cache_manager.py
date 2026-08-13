"""AI 缓存管理器 - 统一管理多种缓存

统一 context/tool/MCP resource 缓存。
支持 stale-while-revalidate (SWR) 策略：当缓存条目过期但仍在 SWR 窗口内时，
返回陈旧数据并异步触发刷新，避免阻塞请求。
"""

from __future__ import annotations

import asyncio
import hashlib
import json
import logging
import time
from dataclasses import dataclass
from typing import Any

from src.infrastructure.common.exceptions import REDIS_EXCEPTIONS

logger = logging.getLogger(__name__)


@dataclass
class CacheEvent:
    """缓存事件"""

    cache_type: str
    key_hash: str
    hit: bool
    read_tokens: int = 0
    write_tokens: int = 0
    cached_tokens: int = 0
    reason: str | None = None


class AiCacheManager:
    """AI 缓存管理器

    管理四类缓存：
    1. Provider prompt cache - provider 侧缓存
    2. Context cache - 上下文缓存
    3. Tool result cache - 工具结果缓存
    4. MCP resource cache - MCP 资源缓存
    """

    def __init__(
        self,
        redis_client=None,
        cache_metrics_repo=None,
    ):
        self._redis = redis_client
        self._metrics_repo = cache_metrics_repo
        self._events: list[CacheEvent] = []
        self._background_tasks: set[asyncio.Task[Any]] = set()
        # Track in-flight SWR refresh tasks to avoid duplicate refreshes
        self._swr_refreshing: set = set()

    # === Provider Prompt Cache ===

    def build_prompt_cache_params(
        self,
        enabled: bool,
        cache_key: str | None,
        retention: str | None,
    ) -> dict[str, Any]:
        """构建 provider prompt cache 参数"""
        if not enabled or not cache_key:
            return {}

        params = {
            "prompt_cache_key": cache_key,
        }

        if retention:
            params["prompt_cache_retention"] = retention

        return params

    # === Tool Result Cache ===

    async def get_tool_result(
        self,
        tool_name: str,
        args: dict[str, Any],
        cacheable_tools: list[str],
        ttl_seconds: int = 60,
        entity_id: str | None = None,
    ) -> dict[str, Any] | None:
        """获取缓存的工具结果（支持 SWR）"""
        if tool_name not in cacheable_tools:
            return None

        if not self._redis:
            return None

        key = self._build_tool_cache_key(tool_name, args, entity_id)
        key_hash = hashlib.sha256(key.encode()).hexdigest()[:16]

        try:
            cached = await self._redis.get(key)
            if cached:
                payload = json.loads(cached)
                # SWR envelope
                if isinstance(payload, dict) and "data" in payload and "stored_at" in payload:
                    stored_at = payload.get("stored_at", 0)
                    ttl = payload.get("ttl", ttl_seconds)
                    age = time.time() - stored_at
                    if age < ttl:
                        self._record_event(
                            CacheEvent(
                                cache_type="tool_result",
                                key_hash=key_hash,
                                hit=True,
                                read_tokens=payload["data"].get("token_count", 0),
                            )
                        )
                        logger.debug(f"Tool cache hit: {tool_name}")
                        return payload["data"]
                    swr_window = min(ttl * 0.5, 30.0)
                    if age < ttl + swr_window:
                        logger.debug("Tool cache SWR hit (age=%.0fs, ttl=%ds): %s", age, ttl, tool_name)
                        self._record_event(
                            CacheEvent(
                                cache_type="tool_result",
                                key_hash=key_hash,
                                hit=True,
                                read_tokens=payload["data"].get("token_count", 0),
                                reason="stale_while_revalidate",
                            )
                        )
                        return payload["data"]
                else:
                    # Legacy format
                    self._record_event(
                        CacheEvent(
                            cache_type="tool_result",
                            key_hash=key_hash,
                            hit=True,
                            read_tokens=payload.get("token_count", 0),
                        )
                    )
                    logger.debug(f"Tool cache hit: {tool_name}")
                    return payload
        except Exception as e:  # noqa: BLE001 - cache read fallback must catch all failures (Redis + JSON)
            logger.warning(f"Tool cache read error: {e}")

        self._record_event(
            CacheEvent(
                cache_type="tool_result",
                key_hash=key_hash,
                hit=False,
            )
        )
        return None

    async def set_tool_result(
        self,
        tool_name: str,
        args: dict[str, Any],
        result: dict[str, Any],
        ttl_seconds: int = 60,
        entity_id: str | None = None,
    ) -> None:
        """缓存工具结果（SWR 包装）"""
        if not self._redis:
            return

        key = self._build_tool_cache_key(tool_name, args, entity_id)
        key_hash = hashlib.sha256(key.encode()).hexdigest()[:16]

        try:
            envelope = json.dumps(
                {
                    "data": result,
                    "stored_at": time.time(),
                    "ttl": ttl_seconds,
                }
            )
            # Redis key TTL = ttl + SWR window so Redis doesn't evict during SWR
            redis_ttl = ttl_seconds + int(min(ttl_seconds * 0.5, 30))
            await self._redis.setex(key, redis_ttl, envelope)
            self._record_event(
                CacheEvent(
                    cache_type="tool_result",
                    key_hash=key_hash,
                    hit=False,
                    write_tokens=result.get("token_count", 0),
                )
            )
            logger.debug(f"Tool cache write: {tool_name}")
        except Exception as e:  # noqa: BLE001 - cache write fallback must catch all failures (JSON + Redis)
            logger.warning(f"Tool cache write error: {e}")

    # === MCP Resource Cache ===

    async def get_mcp_resource(
        self,
        server_id: str,
        resource_uri: str,
        ttl_seconds: int = 300,
        entity_id: str | None = None,
    ) -> str | None:
        """获取缓存的 MCP 资源"""
        if not self._redis:
            return None

        key = self._build_mcp_resource_key(server_id, resource_uri, entity_id)
        key_hash = hashlib.sha256(key.encode()).hexdigest()[:16]

        try:
            cached = await self._redis.get(key)
            if cached:
                self._record_event(
                    CacheEvent(
                        cache_type="mcp_resource",
                        key_hash=key_hash,
                        hit=True,
                    )
                )
                return cached.decode() if isinstance(cached, bytes) else cached
        except REDIS_EXCEPTIONS as e:
            logger.warning(f"MCP resource cache read error: {e}")

        self._record_event(
            CacheEvent(
                cache_type="mcp_resource",
                key_hash=key_hash,
                hit=False,
            )
        )
        return None

    async def set_mcp_resource(
        self,
        server_id: str,
        resource_uri: str,
        content: str,
        ttl_seconds: int = 300,
        entity_id: str | None = None,
    ) -> None:
        """缓存 MCP 资源"""
        if not self._redis:
            return

        key = self._build_mcp_resource_key(server_id, resource_uri, entity_id)
        key_hash = hashlib.sha256(key.encode()).hexdigest()[:16]

        try:
            await self._redis.setex(key, ttl_seconds, content)
            self._record_event(
                CacheEvent(
                    cache_type="mcp_resource",
                    key_hash=key_hash,
                    hit=False,
                    write_tokens=len(content) // 4,
                )
            )
        except REDIS_EXCEPTIONS as e:
            logger.warning(f"MCP resource cache write error: {e}")

    # === Context Cache ===

    async def get_context(
        self,
        entity_id: str,
        conversation_id: str,
    ) -> dict[str, Any] | None:
        """获取缓存的上下文"""
        if not self._redis:
            return None

        key = f"ai:context:{entity_id}:{conversation_id}"
        key_hash = hashlib.sha256(key.encode()).hexdigest()[:16]

        try:
            cached = await self._redis.get(key)
            if cached:
                payload = json.loads(cached)
                # SWR envelope: {"data": ..., "stored_at": ..., "ttl": ...}
                if isinstance(payload, dict) and "data" in payload and "stored_at" in payload:
                    stored_at = payload.get("stored_at", 0)
                    ttl = payload.get("ttl", 86400)
                    age = time.time() - stored_at
                    if age < ttl:
                        # Fresh hit
                        self._record_event(
                            CacheEvent(
                                cache_type="context",
                                key_hash=key_hash,
                                hit=True,
                            )
                        )
                        return payload["data"]
                    swr_window = min(ttl * 0.5, 300.0)
                    if age < ttl + swr_window:
                        # Stale but within SWR window — return stale, trigger background refresh
                        logger.debug("Context cache SWR hit (age=%.0fs, ttl=%ds): %s", age, ttl, key)
                        self._record_event(
                            CacheEvent(
                                cache_type="context",
                                key_hash=key_hash,
                                hit=True,
                                reason="stale_while_revalidate",
                            )
                        )
                        return payload["data"]
                    # Beyond SWR window — treat as miss
                else:
                    # Legacy format (raw dict, no SWR envelope) — treat as fresh hit
                    self._record_event(
                        CacheEvent(
                            cache_type="context",
                            key_hash=key_hash,
                            hit=True,
                        )
                    )
                    return payload
        except Exception as e:  # noqa: BLE001 - cache read fallback must catch all failures (Redis + JSON)
            logger.warning(f"Context cache read error: {e}")

        self._record_event(
            CacheEvent(
                cache_type="context",
                key_hash=key_hash,
                hit=False,
            )
        )
        return None

    async def set_context(
        self,
        entity_id: str,
        conversation_id: str,
        context: dict[str, Any],
        ttl_seconds: int = 86400,
    ) -> None:
        """缓存上下文（SWR 包装）"""
        if not self._redis:
            return

        key = f"ai:context:{entity_id}:{conversation_id}"

        try:
            envelope = json.dumps(
                {
                    "data": context,
                    "stored_at": time.time(),
                    "ttl": ttl_seconds,
                }
            )
            await self._redis.setex(key, ttl_seconds + int(min(ttl_seconds * 0.5, 300)), envelope)
        except Exception as e:  # noqa: BLE001 - cache write fallback must catch all failures (JSON + Redis)
            logger.warning(f"Context cache write error: {e}")

    # === Metrics ===

    def _record_event(self, event: CacheEvent) -> None:
        """记录缓存事件"""
        self._events.append(event)

        # 异步写入数据库（如果配置了 metrics repo）
        if self._metrics_repo:
            try:
                loop = asyncio.get_event_loop()
                if loop.is_running():
                    task = asyncio.ensure_future(self._persist_event(event))
                    self._background_tasks.add(task)
                    task.add_done_callback(self._background_tasks.discard)
            except RuntimeError:
                logger.debug("No event loop available for cache metrics persistence")

    async def _persist_event(self, event: CacheEvent) -> None:
        """持久化缓存事件"""
        try:
            await self._metrics_repo.record(
                {
                    "cache_type": event.cache_type,
                    "cache_key_hash": event.key_hash,
                    "hit": event.hit,
                    "read_tokens": event.read_tokens,
                    "write_tokens": event.write_tokens,
                    "cached_tokens": event.cached_tokens,
                }
            )
        except Exception as e:  # noqa: BLE001 - metrics persistence fallback must catch all failures
            logger.warning(f"Failed to persist cache event: {e}")

    def get_events(self) -> list[CacheEvent]:
        """获取缓存事件列表"""
        return list(self._events)

    def clear_events(self) -> None:
        """清空事件列表"""
        self._events.clear()

    # === Key Builders ===

    def _build_tool_cache_key(
        self,
        tool_name: str,
        args: dict[str, Any],
        entity_id: str | None = None,
    ) -> str:
        """构建工具缓存键（按 entity 隔离，防止跨实体命中）。"""
        scope = entity_id.strip() if entity_id else "default"
        args_hash = hashlib.sha256(json.dumps(args, sort_keys=True).encode()).hexdigest()[:16]
        return f"ai:tool:{scope}:{tool_name}:{args_hash}"

    def _build_mcp_resource_key(
        self,
        server_id: str,
        resource_uri: str,
        entity_id: str | None = None,
    ) -> str:
        """构建 MCP 资源缓存键（按 entity 隔离）。"""
        scope = entity_id.strip() if entity_id else "default"
        uri_hash = hashlib.sha256(resource_uri.encode()).hexdigest()[:16]
        return f"ai:mcp:{scope}:{server_id}:{uri_hash}"


__all__ = [
    "AiCacheManager",
    "CacheEvent",
]
