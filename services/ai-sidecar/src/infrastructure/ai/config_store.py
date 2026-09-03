"""
AI配置存储接口

定义AI实体配置的存储契约，配置统一持久化到数据库。
"""

from abc import ABC, abstractmethod
from typing import Any

from src.domain.ai.todo_graph_pilot import DEFAULT_TODO_GRAPH_PILOT_ENTITY_ID
from src.infrastructure.ai.config.config_normalizer import default_entity_document
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


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

    # ai_entities 的写路径由 Rust 侧统一持有（ADR-0004）；Python 侧只读，防双写分叉。
    @abstractmethod
    async def reload(self) -> None:
        """重新加载配置"""
        pass


def build_default_entity_config() -> dict[str, Any]:
    """Return an isolated default entity document."""
    return default_entity_document()


def build_seed_entity_configs() -> dict[str, dict[str, Any]]:
    """Return the AI entities that should exist in a fresh runtime."""
    return {
        "default": build_default_entity_config(),
        DEFAULT_TODO_GRAPH_PILOT_ENTITY_ID: build_default_entity_config(),
    }


# 默认种子配置 - 完整的AI配置模板，存储于数据库
DEFAULT_CONFIG = build_seed_entity_configs()


__all__ = [
    "DEFAULT_CONFIG",
    "AIConfigStoreInterface",
    "build_default_entity_config",
    "build_seed_entity_configs",
]
