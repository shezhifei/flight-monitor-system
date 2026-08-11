"""
上下文管理器 - 数据模型

包含上下文相关的数据结构、枚举、异常和辅助函数。
"""

import time
from dataclasses import dataclass, field
from enum import StrEnum
from typing import Any

from ..openai_client import Message, MessageRole

_LOCK_PRUNE_INTERVAL_SECONDS = 300.0
_LOCK_IDLE_TTL_SECONDS = 3600.0
_DEFAULT_ACTIVE_CONTEXT_MESSAGE_CAP = 1000


class ContextType(StrEnum):
    """上下文类型枚举"""

    CONVERSATION = "conversation"
    SYSTEM_PROMPT = "system_prompt"
    USER_PROFILE = "user_profile"
    KNOWLEDGE_BASE = "knowledge_base"
    TEMPORARY = "temporary"


@dataclass
class Context:
    """
    上下文数据结构

    Attributes:
        id: 上下文唯一标识符
        type: 上下文类型
        messages: 消息列表（按时间顺序）
        metadata: 任意元数据字典
        created_at: 创建时间戳（秒）
        updated_at: 最后更新时间戳
        model: 关联的模型（用于令牌计数）
        max_tokens: 最大令牌限制（0 表示无限制）
        compressed: 是否已压缩（例如已总结）
        compression_summary: 压缩后的总结文本（如果有）
    """

    id: str
    type: ContextType = ContextType.CONVERSATION
    messages: list[Message] = field(default_factory=list)
    metadata: dict[str, Any] = field(default_factory=dict)
    created_at: float = field(default_factory=time.time)
    updated_at: float = field(default_factory=time.time)
    model: str = "gpt-3.5-turbo"
    max_tokens: int = 0
    compressed: bool = False
    compression_summary: str | None = None
    _cached_token_count: int = 0

    def __post_init__(self) -> None:
        """初始化令牌缓存和基础元数据。"""
        if self.messages and self._cached_token_count <= 0:
            self.recount_token_count()
        else:
            self.metadata["message_count"] = len(self.messages)
        self.metadata.setdefault("last_activity_at", self.updated_at)

    @property
    def token_count(self) -> int:
        """返回当前消息令牌缓存值。"""
        return self._cached_token_count

    def recount_token_count(self) -> int:
        """全量重算上下文令牌数，并更新缓存。"""
        from ..token_counter import count_messages_tokens

        self._cached_token_count = count_messages_tokens(self.messages, self.model)
        self.metadata["message_count"] = len(self.messages)
        return self._cached_token_count

    def is_full(self) -> bool:
        """检查上下文是否已满（超过最大令牌限制）"""
        if self.max_tokens <= 0:
            return False
        return self.token_count > self.max_tokens

    def add_message(self, message: Message) -> None:
        """添加消息并更新时间戳"""
        from ..token_counter import count_message_tokens

        self.messages.append(message)
        self._cached_token_count += count_message_tokens(message, self.model)
        self.updated_at = time.time()
        self.metadata["message_count"] = len(self.messages)
        self.metadata["last_activity_at"] = self.updated_at

    def clear_messages(self) -> None:
        """清空所有消息"""
        self.messages.clear()
        self._cached_token_count = 0
        self.updated_at = time.time()
        self.metadata["message_count"] = 0
        self.metadata["last_activity_at"] = self.updated_at

    def get_recent_messages(self, limit: int = 10) -> list[Message]:
        """获取最近的消息（最新的在末尾）"""
        return self.messages[-limit:] if limit > 0 else self.messages


def _create_default_context(context_id: str, default_model: str) -> Context:
    return Context(
        id=context_id,
        type=ContextType.CONVERSATION,
        model=default_model,
    )


def _truncate_messages(
    messages: list[Message],
    max_tokens: int,
    model: str,
    strategy: str,
) -> tuple[list[Message], list[Message]]:
    from ..token_counter import truncate_messages_to_fit

    truncated = truncate_messages_to_fit(messages, max_tokens, model, strategy)
    removed = messages[: len(messages) - len(truncated)]
    return truncated, removed


def _resolve_message_cap(metadata: dict[str, Any], default_cap: int) -> int:
    raw_value = metadata.get("max_messages")
    if raw_value is None:
        return max(0, int(default_cap or 0))

    try:
        resolved = int(raw_value)
    except (TypeError, ValueError):
        return max(0, int(default_cap or 0))

    return max(0, resolved)


def _trim_messages_to_limit(
    messages: list[Message],
    max_messages: int,
) -> tuple[list[Message], list[Message]]:
    if max_messages <= 0 or len(messages) <= max_messages:
        return messages, []

    leading_system_count = 0
    for message in messages:
        if message.role != MessageRole.SYSTEM:
            break
        leading_system_count += 1

    if leading_system_count >= max_messages:
        return messages[:max_messages], messages[max_messages:]

    preserved = messages[:leading_system_count]
    rolling_messages = messages[leading_system_count:]
    keep_count = max_messages - len(preserved)
    kept = preserved + rolling_messages[-keep_count:]
    removed = rolling_messages[:-keep_count]
    return kept, removed


def _clamp(value: float, minimum: float, maximum: float) -> float:
    return max(minimum, min(maximum, value))


@dataclass
class ContextCachePolicy:
    """上下文缓存策略（活跃度 + 上下文大小 + 用户优先级）。"""

    base_ttl_seconds: int = 86400
    min_ttl_seconds: int = 900
    max_ttl_seconds: int = 604800
    activity_window_seconds: int = 3600
    message_budget: int = 40

    def _priority_score(self, context: Context) -> float:
        priority = str(context.metadata.get("user_priority", "normal")).strip().lower()
        scores = {
            "critical": 1.0,
            "high": 0.9,
            "vip": 0.9,
            "normal": 0.6,
            "medium": 0.6,
            "low": 0.2,
        }
        if context.metadata.get("is_priority_user") is True:
            return 1.0
        return scores.get(priority, 0.6)

    def _activity_score(self, context: Context, now: float | None = None) -> float:
        current = now if now is not None else time.time()
        idle_seconds = max(0.0, current - float(context.updated_at))
        if self.activity_window_seconds <= 0:
            return 1.0
        return _clamp(1.0 - (idle_seconds / float(self.activity_window_seconds)), 0.0, 1.0)

    def _context_size_pressure(self, context: Context) -> float:
        if context.max_tokens and context.max_tokens > 0:
            estimated_budget = max(1, int(context.max_tokens / 200))
        else:
            estimated_budget = max(1, self.message_budget)
        return _clamp(len(context.messages) / float(estimated_budget), 0.0, 1.0)

    def compute_ttl(
        self,
        context: Context,
        *,
        base_ttl_seconds: int | None = None,
        now: float | None = None,
    ) -> int:
        """根据上下文画像计算动态 TTL。"""
        base_ttl = int(base_ttl_seconds or self.base_ttl_seconds)
        activity = self._activity_score(context, now=now)
        priority = self._priority_score(context)
        size_pressure = self._context_size_pressure(context)

        factor = 0.55 + (0.40 * activity) + (0.35 * priority) - (0.30 * size_pressure)
        factor = _clamp(factor, 0.20, 1.80)

        ttl = int(base_ttl * factor)
        return int(_clamp(ttl, float(self.min_ttl_seconds), float(self.max_ttl_seconds)))

    def compute_eviction_score(self, context: Context, now: float | None = None) -> float:
        """分数越低越应优先淘汰。"""
        activity = self._activity_score(context, now=now)
        priority = self._priority_score(context)
        size_pressure = self._context_size_pressure(context)
        return (0.45 * activity) + (0.35 * priority) - (0.20 * size_pressure)


# 自定义异常
class ContextError(Exception):
    """上下文相关错误的基类"""


class ContextNotFoundError(ContextError):
    """上下文未找到"""


class ContextLimitExceededError(ContextError):
    """上下文令牌超限"""


class InvalidContextError(ContextError):
    """无效上下文"""
