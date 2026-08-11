"""AI配置加载器 - 数据库驱动版本

从 PostgreSQL 加载 AI 配置，完全移除本地文件依赖。
"""

from typing import Any

from config.ai_config import AIConfig, merge_ai_configs, validate_ai_config
from pydantic import ValidationError

from src.infrastructure.config.schemas.ai_config_schema import AIConfigSchema
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class AIConfigLoader:
    """AI配置加载器 - 从数据库加载配置

    此版本完全移除了对本地文件的依赖，所有配置从 PostgresAIConfigStore 获取。
    """

    def __init__(self, config_store=None):
        """
        初始化AI配置加载器

        Args:
            config_store: AIConfigStoreInterface 实例（数据库存储）
        """
        self._config_store = config_store
        self._cached_config: AIConfig | None = None
        self._config_schema = AIConfigSchema()

    def set_config_store(self, config_store) -> None:
        """设置配置存储（用于延迟注入）"""
        self._config_store = config_store

    async def load_config_async(self, entity_id: str = "default") -> AIConfig:
        """
        从数据库加载AI配置（异步版本）

        Args:
            entity_id: 实体ID，默认为 "default"

        Returns:
            AIConfig: AI配置实例
        """
        if self._cached_config is not None:
            return self._cached_config

        if not self._config_store:
            raise RuntimeError("配置存储未设置，请先调用 set_config_store()")

        try:
            # 从数据库加载
            config_data = await self._config_store.get(entity_id)

            if config_data is None:
                # 如果数据库中没有，使用默认配置
                from src.infrastructure.ai.config_store import DEFAULT_CONFIG, build_default_entity_config

                config_data = DEFAULT_CONFIG.get(entity_id, build_default_entity_config()).copy()
                logger.warning(f"实体 '{entity_id}' 配置不存在，使用默认配置")

            # 验证配置
            self._validate_config(config_data)

            # 创建配置实例
            ai_config = AIConfig(**self._normalize_config(config_data))

            # 缓存配置
            self._cached_config = ai_config
            logger.info(f"从数据库加载 AI 配置成功: entity_id={entity_id}")
            return ai_config

        except ValidationError as e:
            raise ValueError(f"AI配置验证失败 (entity_id={entity_id}): {e!s}") from e
        except Exception as e:
            raise RuntimeError(f"加载AI配置失败 (entity_id={entity_id}): {e!s}") from e

    def load_config(self, config_path: str | None = None) -> AIConfig:
        """
        同步加载配置（向后兼容）

        注意：此方法仅返回默认配置或缓存配置。
        对于完整的数据库加载，请使用 load_config_async()。
        """
        if self._cached_config is not None:
            return self._cached_config

        # 回退到默认配置
        from src.infrastructure.ai.config_store import build_default_entity_config

        config_data = build_default_entity_config()

        try:
            ai_config = AIConfig(**self._normalize_config(config_data))
            self._cached_config = ai_config
            logger.info("使用默认 AI 配置（同步模式）")
            return ai_config
        except Exception as e:
            raise RuntimeError(f"加载默认AI配置失败: {e!s}") from e

    async def reload_config_async(self, entity_id: str = "default") -> AIConfig:
        """重新加载配置（异步）"""
        self._cached_config = None
        return await self.load_config_async(entity_id)

    def reload_config(self) -> AIConfig:
        """重新加载配置（同步，向后兼容）"""
        self._cached_config = None
        return self.load_config()

    def get_config(self) -> AIConfig:
        """获取当前配置"""
        if self._cached_config is None:
            return self.load_config()
        return self._cached_config

    async def update_config_async(self, entity_id: str, config_updates: dict[str, Any]) -> AIConfig:
        """
        更新配置到数据库

        Args:
            entity_id: 实体ID
            config_updates: 配置更新

        Returns:
            AIConfig: 更新后的配置
        """
        if not self._config_store:
            raise RuntimeError("配置存储未设置")

        # 更新数据库
        updated_data = await self._config_store.update(entity_id, config_updates)

        # 验证更新后的配置
        ai_config = AIConfig(**self._normalize_config(updated_data))

        # 更新缓存
        self._cached_config = ai_config
        logger.info(f"AI 配置已更新: entity_id={entity_id}")

        return ai_config

    def update_config(self, config_updates: dict[str, Any]) -> AIConfig:
        """更新配置（同步，向后兼容）"""
        current_config = self.get_config()
        updated_config = merge_ai_configs(current_config, config_updates)

        if not validate_ai_config(updated_config):
            raise ValueError("配置更新后验证失败")

        self._cached_config = updated_config
        return updated_config

    def get_provider_config(self, provider_name: str) -> dict[str, Any]:
        """获取指定提供商的配置"""
        config = self.get_config()

        if provider_name not in config.providers:
            raise ValueError(f"未找到提供商配置: {provider_name}")

        return config.providers[provider_name].dict()

    def get_model_config(self, model_name: str) -> dict[str, Any]:
        """获取指定模型的配置"""
        config = self.get_config()

        if model_name not in config.models:
            default_model = next(iter(config.models.values())) if config.models else None
            if default_model:
                return default_model.dict()
            raise ValueError(f"未找到模型配置: {model_name}")

        return config.models[model_name].dict()

    def is_provider_enabled(self, provider_name: str) -> bool:
        """检查提供商是否启用"""
        config = self.get_config()
        return provider_name in config.providers

    def _validate_config(self, config_data: dict[str, Any]) -> None:
        """验证配置数据"""
        if not self._config_schema.validate(config_data):
            raise ValueError("配置数据不符合AI配置模式")

    def _normalize_config(self, config_data: dict[str, Any]) -> dict[str, Any]:
        """
        规范化配置数据，确保与 AIConfig 模型兼容

        数据库中的扁平结构可能与 Pydantic 模型的嵌套结构不同，
        此方法负责进行转换。
        """
        normalized = config_data.copy()

        # 移除数据库专用字段
        normalized.pop("_key_encoded", None)

        # 确保 tools 和 monitoring 是嵌套结构
        # （如果数据库中存储为扁平结构，这里进行转换）

        return normalized


class AIHotReloadConfigLoader(AIConfigLoader):
    """支持热重载的AI配置加载器（数据库版本）

    注意：对于数据库驱动的配置，热重载通过轮询数据库实现，
    而非监视文件变化。
    """

    def __init__(self, config_store=None, poll_interval: int = 30):
        """
        初始化热重载配置加载器

        Args:
            config_store: 配置存储实例
            poll_interval: 轮询间隔（秒）
        """
        super().__init__(config_store)
        self.poll_interval = poll_interval
        self._last_check = 0

    async def check_and_reload_async(self, entity_id: str = "default") -> AIConfig | None:
        """
        检查配置是否需要重新加载（异步）

        对于数据库配置，可以通过比较 updated_at 时间戳来判断是否需要重载。
        当前实现简单地强制重载。
        """
        import time

        current_time = time.time()

        if current_time - self._last_check >= self.poll_interval:
            self._last_check = current_time
            try:
                return await self.reload_config_async(entity_id)
            except Exception as e:  # noqa: BLE001 - config hot-reload failures must not crash the caller
                logger.warning(f"配置热重载失败: {e!s}")
                return None

        return None

    def check_and_reload(self) -> AIConfig | None:
        """检查配置是否需要重新加载（同步，向后兼容）"""
        # 同步版本不支持热重载
        return None
