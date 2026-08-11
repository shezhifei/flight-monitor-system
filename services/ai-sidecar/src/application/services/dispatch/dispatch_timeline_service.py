"""派工甘特时间线编排服务。"""

from __future__ import annotations

import json
import logging
from collections import defaultdict
from collections.abc import Iterable
from datetime import UTC, datetime, timedelta
from typing import Any

from src.domain.utils.time_utils import utc_now
from src.infrastructure.common.exceptions import JSON_EXCEPTIONS

# Rust 加速（如果可用）
_USE_RUST_TIMELINE = False
try:
    from rust_sse import layout_dynamic_tracks as _rust_layout_dynamic_tracks
    from rust_sse import layout_fixed_lanes as _rust_layout_fixed_lanes

    _USE_RUST_TIMELINE = True
except ImportError:
    _rust_layout_dynamic_tracks = None  # type: ignore  # optional rust ext — guarded by import guard above
    _rust_layout_fixed_lanes = None  # type: ignore  # optional rust ext — guarded by import guard above

logger = logging.getLogger(__name__)
if _USE_RUST_TIMELINE:
    logger.info("Rust timeline layout acceleration enabled")
else:
    logger.warning("Rust timeline module not available, using Python fallback")


VIEW_MODES = {"flight", "team", "employee", "equipment"}
DEFAULT_WINDOW_PAST_MINUTES = 60
DEFAULT_WINDOW_FUTURE_MINUTES = 360
DEFAULT_TASK_DURATION_MINUTES = 20


class DispatchTimelineService:
    """将派工单原始数据转换为甘特图可渲染结构。"""

    def build_timeline_payload(
        self,
        rows: Iterable[dict[str, Any]],
        view_mode: str,
        is_admin: bool,
        window_start: datetime | None = None,
        window_end: datetime | None = None,
        current_time: datetime | None = None,
    ) -> dict[str, Any]:
        normalized_mode = (view_mode or "flight").strip().lower()
        if normalized_mode not in VIEW_MODES:
            normalized_mode = "flight"

        now = _to_aware(current_time) or utc_now()
        resolved_window_start, resolved_window_end = self.resolve_window(
            window_start=window_start,
            window_end=window_end,
            current_time=now,
        )

        orders = [self._normalize_order_row(row=row, current_time=now) for row in rows]
        orders.sort(key=lambda item: (item["start_time"], item["end_time"], item["order_id"]))

        if normalized_mode == "flight":
            display_items = self._build_flight_view_items(orders=orders, is_admin=is_admin)
            lanes, layout_items = self._layout_dynamic_tracks(display_items)
        elif normalized_mode == "team":
            display_items = self._build_team_view_items(orders=orders)
            lanes, layout_items = self._layout_fixed_lanes(display_items)
        elif normalized_mode == "employee":
            display_items = self._build_employee_view_items(orders=orders)
            lanes, layout_items = self._layout_fixed_lanes(display_items)
        else:
            display_items = self._build_equipment_view_items(orders=orders)
            lanes, layout_items = self._layout_fixed_lanes(display_items)

        order_focus_map: dict[str, str] = {}
        flight_focus_map: dict[str, str] = {}
        for item in layout_items:
            order_id = item.get("order_id")
            if order_id and order_id not in order_focus_map:
                order_focus_map[order_id] = item["id"]

            if item.get("is_flight_summary"):
                flight_id = item.get("flight_id")
                if flight_id:
                    flight_focus_map[flight_id] = item["id"]

        status_counts = self._build_status_counts(orders=orders)
        status_orders = self._build_status_orders(
            orders=orders,
            order_focus_map=order_focus_map,
            flight_focus_map=flight_focus_map,
        )

        return {
            "view_mode": normalized_mode,
            "is_admin": bool(is_admin),
            "window_start": _iso(resolved_window_start),
            "window_end": _iso(resolved_window_end),
            "generated_at": _iso(now),
            "status_counts": status_counts,
            "status_orders": status_orders,
            "lanes": [self._serialize_lane(lane) for lane in lanes],
            "items": [self._serialize_item(item) for item in layout_items],
        }

    @staticmethod
    def resolve_window(
        window_start: datetime | None,
        window_end: datetime | None,
        current_time: datetime | None = None,
    ) -> tuple[datetime, datetime]:
        now = _to_aware(current_time) or utc_now()
        start = _to_aware(window_start)
        end = _to_aware(window_end)

        if start is None and end is None:
            start = now - timedelta(minutes=DEFAULT_WINDOW_PAST_MINUTES)
            end = now + timedelta(minutes=DEFAULT_WINDOW_FUTURE_MINUTES)
        elif start is None and end is not None:
            start = end - timedelta(minutes=DEFAULT_WINDOW_FUTURE_MINUTES + DEFAULT_WINDOW_PAST_MINUTES)
        elif start is not None and end is None:
            end = start + timedelta(minutes=DEFAULT_WINDOW_FUTURE_MINUTES)

        if end <= start:
            end = start + timedelta(minutes=DEFAULT_WINDOW_FUTURE_MINUTES)

        return start, end

    def _normalize_order_row(self, row: dict[str, Any], current_time: datetime) -> dict[str, Any]:
        start_time = _to_aware(
            row.get("actual_start_time")
            or row.get("planned_start_time")
            or row.get("assignment_deadline")
            or row.get("created_at")
            or current_time
        )
        effective_end_source = "default_duration"
        if row.get("actual_end_time") is not None:
            end_time = _to_aware(row.get("actual_end_time"))
            effective_end_source = "actual_end_time"
        elif row.get("estimated_completion_time") is not None:
            end_time = _to_aware(row.get("estimated_completion_time"))
            effective_end_source = "estimated_completion_time"
        elif row.get("planned_end_time") is not None:
            end_time = _to_aware(row.get("planned_end_time"))
            effective_end_source = "planned_end_time"
        else:
            end_time = _to_aware(row.get("actual_start_time"))
            effective_end_source = "actual_start_time"

        if end_time is None:
            end_time = start_time + timedelta(minutes=DEFAULT_TASK_DURATION_MINUTES)
        if end_time <= start_time:
            end_time = start_time + timedelta(minutes=8)
            effective_end_source = "adjusted_min_duration"

        status = str(row.get("status") or "pending")
        flight_id = str(row.get("flight_id") or "")
        flight_no = str(row.get("flight_no") or flight_id or "未知航班")
        task_type_name = str(row.get("task_type_name") or row.get("task_type") or "未命名任务")

        team_name = row.get("team_name")
        individual_username = row.get("individual_username")
        members = _normalize_profiles(row.get("members"), key_id="user_id", key_name="username")
        equipments = _normalize_profiles(row.get("equipments"), key_id="equipment_id", key_name="code")

        member_names = [entry.get("name") for entry in members if entry.get("name")]
        equipment_codes = [entry.get("name") for entry in equipments if entry.get("name")]

        assignee_text = (
            (individual_username or "").strip()
            or (team_name or "").strip()
            or ("/".join(member_names[:2]) if member_names else "未分配")
        )
        equipment_text = "/".join(equipment_codes[:2]) if equipment_codes else "无设备"

        if len(member_names) > 2:
            assignee_text = f"{assignee_text}+{len(member_names) - 2}"
        if len(equipment_codes) > 2:
            equipment_text = f"{equipment_text}+{len(equipment_codes) - 2}"

        return {
            "id": str(row.get("id") or ""),
            "order_id": str(row.get("id") or ""),
            "flight_id": flight_id,
            "flight_no": flight_no,
            "task_type": row.get("task_type"),
            "task_type_name": task_type_name,
            "status": status,
            "start_time": start_time,
            "end_time": end_time,
            "planned_start_time": _to_aware(row.get("planned_start_time")),
            "planned_end_time": _to_aware(row.get("planned_end_time")),
            "actual_start_time": _to_aware(row.get("actual_start_time")),
            "actual_end_time": _to_aware(row.get("actual_end_time")),
            "estimated_completion_time": _to_aware(row.get("estimated_completion_time")),
            "estimated_completion_reported_by": row.get("estimated_completion_reported_by"),
            "estimated_completion_reported_at": _to_aware(row.get("estimated_completion_reported_at")),
            "estimated_completion_note": row.get("estimated_completion_note"),
            "effective_start_time": start_time,
            "effective_end_time": end_time,
            "effective_end_source": effective_end_source,
            "team_id": row.get("team_id"),
            "team_name": team_name,
            "individual_user_id": row.get("individual_user_id"),
            "individual_username": individual_username,
            "stand_id": row.get("stand_id"),
            "stand_code": row.get("stand_code"),
            "gate": row.get("gate"),
            "terminal": row.get("terminal"),
            "source": row.get("source") or "system",
            "schedule_source": row.get("schedule_source") or "current_status_fallback",
            "lock_level": row.get("lock_level") or "optimizable",
            "publication_state": row.get("publication_state") or "published",
            "department_rule_version": row.get("department_rule_version"),
            "crew_requirement_snapshot": row.get("crew_requirement_snapshot") or [],
            "task_crew": row.get("task_crew") or {},
            "qualification_gap": row.get("qualification_gap") or [],
            "availability_reason": row.get("availability_reason"),
            "score_breakdown": row.get("score_breakdown") or {},
            "conflict_reason": row.get("conflict_reason"),
            "dispatch_type": row.get("dispatch_type") or "auto",
            "members": members,
            "equipments": equipments,
            "member_names": member_names,
            "equipment_codes": equipment_codes,
            "display_label": f"{flight_no} | {task_type_name} | {assignee_text} | {equipment_text}",
            "summary_label": flight_no,
        }

    def _build_flight_view_items(self, orders: list[dict[str, Any]], is_admin: bool) -> list[dict[str, Any]]:
        if not is_admin:
            return [
                {
                    **order,
                    "id": order["order_id"],
                    "label": order["display_label"],
                    "is_flight_summary": False,
                    "related_order_ids": [order["order_id"]],
                }
                for order in orders
            ]

        grouped: dict[str, list[dict[str, Any]]] = defaultdict(list)
        for order in orders:
            grouped[order["flight_id"]].append(order)

        items: list[dict[str, Any]] = []
        for flight_id, flight_orders in grouped.items():
            flight_orders.sort(key=lambda item: (item["start_time"], item["order_id"]))
            start_time = min(item["start_time"] for item in flight_orders)
            end_time = max(item["end_time"] for item in flight_orders)
            representative = flight_orders[0]
            summary_id = f"flight-summary:{flight_id}"

            items.append(
                {
                    "id": summary_id,
                    "order_id": None,
                    "flight_id": flight_id,
                    "flight_no": representative["flight_no"],
                    "task_type": None,
                    "task_type_name": f"{len(flight_orders)}项保障任务",
                    "status": self._derive_group_status([item["status"] for item in flight_orders]),
                    "start_time": start_time,
                    "end_time": end_time,
                    "team_id": None,
                    "team_name": None,
                    "individual_user_id": None,
                    "individual_username": None,
                    "stand_id": representative.get("stand_id"),
                    "stand_code": representative.get("stand_code"),
                    "terminal": representative.get("terminal"),
                    "source": representative.get("source") or "system",
                    "dispatch_type": representative.get("dispatch_type") or "auto",
                    "members": [],
                    "equipments": [],
                    "member_names": [],
                    "equipment_codes": [],
                    "display_label": representative["flight_no"],
                    "summary_label": representative["flight_no"],
                    "label": representative["flight_no"],
                    "is_flight_summary": True,
                    "related_order_ids": [order["order_id"] for order in flight_orders],
                    "related_orders": [
                        {
                            "order_id": order["order_id"],
                            "task_type_name": order["task_type_name"],
                            "status": order["status"],
                            "publication_state": order.get("publication_state") or "published",
                            "start_time": _iso(order["start_time"]),
                            "end_time": _iso(order["end_time"]),
                        }
                        for order in flight_orders
                    ],
                }
            )

        items.sort(key=lambda item: (item["start_time"], item["flight_no"]))
        return items

    def _build_team_view_items(self, orders: list[dict[str, Any]]) -> list[dict[str, Any]]:
        items: list[dict[str, Any]] = []
        for order in orders:
            team_id = order.get("team_id")
            lane_key = f"team:{team_id}" if team_id else "team:__unassigned__"
            lane_label = (order.get("team_name") or "").strip() or "未分配班组"

            items.append(
                {
                    **order,
                    "id": order["order_id"],
                    "label": order["display_label"],
                    "lane_key": lane_key,
                    "lane_label": lane_label,
                    "lane_resource_type": "team",
                    "lane_resource_id": str(team_id) if team_id else None,
                    "lane_resource_label": lane_label,
                    "is_flight_summary": False,
                    "related_order_ids": [order["order_id"]],
                }
            )

        return items

    def _build_employee_view_items(self, orders: list[dict[str, Any]]) -> list[dict[str, Any]]:
        items: list[dict[str, Any]] = []

        for order in orders:
            candidates: list[dict[str, str | None]] = []
            seen_user_ids = set()

            for member in order.get("members") or []:
                user_id = member.get("id")
                user_name = member.get("name")
                if user_id in seen_user_ids:
                    continue
                seen_user_ids.add(user_id)
                candidates.append({"id": user_id, "name": user_name})

            individual_user_id = order.get("individual_user_id")
            individual_username = order.get("individual_username")
            if individual_user_id and individual_user_id not in seen_user_ids:
                seen_user_ids.add(individual_user_id)
                candidates.append({"id": individual_user_id, "name": individual_username})

            if not candidates:
                candidates.append({"id": None, "name": "未分配人员"})

            for candidate in candidates:
                user_id = candidate.get("id")
                lane_key = f"employee:{user_id}" if user_id else "employee:__unassigned__"
                lane_label = (candidate.get("name") or "").strip() or "未分配人员"
                item_id = f"{order['order_id']}:employee:{user_id or 'unassigned'}"

                items.append(
                    {
                        **order,
                        "id": item_id,
                        "label": order["display_label"],
                        "lane_key": lane_key,
                        "lane_label": lane_label,
                        "lane_resource_type": "employee",
                        "lane_resource_id": str(user_id) if user_id else None,
                        "lane_resource_label": lane_label,
                        "focus_user_id": user_id,
                        "focus_user_name": lane_label,
                        "is_flight_summary": False,
                        "related_order_ids": [order["order_id"]],
                    }
                )

        return items

    def _build_equipment_view_items(self, orders: list[dict[str, Any]]) -> list[dict[str, Any]]:
        items: list[dict[str, Any]] = []

        for order in orders:
            equipments = list(order.get("equipments") or [])
            if not equipments:
                equipments = [{"id": None, "name": "未分配设备"}]

            for equipment in equipments:
                equipment_id = equipment.get("id")
                equipment_name = (equipment.get("name") or "").strip() or "未分配设备"
                lane_key = f"equipment:{equipment_id}" if equipment_id else "equipment:__unassigned__"
                item_id = f"{order['order_id']}:equipment:{equipment_id or 'unassigned'}"

                items.append(
                    {
                        **order,
                        "id": item_id,
                        "label": order["display_label"],
                        "lane_key": lane_key,
                        "lane_label": equipment_name,
                        "lane_resource_type": "equipment",
                        "lane_resource_id": str(equipment_id) if equipment_id else None,
                        "lane_resource_label": equipment_name,
                        "focus_equipment_id": equipment_id,
                        "focus_equipment_code": equipment_name,
                        "is_flight_summary": False,
                        "related_order_ids": [order["order_id"]],
                    }
                )

        return items

    def _layout_dynamic_tracks(self, items: list[dict[str, Any]]) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
        if _USE_RUST_TIMELINE:
            try:
                return _rust_layout_dynamic_tracks(items)
            except Exception as exc:  # noqa: BLE001 - Rust extension may fail in various ways
                logger.warning(f"Rust layout_dynamic_tracks failed, falling back to Python: {exc}")

        return self._layout_dynamic_tracks_python(items)

    def _layout_dynamic_tracks_python(
        self, items: list[dict[str, Any]]
    ) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
        sorted_items = sorted(items, key=lambda item: (item["start_time"], item["end_time"], item["id"]))
        lane_end_times: list[datetime] = []
        lane_item_counts: dict[int, int] = defaultdict(int)

        for item in sorted_items:
            lane_index = 0
            while lane_index < len(lane_end_times) and item["start_time"] < lane_end_times[lane_index]:
                lane_index += 1

            if lane_index == len(lane_end_times):
                lane_end_times.append(item["end_time"])
            else:
                lane_end_times[lane_index] = max(lane_end_times[lane_index], item["end_time"])

            lane_item_counts[lane_index] += 1
            item["lane_id"] = f"flight-track-{lane_index + 1}"
            item["lane_label"] = f"时间轨道 {lane_index + 1}"
            item["lane_index"] = lane_index
            item["lane_subtrack"] = 0
            item["lane_subtrack_count"] = 1

        lanes = [
            {
                "id": f"flight-track-{index + 1}",
                "label": f"时间轨道 {index + 1}",
                "index": index,
                "subtrack_count": 1,
                "item_count": lane_item_counts.get(index, 0),
            }
            for index in range(len(lane_end_times))
        ]

        return lanes, sorted_items

    def _layout_fixed_lanes(self, items: list[dict[str, Any]]) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
        if _USE_RUST_TIMELINE:
            try:
                lanes, layout_items = _rust_layout_fixed_lanes(items)

                # Rust module may drop resource attributes, backfill them from input items
                lane_resource_map = {}
                for item in items:
                    lane_key = item.get("lane_key")
                    if lane_key and lane_key not in lane_resource_map:
                        lane_resource_map[lane_key] = {
                            "resource_type": item.get("lane_resource_type"),
                            "resource_id": item.get("lane_resource_id"),
                            "resource_label": item.get("lane_resource_label"),
                        }

                for lane in lanes:
                    lane_id = lane.get("id")
                    if lane_id in lane_resource_map:
                        r_data = lane_resource_map[lane_id]
                        if "resource_type" not in lane or not lane["resource_type"]:
                            lane["resource_type"] = r_data["resource_type"]
                        if "resource_id" not in lane or not lane["resource_id"]:
                            lane["resource_id"] = r_data["resource_id"]
                        if "resource_label" not in lane or not lane["resource_label"]:
                            lane["resource_label"] = r_data["resource_label"]

                return lanes, layout_items
            except Exception as exc:  # noqa: BLE001 - Rust extension may fail in various ways
                logger.warning(f"Rust layout_fixed_lanes failed, falling back to Python: {exc}")

        return self._layout_fixed_lanes_python(items)

    def _layout_fixed_lanes_python(
        self, items: list[dict[str, Any]]
    ) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
        grouped: dict[str, list[dict[str, Any]]] = defaultdict(list)
        lane_labels: dict[str, str] = {}

        for item in items:
            lane_key = item.get("lane_key") or "lane:unknown"
            grouped[lane_key].append(item)
            lane_labels[lane_key] = item.get("lane_label") or lane_key

        lane_keys = sorted(grouped.keys(), key=lambda key: _lane_sort_key(key, lane_labels[key]))

        lanes: list[dict[str, Any]] = []
        layout_items: list[dict[str, Any]] = []

        for lane_index, lane_key in enumerate(lane_keys):
            lane_items = sorted(
                grouped[lane_key],
                key=lambda item: (item["start_time"], item["end_time"], item["id"]),
            )
            subtrack_end_times: list[datetime] = []

            for item in lane_items:
                subtrack_index = 0
                while (
                    subtrack_index < len(subtrack_end_times) and item["start_time"] < subtrack_end_times[subtrack_index]
                ):
                    subtrack_index += 1

                if subtrack_index == len(subtrack_end_times):
                    subtrack_end_times.append(item["end_time"])
                else:
                    subtrack_end_times[subtrack_index] = max(
                        subtrack_end_times[subtrack_index],
                        item["end_time"],
                    )

                item["lane_id"] = lane_key
                item["lane_index"] = lane_index
                item["lane_label"] = lane_labels[lane_key]
                item["lane_subtrack"] = subtrack_index
                layout_items.append(item)

            subtrack_count = max(1, len(subtrack_end_times))
            for item in lane_items:
                item["lane_subtrack_count"] = subtrack_count

            lanes.append(
                {
                    "id": lane_key,
                    "label": lane_labels[lane_key],
                    "index": lane_index,
                    "subtrack_count": subtrack_count,
                    "item_count": len(lane_items),
                    "resource_type": lane_items[0].get("lane_resource_type"),
                    "resource_id": lane_items[0].get("lane_resource_id"),
                    "resource_label": lane_items[0].get("lane_resource_label") or lane_labels[lane_key],
                }
            )

        layout_items.sort(key=lambda item: (item["lane_index"], item["start_time"], item["id"]))
        return lanes, layout_items

    @staticmethod
    def _derive_group_status(statuses: list[str]) -> str:
        if not statuses:
            return "pending"

        priority = {
            "in_progress": 5,
            "pending": 4,
            "assigned": 3,
            "completed": 2,
            "cancelled": 1,
        }
        return max(statuses, key=lambda status: priority.get(status, 0))

    @staticmethod
    def _build_status_counts(orders: list[dict[str, Any]]) -> dict[str, int]:
        counts: dict[str, int] = {
            "pending": 0,
            "assigned": 0,
            "in_progress": 0,
            "completed": 0,
            "cancelled": 0,
        }

        for order in orders:
            status = order.get("status") or "pending"
            counts[status] = counts.get(status, 0) + 1

        return counts

    def _build_status_orders(
        self,
        orders: list[dict[str, Any]],
        order_focus_map: dict[str, str],
        flight_focus_map: dict[str, str],
    ) -> dict[str, list[dict[str, Any]]]:
        result: dict[str, list[dict[str, Any]]] = {
            "pending": [],
            "assigned": [],
            "in_progress": [],
            "completed": [],
            "cancelled": [],
        }

        for order in orders:
            status = order.get("status") or "pending"
            focus_item_id = order_focus_map.get(order["order_id"]) or flight_focus_map.get(order.get("flight_id"))
            entry = {
                "order_id": order["order_id"],
                "flight_id": order.get("flight_id"),
                "flight_no": order.get("flight_no"),
                "task_type_name": order.get("task_type_name"),
                "status": status,
                "label": f"{order.get('flight_no')} | {order.get('task_type_name')}",
                "start_time": _iso(order.get("start_time")),
                "end_time": _iso(order.get("end_time")),
                "effective_end_source": order.get("effective_end_source"),
                "focus_item_id": focus_item_id,
                "lock_level": order.get("lock_level"),
                "availability_reason": order.get("availability_reason"),
            }

            if status not in result:
                result[status] = []
            result[status].append(entry)

        return result

    @staticmethod
    def _serialize_lane(lane: dict[str, Any]) -> dict[str, Any]:
        return {
            "id": lane.get("id"),
            "label": lane.get("label"),
            "index": int(lane.get("index") or 0),
            "subtrack_count": int(lane.get("subtrack_count") or 1),
            "item_count": int(lane.get("item_count") or 0),
            "resource_type": lane.get("resource_type"),
            "resource_id": lane.get("resource_id"),
            "resource_label": lane.get("resource_label"),
        }

    @staticmethod
    def _serialize_item(item: dict[str, Any]) -> dict[str, Any]:
        return {
            "id": item.get("id"),
            "order_id": item.get("order_id"),
            "flight_id": item.get("flight_id"),
            "flight_no": item.get("flight_no"),
            "task_type": item.get("task_type"),
            "task_type_name": item.get("task_type_name"),
            "status": item.get("status"),
            "start_time": _iso(item.get("start_time")),
            "end_time": _iso(item.get("end_time")),
            "planned_start_time": _iso(item.get("planned_start_time")),
            "planned_end_time": _iso(item.get("planned_end_time")),
            "actual_start_time": _iso(item.get("actual_start_time")),
            "actual_end_time": _iso(item.get("actual_end_time")),
            "estimated_completion_time": _iso(item.get("estimated_completion_time")),
            "estimated_completion_reported_by": item.get("estimated_completion_reported_by"),
            "estimated_completion_reported_at": _iso(item.get("estimated_completion_reported_at")),
            "estimated_completion_note": item.get("estimated_completion_note"),
            "effective_start_time": _iso(item.get("effective_start_time")),
            "effective_end_time": _iso(item.get("effective_end_time")),
            "effective_end_source": item.get("effective_end_source"),
            "lane_id": item.get("lane_id"),
            "lane_label": item.get("lane_label"),
            "lane_index": int(item.get("lane_index") or 0),
            "lane_subtrack": int(item.get("lane_subtrack") or 0),
            "lane_subtrack_count": int(item.get("lane_subtrack_count") or 1),
            "team_id": item.get("team_id"),
            "team_name": item.get("team_name"),
            "individual_user_id": item.get("individual_user_id"),
            "individual_username": item.get("individual_username"),
            "stand_id": item.get("stand_id"),
            "stand_code": item.get("stand_code"),
            "gate": item.get("gate"),
            "terminal": item.get("terminal"),
            "source": item.get("source"),
            "schedule_source": item.get("schedule_source"),
            "lock_level": item.get("lock_level"),
            "publication_state": item.get("publication_state"),
            "department_rule_version": item.get("department_rule_version"),
            "crew_requirement_snapshot": item.get("crew_requirement_snapshot") or [],
            "task_crew": item.get("task_crew") or {},
            "qualification_gap": item.get("qualification_gap") or [],
            "availability_reason": item.get("availability_reason"),
            "score_breakdown": item.get("score_breakdown") or {},
            "conflict_reason": item.get("conflict_reason"),
            "dispatch_type": item.get("dispatch_type"),
            "members": item.get("members") or [],
            "equipments": item.get("equipments") or [],
            "member_names": item.get("member_names") or [],
            "equipment_codes": item.get("equipment_codes") or [],
            "label": item.get("label") or item.get("display_label") or "",
            "is_flight_summary": bool(item.get("is_flight_summary")),
            "related_order_ids": item.get("related_order_ids") or [],
            "related_orders": item.get("related_orders") or [],
            "focus_user_id": item.get("focus_user_id"),
            "focus_user_name": item.get("focus_user_name"),
            "focus_equipment_id": item.get("focus_equipment_id"),
            "focus_equipment_code": item.get("focus_equipment_code"),
        }


def _to_aware(value: Any | None) -> datetime | None:
    if value is None:
        return None
    if isinstance(value, datetime):
        if value.tzinfo is None:
            return value.replace(tzinfo=UTC)
        return value.astimezone(UTC)
    if isinstance(value, str):
        text = value.strip()
        if not text:
            return None
        if text.endswith("Z"):
            text = text[:-1] + "+00:00"
        try:
            parsed = datetime.fromisoformat(text)
        except ValueError:
            return None
        if parsed.tzinfo is None:
            return parsed.replace(tzinfo=UTC)
        return parsed.astimezone(UTC)
    return None


def _iso(value: datetime | None) -> str | None:
    aware = _to_aware(value)
    if aware is None:
        return None
    return aware.isoformat()


def _normalize_profiles(raw_value: Any, key_id: str, key_name: str) -> list[dict[str, str | None]]:
    profiles: list[dict[str, str | None]] = []
    if isinstance(raw_value, str):
        text = raw_value.strip()
        if text:
            try:
                raw_value = json.loads(text)
            except JSON_EXCEPTIONS as exc:
                logger.warning("profile JSON parse failed; using empty list: %s", exc)
                raw_value = []

    if not isinstance(raw_value, list):
        return profiles

    seen = set()
    for item in raw_value:
        if not isinstance(item, dict):
            continue
        profile_id = item.get(key_id)
        profile_name = item.get(key_name)
        signature = profile_id or f"name:{profile_name}"
        if signature in seen:
            continue
        seen.add(signature)
        profiles.append(
            {
                "id": str(profile_id) if profile_id else None,
                "name": str(profile_name) if profile_name else None,
            }
        )

    return profiles


def _lane_sort_key(lane_key: str, label: str) -> tuple[int, str, str]:
    is_unassigned = 1 if "__unassigned__" in lane_key else 0
    return is_unassigned, (label or "").lower(), lane_key
