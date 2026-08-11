"""
上下文管理器

实现上下文存储和管理，支持令牌计数、上下文窗口管理、自动裁剪和持久化。

本包是原 context_manager 模块的 facade，将其拆分为：
- models: 数据结构、枚举、异常和辅助函数
- manager: 管理器抽象基类和具体实现（内存、Redis）
"""

from .manager import (
    ContextManager,
    MemoryContextManager,
    RedisContextManager,
    get_default_manager,
    logger,
    set_default_manager,
)
from .models import (
    _DEFAULT_ACTIVE_CONTEXT_MESSAGE_CAP,
    _LOCK_IDLE_TTL_SECONDS,
    _LOCK_PRUNE_INTERVAL_SECONDS,
    Context,
    ContextCachePolicy,
    ContextError,
    ContextLimitExceededError,
    ContextNotFoundError,
    ContextType,
    InvalidContextError,
    _clamp,
    _create_default_context,
    _resolve_message_cap,
    _trim_messages_to_limit,
    _truncate_messages,
)

__all__ = [
    "_DEFAULT_ACTIVE_CONTEXT_MESSAGE_CAP",
    "_LOCK_IDLE_TTL_SECONDS",
    "_LOCK_PRUNE_INTERVAL_SECONDS",
    # Dataclasses
    "Context",
    "ContextCachePolicy",
    # Exceptions
    "ContextError",
    "ContextLimitExceededError",
    # Abstract & concrete managers
    "ContextManager",
    "ContextNotFoundError",
    # Enums
    "ContextType",
    "InvalidContextError",
    "MemoryContextManager",
    "RedisContextManager",
    "_clamp",
    # Private helpers (re-exported for backward compatibility)
    "_create_default_context",
    "_resolve_message_cap",
    "_trim_messages_to_limit",
    "_truncate_messages",
    # Default manager helpers
    "get_default_manager",
    # Module-level logger
    "logger",
    "set_default_manager",
]
