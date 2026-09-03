"""
PostgreSQL AI配置持久化存储

提供基于 PostgreSQL 的 AI 实体配置存储，支持动态提示词模板管理。
"""

import asyncio
import json
from typing import Any

from src.infrastructure.common.exceptions import POSTGRES_EXCEPTIONS
from src.infrastructure.database.connection import AsyncDatabaseConnectionInterface
from src.infrastructure.database.query_builder import (
    ComparisonOperator,
    QueryBuilder,
)
from src.infrastructure.logging.core import get_logger

from .config.ai_config_crypto import ConfigEncryptor, get_config_encryptor
from .config.cache_mixin import ConfigCacheMixin
from .config.config_normalizer import normalize_config
from .config_store import (
    AIConfigStoreInterface,
)

logger = get_logger(__name__)


class PostgresAIConfigStore(AIConfigStoreInterface, ConfigCacheMixin):
    """基于 PostgreSQL 的 AI 配置存储实现 (Async)"""

    def __init__(
        self,
        db_connection: AsyncDatabaseConnectionInterface,
        encryptor: ConfigEncryptor | None = None,
    ):
        """
        初始化 PostgreSQL 配置存储

        Args:
            db_connection: 异步数据库连接接口
            encryptor: 可选的注入式加密器；缺省使用进程级共享实例
        """
        self._db_connection = db_connection
        self._encryptor = encryptor or get_config_encryptor()
        self._init_cache()
        self._initialized = False
        self._lock = asyncio.Lock()
        logger.info("PostgresAIConfigStore (Async) 初始化")

    async def _ensure_initialized(self):
        """确保表结构已初始化"""
        if self._initialized:
            return

        async with self._lock:
            if self._initialized:
                return
            await self._init_tables()
            self._initialized = True

    async def _init_tables(self):
        """Schema is managed by migrations — this is now a no-op."""
        logger.info("PostgresAIConfigStore table init: managed by migrations (no-op)")

    async def get_all(self) -> dict[str, dict[str, Any]]:
        """获取所有实体配置"""
        await self._ensure_initialized()
        try:
            async with self._db_connection.connection_context() as conn, conn.cursor() as cursor:
                query, params = (
                    QueryBuilder()
                    .select("*")
                    .from_table("ai_entities")
                    .where("deleted_at", ComparisonOperator.IS_NULL)
                    .build()
                )

                await cursor.execute(query, params)
                rows = await cursor.fetchall()

                result = {}
                for row in rows:
                    config = row["config"] if isinstance(row["config"], dict) else json.loads(row["config"])
                    result[row["id"]] = normalize_config(self._encryptor.decrypt_config(config))

                return result
        except POSTGRES_EXCEPTIONS as e:
            logger.error(f"获取所有 AI 配置失败: {e}", exc_info=True)
            raise

    async def get(self, entity_id: str) -> dict[str, Any] | None:
        """获取指定实体的配置"""
        await self._ensure_initialized()

        if self._cache_valid(entity_id):
            return self._get_cached(entity_id)

        try:
            async with self._db_connection.connection_context() as conn, conn.cursor() as cursor:
                query, params = (
                    QueryBuilder()
                    .select("config", "config_revision")
                    .from_table("ai_entities")
                    .where("id", ComparisonOperator.EQ, entity_id)
                    .where("deleted_at", ComparisonOperator.IS_NULL)
                    .build()
                )

                await cursor.execute(query, params)
                row = await cursor.fetchone()

                if not row:
                    return None

                config = row["config"] if isinstance(row["config"], dict) else json.loads(row["config"])
                config["_config_revision"] = row.get("config_revision", 1)
                decrypted = normalize_config(self._encryptor.decrypt_config(config))
                self._set_cached(entity_id, decrypted)
                return decrypted
        except POSTGRES_EXCEPTIONS as e:
            logger.error(f"获取 AI 配置 '{entity_id}' 失败: {e}")
            return None

    async def reload(self) -> None:
        """重新加载配置（对于 DB 存储，此操作为空）"""
        pass
