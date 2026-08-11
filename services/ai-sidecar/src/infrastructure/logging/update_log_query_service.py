"""Read audit/update history from database."""

from collections.abc import Callable
from datetime import datetime, timedelta
from typing import Any

from src.domain.utils.time_utils import utc_now
from src.infrastructure.common.exceptions import POSTGRES_EXCEPTIONS
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


def _db_pool_provider() -> Any:
    return None


def configure_update_log_db_pool_provider(provider: Callable[[], Any] | None) -> None:
    global _db_pool_provider
    _db_pool_provider = provider or (lambda: None)


class UpdateLogQueryService:
    """Query service for system audit logs."""

    def __init__(self, db_pool_provider: Callable[[], Any] | None = None):
        self._pool = None
        self._db_pool_provider = db_pool_provider or _db_pool_provider

    async def _get_pool(self):
        """Lazily get connection pool."""
        if self._pool is None:
            try:
                self._pool = self._db_pool_provider()
            except POSTGRES_EXCEPTIONS as exc:
                logger.warning(f"Failed to load async connection pool for update logs: {exc}")
                return None
        return self._pool

    async def get_update_history(
        self,
        entity_type: str,
        entity_id: str,
        start_time: datetime | None = None,
        end_time: datetime | None = None,
        page: int = 1,
        page_size: int = 50,
    ) -> list[dict[str, Any]]:
        """Return paged update history for one entity."""
        pool = await self._get_pool()
        if pool is None:
            return []

        offset = (page - 1) * page_size

        query = """
            SELECT
                id, entity_type, entity_id, action, changes,
                user_id, trace_id, created_at
            FROM system_audit_logs
            WHERE entity_type = %s AND entity_id = %s
        """
        params = [entity_type, entity_id]

        if start_time:
            query += " AND created_at >= %s"
            params.append(start_time)

        if end_time:
            query += " AND created_at <= %s"
            params.append(end_time)

        query += " ORDER BY created_at DESC LIMIT %s OFFSET %s"
        params.extend([page_size, offset])

        try:
            async with pool.connection_context() as conn, conn.cursor() as cursor:
                await cursor.execute(query, tuple(params))
                rows = await cursor.fetchall()
                return [self._row_to_dict(row) for row in rows]
        except POSTGRES_EXCEPTIONS as exc:
            logger.warning(f"Failed to query update history entity_type={entity_type} entity_id={entity_id}: {exc}")
            return []

    async def get_recent_updates(
        self, minutes: int = 60, entity_type: str | None = None, limit: int = 100
    ) -> list[dict[str, Any]]:
        """Return recent update logs with optional entity filter."""
        pool = await self._get_pool()
        if pool is None:
            return []

        start_time = utc_now() - timedelta(minutes=minutes)

        query = "SELECT * FROM system_audit_logs WHERE created_at >= %s"
        params = [start_time]

        if entity_type:
            query += " AND entity_type = %s"
            params.append(entity_type)

        query += " ORDER BY created_at DESC LIMIT %s"
        params.append(limit)

        try:
            async with pool.connection_context() as conn, conn.cursor() as cursor:
                await cursor.execute(query, tuple(params))
                rows = await cursor.fetchall()
                return [self._row_to_dict(row) for row in rows]
        except POSTGRES_EXCEPTIONS as exc:
            logger.warning(f"Failed to query recent updates minutes={minutes} entity_type={entity_type}: {exc}")
            return []

    def _row_to_dict(self, row) -> dict[str, Any]:
        """Convert database row to dictionary."""
        return {
            "id": str(row["id"]),
            "entity_type": row["entity_type"],
            "entity_id": row["entity_id"],
            "operation": row["action"],
            "action": row["action"],
            "changes": row["changes"] if row["changes"] else {},
            "user_id": row["user_id"] or "",
            "trace_id": row["trace_id"] or "",
            "timestamp": row["created_at"].isoformat() if row["created_at"] else "",
            "created_at": row["created_at"].isoformat() if row["created_at"] else "",
        }
