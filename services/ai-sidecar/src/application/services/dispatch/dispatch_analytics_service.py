"""Dispatch operations analytics service."""

from __future__ import annotations

from collections import defaultdict
from collections.abc import Iterable
from datetime import datetime, timedelta
from math import sqrt
from typing import Any

from src.domain.utils.time_utils import utc_now


class DispatchAnalyticsService:
    """Aggregate dispatch operations KPIs within a time window."""

    REPLAN_ACTION_PREFIXES = ("replanned",)

    def __init__(
        self,
        order_repo: Any,
        conflict_service: Any,
        resource_utilization_service: Any,
    ) -> None:
        self._order_repo = order_repo
        self._conflict_service = conflict_service
        self._resource_utilization_service = resource_utilization_service

    async def get_operations_summary(
        self,
        window_start: datetime | None = None,
        window_end: datetime | None = None,
    ) -> dict[str, Any]:
        ws, we = self._resolve_window(window_start, window_end)
        orders = await self._get_orders(ws, we)
        conflict_items = await self._get_conflicts(ws, we, len(orders))
        conflict_order_ids = self._conflict_order_ids(conflict_items)
        replan_order_ids = await self._get_replanned_order_ids(orders)
        response_minutes = self._response_minutes(orders)
        team_loads = self._team_occupied_minutes(orders, ws, we)
        equipment_utilization = await self._resource_utilization_service.get_equipment_utilization(ws, we)
        key_stats = self._key_order_stats(orders)
        assigned_orders = [order for order in orders if self._is_assigned(order)]
        completed_orders = [order for order in orders if self._status_value(order) == "completed"]

        equipment_idle_rate = 0.0
        if equipment_utilization:
            avg_rate = sum(float(item.get("utilization_rate") or 0.0) for item in equipment_utilization) / len(
                equipment_utilization
            )
            equipment_idle_rate = round(max(0.0, 1.0 - avg_rate), 4)

        denominator = max(1, len(assigned_orders))
        return {
            "window_start": ws.isoformat(),
            "window_end": we.isoformat(),
            "assigned_order_count": len(assigned_orders),
            "completed_order_count": len(completed_orders),
            "conflict_count": len(conflict_items),
            "conflict_order_count": len(conflict_order_ids),
            "conflict_rate": round(len(conflict_order_ids) / denominator, 4),
            "replanned_order_count": len(replan_order_ids),
            "replan_rate": round(len(replan_order_ids) / denominator, 4),
            "avg_dispatch_response_minutes": self._average(response_minutes),
            "team_load_balance_score": self._load_balance_score(team_loads.values()),
            "equipment_idle_rate": equipment_idle_rate,
            "key_order_count": key_stats["key_order_count"],
            "key_order_ontime_rate": key_stats["key_order_ontime_rate"],
        }

    async def get_breakdown(
        self,
        *,
        window_start: datetime | None = None,
        window_end: datetime | None = None,
        group_by: str = "team",
    ) -> list[dict[str, Any]]:
        ws, we = self._resolve_window(window_start, window_end)
        orders = await self._get_orders(ws, we)
        conflict_items = await self._get_conflicts(ws, we, len(orders))
        conflict_order_ids = self._conflict_order_ids(conflict_items)
        replan_order_ids = await self._get_replanned_order_ids(orders)

        groups: dict[str, dict[str, Any]] = {}
        for order in orders:
            key, label = self._group_key(order, group_by)
            entry = groups.setdefault(
                key,
                {
                    "group_key": key,
                    "group_label": label,
                    "order_count": 0,
                    "assigned_order_count": 0,
                    "completed_order_count": 0,
                    "occupied_minutes": 0.0,
                    "conflict_order_count": 0,
                    "replanned_order_count": 0,
                    "response_minutes": [],
                },
            )
            entry["order_count"] += 1
            if self._is_assigned(order):
                entry["assigned_order_count"] += 1
            if self._status_value(order) == "completed":
                entry["completed_order_count"] += 1
            entry["occupied_minutes"] += self._occupied_minutes(order, ws, we)
            if str(getattr(order, "id", "")).strip() in conflict_order_ids:
                entry["conflict_order_count"] += 1
            if str(getattr(order, "id", "")).strip() in replan_order_ids:
                entry["replanned_order_count"] += 1
            response_minutes = self._response_minutes_for_order(order)
            if response_minutes is not None:
                entry["response_minutes"].append(response_minutes)

        items: list[dict[str, Any]] = []
        for item in groups.values():
            assigned_count = max(1, int(item["assigned_order_count"]))
            items.append(
                {
                    "group_key": item["group_key"],
                    "group_label": item["group_label"],
                    "order_count": item["order_count"],
                    "assigned_order_count": item["assigned_order_count"],
                    "completed_order_count": item["completed_order_count"],
                    "occupied_minutes": round(float(item["occupied_minutes"]), 2),
                    "conflict_order_count": item["conflict_order_count"],
                    "conflict_rate": round(item["conflict_order_count"] / assigned_count, 4),
                    "replanned_order_count": item["replanned_order_count"],
                    "replan_rate": round(item["replanned_order_count"] / assigned_count, 4),
                    "avg_dispatch_response_minutes": self._average(item["response_minutes"]),
                }
            )
        items.sort(key=lambda value: (-int(value["order_count"]), str(value["group_key"])))
        return items

    async def get_performance_trend(
        self,
        *,
        window_start: datetime | None = None,
        window_end: datetime | None = None,
        bucket: str = "hour",
    ) -> list[dict[str, Any]]:
        if bucket != "hour":
            raise ValueError("仅支持按小时聚合趋势")

        ws, we = self._resolve_window(window_start, window_end)
        orders = await self._get_orders(ws, we)
        conflict_items = await self._get_conflicts(ws, we, len(orders))
        conflict_order_ids = self._conflict_order_ids(conflict_items)
        replan_order_ids = await self._get_replanned_order_ids(orders)

        buckets: dict[datetime, dict[str, Any]] = {}
        cursor = ws.replace(minute=0, second=0, microsecond=0)
        while cursor < we:
            buckets[cursor] = {
                "bucket_start": cursor,
                "bucket_end": cursor + timedelta(hours=1),
                "order_count": 0,
                "conflict_order_count": 0,
                "replanned_order_count": 0,
                "response_minutes": [],
            }
            cursor += timedelta(hours=1)

        for order in orders:
            start_time = self._order_start(order)
            if start_time is None:
                continue
            bucket_start = start_time.replace(minute=0, second=0, microsecond=0)
            if bucket_start not in buckets:
                continue
            item = buckets[bucket_start]
            order_id = str(getattr(order, "id", "")).strip()
            item["order_count"] += 1
            if order_id in conflict_order_ids:
                item["conflict_order_count"] += 1
            if order_id in replan_order_ids:
                item["replanned_order_count"] += 1
            response_minutes = self._response_minutes_for_order(order)
            if response_minutes is not None:
                item["response_minutes"].append(response_minutes)

        return [
            {
                "bucket_start": value["bucket_start"].isoformat(),
                "bucket_end": value["bucket_end"].isoformat(),
                "order_count": value["order_count"],
                "conflict_order_count": value["conflict_order_count"],
                "replanned_order_count": value["replanned_order_count"],
                "avg_dispatch_response_minutes": self._average(value["response_minutes"]),
            }
            for _, value in sorted(buckets.items(), key=lambda item: item[0])
        ]

    async def _get_orders(self, ws: datetime, we: datetime) -> list[Any]:
        return await self._order_repo.find_orders_in_window(
            window_start=ws,
            window_end=we,
            statuses=None,
        )

    async def _get_conflicts(self, ws: datetime, we: datetime, order_count: int) -> list[dict[str, Any]]:
        limit = max(200, max(1, order_count) * 8)
        return await self._conflict_service.list_conflicts(window_start=ws, window_end=we, limit=limit)

    async def _get_replanned_order_ids(self, orders: list[Any]) -> set[str]:
        if not hasattr(self._order_repo, "list_logs"):
            return set()

        result: set[str] = set()
        for order in orders:
            order_id = str(getattr(order, "id", "")).strip()
            if not order_id:
                continue
            logs = await self._order_repo.list_logs(order_id, limit=200)
            if any(self._is_replan_action(item.get("action")) for item in logs):
                result.add(order_id)
        return result

    @classmethod
    def _is_replan_action(cls, action: Any) -> bool:
        value = str(action or "").strip().lower()
        return any(value.startswith(prefix) for prefix in cls.REPLAN_ACTION_PREFIXES)

    @staticmethod
    def _conflict_order_ids(conflict_items: list[dict[str, Any]]) -> set[str]:
        result: set[str] = set()
        for item in conflict_items:
            for order_id in item.get("related_dispatch_order_ids") or []:
                value = str(order_id).strip()
                if value:
                    result.add(value)
        return result

    @staticmethod
    def _status_value(order: Any) -> str:
        status = getattr(order, "status", None)
        return str(getattr(status, "value", status) or "").strip().lower()

    @staticmethod
    def _is_assigned(order: Any) -> bool:
        return bool(
            getattr(order, "team_id", None)
            or getattr(order, "individual_user_id", None)
            or getattr(order, "members", None)
        )

    def _group_key(self, order: Any, group_by: str) -> tuple[str, str]:
        if group_by == "terminal":
            value = str(getattr(order, "terminal", None) or "unknown")
            return value, value
        if group_by == "step":
            key = str(getattr(order, "task_type", None) or "unknown")
            label = str(getattr(order, "task_type_name", None) or key)
            return key, label

        key = str(getattr(order, "team_id", None) or "unassigned")
        label = str(getattr(order, "team_name", None) or key)
        return key, label

    def _team_occupied_minutes(self, orders: list[Any], ws: datetime, we: datetime) -> dict[str, float]:
        team_loads: dict[str, float] = defaultdict(float)
        for order in orders:
            team_id = str(getattr(order, "team_id", "") or "").strip()
            if not team_id:
                continue
            team_loads[team_id] += self._occupied_minutes(order, ws, we)
        return dict(team_loads)

    def _response_minutes(self, orders: list[Any]) -> list[float]:
        return [value for value in (self._response_minutes_for_order(order) for order in orders) if value is not None]

    @staticmethod
    def _response_minutes_for_order(order: Any) -> float | None:
        created_at = getattr(order, "created_at", None)
        dispatched_at = getattr(order, "dispatched_at", None)
        if created_at is None or dispatched_at is None:
            return None
        diff = (dispatched_at - created_at).total_seconds() / 60.0
        if diff < 0:
            return None
        return round(diff, 2)

    @staticmethod
    def _occupied_minutes(order: Any, ws: datetime, we: datetime) -> float:
        start = DispatchAnalyticsService._order_start(order)
        end = DispatchAnalyticsService._order_end(order)
        if start is None:
            return 0.0
        if end is None:
            end = start + timedelta(minutes=30)
        overlap_start = max(start, ws)
        overlap_end = min(end, we)
        if overlap_end <= overlap_start:
            return 0.0
        return (overlap_end - overlap_start).total_seconds() / 60.0

    @staticmethod
    def _order_start(order: Any) -> datetime | None:
        return (
            getattr(order, "actual_start_time", None)
            or getattr(order, "planned_start_time", None)
            or getattr(order, "assignment_deadline", None)
            or getattr(order, "created_at", None)
        )

    @staticmethod
    def _order_end(order: Any) -> datetime | None:
        return (
            getattr(order, "actual_end_time", None)
            or getattr(order, "estimated_completion_time", None)
            or getattr(order, "planned_end_time", None)
            or getattr(order, "actual_start_time", None)
            or getattr(order, "planned_start_time", None)
        )

    def _key_order_stats(self, orders: list[Any]) -> dict[str, Any]:
        key_orders = [order for order in orders if self._is_key_order(order)]
        if not key_orders:
            return {"key_order_count": 0, "key_order_ontime_rate": 0.0}

        ontime_count = 0
        for order in key_orders:
            actual_end = self._order_end(order)
            planned_end = getattr(order, "planned_end_time", None)
            deadline = getattr(order, "assignment_deadline", None) or planned_end
            if actual_end is not None and deadline is not None and actual_end <= deadline:
                ontime_count += 1
        return {
            "key_order_count": len(key_orders),
            "key_order_ontime_rate": round(ontime_count / len(key_orders), 4),
        }

    @staticmethod
    def _is_key_order(order: Any) -> bool:
        workflow_context = getattr(order, "workflow_context", None) or {}
        if not isinstance(workflow_context, dict):
            return False
        if workflow_context.get("is_key_flight") is True:
            return True
        priority = (
            str(workflow_context.get("priority") or workflow_context.get("flight_priority") or "").strip().lower()
        )
        return priority in {"high", "critical", "urgent"}

    @staticmethod
    def _load_balance_score(loads: Iterable[float]) -> float:
        values = [float(item) for item in loads if float(item) >= 0]
        if not values:
            return 1.0
        if len(values) == 1:
            return 1.0
        mean = sum(values) / len(values)
        if mean <= 0:
            return 1.0
        variance = sum((value - mean) ** 2 for value in values) / len(values)
        coefficient = sqrt(variance) / mean
        return round(max(0.0, min(1.0, 1.0 - coefficient)), 4)

    @staticmethod
    def _average(values: Iterable[float]) -> float:
        normalized = [float(item) for item in values]
        if not normalized:
            return 0.0
        return round(sum(normalized) / len(normalized), 2)

    @staticmethod
    def _resolve_window(
        window_start: datetime | None,
        window_end: datetime | None,
    ) -> tuple[datetime, datetime]:
        now = utc_now()
        end = window_end or now
        start = window_start or (end - timedelta(hours=12))
        return start, end
