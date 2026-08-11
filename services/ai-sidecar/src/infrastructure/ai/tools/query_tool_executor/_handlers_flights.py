"""Query tool executor for natural-language flight queries."""

from __future__ import annotations

import logging
from typing import Any

from ..base import ToolExecutionError, ToolExecutionStatus

logger = logging.getLogger(__name__)


class _HandlersFlightsMixin:
    """QueryToolExecutor mixin."""

    async def _handle_search_flights_advanced(self, args: dict[str, object]) -> dict[str, object]:
        self._ensure_query_repository()

        criteria: dict[str, Any] = {
            "status": str(args.get("status") or "").strip() or None,
            "airline_code": str(args.get("airline_code") or "").strip().upper() or None,
            "date": self._parse_date(args.get("date")),
            "date_from": self._parse_date(args.get("date_from")),
            "date_to": self._parse_date(args.get("date_to")),
            "has_open_anomaly": args.get("has_open_anomaly"),
            "delay_minutes_gt": self._safe_int(args.get("delay_minutes_gt")),
        }
        scope = self._resolve_scope(args, criteria)
        sort_by, sort_order = self._resolve_sort("search_flights_advanced", args, criteria)
        limit = self._resolve_limit(args, criteria)

        rows = await self._flight_ai_query_repository.search_flights(
            criteria=criteria,
            scope=scope,
            limit=limit,
            offset=0,
            sort_by=sort_by,
            sort_order=sort_order,
        )
        items = [self._row_to_dict(row) for row in rows]
        return {
            "total": len(items),
            "items": items,
            "meta": self._build_meta(scope, sort_by, sort_order, limit),
        }

    async def _handle_count_flights_by_status(self, args: dict[str, object]) -> dict[str, object]:
        self._ensure_query_repository()
        criteria: dict[str, Any] = {
            "date": self._parse_date(args.get("date")),
            "date_from": self._parse_date(args.get("date_from")),
            "date_to": self._parse_date(args.get("date_to")),
        }
        scope = self._resolve_scope(args, criteria)
        group_by_status = await self._flight_ai_query_repository.count_by_status(
            criteria=criteria,
            scope=scope,
        )
        return {
            "total": sum(group_by_status.values()),
            "group_by_status": group_by_status,
            "meta": self._build_meta(scope, None, None, None),
        }

    async def _handle_get_delayed_flights(self, args: dict[str, object]) -> dict[str, object]:
        self._ensure_query_repository()
        min_delay = self._safe_int(args.get("min_delay_minutes"), default=15) or 15
        criteria: dict[str, Any] = {
            "min_delay_minutes": min_delay,
            "date": self._parse_date(args.get("date")),
            "date_from": self._parse_date(args.get("date_from")),
            "date_to": self._parse_date(args.get("date_to")),
        }
        scope = self._resolve_scope(args, criteria)
        sort_by, sort_order = self._resolve_sort("get_delayed_flights", args, criteria)
        limit = self._resolve_limit(args, criteria)

        rows = await self._flight_ai_query_repository.search_flights(
            criteria=criteria,
            scope=scope,
            limit=limit,
            offset=0,
            sort_by=sort_by,
            sort_order=sort_order,
        )
        items = [self._row_to_dict(row) for row in rows]
        return {
            "total": len(items),
            "items": items,
            "meta": self._build_meta(scope, sort_by, sort_order, limit),
        }

    async def _handle_get_flights_by_time_range(self, args: dict[str, object]) -> dict[str, object]:
        self._ensure_query_repository()
        start_time = self._parse_datetime(args.get("start_time"), "start_time")
        end_time = self._parse_datetime(args.get("end_time"), "end_time")
        if start_time >= end_time:
            raise ToolExecutionError("start_time must be before end_time", ToolExecutionStatus.VALIDATION_ERROR)

        criteria: dict[str, Any] = {
            "scheduled_departure_from": start_time,
            "scheduled_departure_to": end_time,
        }
        scope = self._resolve_scope(args, criteria)
        sort_by, sort_order = self._resolve_sort("get_flights_by_time_range", args, criteria)
        limit = self._resolve_limit(args, criteria)

        rows = await self._flight_ai_query_repository.search_flights(
            criteria=criteria,
            scope=scope,
            limit=limit,
            offset=0,
            sort_by=sort_by,
            sort_order=sort_order,
        )
        items = [self._row_to_dict(row) for row in rows]
        return {
            "total": len(items),
            "items": items,
            "meta": self._build_meta(scope, sort_by, sort_order, limit),
        }

    async def _handle_get_abnormal_flights(self, args: dict[str, object]) -> dict[str, object]:
        self._ensure_query_repository()
        criteria: dict[str, Any] = {
            "has_open_anomaly": True,
            "date": self._parse_date(args.get("date")),
            "date_from": self._parse_date(args.get("date_from")),
            "date_to": self._parse_date(args.get("date_to")),
        }
        scope = self._resolve_scope(args, criteria)
        sort_by, sort_order = self._resolve_sort("get_abnormal_flights", args, criteria)
        limit = self._resolve_limit(args, criteria)

        rows = await self._flight_ai_query_repository.search_flights(
            criteria=criteria,
            scope=scope,
            limit=limit,
            offset=0,
            sort_by=sort_by,
            sort_order=sort_order,
        )
        items = [self._row_to_dict(row) for row in rows]
        return {
            "total": len(items),
            "items": items,
            "meta": self._build_meta(scope, sort_by, sort_order, limit),
        }

    async def _handle_get_turnaround_stats(self, args: dict[str, object]) -> dict[str, object]:
        self._ensure_query_repository()
        criteria: dict[str, Any] = {
            "date": self._parse_date(args.get("date")),
            "date_from": self._parse_date(args.get("date_from")),
            "date_to": self._parse_date(args.get("date_to")),
        }
        scope = self._resolve_scope(args, criteria)
        stats = await self._flight_ai_query_repository.get_turnaround_stats(
            criteria=criteria,
            scope=scope,
        )
        payload: dict[str, object] = dict(stats)
        payload["meta"] = self._build_meta(scope, None, None, None)
        return payload

    async def _handle_generate_flight_history_report(self, args: dict[str, object]) -> dict[str, object]:
        self._ensure_insight_service()
        flight_id = await self._resolve_flight_id(args)
        hours = self._normalize_hours(args.get("hours"), default=24)
        incident_type = str(args.get("incident_type") or "").strip() or None
        try:
            payload = await self._flight_insight_service.generate_history_report(
                flight_id=flight_id,
                hours=hours,
                incident_type=incident_type,
                user_id=self.default_user,
            )
        except ToolExecutionError:
            raise
        except Exception as exc:
            raise ToolExecutionError(
                f"生成航班动态报表失败: {exc}",
                ToolExecutionStatus.ERROR,
            ) from exc
        return payload

    async def _handle_generate_flight_event_journey(self, args: dict[str, object]) -> dict[str, object]:
        self._ensure_insight_service()
        flight_id = await self._resolve_flight_id(args)
        hours = self._normalize_hours(args.get("hours"), default=24)
        try:
            payload = await self._flight_insight_service.generate_event_journey(
                flight_id=flight_id,
                hours=hours,
                user_id=self.default_user,
            )
        except ToolExecutionError:
            raise
        except Exception as exc:
            raise ToolExecutionError(
                f"生成航班事件经过失败: {exc}",
                ToolExecutionStatus.ERROR,
            ) from exc
        return payload

    async def _resolve_flight_id(self, args: dict[str, object]) -> str:
        flight_id = str(args.get("flight_id") or "").strip()
        if flight_id:
            return flight_id

        flight_number = str(args.get("flight_number") or "").strip().upper()
        if not flight_number:
            raise ToolExecutionError(
                "缺少航班标识，请提供 flight_id 或 flight_number，或先在航班监控页选中航班",
                ToolExecutionStatus.VALIDATION_ERROR,
            )

        resolved_from_active: str | None = None
        if self._service is not None and hasattr(self._service, "find_flight_by_number"):
            aggregate = await self._service.find_flight_by_number(flight_number)
            if aggregate:
                flight = aggregate.get_flight() if hasattr(aggregate, "get_flight") else aggregate
                candidate = self._safe_value(getattr(flight, "flight_id", None))
                if candidate is not None and str(candidate).strip():
                    resolved_from_active = str(candidate).strip()

        if resolved_from_active:
            return resolved_from_active

        if self._flight_ai_query_repository is not None:
            resolved_from_archive = await self._flight_ai_query_repository.find_flight_id_by_number(
                flight_number=flight_number,
                scope="archive",
            )
            if resolved_from_archive:
                return resolved_from_archive

        raise ToolExecutionError(
            f"未找到航班号为 {flight_number} 的航班，请确认航班号或先在航班监控页选中航班",
            ToolExecutionStatus.NOT_FOUND,
        )
