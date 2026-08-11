"""
AI Manager Factory

基于配置文件创建适当的 AI 管理器实例（Memory/Redis/Postgres）
"""

import os
from collections.abc import Callable
from typing import Any

from src.infrastructure.ai.config_store import AIConfigStoreInterface
from src.infrastructure.ai.context_manager import ContextManager, MemoryContextManager, RedisContextManager
from src.infrastructure.ai.conversation_manager import ConversationManager, MemoryConversationManager
from src.infrastructure.ai.openai_client import AiGateway
from src.infrastructure.ai.postgres_config_store import PostgresAIConfigStore
from src.infrastructure.ai.postgres_conversation_manager import PostgresConversationManager
from src.infrastructure.database.connection import AsyncDatabaseConnectionInterface
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class AIManagerFactory:
    """AI管理器工厂类"""

    _config_store_builder: (
        Callable[[dict[str, Any], AsyncDatabaseConnectionInterface | None], AIConfigStoreInterface] | None
    ) = None

    @classmethod
    def set_config_store_builder(
        cls,
        builder: Callable[[dict[str, Any], AsyncDatabaseConnectionInterface | None], AIConfigStoreInterface] | None,
    ) -> None:
        """设置自定义配置存储构建器（用于替换后端实现/测试注入）。"""
        cls._config_store_builder = builder

    @classmethod
    def reset_config_store_builder(cls) -> None:
        """重置为默认配置存储构建器。"""
        cls._config_store_builder = None

    @staticmethod
    def _build_default_config_store(
        config: dict[str, Any],
        async_db_connection: AsyncDatabaseConnectionInterface | None,
    ) -> AIConfigStoreInterface:
        # 强制执行数据库持久化，不再支持 'file' 后端
        if async_db_connection is None:
            raise ValueError("强制执行数据库持久化：未提供有效的异步数据库连接。请确保传入 async_db_connection。")

        logger.info("强制使用 PostgresAIConfigStore (Async) 存储配置")
        return PostgresAIConfigStore(db_connection=async_db_connection)

    @staticmethod
    def create_context_manager(config: dict[str, Any], default_model: str = "gpt-3.5-turbo") -> ContextManager:
        """
        根据配置创建上下文管理器

        Args:
            config: AI配置字典
            default_model: 默认模型名称

        Returns:
            上下文管理器实例
        """
        storage_config = config.get("storage", {})
        context_backend = storage_config.get("context_backend", "memory")

        if context_backend == "redis":
            redis_url = str(storage_config.get("context_redis_url") or os.environ.get("REDIS_URL") or "").strip()
            if not redis_url:
                raise ValueError("Redis context backend requires REDIS_URL or storage.context_redis_url")
            ttl = storage_config.get("context_ttl_seconds", 604800)

            logger.info(f"创建 RedisContextManager: url={redis_url}, ttl={ttl}s")
            return RedisContextManager(redis_url=redis_url, default_model=default_model, default_ttl=ttl)
        else:
            logger.info("创建 MemoryContextManager")
            return MemoryContextManager(default_model=default_model)

    @staticmethod
    def create_config_store(
        config: dict[str, Any], async_db_connection: AsyncDatabaseConnectionInterface | None = None
    ) -> AIConfigStoreInterface:
        """
        根据配置创建 AI 配置存储

        Args:
            config: AI配置字典
            async_db_connection: 异步数据库连接

        Returns:
            AI 配置存储实例
        """
        builder = AIManagerFactory._config_store_builder or AIManagerFactory._build_default_config_store
        return builder(config, async_db_connection)

    @staticmethod
    def create_conversation_manager(
        config: dict[str, Any],
        context_manager: ContextManager | None = None,
        ai_client: AiGateway | None = None,
        async_db_connection: AsyncDatabaseConnectionInterface | None = None,
        default_model: str = "gpt-3.5-turbo",
    ) -> ConversationManager:
        """
        根据配置创建对话管理器

        Args:
            config: AI配置字典
            context_manager: 上下文管理器
            ai_client: AI客户端
            async_db_connection: 异步数据库连接（用于 PostgresConversationManager）
            default_model: 默认模型名称

        Returns:
            对话管理器实例
        """
        storage_config = config.get("storage", {})
        conversation_backend = storage_config.get("conversation_backend", "memory")

        if conversation_backend == "postgres":
            if async_db_connection is None:
                raise ValueError(
                    "PostgresConversationManager 需要异步数据库连接。请确保在创建对话管理器之前初始化了异步数据库连接。"
                )

            logger.info("创建 PostgresConversationManager")
            return PostgresConversationManager(
                db_connection=async_db_connection,
                context_manager=context_manager,
                ai_client=ai_client,
                default_model=default_model,
            )
        else:
            logger.info("创建 MemoryConversationManager")
            return MemoryConversationManager(
                context_manager=context_manager, ai_client=ai_client, default_model=default_model
            )

    @staticmethod
    def create_ai_managers(
        config: dict[str, Any],
        ai_client: AiGateway | None = None,
        async_db_connection: AsyncDatabaseConnectionInterface | None = None,
        default_model: str = "gpt-3.5-turbo",
    ) -> tuple[ContextManager, ConversationManager]:
        """
        一次性创建上下文和对话管理器

        Args:
            config: AI配置字典
            ai_client: AI客户端
            async_db_connection: 异步数据库连接
            default_model: 默认模型名称

        Returns:
            (ContextManager, ConversationManager) 元组
        """
        # 创建上下文管理器
        context_manager = AIManagerFactory.create_context_manager(config, default_model)

        # 创建对话管理器
        conversation_manager = AIManagerFactory.create_conversation_manager(
            config,
            context_manager=context_manager,
            ai_client=ai_client,
            async_db_connection=async_db_connection,
            default_model=default_model,
        )

        logger.info("AI 管理器创建完成")
        return context_manager, conversation_manager
