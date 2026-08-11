"""
对话管理器 - 数据模型

包含对话状态枚举、元数据、对话实体、异常类及辅助函数。
"""

import time
from dataclasses import dataclass, field
from enum import Enum, StrEnum
from typing import Any


def _resolve_user_priority(
    user_info: dict[str, Any] | None,
    custom_data: dict[str, Any] | None,
) -> str:
    for source in (custom_data or {}, user_info or {}):
        for key in ("user_priority", "priority"):
            value = source.get(key)
            if value is not None:
                normalized = str(value).strip().lower()
                if normalized:
                    return normalized
    return "normal"


def _build_context_metadata(
    user_id: str | None,
    user_info: dict[str, Any] | None,
    custom_data: dict[str, Any] | None,
    max_tokens: int,
    last_activity_at: float | None,
    max_messages: int | None = None,
) -> dict[str, Any]:
    metadata = {
        "user_id": user_id,
        "user_priority": _resolve_user_priority(user_info, custom_data),
        "max_tokens": max_tokens,
        "last_activity_at": last_activity_at,
        "max_messages": max_messages,
    }
    return {key: value for key, value in metadata.items() if value is not None}


class ConversationStatus(StrEnum):
    """对话状态枚举"""

    ACTIVE = "active"  # 活跃
    PAUSED = "paused"  # 暂停
    ENDED = "ended"  # 已结束
    EXPIRED = "expired"  # 已过期


@dataclass
class ConversationMetadata:
    """对话元数据"""

    user_id: str | None = None
    user_name: str | None = None
    user_info: dict[str, Any] | None = None
    session_id: str | None = None
    client_info: dict[str, Any] | None = None  # 客户端信息（浏览器、设备等）
    tags: list[str] = field(default_factory=list)
    custom_data: dict[str, Any] = field(default_factory=dict)

    # 时间戳
    created_at: float = field(default_factory=time.time)
    updated_at: float = field(default_factory=time.time)
    last_activity_at: float = field(default_factory=time.time)
    ended_at: float | None = None

    # 统计信息
    message_count: int = 0
    total_tokens: int = 0
    total_cost: float = 0.0  # 估算成本（美元）

    def to_dict(self) -> dict[str, Any]:
        """将元数据转换为字典，支持 JSON 序列化"""
        return {
            "user_id": self.user_id,
            "user_name": self.user_name,
            "user_info": self.user_info,
            "session_id": self.session_id,
            "client_info": self.client_info,
            "tags": self.tags,
            "custom_data": self.custom_data,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
            "last_activity_at": self.last_activity_at,
            "ended_at": self.ended_at,
            "message_count": self.message_count,
            "total_tokens": self.total_tokens,
            "total_cost": self.total_cost,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "ConversationMetadata":
        """从字典创建元数据对象"""
        return cls(
            user_id=data.get("user_id"),
            user_name=data.get("user_name"),
            user_info=data.get("user_info"),
            session_id=data.get("session_id"),
            client_info=data.get("client_info"),
            tags=data.get("tags", []),
            custom_data=data.get("custom_data", {}),
            created_at=data.get("created_at", time.time()),
            updated_at=data.get("updated_at", time.time()),
            last_activity_at=data.get("last_activity_at", time.time()),
            ended_at=data.get("ended_at"),
            message_count=data.get("message_count", 0),
            total_tokens=data.get("total_tokens", 0),
            total_cost=data.get("total_cost", 0.0),
        )


@dataclass
class Conversation:
    """对话实体"""

    id: str
    title: str | None = None
    status: ConversationStatus = ConversationStatus.ACTIVE
    metadata: ConversationMetadata = field(default_factory=ConversationMetadata)

    # 关联的上下文 ID（用于存储消息历史）
    context_id: str | None = None

    # 关联的 AI 模型信息
    model: str = "gpt-3.5-turbo"
    temperature: float = 0.7
    max_tokens: int = 0  # 0 表示无限制

    # 系统提示词
    system_prompt: str | None = None

    # 父对话 ID（用于分支对话）
    parent_id: str | None = None

    # 扩展字段
    extensions: dict[str, Any] = field(default_factory=dict)

    def is_active(self) -> bool:
        """是否活跃"""
        return self.status == ConversationStatus.ACTIVE

    def is_expired(self, ttl_seconds: int = 86400) -> bool:
        """是否过期"""
        return time.time() - self.metadata.last_activity_at > ttl_seconds

    def update_activity(self) -> None:
        """更新活动时间"""
        self.metadata.last_activity_at = time.time()
        self.metadata.updated_at = time.time()

    def to_dict(self) -> dict[str, Any]:
        """将对话对象转换为字典，支持 JSON 序列化"""
        return {
            "id": self.id,
            "title": self.title,
            "status": self.status.value if isinstance(self.status, Enum) else self.status,
            "metadata": self.metadata.to_dict(),
            "context_id": self.context_id,
            "model": self.model,
            "temperature": self.temperature,
            "max_tokens": self.max_tokens,
            "system_prompt": self.system_prompt,
            "parent_id": self.parent_id,
            "extensions": self.extensions,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "Conversation":
        """从字典创建对话对象"""
        status_str = data.get("status", ConversationStatus.ACTIVE.value)
        try:
            status = ConversationStatus(status_str)
        except (ValueError, TypeError):
            status = ConversationStatus.ACTIVE

        metadata_dict = data.get("metadata", {})
        metadata = ConversationMetadata.from_dict(metadata_dict) if metadata_dict else ConversationMetadata()

        return cls(
            id=data["id"],
            title=data.get("title"),
            status=status,
            metadata=metadata,
            context_id=data.get("context_id"),
            model=data.get("model", "gpt-3.5-turbo"),
            temperature=data.get("temperature", 0.7),
            max_tokens=data.get("max_tokens", 0),
            system_prompt=data.get("system_prompt"),
            parent_id=data.get("parent_id"),
            extensions=data.get("extensions", {}),
        )


# 自定义异常
class ConversationError(Exception):
    """对话相关错误的基类"""


class ConversationNotFoundError(ConversationError):
    """对话未找到"""


class ConversationLimitExceededError(ConversationError):
    """对话限制超限"""


class InvalidConversationError(ConversationError):
    """无效对话"""
