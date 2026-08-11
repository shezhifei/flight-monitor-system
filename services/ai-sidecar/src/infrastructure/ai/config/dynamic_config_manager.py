"""
动态配置管理器 (已弃用 - 仅保留向后兼容性)

此模块已被数据库驱动的配置系统取代。
所有配置现在从 PostgresAIConfigStore 加载。

@deprecated: 请使用 AIConfigLoader.load_config_async() 替代
"""

import asyncio
import warnings
from typing import Any

from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class DynamicConfigManager:
    """
    动态配置管理器 (已弃用)

    此类已被标记为弃用。所有 AI 配置现在从数据库加载。
    保留此类仅为向后兼容性。

    迁移指南:
        旧代码:
            mgr = DynamicConfigManager()
            config = mgr.load_file_config()

        新代码:
            from src.infrastructure.ai.config import AIConfigLoader
            loader = AIConfigLoader(config_store)
            config = await loader.load_config_async()
    """

    def __init__(self, config_dir: str = "config"):
        warnings.warn(
            "DynamicConfigManager 已弃用，请使用 AIConfigLoader 从数据库加载配置", DeprecationWarning, stacklevel=2
        )
        self._config: dict[str, Any] = {}
        self._lock = asyncio.Lock()
        logger.warning("DynamicConfigManager 已弃用，配置应从数据库加载")

    async def load_config(self) -> dict[str, Any]:
        """
        加载配置 (已弃用)

        返回空配置，实际配置应从数据库获取。
        """
        async with self._lock:
            # 返回默认配置
            from src.infrastructure.ai.config_store import build_default_entity_config

            return build_default_entity_config()

    def load_file_config(self, file_path: str | None = None) -> dict[str, Any]:
        """
        从文件加载配置 (已弃用)

        优先兼容旧行为：当提供 file_path 时尝试读取该文件。
        若读取失败，则回退到数据库默认配置。
        """
        if file_path:
            try:
                import yaml

                with open(file_path, encoding="utf-8") as f:
                    loaded = yaml.safe_load(f) or {}

                if isinstance(loaded, dict):
                    logger.debug(f"load_file_config() 读取已弃用配置文件: {file_path}")
                    return loaded
                logger.warning(f"配置文件内容不是字典，忽略: {file_path}")
            except Exception as exc:  # noqa: BLE001 - deprecated config file read failures fall back to defaults
                logger.warning(f"读取已弃用配置文件失败，回退默认配置: {exc}")

        from src.infrastructure.ai.config_store import build_default_entity_config

        logger.debug("load_file_config() 已弃用，返回数据库默认配置")
        return build_default_entity_config()

    def _merge_configs(self, base: dict[str, Any], override: dict[str, Any]) -> dict[str, Any]:
        """
        递归合并两个配置字典
        """
        result = base.copy()
        for key, value in override.items():
            if key in result and isinstance(result[key], dict) and isinstance(value, dict):
                result[key] = self._merge_configs(result[key], value)
            else:
                result[key] = value
        return result

    def get_value(self, key_path: str, default: Any = None) -> Any:
        """
        获取特定的配置值 (已弃用)
        """
        from src.infrastructure.ai.config_store import DEFAULT_CONFIG

        current = DEFAULT_CONFIG["default"]
        keys = key_path.split(".")

        for k in keys:
            if isinstance(current, dict) and k in current:
                current = current[k]
            else:
                return default

        return current
