"""应用配置集成"""

import os
from pathlib import Path
from typing import Any, Optional

from src.infrastructure.common.runtime_utils import get_runtime_holder
from src.infrastructure.logging.core import get_logger

from ..config_manager import ConfigManager

logger = get_logger(__name__)

_app_config_integration: Optional["ApplicationConfigIntegration"] = None


def _sync_app_config_integration(
    instance: Optional["ApplicationConfigIntegration"],
) -> Optional["ApplicationConfigIntegration"]:
    global _app_config_integration

    _app_config_integration = instance
    runtime_holder = get_runtime_holder()
    if runtime_holder is not None:
        runtime_holder.app_config_integration = instance
    return instance


class ApplicationConfigIntegration:
    """应用配置集成类"""

    def __init__(self, config_manager: ConfigManager | None = None):
        """
        初始化应用配置集成

        Args:
            config_manager: 可选的配置管理器实例
        """
        if config_manager is None:
            config_manager = ConfigManager()
            # 默认配置加载逻辑
            from ..sources.env_source import EnvSource
            from ..sources.file_source import FileSource
            from ..types import ConfigSourcePriority

            try:
                file_source = FileSource(file_path="config/app_config.yaml", priority=ConfigSourcePriority.FILE)
                config_manager.add_source(file_source.get_config_source())
            except Exception as exc:  # noqa: BLE001 - config source loading must not abort startup
                logger.error(f"Failed to load main app config source: {exc}")

            # Check if local runtime overrides are enabled natively (no app imports)
            is_distributed = str(os.getenv("APP_DISTRIBUTED_MODE", "")).strip().lower() in {"1", "true", "yes", "on"}
            if not is_distributed and str(os.getenv("APP_RUNTIME_ROLE", "all")).strip().lower() in {"api", "worker"}:
                is_distributed = True

            local_overrides = True
            if "LOCAL_RUNTIME_OVERRIDES_ENABLED" in os.environ:
                local_overrides = str(os.getenv("LOCAL_RUNTIME_OVERRIDES_ENABLED")).strip().lower() in {
                    "1",
                    "true",
                    "yes",
                    "on",
                }
            else:
                local_overrides = not is_distributed

            if local_overrides:
                try:
                    runtime_override_path = Path("config/runtime_overrides.yaml")
                    runtime_override_path.parent.mkdir(parents=True, exist_ok=True)
                    if not runtime_override_path.exists():
                        runtime_override_path.write_text("{}\n", encoding="utf-8")

                    runtime_override_source = FileSource(
                        file_path=str(runtime_override_path),
                        priority=ConfigSourcePriority.FILE,
                    )
                    config_manager.add_source(runtime_override_source.get_config_source())
                except Exception as exc:  # noqa: BLE001 - config source loading must not abort startup
                    logger.error(f"Failed to load runtime override config source: {exc}")
            else:
                logger.info("Distributed runtime mode detected; config runtime override file source disabled")

            try:
                env_source = EnvSource(priority=ConfigSourcePriority.ENVIRONMENT)
                config_manager.add_source(env_source.get_config_source())
            except Exception as exc:  # noqa: BLE001 - config source loading must not abort startup
                logger.error(f"Failed to load environment config source: {exc}")

            config_manager.load_config()

        self._config_manager = config_manager
        self._config_data: dict[str, Any] = {}
        self._initialized = False

    def initialize(self) -> "ApplicationConfigIntegration":
        """
        初始化配置集成

        Returns:
            自身实例，支持链式调用
        """
        if not self._initialized:
            self._config_data = self._config_manager.get_config_data()
            self._initialized = True
        return self

    def get_config(self, path: str = "", default: Any = None) -> Any:
        """
        获取配置值

        Args:
            path: 配置路径
            default: 默认值

        Returns:
            配置值
        """
        if not self._initialized:
            self.initialize()

        if not path:
            return self._config_data

        return self._config_manager.get(path, default)

    def reload_config(self) -> dict[str, Any]:
        """
        重新加载配置

        Returns:
            重新加载的配置数据
        """
        self._config_manager.refresh_config()
        self._config_data = self._config_manager.get_config_data()
        return self._config_data

    def shutdown(self) -> None:
        """关闭配置集成"""
        # 清理资源
        self._config_data = {}
        self._initialized = False


# 全局应用配置集成实例


def configure_app_config_integration(
    config_manager: ConfigManager | None = None,
) -> ApplicationConfigIntegration:
    """绑定全局应用配置集成到指定配置管理器。"""
    return _sync_app_config_integration(ApplicationConfigIntegration(config_manager))


def get_app_config_integration() -> ApplicationConfigIntegration:
    """
    获取全局应用配置集成实例

    Returns:
        应用配置集成实例
    """
    runtime_holder = get_runtime_holder()
    if runtime_holder is not None:
        runtime_instance = getattr(runtime_holder, "app_config_integration", None)
        if runtime_instance is not None:
            return _sync_app_config_integration(runtime_instance)

    if _app_config_integration is None:
        return _sync_app_config_integration(ApplicationConfigIntegration())
    return _sync_app_config_integration(_app_config_integration)


def initialize_app_config(environment: str | None = None) -> dict[str, Any]:
    """
    初始化应用配置

    Args:
        environment: 环境名称

    Returns:
        初始化的配置数据
    """
    config_integration = get_app_config_integration()
    config_integration.initialize()
    return config_integration.get_config()
