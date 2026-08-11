"""
上下文管理器 - 管理器实现

包含上下文管理器的抽象基类和具体实现（内存、Redis）。
"""

import asyncio
import time
from abc import ABC, abstractmethod
from typing import Any

import redis.asyncio as redis

from src.infrastructure.common.exceptions import JSON_EXCEPTIONS, REDIS_EXCEPTIONS
from src.infrastructure.logging.core import get_logger

from ..openai_client import Message, MessageRole
from .models import (
    _DEFAULT_ACTIVE_CONTEXT_MESSAGE_CAP,
    _LOCK_IDLE_TTL_SECONDS,
    _LOCK_PRUNE_INTERVAL_SECONDS,
    Context,
    ContextCachePolicy,
    ContextError,
    ContextNotFoundError,
    ContextType,
    _create_default_context,
    _resolve_message_cap,
    _trim_messages_to_limit,
    _truncate_messages,
)

logger = get_logger(__name__)


class ContextManager(ABC):
    """
    上下文管理器抽象基类

    定义上下文存储和管理的标准接口。
    """

    @abstractmethod
    async def get_context(self, context_id: str) -> Context:
        """
        根据 ID 获取上下文

        Args:
            context_id: 上下文标识符

        Returns:
            Context 对象

        Raises:
            ContextNotFoundError: 如果上下文不存在
        """

    @abstractmethod
    async def save_context(self, context_id: str, context: Context) -> None:
        """
        保存上下文

        Args:
            context_id: 上下文标识符
            context: Context 对象
        """

    @abstractmethod
    async def add_message(self, context_id: str, message: Message) -> None:
        """
        向上下文添加消息

        Args:
            context_id: 上下文标识符
            message: 要添加的消息
        """

    @abstractmethod
    async def trim_context(self, context_id: str, max_tokens: int) -> list[Message]:
        """
        修剪上下文，使其令牌数不超过限制

        Args:
            context_id: 上下文标识符
            max_tokens: 最大令牌数

        Returns:
            被移除的消息列表
        """

    @abstractmethod
    async def delete_context(self, context_id: str) -> bool:
        """
        删除上下文

        Args:
            context_id: 上下文标识符

        Returns:
            是否成功删除
        """

    @abstractmethod
    async def list_contexts(self, type_filter: ContextType | None = None) -> list[str]:
        """
        列出所有上下文 ID（可选按类型过滤）

        Args:
            type_filter: 可选的类型过滤器

        Returns:
            上下文 ID 列表
        """


class MemoryContextManager(ContextManager):
    """
    基于内存的上下文管理器实现

    将上下文存储在内存字典中。适用于单实例、非持久化场景。
    """

    def __init__(
        self,
        default_model: str = "gpt-3.5-turbo",
        cache_policy: ContextCachePolicy | None = None,
        default_max_messages: int = _DEFAULT_ACTIVE_CONTEXT_MESSAGE_CAP,
    ):
        self._contexts: dict[str, Context] = {}
        self._default_model = default_model
        self._cache_policy = cache_policy or ContextCachePolicy()
        self._default_max_messages = max(0, int(default_max_messages or 0))
        self._global_lock = asyncio.Lock()
        self._context_locks: dict[str, asyncio.Lock] = {}
        self._context_lock_timestamps: dict[str, float] = {}
        self._last_lock_prune_at = 0.0
        # 兼容旧实现中可能直接引用 _lock 的调用。
        self._lock = self._global_lock
        logger.info(f"MemoryContextManager 初始化完成，默认模型: {default_model}")

    async def _get_context_lock(
        self,
        context_id: str,
        *,
        create_if_missing: bool = False,
    ) -> asyncio.Lock:
        """获取或创建 context 级别锁。"""
        await self._maybe_prune_stale_locks()
        lock = self._context_locks.get(context_id)
        if lock is not None:
            self._context_lock_timestamps[context_id] = time.time()
            return lock

        async with self._global_lock:
            lock = self._context_locks.get(context_id)
            if lock is not None:
                self._context_lock_timestamps[context_id] = time.time()
                return lock
            if not create_if_missing and context_id not in self._contexts:
                return asyncio.Lock()
            lock = asyncio.Lock()
            self._context_locks[context_id] = lock
            self._context_lock_timestamps[context_id] = time.time()
            return lock

    async def _maybe_prune_stale_locks(self) -> None:
        now = time.time()
        if now - self._last_lock_prune_at < _LOCK_PRUNE_INTERVAL_SECONDS:
            return
        self._last_lock_prune_at = now
        await self.prune_stale_locks()

    async def prune_stale_locks(self, max_idle_seconds: float = _LOCK_IDLE_TTL_SECONDS) -> int:
        now = time.time()
        removed = 0
        async with self._global_lock:
            for context_id, lock in list(self._context_locks.items()):
                if context_id in self._contexts:
                    continue
                if lock.locked():
                    continue
                last_used = float(self._context_lock_timestamps.get(context_id, 0.0) or 0.0)
                if last_used and now - last_used < max_idle_seconds:
                    continue
                self._context_locks.pop(context_id, None)
                self._context_lock_timestamps.pop(context_id, None)
                removed += 1
        return removed

    def _trim_context_unlocked(
        self,
        context_id: str,
        context: Context,
        max_tokens: int,
        strategy: str,
    ) -> list[Message]:
        original_messages = context.messages.copy()
        truncated, removed = _truncate_messages(original_messages, max_tokens, context.model, strategy)
        context.messages = truncated
        context.recount_token_count()
        context.updated_at = time.time()
        context.metadata["last_activity_at"] = context.updated_at

        if removed:
            logger.info(
                f"Context '{context_id}' trimmed: removed {len(removed)} messages, "
                f"remaining {len(truncated)} messages, tokens={context.token_count}"
            )

        return removed

    def _apply_message_retention_unlocked(
        self,
        context_id: str,
        context: Context,
    ) -> list[Message]:
        message_cap = _resolve_message_cap(context.metadata, self._default_max_messages)
        if context.max_tokens > 0 or message_cap <= 0:
            return []

        truncated, removed = _trim_messages_to_limit(context.messages.copy(), message_cap)
        if not removed:
            return []

        context.messages = truncated
        context.recount_token_count()
        context.updated_at = time.time()
        context.metadata["message_count"] = len(context.messages)
        context.metadata["last_activity_at"] = context.updated_at
        context.metadata["max_messages"] = message_cap
        logger.info(
            f"Context '{context_id}' reached rolling message cap ({message_cap}), removed {len(removed)} old messages"
        )
        return removed

    async def get_context(self, context_id: str) -> Context:
        """获取上下文"""
        lock = await self._get_context_lock(context_id)
        async with lock:
            if context_id not in self._contexts:
                raise ContextNotFoundError(f"Context '{context_id}' not found")
            return self._contexts[context_id]

    async def save_context(self, context_id: str, context: Context) -> None:
        """保存上下文"""
        lock = await self._get_context_lock(context_id, create_if_missing=True)
        async with lock:
            context.recount_token_count()
            context.metadata.setdefault("max_messages", self._default_max_messages)
            self._apply_message_retention_unlocked(context_id, context)
            context.metadata["last_activity_at"] = context.updated_at
            async with self._global_lock:
                self._contexts[context_id] = context
            logger.debug(f"Context '{context_id}' saved")

    async def add_message(self, context_id: str, message: Message) -> None:
        """向上下文添加消息"""
        lock = await self._get_context_lock(context_id, create_if_missing=True)
        async with lock:
            if context_id not in self._contexts:
                context = _create_default_context(context_id, self._default_model)
                async with self._global_lock:
                    self._contexts[context_id] = context
                logger.info(f"Context '{context_id}' auto-created")

            context = self._contexts[context_id]
            context.add_message(message)
            context.metadata.setdefault("max_messages", self._default_max_messages)

            # 如果设置了最大令牌数且上下文已满，自动修剪
            if context.max_tokens and context.max_tokens > 0 and context.is_full():
                logger.info(f"Context '{context_id}' exceeded max tokens ({context.max_tokens}), auto-trimming")
                self._trim_context_unlocked(
                    context_id=context_id,
                    context=context,
                    max_tokens=context.max_tokens,
                    strategy="remove_oldest",
                )
            else:
                self._apply_message_retention_unlocked(context_id, context)

    async def trim_context(self, context_id: str, max_tokens: int, strategy: str = "remove_oldest") -> list[Message]:
        """
        修剪上下文，使其令牌数不超过限制

        Args:
            context_id: 上下文标识符
            max_tokens: 最大令牌数
            strategy: 修剪策略，参见 token_counter.truncate_messages_to_fit

        Returns:
            被移除的消息列表
        """
        lock = await self._get_context_lock(context_id)
        async with lock:
            if context_id not in self._contexts:
                raise ContextNotFoundError(f"Context '{context_id}' not found")

            context = self._contexts[context_id]
            return self._trim_context_unlocked(
                context_id=context_id,
                context=context,
                max_tokens=max_tokens,
                strategy=strategy,
            )

    async def delete_context(self, context_id: str) -> bool:
        """删除上下文"""
        lock = await self._get_context_lock(context_id)
        async with lock:
            async with self._global_lock:
                if context_id in self._contexts:
                    del self._contexts[context_id]
                    self._context_locks.pop(context_id, None)
                    self._context_lock_timestamps.pop(context_id, None)
                    logger.info(f"Context '{context_id}' deleted")
                    return True
            return False

    async def list_contexts(self, type_filter: ContextType | None = None) -> list[str]:
        """列出所有上下文 ID"""
        async with self._global_lock:
            if type_filter is None:
                return list(self._contexts.keys())

            return [ctx_id for ctx_id, ctx in self._contexts.items() if ctx.type == type_filter]

    async def compress_context(
        self, context_id: str, compression_ratio: float = 0.5, keep_system_messages: bool = True
    ) -> str:
        """
        压缩上下文：将旧消息总结为简短摘要

        注意：这是一个简单的实现，实际应用中可能需要调用 AI 模型进行总结。

        Args:
            context_id: 上下文标识符
            compression_ratio: 压缩比例（0-1），保留的消息比例
            keep_system_messages: 是否保留系统消息

        Returns:
            生成的摘要文本
        """
        lock = await self._get_context_lock(context_id)
        async with lock:
            if context_id not in self._contexts:
                raise ContextNotFoundError(f"Context '{context_id}' not found")

            context = self._contexts[context_id]

            if not context.messages:
                return ""

            # 简单实现：将旧消息的内容连接起来作为摘要
            # 实际项目中应调用 AI 模型生成摘要
            old_messages = context.messages[: -int(len(context.messages) * compression_ratio)]
            if not old_messages:
                return ""

            summary_parts = []
            for msg in old_messages:
                if keep_system_messages and msg.role == MessageRole.SYSTEM:
                    continue
                summary_parts.append(f"{msg.role}: {msg.content[:100]}...")

            summary = " | ".join(summary_parts)
            context.compressed = True
            context.compression_summary = summary

            # 移除被总结的消息
            context.messages = context.messages[-int(len(context.messages) * compression_ratio) :]
            context.recount_token_count()
            context.updated_at = time.time()
            context.metadata["last_activity_at"] = context.updated_at

            logger.info(f"Context '{context_id}' compressed, summary length: {len(summary)}")
            return summary

    async def cleanup_expired_contexts(self, ttl_seconds: int = 86400, max_contexts: int = 0) -> int:
        """
        清理过期上下文（基于更新时间）

        Args:
            ttl_seconds: 基准生存时间（秒）
            max_contexts: 上下文容量上限（0 表示不限）

        Returns:
            被清理的上下文数量
        """
        async with self._global_lock:
            now = time.time()
            snapshot = list(self._contexts.items())

        expired_ids: list[str] = []
        for ctx_id, ctx in snapshot:
            metadata_ttl = ctx.metadata.get("ttl")
            if metadata_ttl is not None:
                effective_ttl = int(metadata_ttl)
            else:
                effective_ttl = self._cache_policy.compute_ttl(
                    ctx,
                    base_ttl_seconds=ttl_seconds,
                    now=now,
                )

            if now - float(ctx.updated_at) > effective_ttl:
                expired_ids.append(ctx_id)

        expired_set = set(expired_ids)
        overflow_ids: list[str] = []
        remaining_items = [(ctx_id, ctx) for ctx_id, ctx in snapshot if ctx_id not in expired_set]
        if max_contexts > 0 and len(remaining_items) > max_contexts:
            overflow = len(remaining_items) - max_contexts
            ranked_contexts = sorted(
                remaining_items,
                key=lambda item: self._cache_policy.compute_eviction_score(item[1], now=now),
            )
            overflow_ids = [ctx_id for ctx_id, _ in ranked_contexts[:overflow]]

        to_delete = list(dict.fromkeys(expired_ids + overflow_ids))
        deleted_count = 0
        for ctx_id in to_delete:
            if await self.delete_context(ctx_id):
                deleted_count += 1

        if deleted_count:
            logger.info(f"Cleaned up {deleted_count} contexts by policy")

        await self.prune_stale_locks()

        return deleted_count


class RedisContextManager(ContextManager):
    """
    基于 Redis 的上下文管理器（支持持久化和分布式）

    Redis 数据结构：
    - ai:context:{id}:meta -> HASH（上下文元数据）
    - ai:context:{id}:msgs -> LIST（每个元素是一条 JSON 序列化消息）
    """

    def __init__(
        self,
        redis_url: str = "",
        default_model: str = "gpt-3.5-turbo",
        default_ttl: int = 604800,  # 7 天
        cache_policy: ContextCachePolicy | None = None,
        default_max_messages: int = _DEFAULT_ACTIVE_CONTEXT_MESSAGE_CAP,
    ):
        """
        初始化 Redis 上下文管理器

        Args:
            redis_url: Redis 连接 URL
            default_model: 默认模型名称
            default_ttl: 默认过期时间（秒）
        """
        try:
            import redis.asyncio as redis
        except ImportError:
            raise ImportError("需要安装 redis 库。请运行: pip install redis") from None
        if not str(redis_url or "").strip():
            raise ValueError("RedisContextManager requires an explicit redis_url")

        self.redis_client = redis.from_url(redis_url, decode_responses=False)
        self._key_prefix = "ai:context:"
        self._default_model = default_model
        self._default_ttl = default_ttl
        self._cache_policy = cache_policy or ContextCachePolicy(base_ttl_seconds=default_ttl)
        self._default_max_messages = max(0, int(default_max_messages or 0))
        self._global_lock = asyncio.Lock()
        self._context_locks: dict[str, asyncio.Lock] = {}
        self._context_lock_timestamps: dict[str, float] = {}
        self._last_lock_prune_at = 0.0
        logger.info(f"RedisContextManager 初始化完成，Redis URL: {redis_url}")

    def _ensure_runtime_state(self) -> None:
        if not hasattr(self, "_key_prefix"):
            self._key_prefix = "ai:context:"
        if not hasattr(self, "_default_model"):
            self._default_model = "gpt-3.5-turbo"
        if not hasattr(self, "_default_ttl"):
            self._default_ttl = 604800
        if not hasattr(self, "_cache_policy"):
            self._cache_policy = ContextCachePolicy(base_ttl_seconds=self._default_ttl)
        if not hasattr(self, "_default_max_messages"):
            self._default_max_messages = _DEFAULT_ACTIVE_CONTEXT_MESSAGE_CAP
        if not hasattr(self, "_global_lock"):
            self._global_lock = asyncio.Lock()
        if not hasattr(self, "_context_locks"):
            self._context_locks = {}
        if not hasattr(self, "_context_lock_timestamps"):
            self._context_lock_timestamps = {}
        if not hasattr(self, "_last_lock_prune_at"):
            self._last_lock_prune_at = 0.0

    async def _get_context_lock(
        self,
        context_id: str,
        *,
        create_if_missing: bool = False,
    ) -> asyncio.Lock:
        self._ensure_runtime_state()
        await self._maybe_prune_stale_locks()
        lock = self._context_locks.get(context_id)
        if lock is not None:
            self._context_lock_timestamps[context_id] = time.time()
            return lock

        if not create_if_missing and not await self._context_exists(context_id):
            return asyncio.Lock()

        async with self._global_lock:
            lock = self._context_locks.get(context_id)
            if lock is not None:
                self._context_lock_timestamps[context_id] = time.time()
                return lock
            lock = asyncio.Lock()
            self._context_locks[context_id] = lock
            self._context_lock_timestamps[context_id] = time.time()
            return lock

    async def _maybe_prune_stale_locks(self) -> None:
        now = time.time()
        if now - self._last_lock_prune_at < _LOCK_PRUNE_INTERVAL_SECONDS:
            return
        self._last_lock_prune_at = now
        await self.prune_stale_locks()

    async def _context_exists(self, context_id: str) -> bool:
        meta_key = self._make_meta_key(context_id)
        msgs_key = self._make_msgs_key(context_id)
        try:
            return bool(await self.redis_client.exists(meta_key, msgs_key))
        except (redis.exceptions.RedisError, ConnectionError, TimeoutError) as error:
            logger.warning(
                "redis_exists_failed",
                context_id=context_id,
                error=str(error),
                message="Redis exists check failed; assuming context exists to avoid unsafe lock pruning",
            )
            return True

    async def prune_stale_locks(self, max_idle_seconds: float = _LOCK_IDLE_TTL_SECONDS) -> int:
        self._ensure_runtime_state()
        now = time.time()
        candidates: list[str] = []

        async with self._global_lock:
            for context_id, lock in list(self._context_locks.items()):
                if lock.locked():
                    continue
                last_used = float(self._context_lock_timestamps.get(context_id, 0.0) or 0.0)
                if last_used and now - last_used < max_idle_seconds:
                    continue
                candidates.append(context_id)

        removed = 0
        for context_id in candidates:
            if await self._context_exists(context_id):
                continue
            async with self._global_lock:
                lock = self._context_locks.get(context_id)
                if lock is None or lock.locked():
                    continue
                self._context_locks.pop(context_id, None)
                self._context_lock_timestamps.pop(context_id, None)
                removed += 1
        return removed

    async def _get_or_create_context(self, context_id: str) -> Context:
        lock = await self._get_context_lock(context_id, create_if_missing=True)
        async with lock:
            try:
                return await self._get_context_unlocked(context_id)
            except ContextNotFoundError:
                context = _create_default_context(context_id, self._default_model)
                logger.info(f"Context '{context_id}' auto-created in Redis")
                return context

    def _trim_context_object(
        self,
        context_id: str,
        context: Context,
        max_tokens: int,
        strategy: str,
    ) -> list[Message]:
        original_messages = context.messages.copy()
        truncated, removed = _truncate_messages(
            original_messages,
            max_tokens,
            context.model,
            strategy,
        )
        context.messages = truncated
        context.recount_token_count()
        context.updated_at = time.time()
        context.metadata["last_activity_at"] = context.updated_at

        if removed:
            logger.info(
                f"Context '{context_id}' trimmed: removed {len(removed)} messages, "
                f"remaining {len(truncated)} messages, tokens={context.token_count}"
            )

        return removed

    def _make_key(self, context_id: str) -> str:
        """兼容旧实现：返回 meta 键。"""
        return self._make_meta_key(context_id)

    def _make_meta_key(self, context_id: str) -> str:
        return f"{self._key_prefix}{context_id}:meta"

    def _make_msgs_key(self, context_id: str) -> str:
        return f"{self._key_prefix}{context_id}:msgs"

    @staticmethod
    def _decode_if_bytes(value: Any) -> Any:
        if isinstance(value, bytes):
            return value.decode("utf-8")
        return value

    @classmethod
    def _to_str(cls, value: Any, default: str = "") -> str:
        decoded = cls._decode_if_bytes(value)
        if decoded is None:
            return default
        return str(decoded)

    @classmethod
    def _to_int(cls, value: Any, default: int = 0) -> int:
        decoded = cls._decode_if_bytes(value)
        if decoded is None:
            return default
        try:
            return int(decoded)
        except (TypeError, ValueError):
            return default

    @classmethod
    def _to_float(cls, value: Any, default: float = 0.0) -> float:
        decoded = cls._decode_if_bytes(value)
        if decoded is None:
            return default
        try:
            return float(decoded)
        except (TypeError, ValueError):
            return default

    def _resolve_ttl(self, context: Context) -> int:
        ttl_value = context.metadata.get("ttl")
        if ttl_value is not None:
            return int(ttl_value)

        computed_ttl = context.metadata.get("computed_ttl")
        if computed_ttl is not None:
            return int(computed_ttl)

        ttl = self._cache_policy.compute_ttl(
            context,
            base_ttl_seconds=self._default_ttl,
        )
        context.metadata["computed_ttl"] = ttl
        return ttl

    def _apply_message_retention(self, context_id: str, context: Context) -> list[Message]:
        message_cap = _resolve_message_cap(context.metadata, self._default_max_messages)
        if context.max_tokens > 0 or message_cap <= 0:
            return []

        truncated, removed = _trim_messages_to_limit(context.messages.copy(), message_cap)
        if not removed:
            return []

        context.messages = truncated
        context.recount_token_count()
        context.updated_at = time.time()
        context.metadata["message_count"] = len(context.messages)
        context.metadata["last_activity_at"] = context.updated_at
        context.metadata["max_messages"] = message_cap
        logger.info(
            f"Context '{context_id}' reached rolling message cap ({message_cap}), removed {len(removed)} old messages"
        )
        return removed

    def _build_meta_mapping(self, context: Context, ttl: int) -> dict[str, Any]:
        return {
            "id": context.id,
            "type": context.type.value,
            "created_at": f"{float(context.created_at):.6f}",
            "updated_at": f"{float(context.updated_at):.6f}",
            "model": context.model,
            "max_tokens": str(int(context.max_tokens)),
            "compressed": "1" if context.compressed else "0",
            "compression_summary": context.compression_summary or "",
            "token_count": str(int(context.token_count)),
            "message_count": str(len(context.messages)),
            "ttl": str(int(ttl)),
            "metadata_json": self._serialize_metadata(context.metadata),
        }

    @staticmethod
    def _serialize_metadata(metadata: dict[str, Any]) -> str:
        import json

        return json.dumps(metadata, ensure_ascii=False)

    @staticmethod
    def _deserialize_metadata(raw_metadata: Any) -> dict[str, Any]:
        import json

        value = raw_metadata.decode("utf-8") if isinstance(raw_metadata, bytes) else raw_metadata
        if not value:
            return {}
        try:
            loaded = json.loads(value)
        except JSON_EXCEPTIONS as exc:
            logger.warning("context_metadata_deserialize_failed", exc_info=exc)
            return {}
        return loaded if isinstance(loaded, dict) else {}

    def _serialize_context(self, context: Context) -> bytes:
        """序列化上下文对象为 JSON bytes（兼容工具函数）。"""
        import json

        data = {
            "id": context.id,
            "type": context.type.value,
            "messages": [self._message_to_dict(msg) for msg in context.messages],
            "metadata": context.metadata,
            "created_at": context.created_at,
            "updated_at": context.updated_at,
            "model": context.model,
            "max_tokens": context.max_tokens,
            "compressed": context.compressed,
            "compression_summary": context.compression_summary,
            "token_count": context.token_count,
        }

        return json.dumps(data, ensure_ascii=False).encode("utf-8")

    def _deserialize_context(self, data: bytes) -> Context:
        """反序列化 JSON bytes 为上下文对象（兼容工具函数）。"""
        import json

        obj = json.loads(data.decode("utf-8"))
        messages = [self._dict_to_message(msg_dict) for msg_dict in obj.get("messages", [])]

        raw_type = obj.get("type", ContextType.CONVERSATION.value)
        try:
            context_type = ContextType(raw_type)
        except ValueError:
            context_type = ContextType.CONVERSATION

        context = Context(
            id=obj["id"],
            type=context_type,
            messages=messages,
            metadata=obj.get("metadata", {}),
            created_at=obj.get("created_at", time.time()),
            updated_at=obj.get("updated_at", time.time()),
            model=obj.get("model", self._default_model),
            max_tokens=obj.get("max_tokens", 0),
            compressed=obj.get("compressed", False),
            compression_summary=obj.get("compression_summary"),
        )
        # 反序列化后全量重算一次，避免缓存与真实消息不一致。
        context.recount_token_count()
        return context

    def _message_to_dict(self, message: Message) -> dict[str, Any]:
        """将 Message 对象转换为字典"""
        d = {
            "role": message.role.value,
            "content": message.content,
            "name": message.name,
            "tool_calls": message.tool_calls,
            "tool_call_id": message.tool_call_id,
        }
        if message.metadata:
            d["metadata"] = message.metadata
        return d

    def _dict_to_message(self, data: dict[str, Any]) -> Message:
        """将字典转换为 Message 对象"""
        return Message(
            role=MessageRole(data["role"]),
            content=data["content"],
            name=data.get("name"),
            tool_calls=data.get("tool_calls"),
            tool_call_id=data.get("tool_call_id"),
            metadata=data.get("metadata"),
        )

    async def _get_context_unlocked(self, context_id: str) -> Context:
        meta_key = self._make_meta_key(context_id)
        msgs_key = self._make_msgs_key(context_id)

        pipe = self.redis_client.pipeline()
        pipe.hgetall(meta_key)
        pipe.lrange(msgs_key, 0, -1)
        meta_raw, msg_items = await pipe.execute()

        if not meta_raw:
            raise ContextNotFoundError(f"Context '{context_id}' not found in Redis")

        meta: dict[str, Any] = {
            self._to_str(key): value for key, value in (meta_raw.items() if isinstance(meta_raw, dict) else [])
        }

        messages: list[Message] = []
        for raw in msg_items or []:
            try:
                payload = raw.decode("utf-8") if isinstance(raw, bytes) else raw
                import json

                msg_dict = json.loads(payload)
                messages.append(self._dict_to_message(msg_dict))
            except (json.JSONDecodeError, KeyError, TypeError, ValueError) as exc:
                logger.warning(
                    "Skipping malformed message while loading context '%s'",
                    context_id,
                    exc_info=exc,
                )

        metadata = self._deserialize_metadata(meta.get("metadata_json"))
        raw_type = self._to_str(meta.get("type"), ContextType.CONVERSATION.value)
        try:
            context_type = ContextType(raw_type)
        except ValueError:
            context_type = ContextType.CONVERSATION

        context = Context(
            id=self._to_str(meta.get("id"), context_id),
            type=context_type,
            messages=messages,
            metadata=metadata,
            created_at=self._to_float(meta.get("created_at"), time.time()),
            updated_at=self._to_float(meta.get("updated_at"), time.time()),
            model=self._to_str(meta.get("model"), self._default_model),
            max_tokens=self._to_int(meta.get("max_tokens"), 0),
            compressed=self._to_str(meta.get("compressed"), "0") in {"1", "true", "True"},
            compression_summary=self._to_str(meta.get("compression_summary"), "") or None,
            _cached_token_count=self._to_int(meta.get("token_count"), 0),
        )

        if context.token_count <= 0 and context.messages:
            context.recount_token_count()
        context.metadata["message_count"] = len(context.messages)
        context.metadata["last_activity_at"] = context.updated_at
        return context

    async def _save_context_unlocked(self, context_id: str, context: Context) -> None:
        import json

        context.recount_token_count()
        context.metadata.setdefault("max_messages", self._default_max_messages)
        self._apply_message_retention(context_id, context)
        context.updated_at = time.time()
        context.metadata["message_count"] = len(context.messages)
        context.metadata["last_activity_at"] = context.updated_at

        ttl = self._resolve_ttl(context)
        meta_key = self._make_meta_key(context_id)
        msgs_key = self._make_msgs_key(context_id)

        messages_payload = [
            json.dumps(self._message_to_dict(msg), ensure_ascii=False).encode("utf-8") for msg in context.messages
        ]
        meta_mapping = self._build_meta_mapping(context, ttl)

        pipe = self.redis_client.pipeline()
        pipe.delete(msgs_key)
        if messages_payload:
            batch_size = 100
            for i in range(0, len(messages_payload), batch_size):
                pipe.rpush(msgs_key, *messages_payload[i : i + batch_size])
        pipe.hset(meta_key, mapping=meta_mapping)
        pipe.expire(meta_key, ttl)
        pipe.expire(msgs_key, ttl)
        await pipe.execute()

    async def get_context(self, context_id: str) -> Context:
        """从 Redis 获取上下文"""
        lock = await self._get_context_lock(context_id)
        async with lock:
            try:
                context = await self._get_context_unlocked(context_id)
                logger.debug(f"Context '{context_id}' loaded from Redis")
                return context
            except ContextNotFoundError:
                async with self._global_lock:
                    lock = self._context_locks.get(context_id)
                    if lock is not None and not lock.locked():
                        self._context_locks.pop(context_id, None)
                        self._context_lock_timestamps.pop(context_id, None)
                raise
            except REDIS_EXCEPTIONS as e:
                logger.error(f"Failed to get context '{context_id}' from Redis: {e}")
                raise ContextError(f"Redis get context error: {e}") from e

    async def save_context(self, context_id: str, context: Context) -> None:
        """保存上下文到 Redis（整体覆写）"""
        lock = await self._get_context_lock(context_id, create_if_missing=True)
        async with lock:
            try:
                await self._save_context_unlocked(context_id, context)
                logger.debug(f"Context '{context_id}' saved to Redis")
            except REDIS_EXCEPTIONS as e:
                logger.error(f"Failed to save context '{context_id}' to Redis: {e}")
                raise ContextError(f"Redis save context error: {e}") from e

    async def add_message(self, context_id: str, message: Message) -> None:
        """
        向上下文追加消息。

        常规路径使用 LIST 追加 + HASH 原子更新；仅在触发 token 裁剪时回退到全量读写。
        """
        import json

        from ..token_counter import count_message_tokens

        lock = await self._get_context_lock(context_id, create_if_missing=True)
        async with lock:
            try:
                meta_key = self._make_meta_key(context_id)
                msgs_key = self._make_msgs_key(context_id)
                meta_raw = await self.redis_client.hgetall(meta_key)
                now = time.time()

                if meta_raw:
                    meta: dict[str, Any] = {
                        self._to_str(key): value
                        for key, value in (meta_raw.items() if isinstance(meta_raw, dict) else [])
                    }
                    model = self._to_str(meta.get("model"), self._default_model)
                    max_tokens = self._to_int(meta.get("max_tokens"), 0)
                    token_count = self._to_int(meta.get("token_count"), 0)
                    message_count = self._to_int(meta.get("message_count"), 0)
                    created_at = self._to_float(meta.get("created_at"), now)
                    context_type = self._to_str(meta.get("type"), ContextType.CONVERSATION.value)
                    compressed = self._to_str(meta.get("compressed"), "0") in {"1", "true", "True"}
                    compression_summary = self._to_str(meta.get("compression_summary"), "")
                    metadata = self._deserialize_metadata(meta.get("metadata_json"))
                else:
                    meta = {}
                    model = self._default_model
                    max_tokens = 0
                    token_count = 0
                    message_count = 0
                    created_at = now
                    context_type = ContextType.CONVERSATION.value
                    compressed = False
                    compression_summary = ""
                    metadata = {}

                message_tokens = count_message_tokens(message, model)
                new_token_count = token_count + message_tokens
                new_message_count = message_count + 1
                metadata["message_count"] = new_message_count
                metadata["last_activity_at"] = now
                metadata.setdefault("max_messages", self._default_max_messages)
                message_cap = _resolve_message_cap(metadata, self._default_max_messages)

                if max_tokens > 0 and new_token_count > max_tokens:
                    try:
                        context = await self._get_context_unlocked(context_id)
                    except ContextNotFoundError:
                        context = _create_default_context(context_id, model)
                        context.max_tokens = max_tokens
                        context.metadata.update(metadata)
                    context.add_message(message)
                    logger.info(f"Context '{context_id}' exceeded max tokens ({context.max_tokens}), auto-trimming")
                    self._trim_context_object(
                        context_id=context_id,
                        context=context,
                        max_tokens=context.max_tokens,
                        strategy="remove_oldest",
                    )
                    await self._save_context_unlocked(context_id, context)
                    return

                if max_tokens <= 0 and message_cap > 0 and new_message_count > message_cap:
                    try:
                        context = await self._get_context_unlocked(context_id)
                    except ContextNotFoundError:
                        context = _create_default_context(context_id, model)
                        context.metadata.update(metadata)
                    context.add_message(message)
                    context.metadata["max_messages"] = message_cap
                    self._apply_message_retention(context_id, context)
                    await self._save_context_unlocked(context_id, context)
                    return

                ttl_from_meta = metadata.get("ttl")
                if ttl_from_meta is not None:
                    ttl = int(ttl_from_meta)
                else:
                    ttl = self._to_int(meta.get("ttl"), 0)
                    if ttl <= 0:
                        ttl = self._to_int(metadata.get("computed_ttl"), self._default_ttl)
                    if ttl <= 0:
                        ttl = self._default_ttl
                    metadata["computed_ttl"] = ttl

                meta_mapping = {
                    "id": context_id,
                    "type": context_type,
                    "created_at": f"{created_at:.6f}",
                    "updated_at": f"{now:.6f}",
                    "model": model,
                    "max_tokens": str(max_tokens),
                    "compressed": "1" if compressed else "0",
                    "compression_summary": compression_summary,
                    "token_count": str(new_token_count),
                    "message_count": str(new_message_count),
                    "ttl": str(ttl),
                    "metadata_json": self._serialize_metadata(metadata),
                }
                msg_data = json.dumps(self._message_to_dict(message), ensure_ascii=False).encode("utf-8")

                pipe = self.redis_client.pipeline()
                pipe.rpush(msgs_key, msg_data)
                pipe.hset(meta_key, mapping=meta_mapping)
                pipe.expire(meta_key, ttl)
                pipe.expire(msgs_key, ttl)
                await pipe.execute()
            except REDIS_EXCEPTIONS as e:
                logger.error(f"Failed to add message to context '{context_id}': {e}")
                raise ContextError(f"Redis add message error: {e}") from e

    async def trim_context(self, context_id: str, max_tokens: int, strategy: str = "remove_oldest") -> list[Message]:
        """修剪上下文，使其令牌数不超过限制。"""
        lock = await self._get_context_lock(context_id)
        async with lock:
            try:
                context = await self._get_context_unlocked(context_id)
                original_messages = context.messages.copy()
                truncated, removed = _truncate_messages(
                    original_messages,
                    max_tokens,
                    context.model,
                    strategy,
                )
                if not removed:
                    return []

                context.messages = truncated
                context.recount_token_count()
                context.updated_at = time.time()
                context.metadata["last_activity_at"] = context.updated_at

                ttl = self._resolve_ttl(context)
                meta_mapping = self._build_meta_mapping(context, ttl)
                meta_key = self._make_meta_key(context_id)
                msgs_key = self._make_msgs_key(context_id)

                pipe = self.redis_client.pipeline()
                if strategy == "remove_oldest":
                    keep_count = len(truncated)
                    if keep_count <= 0:
                        pipe.delete(msgs_key)
                    else:
                        start = len(original_messages) - keep_count
                        end = len(original_messages) - 1
                        pipe.ltrim(msgs_key, start, end)
                    pipe.hset(meta_key, mapping=meta_mapping)
                    pipe.expire(meta_key, ttl)
                    pipe.expire(msgs_key, ttl)
                    await pipe.execute()
                elif strategy == "remove_newest":
                    keep_count = len(truncated)
                    if keep_count <= 0:
                        pipe.delete(msgs_key)
                    else:
                        pipe.ltrim(msgs_key, 0, keep_count - 1)
                    pipe.hset(meta_key, mapping=meta_mapping)
                    pipe.expire(meta_key, ttl)
                    pipe.expire(msgs_key, ttl)
                    await pipe.execute()
                else:
                    await self._save_context_unlocked(context_id, context)

                logger.info(
                    f"Context '{context_id}' trimmed: removed {len(removed)} messages, "
                    f"remaining {len(truncated)} messages, tokens={context.token_count}"
                )
                return removed
            except ContextNotFoundError:
                raise
            except REDIS_EXCEPTIONS as e:
                logger.error(f"Failed to trim context '{context_id}': {e}")
                raise ContextError(f"Redis trim context error: {e}") from e

    async def delete_context(self, context_id: str) -> bool:
        """从 Redis 删除上下文"""
        lock = await self._get_context_lock(context_id)
        async with lock:
            try:
                meta_key = self._make_meta_key(context_id)
                msgs_key = self._make_msgs_key(context_id)
                pipe = self.redis_client.pipeline()
                pipe.delete(meta_key)
                pipe.delete(msgs_key)
                deleted_meta, deleted_msgs = await pipe.execute()
                deleted = (deleted_meta + deleted_msgs) > 0
                if deleted:
                    logger.info(f"Context '{context_id}' deleted from Redis")
                async with self._global_lock:
                    self._context_locks.pop(context_id, None)
                    self._context_lock_timestamps.pop(context_id, None)
                return deleted
            except REDIS_EXCEPTIONS as e:
                logger.error(f"Failed to delete context '{context_id}': {e}")
                return False

    async def list_contexts(self, type_filter: ContextType | None = None) -> list[str]:
        """列出 Redis 中所有上下文 ID（基于 meta 键）。"""
        try:
            pattern = f"{self._key_prefix}*:meta"
            context_ids: list[str] = []
            cursor = 0

            while True:
                cursor, keys = await self.redis_client.scan(cursor, match=pattern, count=100)
                for key in keys:
                    key_str = self._to_str(key)
                    raw_id = key_str[len(self._key_prefix) :]
                    context_id = raw_id[:-5] if raw_id.endswith(":meta") else raw_id
                    context_ids.append(context_id)
                if cursor == 0:
                    break

            if type_filter is None:
                return context_ids

            if not context_ids:
                return []

            pipe = self.redis_client.pipeline()
            for ctx_id in context_ids:
                pipe.hget(self._make_meta_key(ctx_id), "type")

            try:
                raw_types = await pipe.execute()
            except REDIS_EXCEPTIONS as exc:
                logger.warning("redis_pipeline_type_lookup_failed", exc_info=exc)
                raw_types = []

            if not raw_types:
                return []

            return [
                ctx_id
                for ctx_id, raw_type in zip(context_ids, raw_types, strict=False)
                if self._to_str(raw_type) == type_filter.value
            ]
        except REDIS_EXCEPTIONS as e:
            logger.error(f"Failed to list contexts: {e}")
            return []


# 全局默认管理器（内存）


def get_default_manager() -> ContextManager:
    """获取默认的上下文管理器实例（内存）"""
    from src.infrastructure.runtime.providers import get_runtime_container

    container = get_runtime_container()
    if container is not None:
        _default_manager = getattr(container, "default_manager", None)
        if _default_manager is not None:
            return _default_manager
    _default_manager = MemoryContextManager()
    if container is not None:
        container.default_manager = _default_manager
    return _default_manager


def set_default_manager(manager: ContextManager) -> None:
    """设置默认的上下文管理器"""
    from src.infrastructure.runtime.providers import get_runtime_container

    container = get_runtime_container()
    if container is not None:
        container.default_manager = manager
