"""缓存指标仓储 - 操作 ai_cache_metrics 表"""

import logging
from abc import ABC, abstractmethod
from typing import Any

logger = logging.getLogger(__name__)


class CacheMetricsRepository(ABC):
    """缓存指标仓储接口"""

    @abstractmethod
    async def record(self, data: dict[str, Any]) -> dict[str, Any]:
        """记录缓存事件"""
        pass

    @abstractmethod
    async def query(
        self,
        entity_id: str | None = None,
        cache_type: str | None = None,
        limit: int = 100,
    ) -> list[dict[str, Any]]:
        """查询缓存指标"""
        pass

    @abstractmethod
    async def get_summary(
        self,
        entity_id: str | None = None,
        hours: int = 24,
    ) -> dict[str, Any]:
        """获取缓存命中率摘要"""
        pass


class PostgresCacheMetricsRepository(CacheMetricsRepository):
    """PostgreSQL 缓存指标仓储实现"""

    def __init__(self, pool):
        self._pool = pool

    async def record(self, data: dict[str, Any]) -> dict[str, Any]:
        query = """
            INSERT INTO ai_cache_metrics (
                entity_id, run_id, model_id, cache_type,
                cache_key_hash, hit, read_tokens, write_tokens,
                cached_tokens, estimated_savings
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING *
        """
        import json

        async with self._pool.acquire() as conn:
            row = await conn.fetchrow(
                query,
                data.get("entity_id"),
                data.get("run_id"),
                data.get("model_id"),
                data.get("cache_type", "unknown"),
                data.get("cache_key_hash", ""),
                data.get("hit", False),
                data.get("read_tokens", 0),
                data.get("write_tokens", 0),
                data.get("cached_tokens", 0),
                json.dumps(data.get("estimated_savings", {})),
            )
            return dict(row)

    async def query(
        self,
        entity_id: str | None = None,
        cache_type: str | None = None,
        limit: int = 100,
    ) -> list[dict[str, Any]]:
        conditions = []
        params = []
        idx = 1

        if entity_id:
            conditions.append(f"entity_id = ${idx}")
            params.append(entity_id)
            idx += 1
        if cache_type:
            conditions.append(f"cache_type = ${idx}")
            params.append(cache_type)
            idx += 1

        where = "WHERE " + " AND ".join(conditions) if conditions else ""
        params.append(limit)

        query = f"""
            SELECT * FROM ai_cache_metrics
            {where}
            ORDER BY created_at DESC
            LIMIT ${idx}
        """
        async with self._pool.acquire() as conn:
            rows = await conn.fetch(query, *params)
            return [dict(row) for row in rows]

    async def get_summary(
        self,
        entity_id: str | None = None,
        hours: int = 24,
    ) -> dict[str, Any]:
        condition = ""
        params = [hours]
        if entity_id:
            condition = "AND entity_id = $2"
            params.append(entity_id)

        query = f"""
            SELECT
                cache_type,
                COUNT(*) as total_events,
                COUNT(*) FILTER (WHERE hit = TRUE) as hits,
                COUNT(*) FILTER (WHERE hit = FALSE) as misses,
                SUM(cached_tokens) as total_cached_tokens,
                SUM(read_tokens) as total_read_tokens,
                SUM(write_tokens) as total_write_tokens
            FROM ai_cache_metrics
            WHERE created_at > now() - interval '{hours} hours'
            {condition}
            GROUP BY cache_type
            ORDER BY cache_type
        """
        async with self._pool.acquire() as conn:
            rows = await conn.fetch(query, *params)
            return {
                "period_hours": hours,
                "by_cache_type": [dict(row) for row in rows],
            }
