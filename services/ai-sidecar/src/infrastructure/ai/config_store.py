"""
AI配置存储接口

定义AI实体配置的存储契约。
强制执行数据库持久化，已移除本地文件存储实现。
"""

from abc import ABC, abstractmethod
from copy import deepcopy
from typing import Any

from src.domain.ai.todo_graph_pilot import DEFAULT_TODO_GRAPH_PILOT_ENTITY_ID
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

    async def get_entity_config(self, entity_id: str) -> dict[str, Any] | None:
        """获取实体配置（别名，为了兼容性）"""
        return await self.get(entity_id)

    @abstractmethod
    async def update(self, entity_id: str, config: dict[str, Any]) -> dict[str, Any]:
        """更新实体配置"""
        pass

    @abstractmethod
    async def delete(self, entity_id: str) -> bool:
        """删除实体配置"""
        pass

    @abstractmethod
    async def reload(self) -> None:
        """重新加载配置"""
        pass


_DEFAULT_ENTITY_CONFIG_TEMPLATE = {
    # === 连接配置 ===
    "api_key": "",
    "base_url": "https://api.openai.com/v1",
    "default_model": "gpt-3.5-turbo",
    "api_format": "chat_completions",  # "chat_completions" | "responses"
    # === 运行时参数 (原 ai_dynamic_config.yaml) ===
    "temperature": 0.7,
    "max_tokens": 2000,
    "top_p": 0.95,
    "frequency_penalty": 0.0,
    "presence_penalty": 0.0,
    # === 网络参数 ===
    "timeout": 30.0,
    "max_retries": 3,
    "retry_delay": 0.5,
    # === 模型元数据 ===
    "cost_per_1k_input": 0.0015,
    "cost_per_1k_output": 0.002,
    "context_window": 128000,
    # === 工具执行配置 (原 ai_dynamic_config.yaml -> tools) ===
    "tools": {
        "timeout": 30,
        "max_retries": 3,
        "retry_delay": 1.0,
        "auto_execute": True,
    },
    # === 监控配置 (原 ai_dynamic_config.yaml -> monitoring) ===
    "monitoring": {
        "metrics_enabled": True,
        "trace_enabled": False,
        "log_prompts": False,
        "mask_sensitive": True,
    },
    # === 多模态端点配置 (v6.0 新增) ===
    "endpoints": {
        "chat": None,  # DeepSeek endpoint
        "vision": None,  # GLM-4V endpoint
        "asr": None,  # SenseVoice endpoint
        "tts": None,  # CosyVoice endpoint
    },
    # === 工具权限 ===
    "allowed_tool_categories": ["flight", "flight_event", "todo", "business_case"],
    "allowed_tools": None,
    "denied_tools": [],
    # === Todo Agent graph rollout ===
    "todo_agent_graph_enabled": False,
    "todo_agent_graph_runtime_enabled": False,
    "graph_runtime_enabled": False,
    # === Prompt Cache ===
    "prompt_cache": {
        "enabled": False,
        "retention": None,  # "in_memory" | "24h"
        "namespace": "flight_monitor",
    },
    # === 提示词 ===
    "system_prompt": "你是一个航班监控系统的AI助手，可以帮助用户查询航班信息、管理航班事件和待办事项。",
    "task_template": None,
}


def build_default_entity_config() -> dict[str, Any]:
    """Return an isolated default entity config template."""
    return deepcopy(_DEFAULT_ENTITY_CONFIG_TEMPLATE)


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
