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
    Context,
    ContextCachePolicy,
    ContextError,
    ContextLimitExceededError,
    ContextNotFoundError,
    ContextType,
    InvalidContextError,
)

__all__ = [
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
    # Default manager helpers
    "get_default_manager",
    # Module-level logger
    "logger",
    "set_default_manager",
]
