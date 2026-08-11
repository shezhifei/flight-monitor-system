"""AI 模型目录仓储 - 操作 ai_model_catalog 表"""

import logging
from abc import ABC, abstractmethod
from typing import Any

logger = logging.getLogger(__name__)


class ModelCatalogRepository(ABC):
    """模型目录仓储接口"""

    @abstractmethod
    async def find_all(self, active_only: bool = True) -> list[dict[str, Any]]:
        """获取所有模型"""
        pass

    @abstractmethod
    async def find_by_id(self, model_id: str) -> dict[str, Any] | None:
        """根据 ID 获取模型"""
        pass

    @abstractmethod
    async def upsert(self, model_id: str, data: dict[str, Any]) -> dict[str, Any]:
        """创建或更新模型"""
        pass

    @abstractmethod
    async def delete(self, model_id: str) -> bool:
        """删除模型"""
        pass

    @abstractmethod
    async def find_by_provider(self, provider: str) -> list[dict[str, Any]]:
        """根据 provider 查找模型"""
        pass


class PostgresModelCatalogRepository(ModelCatalogRepository):
    """PostgreSQL 模型目录仓储实现"""

    def __init__(self, pool):
        self._pool = pool

    async def find_all(self, active_only: bool = True) -> list[dict[str, Any]]:
        """获取所有模型"""
        query = """
            SELECT model_id, provider, provider_model, api_format,
                   input_modalities, output_modalities, capabilities,
                   context_window, max_output_tokens, cost,
                   is_active, created_at, updated_at
            FROM ai_model_catalog
            WHERE ($1::bool IS FALSE OR is_active = TRUE)
            ORDER BY provider, model_id
        """
        async with self._pool.acquire() as conn:
            rows = await conn.fetch(query, active_only)
            return [dict(row) for row in rows]

    async def find_by_id(self, model_id: str) -> dict[str, Any] | None:
        """根据 ID 获取模型"""
        query = """
            SELECT model_id, provider, provider_model, api_format,
                   input_modalities, output_modalities, capabilities,
                   context_window, max_output_tokens, cost,
                   is_active, created_at, updated_at
            FROM ai_model_catalog
            WHERE model_id = $1
        """
        async with self._pool.acquire() as conn:
            row = await conn.fetchrow(query, model_id)
            return dict(row) if row else None

    async def upsert(self, model_id: str, data: dict[str, Any]) -> dict[str, Any]:
        """创建或更新模型"""
        import json

        query = """
            INSERT INTO ai_model_catalog (
                model_id, provider, provider_model, api_format,
                input_modalities, output_modalities, capabilities,
                context_window, max_output_tokens, cost, is_active
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (model_id) DO UPDATE SET
                provider = EXCLUDED.provider,
                provider_model = EXCLUDED.provider_model,
                api_format = EXCLUDED.api_format,
                input_modalities = EXCLUDED.input_modalities,
                output_modalities = EXCLUDED.output_modalities,
                capabilities = EXCLUDED.capabilities,
                context_window = EXCLUDED.context_window,
                max_output_tokens = EXCLUDED.max_output_tokens,
                cost = EXCLUDED.cost,
                is_active = EXCLUDED.is_active,
                updated_at = now()
            RETURNING *
        """
        async with self._pool.acquire() as conn:
            row = await conn.fetchrow(
                query,
                model_id,
                data.get("provider", "openai"),
                data.get("provider_model", model_id),
                data.get("api_format", "chat_completions"),
                json.dumps(data.get("input_modalities", ["text"])),
                json.dumps(data.get("output_modalities", ["text"])),
                json.dumps(data.get("capabilities", {})),
                data.get("context_window", 128000),
                data.get("max_output_tokens", 4096),
                json.dumps(data.get("cost", {})),
                data.get("is_active", True),
            )
            return dict(row)

    async def delete(self, model_id: str) -> bool:
        """删除模型（软删除）"""
        query = """
            UPDATE ai_model_catalog SET is_active = FALSE, updated_at = now()
            WHERE model_id = $1
        """
        async with self._pool.acquire() as conn:
            result = await conn.execute(query, model_id)
            return result == "UPDATE 1"

    async def find_by_provider(self, provider: str) -> list[dict[str, Any]]:
        """根据 provider 查找模型"""
        query = """
            SELECT * FROM ai_model_catalog
            WHERE provider = $1 AND is_active = TRUE
            ORDER BY model_id
        """
        async with self._pool.acquire() as conn:
            rows = await conn.fetch(query, provider)
            return [dict(row) for row in rows]
