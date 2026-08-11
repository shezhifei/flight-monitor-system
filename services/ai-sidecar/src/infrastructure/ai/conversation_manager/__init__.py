"""
对话管理器 - facade 模式

重新导出所有公共符号，保持向后兼容的导入路径：
    from src.infrastructure.ai.conversation_manager import ConversationManager

实现拆分为两个子模块：
    - models:  数据模型、枚举、异常及辅助函数
    - manager: 管理器抽象基类、内存/Redis 实现、工厂函数
"""

from .manager import (
    ConversationManager,
    MemoryConversationManager,
    RedisConversationManager,
    get_default_conversation_manager,
    set_default_conversation_manager,
)
from .models import (
    Conversation,
    ConversationError,
    ConversationLimitExceededError,
    ConversationMetadata,
    ConversationNotFoundError,
    ConversationStatus,
    InvalidConversationError,
    _build_context_metadata,
    _resolve_user_priority,
)

__all__ = [
    "Conversation",
    "ConversationError",
    "ConversationLimitExceededError",
    "ConversationManager",
    "ConversationMetadata",
    "ConversationNotFoundError",
    "ConversationStatus",
    "InvalidConversationError",
    "MemoryConversationManager",
    "RedisConversationManager",
    "_build_context_metadata",
    "_resolve_user_priority",
    "get_default_conversation_manager",
    "set_default_conversation_manager",
]
