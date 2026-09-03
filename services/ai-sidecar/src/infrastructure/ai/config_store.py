"""
AI配置存储接口

定义AI实体配置的存储契约，配置统一持久化到数据库。
"""

from abc import ABC, abstractmethod
from typing import Any


class AIConfigStoreInterface(ABC):
    """AI配置存储接口"""

    @abstractmethod
    async def get_all(self) -> dict[str, dict[str, Any]]:
        """获取所有实体配置"""
        pass

    @abstractmethod
    async def get(self, entity_id: str) -> dict[str, Any] | None:
        """获取指定实体的配置"""
        pass

    # ai_entities 的写路径（含默认与 pilot 实体播种）由 Rust 侧统一持有（ADR-0004）；
    # Python 侧只读，防双写分叉。
    @abstractmethod
    async def reload(self) -> None:
        """重新加载配置"""
        pass


__all__ = [
    "AIConfigStoreInterface",
]
