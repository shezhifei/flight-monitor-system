"""Todo agent context repository.

Stores AI execution context outside the Todo aggregate for compatibility migration.
"""

from __future__ import annotations

import logging
import time
from dataclasses import dataclass
from datetime import datetime

from src.infrastructure.database.async_connection_pool import AsyncPooledDatabaseConnection

logger = logging.getLogger(__name__)


@dataclass
class TodoAgentContext:
    todo_id: str
    agent_entity_id: str = "default"
    agent_run_id: str | None = None
    agent_status: str = "pending"
    updated_by: str = "system"
    updated_at: datetime | None = None
    version: int = 1


class TodoAgentContextRepository:
    """Repository for Todo agent extension context with compatibility dual-write."""

    def __init__(
        self,
        db: AsyncPooledDatabaseConnection,
    ):
        self._db = db
        # Legacy compatibility path has been retired.
        self._legacy_retired = True
        self._metrics: dict[str, float] = {
            "get_calls": 0,
            "get_context_hits": 0,
            "get_legacy_hits": 0,
            "get_misses": 0,
            "get_duration_ms_total": 0.0,
            "batch_get_calls": 0,
            "batch_get_requested_ids_total": 0,
            "batch_get_context_hits": 0,
            "batch_get_legacy_hits": 0,
            "batch_get_duration_ms_total": 0.0,
            "find_todo_ids_calls": 0,
            "find_todo_ids_context_preferred_calls": 0,
            "find_todo_ids_legacy_preferred_calls": 0,
            "find_todo_ids_hybrid_calls": 0,
            "find_todo_ids_duration_ms_total": 0.0,
        }

    async def get(self, todo_id: str) -> TodoAgentContext | None:
        started = time.perf_counter()
        self._inc_metric("get_calls")
        normalized_todo_id = str(todo_id or "").strip()
        if not normalized_todo_id:
            self._inc_metric("get_misses")
            self._inc_metric("get_duration_ms_total", self._duration_ms(started))
            return None

        async with self._db.connection_context() as conn, conn.cursor() as cursor:
            row = await self._select_context_row(cursor, normalized_todo_id)
            if row:
                self._inc_metric("get_context_hits")
                self._inc_metric("get_duration_ms_total", self._duration_ms(started))
                return self._to_context(dict(row))

            self._inc_metric("get_misses")
            self._inc_metric("get_duration_ms_total", self._duration_ms(started))
            return None

    async def batch_get(self, todo_ids: list[str]) -> dict[str, TodoAgentContext]:
        started = time.perf_counter()
        self._inc_metric("batch_get_calls")
        normalized_ids = [str(todo_id or "").strip() for todo_id in (todo_ids or []) if str(todo_id or "").strip()]
        normalized_ids = list(dict.fromkeys(normalized_ids))
        self._inc_metric("batch_get_requested_ids_total", float(len(normalized_ids)))
        if not normalized_ids:
            self._inc_metric("batch_get_duration_ms_total", self._duration_ms(started))
            return {}

        contexts: dict[str, TodoAgentContext] = {}

        async with self._db.connection_context() as conn, conn.cursor() as cursor:
            context_rows = await self._select_context_rows(cursor, normalized_ids)
            self._inc_metric("batch_get_context_hits", float(len(context_rows)))
            for row in context_rows:
                context = self._to_context(dict(row))
                contexts[context.todo_id] = context

        self._inc_metric("batch_get_duration_ms_total", self._duration_ms(started))
        return contexts

    async def upsert(self, context: TodoAgentContext) -> TodoAgentContext:
        normalized = self._normalize_context(context)

        async with self._db.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(
                """
                    INSERT INTO todo_agent_context (
                        todo_id,
                        agent_entity_id,
                        agent_run_id,
                        agent_status,
                        updated_by,
                        version
                    ) VALUES (%s, %s, %s, %s, %s, %s)
                    ON CONFLICT (todo_id) DO UPDATE SET
                        agent_entity_id = EXCLUDED.agent_entity_id,
                        agent_run_id = EXCLUDED.agent_run_id,
                        agent_status = EXCLUDED.agent_status,
                        updated_by = EXCLUDED.updated_by,
                        updated_at = CURRENT_TIMESTAMP,
                        version = todo_agent_context.version + 1
                    RETURNING
                        todo_id,
                        agent_entity_id,
                        agent_run_id,
                        agent_status,
                        updated_by,
                        updated_at,
                        version
                    """,
                (
                    normalized.todo_id,
                    normalized.agent_entity_id,
                    normalized.agent_run_id,
                    normalized.agent_status,
                    normalized.updated_by,
                    max(1, int(normalized.version or 1)),
                ),
            )
            row = await cursor.fetchone()

        if not row:
            return normalized
        return self._to_context(dict(row))

    async def upsert_partial(
        self,
        todo_id: str,
        agent_entity_id: str | None = None,
        agent_run_id: str | None = None,
        agent_status: str | None = None,
        updated_by: str = "system",
    ) -> TodoAgentContext:
        normalized_todo_id = str(todo_id or "").strip()
        if not normalized_todo_id:
            raise ValueError("todo_id is required for upsert_partial")

        existing = await self.get(normalized_todo_id)
        base = existing or TodoAgentContext(todo_id=normalized_todo_id)

        merged = TodoAgentContext(
            todo_id=normalized_todo_id,
            agent_entity_id=(
                self._normalize_agent_entity_id(agent_entity_id)
                if agent_entity_id is not None
                else self._normalize_agent_entity_id(base.agent_entity_id)
            ),
            agent_run_id=agent_run_id if agent_run_id is not None else base.agent_run_id,
            agent_status=(
                self._normalize_agent_status(agent_status)
                if agent_status is not None
                else self._normalize_agent_status(base.agent_status)
            ),
            updated_by=self._normalize_updated_by(updated_by),
            version=max(1, int((base.version if existing else 1) or 1)),
        )
        return await self.upsert(merged)

    async def find_todo_ids(
        self,
        *,
        agent_status: str | None = None,
        agent_entity_id: str | None = None,
        agent_run_id: str | None = None,
        limit: int = 20,
        offset: int = 0,
    ) -> list[str]:
        """Query todo ids by agent context filters via extension table path."""
        started = time.perf_counter()
        self._inc_metric("find_todo_ids_calls")
        normalized_limit = max(1, int(limit or 20))
        normalized_offset = max(0, int(offset or 0))

        join_type = "INNER JOIN"
        self._inc_metric("find_todo_ids_context_preferred_calls")
        entity_expr = "COALESCE(NULLIF(BTRIM(tac.agent_entity_id), ''), 'default')"
        status_expr = "COALESCE(NULLIF(BTRIM(tac.agent_status), ''), 'pending')"
        run_expr = "NULLIF(BTRIM(tac.agent_run_id), '')"
        updated_expr = "COALESCE(tac.updated_at, CURRENT_TIMESTAMP)"

        conditions = ["COALESCE(t.is_deleted, FALSE) = FALSE"]
        params: list[object] = []

        normalized_status = str(agent_status or "").strip()
        if normalized_status:
            conditions.append(f"{status_expr} = %s")
            params.append(normalized_status)

        normalized_entity = str(agent_entity_id or "").strip()
        if normalized_entity:
            conditions.append(f"{entity_expr} = %s")
            params.append(normalized_entity)

        normalized_run_id = str(agent_run_id or "").strip()
        if normalized_run_id:
            conditions.append(f"{run_expr} = %s")
            params.append(normalized_run_id)

        query = f"""
            SELECT t.todo_id
            FROM todos t
            {join_type} todo_agent_context tac ON tac.todo_id = t.todo_id
            WHERE {" AND ".join(conditions)}
            ORDER BY {updated_expr} DESC, t.todo_id DESC
            LIMIT %s OFFSET %s
        """
        params.extend([normalized_limit, normalized_offset])

        async with self._db.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(query, tuple(params))
            rows = await cursor.fetchall()
        self._inc_metric("find_todo_ids_duration_ms_total", self._duration_ms(started))

        return [
            str((row.get("todo_id") if isinstance(row, dict) else row[0]) or "").strip()
            for row in rows
            if str((row.get("todo_id") if isinstance(row, dict) else row[0]) or "").strip()
        ]

    def get_metrics_snapshot(self) -> dict[str, float]:
        """Return lightweight observability counters for migration decisions."""
        snapshot = dict(self._metrics)

        get_calls = snapshot.get("get_calls", 0.0)
        snapshot["get_context_hit_ratio"] = snapshot.get("get_context_hits", 0.0) / get_calls if get_calls else 0.0
        snapshot["get_legacy_hit_ratio"] = snapshot.get("get_legacy_hits", 0.0) / get_calls if get_calls else 0.0
        snapshot["get_avg_duration_ms"] = snapshot.get("get_duration_ms_total", 0.0) / get_calls if get_calls else 0.0

        batch_calls = snapshot.get("batch_get_calls", 0.0)
        requested_total = snapshot.get("batch_get_requested_ids_total", 0.0)
        snapshot["batch_get_context_hit_ratio"] = (
            snapshot.get("batch_get_context_hits", 0.0) / requested_total if requested_total else 0.0
        )
        snapshot["batch_get_legacy_hit_ratio"] = (
            snapshot.get("batch_get_legacy_hits", 0.0) / requested_total if requested_total else 0.0
        )
        snapshot["batch_get_avg_duration_ms"] = (
            snapshot.get("batch_get_duration_ms_total", 0.0) / batch_calls if batch_calls else 0.0
        )

        find_calls = snapshot.get("find_todo_ids_calls", 0.0)
        snapshot["find_todo_ids_avg_duration_ms"] = (
            snapshot.get("find_todo_ids_duration_ms_total", 0.0) / find_calls if find_calls else 0.0
        )
        snapshot["find_todo_ids_context_preferred_ratio"] = (
            snapshot.get("find_todo_ids_context_preferred_calls", 0.0) / find_calls if find_calls else 0.0
        )
        snapshot["find_todo_ids_legacy_preferred_ratio"] = (
            snapshot.get("find_todo_ids_legacy_preferred_calls", 0.0) / find_calls if find_calls else 0.0
        )
        snapshot["find_todo_ids_hybrid_ratio"] = (
            snapshot.get("find_todo_ids_hybrid_calls", 0.0) / find_calls if find_calls else 0.0
        )
        legacy_hit_ratio = max(
            snapshot.get("get_legacy_hit_ratio", 0.0),
            snapshot.get("batch_get_legacy_hit_ratio", 0.0),
            snapshot.get("find_todo_ids_legacy_preferred_ratio", 0.0),
        )
        snapshot["todo_agent_context_legacy_hit_ratio"] = float(legacy_hit_ratio)
        snapshot["legacy_retired"] = bool(self._legacy_retired)
        try:
            from src.infrastructure.ai.monitoring.metrics import metrics as runtime_metrics

            runtime_metrics.set_gauge("todo_agent_context_legacy_hit_ratio", float(legacy_hit_ratio))
        except Exception as exc:  # noqa: BLE001 - metrics bridge is best-effort
            # Metrics bridge is best-effort and should not affect repository reads.
            logger.warning("runtime metrics bridge unavailable for todo agent context snapshot: %s", exc)
        return snapshot

    def _normalize_context(self, context: TodoAgentContext) -> TodoAgentContext:
        return TodoAgentContext(
            todo_id=str(context.todo_id or "").strip(),
            agent_entity_id=self._normalize_agent_entity_id(context.agent_entity_id),
            agent_run_id=context.agent_run_id,
            agent_status=self._normalize_agent_status(context.agent_status),
            updated_by=self._normalize_updated_by(context.updated_by),
            updated_at=context.updated_at,
            version=max(1, int(context.version or 1)),
        )

    @staticmethod
    def _normalize_agent_entity_id(value: str | None) -> str:
        normalized = str(value or "").strip()
        return normalized or "default"

    @staticmethod
    def _normalize_agent_status(value: str | None) -> str:
        normalized = str(value or "").strip()
        return normalized or "pending"

    @staticmethod
    def _normalize_updated_by(value: str | None) -> str:
        normalized = str(value or "").strip()
        return normalized or "system"

    def _to_context(self, row: dict[str, object]) -> TodoAgentContext:
        return TodoAgentContext(
            todo_id=str(row.get("todo_id") or "").strip(),
            agent_entity_id=self._normalize_agent_entity_id(row.get("agent_entity_id")),
            agent_run_id=row.get("agent_run_id") if row.get("agent_run_id") is not None else None,
            agent_status=self._normalize_agent_status(row.get("agent_status")),
            updated_by=self._normalize_updated_by(row.get("updated_by")),
            updated_at=row.get("updated_at") if isinstance(row.get("updated_at"), datetime) else None,
            version=max(1, int(row.get("version") or 1)),
        )

    async def _select_context_row(self, cursor, todo_id: str):
        await cursor.execute(
            """
            SELECT
                todo_id,
                agent_entity_id,
                agent_run_id,
                agent_status,
                updated_by,
                updated_at,
                version
            FROM todo_agent_context
            WHERE todo_id = %s
            """,
            (todo_id,),
        )
        return await cursor.fetchone()

    async def _select_context_rows(self, cursor, todo_ids: list[str]):
        if not todo_ids:
            return []
        placeholders = ", ".join(["%s"] * len(todo_ids))
        await cursor.execute(
            f"""
            SELECT
                todo_id,
                agent_entity_id,
                agent_run_id,
                agent_status,
                updated_by,
                updated_at,
                version
            FROM todo_agent_context
            WHERE todo_id IN ({placeholders})
            """,
            tuple(todo_ids),
        )
        return await cursor.fetchall()

    def _inc_metric(self, key: str, value: float = 1.0) -> None:
        self._metrics[key] = float(self._metrics.get(key, 0.0)) + float(value)

    @staticmethod
    def _duration_ms(started: float) -> float:
        return (time.perf_counter() - started) * 1000.0
