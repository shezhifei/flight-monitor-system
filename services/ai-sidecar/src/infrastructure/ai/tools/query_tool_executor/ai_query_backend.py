"""Real read-only query backend for the streaming tool path (hybrid agent Task A2).

Implements the :class:`ReadOnlyBackend` protocol against the ``ai_query``
read-only views (``ai_query.v_flights``, ``ai_query.v_daily_kpi``) via asyncpg.

Guarantees:
* No mock data: every result comes from the ``ai_query`` read surface.
* Every query is parameterized and read-only; sort columns are allowlisted.
* Results carry a ``source`` field naming the read surface they came from.
* Failures surface as errors — the executor never fabricates a success.
"""

from __future__ import annotations

import logging
from datetime import UTC, date, datetime
from typing import Any

from ..base import ToolExecutionStatus
from ..query_tools import QueryToolName
from .executor import QueryToolExecutor
from .protocols import QueryScope

logger = logging.getLogger(__name__)

_SORT_COLUMNS: dict[str, str] = {
    "auto": "scheduled_departure",
    "scheduled_departure": "scheduled_departure",
    "delay_minutes": "delay_minutes",
    "status": "status",
    "airline_code": "airline_code",
    "archived_at": "updated_at",
}


def _iso(value: Any) -> str | None:
    if isinstance(value, datetime):
        return value.isoformat()
    if value is None:
        return None
    return str(value)


class AiQueryFlightRepository:
    """asyncpg-backed ``FlightAIQueryRepositoryReader`` over ``ai_query`` views."""

    def __init__(self, pool: Any) -> None:
        self._pool = pool

    @staticmethod
    def _add_param(params: list[Any], value: Any) -> str:
        params.append(value)
        return f"${len(params)}"

    @staticmethod
    def _build_where(criteria: dict[str, Any] | None) -> tuple[str, list[Any]]:
        """Map handler criteria to a parameterized WHERE clause (read-only)."""
        clauses: list[str] = []
        params: list[Any] = []
        criteria = criteria or {}

        add = AiQueryFlightRepository._add_param

        status = criteria.get("status")
        if status:
            clauses.append(f"status = {add(params, str(status))}")
        airline = criteria.get("airline_code")
        if airline:
            clauses.append(f"airline_code = {add(params, str(airline))}")

        for date_key in ("date", "date_from", "date_to"):
            value = criteria.get(date_key)
            if not value:
                continue
            if isinstance(value, datetime):
                value = value.date()
            if isinstance(value, str) and value.strip():
                value = date.fromisoformat(value[:10])
            if not isinstance(value, date):
                continue
            if date_key == "date":
                clauses.append(f"execution_date = {add(params, value)}::date")
            elif date_key == "date_from":
                clauses.append(f"execution_date >= {add(params, value)}::date")
            else:
                clauses.append(f"execution_date <= {add(params, value)}::date")

        for key in ("scheduled_departure_from", "scheduled_departure_to"):
            value = criteria.get(key)
            if not value:
                continue
            if isinstance(value, datetime):
                value = value.isoformat()
            if not str(value).strip():
                continue
            op = ">=" if key.endswith("from") else "<="
            clauses.append(f"scheduled_departure {op} {add(params, str(value))}::timestamptz")

        has_open_anomaly = criteria.get("has_open_anomaly")
        if has_open_anomaly is not None:
            clauses.append(f"has_open_anomaly = {add(params, bool(has_open_anomaly))}")

        min_delay = criteria.get("min_delay_minutes")
        if min_delay is None:
            min_delay = criteria.get("delay_minutes_gt")
        if min_delay is not None:
            clauses.append(f"delay_minutes >= {add(params, int(min_delay))}")

        where = "WHERE " + " AND ".join(clauses) if clauses else ""
        return where, params

    async def search_flights(
        self,
        *,
        criteria: dict[str, Any] | None,
        scope: QueryScope = "active",
        limit: int = 100,
        offset: int = 0,
        sort_by: str = "scheduled_departure",
        sort_order: str = "desc",
    ) -> list[dict[str, Any]]:
        where, params = self._build_where(criteria)
        sort_col = _SORT_COLUMNS.get(str(sort_by or "").strip(), "scheduled_departure")
        order = "ASC" if str(sort_order or "").strip().lower() == "asc" else "DESC"
        params.append(int(limit))
        params.append(int(offset))
        sql = (
            f"SELECT * FROM ai_query.v_flights {where} "
            f"ORDER BY {sort_col} {order} NULLS LAST "
            f"LIMIT ${len(params) - 1} OFFSET ${len(params)}"
        )
        async with self._pool.acquire() as conn:
            rows = await conn.fetch(sql, *params)
        return [dict(row) for row in rows]

    async def count_by_status(
        self,
        *,
        criteria: dict[str, Any] | None,
        scope: QueryScope = "active",
    ) -> dict[str, int]:
        where, params = self._build_where(criteria)
        sql = (
            f"SELECT status, COUNT(*)::int AS count FROM ai_query.v_flights {where} "
            "GROUP BY status ORDER BY status"
        )
        async with self._pool.acquire() as conn:
            rows = await conn.fetch(sql, *params)
        return {str(row["status"] or "unknown"): int(row["count"]) for row in rows}

    async def get_turnaround_stats(
        self,
        *,
        criteria: dict[str, Any] | None,
        scope: QueryScope = "active",
    ) -> dict[str, float | None]:
        clauses: list[str] = []
        params: list[Any] = []
        add = self._add_param
        criteria = criteria or {}
        for date_key in ("date", "date_from", "date_to"):
            value = criteria.get(date_key)
            if not value:
                continue
            if isinstance(value, datetime):
                value = value.date()
            if isinstance(value, str) and value.strip():
                value = date.fromisoformat(value[:10])
            if not isinstance(value, date):
                continue
            if date_key == "date":
                clauses.append(f"flight_date = {add(params, value)}::date")
            elif date_key == "date_from":
                clauses.append(f"flight_date >= {add(params, value)}::date")
            else:
                clauses.append(f"flight_date <= {add(params, value)}::date")
        where = "WHERE " + " AND ".join(clauses) if clauses else ""
        sql = (
            f"SELECT COUNT(*)::int AS total_flights, "
            "AVG(avg_turnaround_minutes)::float AS avg_turnaround_minutes, "
            "AVG(p90_turnaround_minutes)::float AS p90_turnaround_minutes, "
            "AVG(on_time_departure_rate)::float AS on_time_departure_rate, "
            "AVG(on_time_arrival_rate)::float AS on_time_arrival_rate, "
            "AVG(abnormal_ratio)::float AS abnormal_ratio "
            f"FROM ai_query.v_daily_kpi {where}"
        )
        async with self._pool.acquire() as conn:
            row = await conn.fetchrow(sql, *params)
        if row is None:
            return {}
        return {
            "total_flights": row["total_flights"],
            "avg_turnaround_minutes": row["avg_turnaround_minutes"],
            "p90_turnaround_minutes": row["p90_turnaround_minutes"],
            "on_time_departure_rate": row["on_time_departure_rate"],
            "on_time_arrival_rate": row["on_time_arrival_rate"],
            "abnormal_ratio": row["abnormal_ratio"],
        }

    async def find_flight_id_by_number(
        self,
        *,
        flight_number: str,
        scope: QueryScope = "active",
    ) -> str | None:
        sql = "SELECT flight_id FROM ai_query.v_flights WHERE flight_number = $1 LIMIT 1"
        async with self._pool.acquire() as conn:
            row = await conn.fetchrow(sql, str(flight_number))
        if row is None:
            return None
        value = row["flight_id"]
        return str(value) if value is not None else None

    async def count_departures_by_time_buckets(
        self,
        *,
        criteria: dict[str, Any] | None,
        start_time: datetime,
        end_time: datetime,
        granularity: str = "hour",
        scope: QueryScope = "active",
    ) -> list[dict[str, Any]]:
        bucket = "hour" if granularity == "hour" else "day"
        params: list[Any] = [start_time.isoformat(), end_time.isoformat()]
        sql = (
            f"SELECT date_trunc('{bucket}', scheduled_departure) AS bucket, COUNT(*)::int AS count "
            "FROM ai_query.v_flights "
            "WHERE scheduled_departure BETWEEN $1::timestamptz AND $2::timestamptz "
            "GROUP BY 1 ORDER BY 1"
        )
        async with self._pool.acquire() as conn:
            rows = await conn.fetch(sql, *params)
        return [
            {"bucket": _iso(row["bucket"]), "count": int(row["count"])} for row in rows
        ]


class AiQueryReadOnlyBackend:
    """Read-only backend wiring the streaming tool path to real ``ai_query`` data.

    ``flight_status_lookup`` is served directly from ``ai_query.v_flights``; the
    builtin query catalog tools are delegated to :class:`QueryToolExecutor`
    backed by :class:`AiQueryFlightRepository`.
    """

    def __init__(self, pool: Any) -> None:
        self._query_executor = QueryToolExecutor(
            flight_ai_query_repository=AiQueryFlightRepository(pool),
        )
        self._query_tool_names = frozenset(name.value for name in QueryToolName)

    async def execute_read_only(self, tool_name: str, arguments: dict[str, Any]) -> dict[str, Any]:
        if tool_name == "flight_status_lookup":
            return await self._flight_status_lookup(arguments)

        if tool_name not in self._query_tool_names:
            raise ValueError(f"Unknown read-only tool: {tool_name}")

        result = await self._query_executor.execute_tool_call("read-only", tool_name, arguments)
        if result.status != ToolExecutionStatus.SUCCESS:
            raise RuntimeError(
                f"{tool_name} failed: {result.error_message or result.message or result.status}"
            )
        payload = dict(result.result or {})
        # Task H1: freshness is a runtime invariant — stamp the read time so
        # the PostToolUse freshness hook can verify evidence age.
        payload.setdefault("as_of", datetime.now(UTC).isoformat())
        return payload

    async def _flight_status_lookup(self, arguments: dict[str, Any]) -> dict[str, Any]:
        flight_id = str(arguments.get("flight_id") or "").strip()
        if not flight_id:
            raise ValueError("flight_id is required")
        sql = (
            "SELECT flight_id, flight_number, airline_code, status, "
            "scheduled_departure, estimated_departure, actual_departure, "
            "scheduled_arrival, estimated_arrival, actual_arrival, "
            "stand, gate, terminal, delay_minutes, has_open_anomaly, open_anomaly_count "
            "FROM ai_query.v_flights WHERE flight_id = $1 LIMIT 1"
        )
        async with self._pool.acquire() as conn:
            row = await conn.fetchrow(sql, flight_id)
        if row is None:
            return {
                "flight_id": flight_id,
                "status": "not_found",
                "source": "ai_query.v_flights",
                "as_of": datetime.now(UTC).isoformat(),
            }
        return {
            "flight_id": row["flight_id"],
            "flight_number": row["flight_number"],
            "airline_code": row["airline_code"],
            "status": row["status"],
            "scheduled_departure": _iso(row["scheduled_departure"]),
            "estimated_departure": _iso(row["estimated_departure"]),
            "actual_departure": _iso(row["actual_departure"]),
            "scheduled_arrival": _iso(row["scheduled_arrival"]),
            "estimated_arrival": _iso(row["estimated_arrival"]),
            "actual_arrival": _iso(row["actual_arrival"]),
            "stand": row["stand"],
            "gate": row["gate"],
            "terminal": row["terminal"],
            "delay_minutes": row["delay_minutes"],
            "has_open_anomaly": bool(row["has_open_anomaly"]),
            "open_anomaly_count": int(row["open_anomaly_count"] or 0),
            "source": "ai_query.v_flights",
            "as_of": datetime.now(UTC).isoformat(),
        }


__all__ = [
    "AiQueryFlightRepository",
    "AiQueryReadOnlyBackend",
]
