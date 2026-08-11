"""配置管理器实现"""

import copy
import os
import re
import threading
from collections import deque
from typing import Any

from src.infrastructure.logging.core import get_logger

from .config_accessor import ConfigAccessor
from .config_loader import ConfigLoader
from .config_validator import ConfigValidator
from .exceptions import ConfigLoadingError
from .schemas.base_schema import BaseSchema
from .types import ConfigChangeListener, ConfigData, ConfigSource

logger = get_logger(__name__)


class ConfigManager:
    """配置管理器 - 主入口类"""

    _NOT_LOADED_MSG = "Configuration not loaded. Call load_config() first."
    _ENV_PATTERN = re.compile(r"\$\{([^}:]+)(?::?-?([^}]*))?\}")

    def __init__(self):
        """初始化配置管理器"""
        self._loader = ConfigLoader()
        self._validator = ConfigValidator()
        self._accessor: ConfigAccessor | None = None
        self._config_data: ConfigData = {}
        self._lock = threading.RLock()  # 线程安全锁
        self._change_listeners: list[tuple[int, int, ConfigChangeListener]] = []
        self._listener_seq = 0
        self._schema_validators: dict[str, BaseSchema] = {}

    def add_source(self, source: ConfigSource) -> None:
        """
        添加配置源

        Args:
            source: 配置源对象
        """
        with self._lock:
            self._loader.add_source(source)

    def register_schema(self, name: str, schema: BaseSchema) -> None:
        """
        注册配置模式

        Args:
            name: 模式名称
            schema: 配置模式对象
        """
        with self._lock:
            self._validator.register_schema(name, schema)
            self._schema_validators[name] = schema

    def load_config(self, schema_name: str | None = None) -> "ConfigManager":
        """
        从所有配置源加载配置

        Args:
            schema_name: 模式名称（可选），用于验证

        Returns:
            配置管理器实例（支持链式调用）
        """
        with self._lock:
            try:
                # 加载配置
                self._config_data = self._loader.load_config()

                # 验证配置（如果指定了模式）
                if schema_name:
                    self._validator.validate_config(self._config_data, schema_name)

                # 创建访问器
                self._accessor = ConfigAccessor(self._config_data)

                return self
            except Exception as e:
                raise ConfigLoadingError(
                    source="ConfigManager", message=f"Failed to load configuration: {e!s}", cause=e
                ) from e

    def refresh_config(self, schema_name: str | None = None) -> "ConfigManager":
        """
        刷新配置（重新从所有源加载）

        Args:
            schema_name: 模式名称（可选），用于验证

        Returns:
            配置管理器实例（支持链式调用）
        """
        with self._lock:
            # 检查配置是否有变化
            old_config = copy.deepcopy(self._config_data)

            # 重新加载配置
            self.load_config(schema_name)

            # 比较新旧配置，触发变更事件
            self._notify_config_changes(old_config, self._config_data)

            return self

    def _require_accessor(self) -> ConfigAccessor:
        """返回已初始化的配置访问器，未加载配置时抛错。"""
        if self._accessor is None:
            raise ConfigLoadingError(source="ConfigManager", message=self._NOT_LOADED_MSG)
        return self._accessor

    def get(self, path: str, default: Any = None) -> Any:
        """
        获取配置值

        Args:
            path: 配置路径
            default: 默认值

        Returns:
            配置值
        """
        with self._lock:
            accessor = self._require_accessor()
            return accessor.get(path, default)

    def get_str(self, path: str, default: str = "") -> str:
        """获取字符串类型的配置值"""
        with self._lock:
            accessor = self._require_accessor()
            return accessor.get_str(path, default)

    def get_int(self, path: str, default: int = 0) -> int:
        """获取整数类型的配置值"""
        with self._lock:
            accessor = self._require_accessor()
            return accessor.get_int(path, default)

    def get_float(self, path: str, default: float = 0.0) -> float:
        """获取浮点数类型的配置值"""
        with self._lock:
            accessor = self._require_accessor()
            return accessor.get_float(path, default)

    def get_bool(self, path: str, default: bool = False) -> bool:
        """获取布尔类型的配置值"""
        with self._lock:
            accessor = self._require_accessor()
            return accessor.get_bool(path, default)

    def get_list(self, path: str, default: list | None = None) -> list:
        """获取列表类型的配置值"""
        with self._lock:
            accessor = self._require_accessor()
            return accessor.get_list(path, default)

    def get_dict(self, path: str, default: dict | None = None) -> dict:
        """获取字典类型的配置值"""
        with self._lock:
            accessor = self._require_accessor()
            return accessor.get_dict(path, default)

    def set(self, path: str, value: Any) -> "ConfigManager":
        """
        设置配置值

        Args:
            path: 配置路径
            value: 配置值

        Returns:
            配置管理器实例（支持链式调用）
        """
        with self._lock:
            accessor = self._require_accessor()

            # 获取旧值用于比较
            old_value = accessor.get(path)

            # 设置新值
            accessor.set(path, value)
            self._config_data = accessor.to_dict()

            # 触发变更事件
            self._notify_value_change(path, old_value, value)

            return self

    def has(self, path: str) -> bool:
        """
        检查配置路径是否存在

        Args:
            path: 配置路径

        Returns:
            路径是否存在
        """
        with self._lock:
            if self._accessor is None:
                return False
            return self._accessor.has(path)

    def keys(self, path: str = "") -> list[str]:
        """
        获取指定路径下的所有键

        Args:
            path: 配置路径（默认为根路径）

        Returns:
            键列表
        """
        with self._lock:
            if self._accessor is None:
                return []
            return self._accessor.keys(path)

    def validate_config(self, schema_name: str | None = None) -> bool:
        """
        验证当前配置

        Args:
            schema_name: 模式名称（可选）

        Returns:
            验证是否通过
        """
        with self._lock:
            return self._validator.validate_config(self._config_data, schema_name)

    def get_config_data(self) -> ConfigData:
        """
        获取完整的配置数据

        Returns:
            配置数据副本
        """
        with self._lock:
            config_copy = copy.deepcopy(self._config_data)
            # 应用环境变量替换
            return self._process_env_substitution(config_copy)

    def add_change_listener(self, listener: ConfigChangeListener, priority: int = 100) -> None:
        """
        添加配置变更监听器

        Args:
            listener: 变更监听器
            priority: 监听器优先级，数值越小越先执行
        """
        with self._lock:
            self._listener_seq += 1
            self._change_listeners.append((int(priority), self._listener_seq, listener))

    def remove_change_listener(self, listener: ConfigChangeListener) -> None:
        """
        移除配置变更监听器

        Args:
            listener: 变更监听器
        """
        with self._lock:
            self._change_listeners = [
                (priority, seq, entry) for priority, seq, entry in self._change_listeners if entry is not listener
            ]

    def _notify_value_change(self, key: str, old_value: Any, new_value: Any) -> None:
        """
        通知配置值变更

        Args:
            key: 配置键
            old_value: 旧值
            new_value: 新值
        """
        for _, _, listener in sorted(self._change_listeners, key=lambda item: (item[0], item[1])):
            try:
                listener.on_config_changed(key, old_value, new_value)
            except Exception as e:  # noqa: BLE001 - config change listener callbacks must not propagate failures
                logger.error(f"Error in config change listener: {e!s}")

    def _notify_config_changes(self, old_config: ConfigData, new_config: ConfigData) -> None:
        """
        通知配置变更

        Args:
            old_config: 旧配置
            new_config: 新配置
        """
        # 比较配置变化并通知变更
        all_keys = set(old_config.keys()) | set(new_config.keys())

        for key in all_keys:
            old_value = old_config.get(key)
            new_value = new_config.get(key)

            if old_value != new_value:
                self._notify_value_change(key, old_value, new_value)

    def validate_value(self, key: str, value: Any, schema_name: str | None = None) -> bool:
        """
        验证单个配置值

        Args:
            key: 配置键
            value: 配置值
            schema_name: 模式名称（可选）

        Returns:
            验证是否通过
        """
        with self._lock:
            return self._validator.validate_value(key, value, schema_name)

    def get_source_names(self) -> list[str]:
        """
        获取所有配置源的名称

        Returns:
            配置源名称列表
        """
        with self._lock:
            return self._loader.get_source_names()

    def _process_env_substitution(self, value: Any) -> Any:
        """
        处理环境变量替换

        Args:
            value: 配置值

        Returns:
            处理后的配置值
        """

        def replace_env_var(match) -> str:
            env_var = match.group(1)
            default_val = match.group(2)

            if env_var in os.environ:
                return os.environ[env_var]
            if default_val is not None:
                return default_val
            return match.group(0)

        if isinstance(value, str):
            return self._ENV_PATTERN.sub(replace_env_var, value)

        # 迭代式处理，避免深层递归带来的性能问题
        root = copy.deepcopy(value)
        if not isinstance(root, (dict, list)):
            return root

        queue = deque([(None, None, root)])
        while queue:
            _parent, _key, current = queue.popleft()

            if isinstance(current, dict):
                for child_key, child_value in current.items():
                    if isinstance(child_value, str):
                        current[child_key] = self._ENV_PATTERN.sub(replace_env_var, child_value)
                    elif isinstance(child_value, (dict, list)):
                        queue.append((current, child_key, child_value))
                continue

            if isinstance(current, list):
                for index, item in enumerate(current):
                    if isinstance(item, str):
                        current[index] = self._ENV_PATTERN.sub(replace_env_var, item)
                    elif isinstance(item, (dict, list)):
                        queue.append((current, index, item))

        return root

    def to_dict(self) -> ConfigData:
        """
        获取配置数据的字典表示

        Returns:
            配置数据副本
        """
        with self._lock:
            return self.get_config_data()

    def get_config_snapshot(self) -> ConfigData:
        """
        获取配置快照（向后兼容方法）

        Returns:
            配置数据副本
        """
        return self.get_config_data()
