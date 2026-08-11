"""
PostgreSQL 实现的共享上下文池（Shared Context Pool）。

基于 agent_shared_context 表，提供跨进程 / 重启后可恢复的持久化存储。
与 MemorySharedContextPool 接口完全一致，可直接替换。
"""

from __future__ import annotations

from datetime import datetime
from typing import Any

from src.domain.utils.time_utils import utc_now
from src.infrastructure.database.async_connection_pool import AsyncPooledDatabaseConnection
from src.infrastructure.logging.core import get_logger
from src.shared.id_generator import generate_short_id

from .shared_context_pool import ContextEntry, SharedContextPool

logger = get_logger(__name__)


class PostgresSharedContextPool(SharedContextPool):
    """PostgreSQL 持久化实现的共享上下文池。

    使用 (root_todo_id, source_todo_id) 唯一索引保证 Upsert 幂等。
    tags 列使用 PostgreSQL 原生数组类型 + GIN 索引支持标签检索。
    """

    def __init__(self, db: AsyncPooledDatabaseConnection) -> None:
        self._db = db

    # ------------------------------------------------------------------
    # write_or_update
    # ------------------------------------------------------------------

    async def write_or_update(self, root_todo_id: str, entry: ContextEntry) -> None:
        """以 (root_todo_id, source_todo_id) 为粒度 Upsert。"""
        entry_id = generate_short_id()
        async with self._db.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(
                """
                    INSERT INTO agent_shared_context (
                        id,
                        root_todo_id,
                        source_todo_id,
                        source_todo_title,
                        agent_entity_id,
                        content_type,
                        content,
                        tags,
                        token_count,
                        created_at
                    ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
                    ON CONFLICT (root_todo_id, source_todo_id) DO UPDATE SET
                        source_todo_title = EXCLUDED.source_todo_title,
                        agent_entity_id   = EXCLUDED.agent_entity_id,
                        content_type      = EXCLUDED.content_type,
                        content           = EXCLUDED.content,
                        tags              = EXCLUDED.tags,
                        token_count       = EXCLUDED.token_count,
                        created_at        = EXCLUDED.created_at
                    """,
                (
                    entry_id,
                    root_todo_id,
                    entry.source_todo_id,
                    entry.source_todo_title,
                    entry.agent_entity_id,
                    entry.content_type,
                    entry.content,
                    entry.tags or [],
                    entry.token_count,
                    entry.created_at or utc_now(),
                ),
            )
        logger.debug(f"PostgresSharedContextPool upsert: root={root_todo_id} source={entry.source_todo_id}")

    # ------------------------------------------------------------------
    # read_for_dependencies
    # ------------------------------------------------------------------

    async def read_for_dependencies(
        self,
        root_todo_id: str,
        dependency_todo_ids: list[str],
        max_tokens: int = 2000,
    ) -> list[ContextEntry]:
        if not dependency_todo_ids:
            return []

        placeholders = ", ".join(["%s"] * len(dependency_todo_ids))
        async with self._db.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(
                f"""
                    SELECT
                        source_todo_id,
                        source_todo_title,
                        agent_entity_id,
                        content_type,
                        content,
                        tags,
                        token_count,
                        created_at
                    FROM agent_shared_context
                    WHERE root_todo_id = %s
                      AND source_todo_id IN ({placeholders})
                    ORDER BY created_at ASC
                    """,
                (root_todo_id, *dependency_todo_ids),
            )
            rows = await cursor.fetchall()

        entries = [self._row_to_entry(row) for row in rows]
        return self._apply_token_budget(entries, max_tokens)

    # ------------------------------------------------------------------
    # read_by_tags
    # ------------------------------------------------------------------

    async def read_by_tags(
        self,
        root_todo_id: str,
        tags: list[str],
        max_tokens: int = 2000,
    ) -> list[ContextEntry]:
        if not tags:
            return []

        # PostgreSQL 数组重叠运算符 &&
        async with self._db.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(
                """
                    SELECT
                        source_todo_id,
                        source_todo_title,
                        agent_entity_id,
                        content_type,
                        content,
                        tags,
                        token_count,
                        created_at
                    FROM agent_shared_context
                    WHERE root_todo_id = %s
                      AND tags && %s
                    ORDER BY created_at ASC
                    """,
                (root_todo_id, tags),
            )
            rows = await cursor.fetchall()

        entries = [self._row_to_entry(row) for row in rows]
        return self._apply_token_budget(entries, max_tokens)

    # ------------------------------------------------------------------
    # read_all
    # ------------------------------------------------------------------

    async def read_all(
        self,
        root_todo_id: str,
        max_tokens: int = 4000,
    ) -> list[ContextEntry]:
        async with self._db.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(
                """
                    SELECT
                        source_todo_id,
                        source_todo_title,
                        agent_entity_id,
                        content_type,
                        content,
                        tags,
                        token_count,
                        created_at
                    FROM agent_shared_context
                    WHERE root_todo_id = %s
                    ORDER BY created_at ASC
                    """,
                (root_todo_id,),
            )
            rows = await cursor.fetchall()

        entries = [self._row_to_entry(row) for row in rows]
        return self._apply_token_budget(entries, max_tokens)

    # ------------------------------------------------------------------
    # clear
    # ------------------------------------------------------------------

    async def clear(self, root_todo_id: str) -> None:
        async with self._db.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(
                "DELETE FROM agent_shared_context WHERE root_todo_id = %s",
                (root_todo_id,),
            )
        logger.debug(f"PostgresSharedContextPool cleared root={root_todo_id}")

    # ------------------------------------------------------------------
    # cleanup_old_entries — 按时间批量清除过期数据
    # ------------------------------------------------------------------

    async def cleanup_old_entries(self, older_than_hours: int = 72) -> int:
        """清理超过指定小时数的旧条目，返回被删除的行数。"""
        async with self._db.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(
                """
                    DELETE FROM agent_shared_context
                    WHERE created_at < CURRENT_TIMESTAMP - INTERVAL '%s hours'
                    """,
                (older_than_hours,),
            )
            deleted = cursor.rowcount or 0
        if deleted:
            logger.info(f"PostgresSharedContextPool cleanup: deleted {deleted} entries older than {older_than_hours}h")
        return deleted

    # ------------------------------------------------------------------
    # Helpers
    # ------------------------------------------------------------------

    @staticmethod
    def _row_to_entry(row: Any) -> ContextEntry:
        """将数据库行转换为 ContextEntry。"""
        if isinstance(row, dict):
            return ContextEntry(
                source_todo_id=str(row.get("source_todo_id", "")),
                source_todo_title=str(row.get("source_todo_title", "")),
                agent_entity_id=str(row.get("agent_entity_id", "default")),
                content_type=str(row.get("content_type", "distilled_conclusion")),
                content=str(row.get("content", "")),
                tags=list(row.get("tags") or []),
                token_count=int(row.get("token_count", 0) or 0),
                created_at=row.get("created_at") if isinstance(row.get("created_at"), datetime) else utc_now(),
            )
        # tuple-style row
        return ContextEntry(
            source_todo_id=str(row[0] or ""),
            source_todo_title=str(row[1] or ""),
            agent_entity_id=str(row[2] or "default"),
            content_type=str(row[3] or "distilled_conclusion"),
            content=str(row[4] or ""),
            tags=list(row[5] or []),
            token_count=int(row[6] or 0),
            created_at=row[7] if isinstance(row[7], datetime) else utc_now(),
        )

    @staticmethod
    def _apply_token_budget(
        entries: list[ContextEntry],
        max_tokens: int,
    ) -> list[ContextEntry]:
        """按创建时间排序，贪心取到 token 预算用完为止。"""
        result: list[ContextEntry] = []
        remaining = max_tokens
        for entry in entries:  # 已按 created_at ASC 排序
            if remaining <= 0:
                break
            if entry.token_count <= remaining:
                result.append(entry)
                remaining -= entry.token_count
            else:
                ratio = remaining / max(entry.token_count, 1)
                truncated_len = max(1, int(len(entry.content) * ratio))
                truncated_entry = ContextEntry(
                    source_todo_id=entry.source_todo_id,
                    source_todo_title=entry.source_todo_title,
                    agent_entity_id=entry.agent_entity_id,
                    content_type=entry.content_type,
                    content=entry.content[:truncated_len] + "…(已截断)",
                    tags=entry.tags,
                    created_at=entry.created_at,
                    token_count=remaining,
                )
                result.append(truncated_entry)
                remaining = 0
        return result
