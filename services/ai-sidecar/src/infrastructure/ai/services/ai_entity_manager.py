from typing import Any

from src.infrastructure.logging.core import get_logger

from ..ai_entity import AIEntity, AIEntityConfig

logger = get_logger(__name__)


class AIEntityManager:
    """
    AI实体管理器

    负责AI实体的生命周期管理：创建、更新、删除、查询。
    支持动态配置更新。
    """

    def __init__(self):
        self._entities: dict[str, AIEntity] = {}
        logger.info("AIEntityManager initialized")

    async def create_entity(self, config: AIEntityConfig, entity_id: str | None = None) -> str:
        """
        创建新的AI实体

        Args:
            config: 实体配置
            entity_id: 可选的实体ID，如果不提供则自动生成

        Returns:
            实体ID
        """
        try:
            entity = AIEntity(config=config, entity_id=entity_id)
            # 等待初始化的第一阶段（确保基本组件就绪）
            # 注意：AIEntity的__init__中启动了后台初始化任务，这里我们可能需要确保它至少处于可用状态
            # 由于 _ensure_initialized 是 async，我们可以在这里显式调用它现在的逻辑或者信任后台任务
            # 为了稳健性，如果需要确保创建即立即可用，应该 await 它的初始化逻辑

            # 由于 AIEntity 设计是 lazy async init，这里我们存入管理字典即可
            self._entities[entity.entity_id] = entity
            logger.info(f"Created AI entity: {entity.entity_id}")
            return entity.entity_id
        except Exception as e:
            logger.error(f"Failed to create AI entity: {e}")
            raise

    async def get_entity(self, entity_id: str) -> AIEntity | None:
        """获取实体实例"""
        return self._entities.get(entity_id)

    async def update_entity_config(self, entity_id: str, updates: dict[str, Any]):
        """
        更新实体配置

        Args:
            entity_id: 实体ID
            updates: 配置更新字典
        """
        entity = self._entities.get(entity_id)
        if not entity:
            raise ValueError(f"Entity not found: {entity_id}")

        # 更新配置对象
        # 注意：这需要 AIEntityConfig 支持部分更新或重新加载
        for key, value in updates.items():
            if hasattr(entity.config, key):
                setattr(entity.config, key, value)
            else:
                logger.warning(f"Unknown config key: {key} for entity {entity_id}")

        # 触发实体重新初始化或配置应用
        # 这可能需要 AIEntity 提供一个 reload_config 或类似方法
        # 目前 AIEntity 比较简单，修改 config 后，下一次调用 _ensure_initialized 可能不会生效除非重置组件
        # 这里暂时假设修改配置对下一次请求生效（如果是动态参数如 temperature）
        # 如果是 api_key 等连接参数，可能需要重置 client

        logger.info(f"Updated config for entity: {entity_id}")

    async def delete_entity(self, entity_id: str):
        """删除实体"""
        if entity_id in self._entities:
            # 执行清理逻辑（如果需要）
            del self._entities[entity_id]
            logger.info(f"Deleted entity: {entity_id}")

    async def list_entities(self) -> list[dict[str, Any]]:
        """列出所有实体摘要信息"""
        return [
            {
                "id": entity.entity_id,
                "model": entity.config.default_model,
                "status": "active",  # 简化状态
            }
            for entity in self._entities.values()
        ]

    def _sanitize_config_for_status(self, config: AIEntityConfig) -> dict[str, Any]:
        """返回适合 status 接口展示的配置副本，移除敏感凭据。"""
        sanitized = {
            key: value
            for key, value in config.__dict__.items()
            if key not in {"api_key", "authorization", "secret", "password"}
        }
        sanitized["has_api_key"] = bool(getattr(config, "api_key", None))
        return sanitized

    async def get_entity_status(self, entity_id: str) -> dict[str, Any]:
        """获取实体详细状态"""
        entity = self._entities.get(entity_id)
        if not entity:
            raise ValueError(f"Entity not found: {entity_id}")

        return {
            "id": entity.entity_id,
            "metrics": entity.metrics,
            "config": self._sanitize_config_for_status(entity.config),
        }
