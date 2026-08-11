"""
通知端口接口

定义领域层可使用的通知抽象。
"""

from abc import ABC, abstractmethod
from typing import Any


class NotificationPort(ABC):
    """通知端口接口"""

    @abstractmethod
    async def notify_user(
        self,
        user_id: str,
        title: str,
        body: str,
        category: str,
        severity: str = "info",
        flight_id: str | None = None,
        dispatch_order_id: str | None = None,
        group_id: str | None = None,
        related_entity_type: str | None = None,
        related_entity_id: str | None = None,
        origin_type: str = "manual",
        receipt_required: bool = False,
        receipt_group_id: str | None = None,
        sender_user_id: str | None = None,
        sender_username_snapshot: str | None = None,
    ) -> None:
        """发送用户通知"""
        ...

    @abstractmethod
    async def notify_ai_event(self, event_type: str, data: dict[str, Any]) -> None:
        """发送 AI 事件通知"""
        ...
