"""Query tool executor for natural-language flight queries."""

from __future__ import annotations

import logging
from datetime import date, datetime
from typing import Any, cast
from zoneinfo import ZoneInfo

from src.domain.models.flight import FlightStatus
from src.domain.models.todo import TodoId
from src.domain.models.todo_query import TodoQueryOptions

from ..query_tools import QueryToolName
from ._filters import _FiltersMixin
from .protocols import (
    QueryScope,
)

logger = logging.getLogger(__name__)


class _BuildersMixin:
    """QueryToolExecutor mixin."""

    def _attach_query_meta(
        self: dict[str, object],
        *,
        intent: str,
        dataset: str,
        adapter: str,
        extra_meta: dict[str, Any] | None = None,
    ) -> None:
        meta_payload = self.get("meta") if isinstance(self.get("meta"), dict) else {}
        self["meta"] = {
            **meta_payload,
            "router": "QUERY",
            "intent": intent,
            "dataset": dataset,
            "legacy_adapter": adapter,
            **(extra_meta or {}),
        }

    def _anomaly_to_dict(self, anomaly: Any) -> dict[str, Any]:
        anomaly_type = self._safe_value(getattr(anomaly, "anomaly_type", None))
        severity = self._safe_value(getattr(anomaly, "severity", None))
        status = self._safe_value(getattr(anomaly, "status", None))
        detected_at = getattr(anomaly, "detected_at", None)
        resolved_at = getattr(anomaly, "resolved_at", None)
        return {
            "anomaly_id": self._safe_value(getattr(anomaly, "anomaly_id", None)),
            "flight_id": self._safe_value(getattr(anomaly, "flight_id", None)),
            "anomaly_type": anomaly_type,
            "severity": severity,
            "status": status,
            "title": self._safe_value(getattr(anomaly, "title", None)),
            "description": self._safe_value(getattr(anomaly, "description", None)),
            "detected_at": detected_at.isoformat() if isinstance(detected_at, datetime) else None,
            "resolved_at": resolved_at.isoformat() if isinstance(resolved_at, datetime) else None,
            "escalation_level": int(getattr(anomaly, "escalation_level", 0) or 0),
            "linked_todo_id": self._safe_value(getattr(anomaly, "linked_todo_id", None)),
            "rule_id": self._safe_value(getattr(anomaly, "rule_id", None)),
            "context_data": getattr(anomaly, "context_data", {}) or {},
        }

    def _todo_to_dict(self, payload: Any) -> dict[str, Any]:
        todo = payload.get_todo() if hasattr(payload, "get_todo") else payload
        due_date = getattr(todo, "due_date", None)
        created_at = getattr(todo, "created_at", None)
        updated_at = getattr(todo, "updated_at", None)
        deleted_at = getattr(todo, "deleted_at", None)
        return {
            "id": self._safe_value(getattr(todo, "todo_id", None)),
            "title": self._safe_value(getattr(todo, "title", None)),
            "description": self._safe_value(getattr(todo, "description", None)),
            "priority": self._safe_value(getattr(todo, "priority", None)),
            "status": self._safe_value(getattr(todo, "status", None)),
            "category": self._safe_value(getattr(todo, "category", None)),
            "assigned_to": self._safe_value(getattr(todo, "assigned_to", None)),
            "due_date": due_date.isoformat() if isinstance(due_date, datetime) else None,
            "progress": int(getattr(todo, "progress", 0) or 0),
            "tags": list(getattr(todo, "tags", []) or []),
            "estimated_duration": getattr(todo, "estimated_duration", None),
            "actual_duration": getattr(todo, "actual_duration", None),
            "parent_todo_id": self._safe_value(getattr(todo, "parent_todo_id", None)),
            "depends_on": [self._safe_value(item) for item in list(getattr(todo, "depends_on", []) or [])],
            "execution_order": getattr(todo, "execution_order", None),
            "source_type": self._safe_value(getattr(todo, "source_type", None)),
            "source_id": self._safe_value(getattr(todo, "source_id", None)),
            "is_deleted": bool(getattr(todo, "is_deleted", False)),
            "deleted_at": deleted_at.isoformat() if isinstance(deleted_at, datetime) else None,
            "created_at": created_at.isoformat() if isinstance(created_at, datetime) else None,
            "updated_at": updated_at.isoformat() if isinstance(updated_at, datetime) else None,
        }

    def _build_todo_query_options(self, *, limit: int, filters: dict[str, Any]) -> TodoQueryOptions:
        status_filter = self._first_non_empty(filters, "status")
        priority_filter = self._first_non_empty(filters, "priority")
        category_filter = self._first_non_empty(filters, "category")
        assignee_filter = self._first_non_empty(filters, "assignee", "owner")
        safe_limit = max(1, min(int(limit or 20), 500))
        try:
            return TodoQueryOptions(
                limit=safe_limit,
                offset=0,
                status_filter=status_filter,
                priority_filter=priority_filter,
                category_filter=category_filter,
                assignee_filter=assignee_filter,
            )
        except TypeError:
            options = TodoQueryOptions()
            options.limit = safe_limit
            options.offset = 0
            options.status_filter = status_filter
            options.priority_filter = priority_filter
            options.category_filter = category_filter
            options.assignee_filter = assignee_filter
            return options

    async def _get_todo_by_identifier(self, todo_id: str) -> Any | None:
        self._ensure_todo_service()
        normalized = str(todo_id or "").strip()
        if not normalized:
            return None

        try:
            todo_identifier: Any = TodoId(normalized)
        except (TypeError, ValueError) as exc:
            logger.warning("TodoId construction failed; falling back to raw string: %s", exc)
            todo_identifier = normalized
        return await self._todo_service.get_todo(todo_identifier)

    def _todo_in_time_window(
        *,
        item: dict[str, Any],
        start_date: datetime | None,
        end_date: datetime | None,
    ) -> bool:
        if start_date is None and end_date is None:
            return True

        candidates = [
            item.get("due_date"),
            item.get("created_at"),
            item.get("updated_at"),
        ]
        parsed_values: list[datetime] = []
        for candidate in candidates:
            if not candidate:
                continue
            text = str(candidate).strip()
            if not text:
                continue
            try:
                parsed_values.append(datetime.fromisoformat(text))
            except ValueError:
                continue

        if not parsed_values:
            return False

        start_ts = _FiltersMixin._datetime_to_epoch(start_date) if start_date is not None else None
        end_ts = _FiltersMixin._datetime_to_epoch(end_date) if end_date is not None else None

        for value in parsed_values:
            current_ts = _FiltersMixin._datetime_to_epoch(value)
            if start_ts is not None and current_ts < start_ts:
                continue
            if end_ts is not None and current_ts > end_ts:
                continue
            return True
        return False

    def _normalize_alert_stats(self, raw_stats: Any) -> dict[str, Any]:
        data = dict(raw_stats) if isinstance(raw_stats, dict) else {}
        return {
            "total": self._safe_int(data.get("total"), default=0) or 0,
            "open": self._safe_int(data.get("open"), default=0) or 0,
            "acknowledged": self._safe_int(data.get("acknowledged"), default=0) or 0,
            "resolved": self._safe_int(data.get("resolved"), default=0) or 0,
            "critical": self._safe_int(data.get("critical"), default=0) or 0,
            "escalated": self._safe_int(data.get("escalated"), default=0) or 0,
        }

    def _normalize_todo_stats(self, raw_stats: Any) -> dict[str, Any]:
        if isinstance(raw_stats, dict):
            data = dict(raw_stats)
            get_value = data.get
        else:
            data = {}

            def get_value(key, default=None):
                return getattr(raw_stats, key, default)

        status_stats = get_value("status_stats")
        if not isinstance(status_stats, dict):
            status_stats = get_value("group_by_status")
        if not isinstance(status_stats, dict):
            status_stats = {}

        pending = self._safe_int(get_value("pending", get_value("pending_count", 0)), default=0) or 0
        in_progress = self._safe_int(get_value("in_progress", get_value("in_progress_count", 0)), default=0) or 0
        completed = self._safe_int(get_value("completed", get_value("completed_count", 0)), default=0) or 0
        cancelled = self._safe_int(get_value("cancelled", get_value("cancelled_count", 0)), default=0) or 0
        blocked = self._safe_int(get_value("blocked", get_value("blocked_count", 0)), default=0) or 0

        if not status_stats:
            status_stats = {
                "pending": pending,
                "in_progress": in_progress,
                "completed": completed,
                "cancelled": cancelled,
                "blocked": blocked,
            }

        total = self._safe_int(
            get_value("total", get_value("total_count", get_value("count", None))),
            default=None,
        )
        if total is None:
            total = int(sum(int(value or 0) for value in status_stats.values()))

        return {
            "total": int(total),
            "pending": pending,
            "in_progress": in_progress,
            "completed": completed,
            "cancelled": cancelled,
            "blocked": blocked,
            "overdue": self._safe_int(get_value("overdue", get_value("overdue_count", 0)), default=0) or 0,
            "due_today": self._safe_int(get_value("due_today", get_value("due_today_count", 0)), default=0) or 0,
            "status_stats": {str(key): int(value or 0) for key, value in status_stats.items()},
        }

    def _resolve_scope(self, args: dict[str, object], criteria: dict[str, Any]) -> QueryScope:
        explicit_scope = str(args.get("data_scope") or "").strip().lower()
        if explicit_scope in {"active", "archive", "all"}:
            return cast(QueryScope, explicit_scope)

        if explicit_scope == "auto" or not explicit_scope:
            return self._infer_scope_from_dates(criteria)

        return self._infer_scope_from_dates(criteria)

    def _infer_scope_from_dates(self: dict[str, Any]) -> QueryScope:
        dates: list[date] = []

        for key in ("date", "date_from", "date_to"):
            raw = self.get(key)
            if isinstance(raw, date):
                dates.append(raw)

        scheduled_from = self.get("scheduled_departure_from")
        if isinstance(scheduled_from, datetime):
            dates.append(scheduled_from.date())
        scheduled_to = self.get("scheduled_departure_to")
        if isinstance(scheduled_to, datetime):
            dates.append(scheduled_to.date())

        if not dates:
            return "active"

        today_cn = datetime.now(ZoneInfo("Asia/Shanghai")).date()
        earliest = min(dates)
        latest = max(dates)
        if latest < today_cn:
            return "archive"
        if earliest >= today_cn:
            return "active"
        return "all"

    def _resolve_sort(
        self,
        tool_name: str,
        args: dict[str, object],
        criteria: dict[str, Any],
    ) -> tuple[str, str]:
        explicit_sort = str(args.get("sort_by") or "").strip().lower()
        explicit_order = str(args.get("sort_order") or "").strip().lower()

        default_sort, default_order = self._default_sort_for_tool(tool_name, criteria)
        if explicit_sort and explicit_sort != "auto" and explicit_sort in self._ALLOWED_SORT_FIELDS:
            applied_sort = explicit_sort
        else:
            applied_sort = default_sort

        if explicit_order in {"asc", "desc"}:
            applied_order = explicit_order
        elif explicit_order == "auto":
            applied_order = default_order
        elif explicit_sort and explicit_sort in self._ALLOWED_SORT_FIELDS:
            applied_order = self._DEFAULT_SORT_ORDER.get(applied_sort, "desc")
        else:
            applied_order = default_order

        return applied_sort, applied_order

    def _default_sort_for_tool(self, tool_name: str, criteria: dict[str, Any]) -> tuple[str, str]:
        if tool_name == QueryToolName.GET_DELAYED_FLIGHTS.value:
            return "delay_minutes", "desc"

        if tool_name == QueryToolName.GET_FLIGHTS_BY_TIME_RANGE.value:
            return "scheduled_departure", "asc"

        if tool_name == QueryToolName.GET_ABNORMAL_FLIGHTS.value:
            has_time_filter = bool(criteria.get("date") or criteria.get("date_from") or criteria.get("date_to"))
            if has_time_filter:
                return "scheduled_departure", "asc"
            return "delay_minutes", "desc"

        if tool_name == QueryToolName.SEARCH_FLIGHTS_ADVANCED.value:
            if criteria.get("delay_minutes_gt") is not None or criteria.get("min_delay_minutes") is not None:
                return "delay_minutes", "desc"
            if criteria.get("date_from") is not None or criteria.get("date_to") is not None:
                return "scheduled_departure", "asc"
            return "scheduled_departure", "desc"

        return "scheduled_departure", "desc"

    def _resolve_limit(self, args: dict[str, object], criteria: dict[str, Any]) -> int:
        explicit_limit = self._safe_int(args.get("limit"))
        if explicit_limit is not None:
            return max(1, min(explicit_limit, 500))

        selective_filters = self._count_effective_filters(criteria)
        if selective_filters <= 1:
            return 60
        if selective_filters <= 3:
            return 120
        return 200

    def _count_effective_filters(self: dict[str, Any]) -> int:
        keys = (
            "flight_id",
            "flight_no",
            "status",
            "airline_code",
            "date",
            "date_from",
            "date_to",
            "scheduled_departure_from",
            "scheduled_departure_to",
            "has_open_anomaly",
            "delay_minutes_gt",
            "min_delay_minutes",
        )
        count = 0
        for key in keys:
            value = self.get(key)
            if value is None:
                continue
            if isinstance(value, str) and not value.strip():
                continue
            count += 1
        return count

    def _build_meta(
        self,
        scope: QueryScope,
        sort_by: str | None,
        sort_order: str | None,
        limit: int | None,
    ) -> dict[str, object]:
        return {
            "applied_scope": scope,
            "applied_sort_by": sort_by,
            "applied_sort_order": sort_order,
            "applied_limit": limit,
        }

    def _row_to_dict(self, row: dict[str, Any]) -> dict[str, object]:
        scheduled_departure = row.get("scheduled_departure")
        estimated_departure = row.get("estimated_departure")
        scheduled_arrival = row.get("scheduled_arrival")
        estimated_arrival = row.get("estimated_arrival")
        archived_at = row.get("archived_at")
        inbound_leg = row.get("inbound_leg_json") if isinstance(row.get("inbound_leg_json"), dict) else None
        outbound_leg = row.get("outbound_leg_json") if isinstance(row.get("outbound_leg_json"), dict) else None

        status = FlightStatus.from_any(row.get("status"))
        status_value = status.label if status is not None else self._safe_value(row.get("status"))

        delay_minutes = self._safe_float(row.get("delay_minutes"))
        if (
            delay_minutes is None
            and isinstance(estimated_departure, datetime)
            and isinstance(scheduled_departure, datetime)
        ):
            delay_minutes = (estimated_departure - scheduled_departure).total_seconds() / 60.0

        primary_flight_number = self._safe_value(row.get("flight_number"))
        if not primary_flight_number and isinstance(outbound_leg, dict):
            primary_flight_number = self._safe_value(outbound_leg.get("flight_no"))
        if not primary_flight_number and isinstance(inbound_leg, dict):
            primary_flight_number = self._safe_value(inbound_leg.get("flight_no"))

        has_open_anomaly = bool(row.get("has_open_anomaly", False))
        open_anomaly_count = self._safe_int(row.get("open_anomaly_count"), default=0) or 0
        acknowledged_anomaly_count = self._safe_int(row.get("acknowledged_anomaly_count"), default=0) or 0

        payload = {
            "flight_id": self._safe_value(row.get("flight_id")),
            "flight_number": primary_flight_number,
            "status": status_value,
            "airline_code": row.get("airline_code"),
            "scheduled_departure": scheduled_departure.isoformat()
            if isinstance(scheduled_departure, datetime)
            else None,
            "estimated_departure": estimated_departure.isoformat()
            if isinstance(estimated_departure, datetime)
            else None,
            "scheduled_arrival": scheduled_arrival.isoformat() if isinstance(scheduled_arrival, datetime) else None,
            "estimated_arrival": estimated_arrival.isoformat() if isinstance(estimated_arrival, datetime) else None,
            "gate": self._safe_value(row.get("gate")),
            "stand": self._safe_value(row.get("stand")),
            "has_open_anomaly": has_open_anomaly,
            "anomaly_summary": {
                "has_open_anomaly": has_open_anomaly,
                "open_count": int(open_anomaly_count),
                "acknowledged_count": int(acknowledged_anomaly_count),
            },
            "inbound_leg": inbound_leg,
            "outbound_leg": outbound_leg,
            "delay_minutes": round(delay_minutes, 2) if isinstance(delay_minutes, (int, float)) else None,
        }

        if isinstance(archived_at, datetime):
            payload["archived_at"] = archived_at.isoformat()
        elif archived_at is not None:
            payload["archived_at"] = str(archived_at)
        else:
            payload["archived_at"] = None
        return payload

    def _safe_value(self: object) -> object:
        return self.value if hasattr(self, "value") else self

    def _safe_int(self: object, default: int | None = None) -> int | None:
        try:
            if self is None:
                return default
            return int(self)
        except (TypeError, ValueError):
            return default

    def _safe_float(self: object) -> float | None:
        try:
            if self is None:
                return None
            return float(self)
        except (TypeError, ValueError):
            return None

    def _normalize_hours(self: object, default: int) -> int:
        try:
            hours = int(self) if self is not None else int(default)
        except (TypeError, ValueError):
            hours = int(default)
        return max(1, min(hours, 168))
