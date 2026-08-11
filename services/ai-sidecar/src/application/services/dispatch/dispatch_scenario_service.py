"""Dispatch scenario preview service."""

from __future__ import annotations

from collections.abc import Iterable
from datetime import datetime, timedelta
from typing import Any, ClassVar

from src.domain.utils.time_utils import utc_now


class DispatchScenarioService:
    """Preview dispatch disruption impacts without persisting changes."""

    ACTIVE_STATUSES: ClassVar[set[str]] = {"pending", "assigned", "in_progress", "completed"}

    def __init__(self, order_repo: Any) -> None:
        self._order_repo = order_repo

    async def preview(
        self,
        *,
        window_start: datetime,
        window_end: datetime,
        equipment_unavailable_ids: list[str] | None = None,
        closed_stand_ids: list[str] | None = None,
        delayed_orders: list[dict[str, Any]] | None = None,
        frozen_order_ids: list[str] | None = None,
    ) -> dict[str, Any]:
        orders = await self._order_repo.find_orders_in_window(
            window_start=window_start,
            window_end=window_end,
            statuses=list(self.ACTIVE_STATUSES),
        )
        states = self._build_states(orders)

        unavailable_equipment = {str(item).strip() for item in (equipment_unavailable_ids or []) if str(item).strip()}
        closed_stands = {str(item).strip() for item in (closed_stand_ids or []) if str(item).strip()}
        frozen_ids = {str(item).strip() for item in (frozen_order_ids or []) if str(item).strip()}
        delayed_map = self._normalize_delays(delayed_orders)

        impacted_orders: list[dict[str, Any]] = []
        projected_conflicts: list[dict[str, Any]] = []
        recommendations: list[dict[str, Any]] = []
        changed_orders: set[str] = set()
        impacted_order_ids: set[str] = set()
        equipment_impacted = 0
        stand_impacted = 0

        for order_id, delay_minutes in delayed_map.items():
            state = states.get(order_id)
            if state is None:
                continue
            state["projected_start_time"] = state["projected_start_time"] + timedelta(minutes=delay_minutes)
            state["projected_end_time"] = state["projected_end_time"] + timedelta(minutes=delay_minutes)
            changed_orders.add(order_id)
            impacted_order_ids.add(order_id)
            impacted_orders.append(
                self._impact_item(
                    state,
                    impact_type="delay",
                    severity="medium" if delay_minutes < 20 else "high",
                    message=f"订单预计顺延 {delay_minutes} 分钟",
                )
            )
            recommendations.append(
                {
                    "dispatch_order_id": order_id,
                    "action": "shift_window",
                    "reason": f"任务预计延迟 {delay_minutes} 分钟，建议在局部窗口内重排后续任务",
                    "requires_manual_confirmation": order_id in frozen_ids or delay_minutes >= 30,
                }
            )

        for state in states.values():
            order_id = state["order_id"]
            impacted_equipment = unavailable_equipment.intersection(state["equipment_ids"])
            if impacted_equipment:
                equipment_impacted += 1
                impacted_order_ids.add(order_id)
                projected_conflicts.append(
                    {
                        "conflict_type": "equipment_unavailable",
                        "severity": "high",
                        "resource_id": sorted(impacted_equipment)[0],
                        "resource_name": None,
                        "related_dispatch_order_ids": [order_id],
                        "message": "设备在仿真场景中不可用",
                        "suggested_action": "更换设备或改派其它资源",
                        "context": {"scenario": "equipment_unavailable"},
                    }
                )
                impacted_orders.append(
                    self._impact_item(
                        state,
                        impact_type="equipment_unavailable",
                        severity="high",
                        message="当前派工依赖设备在场景中停用",
                    )
                )
                recommendations.append(
                    {
                        "dispatch_order_id": order_id,
                        "action": "replace_equipment",
                        "reason": "涉及停机设备，建议优先尝试替换设备资源",
                        "requires_manual_confirmation": order_id in frozen_ids,
                    }
                )

            if state["stand_id"] and state["stand_id"] in closed_stands:
                stand_impacted += 1
                impacted_order_ids.add(order_id)
                projected_conflicts.append(
                    {
                        "conflict_type": "stand_closed",
                        "severity": "high",
                        "resource_id": state["stand_id"],
                        "resource_name": state["stand_code"],
                        "related_dispatch_order_ids": [order_id],
                        "message": "机位在仿真场景中关闭",
                        "suggested_action": "切换机位或发起人工确认",
                        "context": {"scenario": "stand_closed"},
                    }
                )
                impacted_orders.append(
                    self._impact_item(
                        state,
                        impact_type="stand_closed",
                        severity="high",
                        message="当前派工所在机位在场景中关闭",
                    )
                )
                recommendations.append(
                    {
                        "dispatch_order_id": order_id,
                        "action": "manual_review",
                        "reason": "机位关闭通常需要联动航班与现场指挥确认",
                        "requires_manual_confirmation": True,
                    }
                )

        projected_conflicts.extend(self._project_overlap_conflicts(states))
        for conflict in projected_conflicts:
            impacted_order_ids.update(
                str(item).strip() for item in conflict.get("related_dispatch_order_ids") or [] if str(item).strip()
            )

        for conflict in projected_conflicts:
            related_order_ids = [
                str(item).strip() for item in conflict.get("related_dispatch_order_ids") or [] if str(item).strip()
            ]
            for order_id in related_order_ids:
                if order_id not in states:
                    continue
                recommendations.append(
                    {
                        "dispatch_order_id": order_id,
                        "action": "local_replan",
                        "reason": str(conflict.get("message") or "场景扰动导致资源冲突"),
                        "requires_manual_confirmation": order_id in frozen_ids
                        or str(conflict.get("severity") or "") == "high",
                    }
                )

        requires_manual_confirmation = bool(
            stand_impacted
            or any(item.get("requires_manual_confirmation") for item in recommendations)
            or frozen_ids.intersection(impacted_order_ids)
        )
        risk_level = self._risk_level(
            conflict_count=len(projected_conflicts),
            impacted_count=len(impacted_order_ids),
            stand_impacted=stand_impacted,
            equipment_impacted=equipment_impacted,
        )

        return {
            "window_start": window_start.isoformat(),
            "window_end": window_end.isoformat(),
            "generated_at": utc_now().isoformat(),
            "impact_summary": {
                "impacted_orders": len(impacted_order_ids),
                "projected_conflicts": len(projected_conflicts),
                "delayed_orders": len([order_id for order_id in delayed_map if order_id in states]),
                "equipment_unavailable_orders": equipment_impacted,
                "stand_closed_orders": stand_impacted,
            },
            "projected_conflicts": self._deduplicate_conflicts(projected_conflicts),
            "impacted_orders": self._deduplicate_impacts(impacted_orders),
            "recommendations": self._deduplicate_recommendations(recommendations),
            "changed_orders": sorted(changed_orders),
            "risk_level": risk_level,
            "requires_manual_confirmation": requires_manual_confirmation,
        }

    def _build_states(self, orders: Iterable[Any]) -> dict[str, dict[str, Any]]:
        states: dict[str, dict[str, Any]] = {}
        for order in orders:
            order_id = str(getattr(order, "id", "")).strip()
            if not order_id:
                continue
            original_start = self._order_start(order)
            original_end = self._order_end(order)
            if original_start is None:
                continue
            if original_end is None:
                original_end = original_start + timedelta(minutes=30)
            states[order_id] = {
                "order": order,
                "order_id": order_id,
                "flight_id": getattr(order, "flight_id", None),
                "team_id": str(getattr(order, "team_id", "") or "").strip(),
                "team_name": getattr(order, "team_name", None),
                "individual_user_id": str(getattr(order, "individual_user_id", "") or "").strip(),
                "individual_username": getattr(order, "individual_username", None),
                "stand_id": str(getattr(order, "stand_id", "") or "").strip(),
                "stand_code": getattr(order, "stand_code", None),
                "equipment_ids": self._equipment_ids(order),
                "original_start_time": original_start,
                "original_end_time": original_end,
                "projected_start_time": original_start,
                "projected_end_time": original_end,
            }
        return states

    @staticmethod
    def _normalize_delays(delayed_orders: list[dict[str, Any]] | None) -> dict[str, int]:
        result: dict[str, int] = {}
        for item in delayed_orders or []:
            order_id = str((item or {}).get("dispatch_order_id") or "").strip()
            if not order_id:
                continue
            try:
                delay_minutes = int(float((item or {}).get("delay_minutes") or 0))
            except (TypeError, ValueError):
                delay_minutes = 0
            if delay_minutes <= 0:
                continue
            result[order_id] = delay_minutes
        return result

    def _project_overlap_conflicts(self, states: dict[str, dict[str, Any]]) -> list[dict[str, Any]]:
        conflicts: list[dict[str, Any]] = []
        values = list(states.values())
        for index, left in enumerate(values):
            for right in values[index + 1 :]:
                if not self._overlaps(left, right):
                    continue
                if left["team_id"] and left["team_id"] == right["team_id"]:
                    conflicts.append(
                        self._conflict(
                            "team_overlap",
                            "high",
                            left["team_id"],
                            left["team_name"] or right["team_name"],
                            [left["order_id"], right["order_id"]],
                            "场景扰动后同一班组时间窗口重叠",
                            "优先尝试更换班组或交换顺序",
                        )
                    )
                if left["individual_user_id"] and left["individual_user_id"] == right["individual_user_id"]:
                    conflicts.append(
                        self._conflict(
                            "individual_overlap",
                            "high",
                            left["individual_user_id"],
                            left["individual_username"] or right["individual_username"],
                            [left["order_id"], right["order_id"]],
                            "场景扰动后同一人员时间窗口重叠",
                            "调整执行人或改派班组",
                        )
                    )
                if left["stand_id"] and left["stand_id"] == right["stand_id"]:
                    conflicts.append(
                        self._conflict(
                            "stand_overlap",
                            "medium",
                            left["stand_id"],
                            left["stand_code"] or right["stand_code"],
                            [left["order_id"], right["order_id"]],
                            "场景扰动后机位保障时间重叠",
                            "复核机位计划并协调窗口",
                        )
                    )
                common_equipment = sorted(left["equipment_ids"].intersection(right["equipment_ids"]))
                if common_equipment:
                    conflicts.append(
                        self._conflict(
                            "equipment_overlap",
                            "high",
                            common_equipment[0],
                            None,
                            [left["order_id"], right["order_id"]],
                            "场景扰动后同一设备被重复占用",
                            "优先替换设备或错峰执行",
                        )
                    )
        return conflicts

    @staticmethod
    def _overlaps(left: dict[str, Any], right: dict[str, Any]) -> bool:
        return (
            left["projected_start_time"] < right["projected_end_time"]
            and right["projected_start_time"] < left["projected_end_time"]
        )

    @staticmethod
    def _conflict(
        conflict_type: str,
        severity: str,
        resource_id: str | None,
        resource_name: str | None,
        related_dispatch_order_ids: list[str],
        message: str,
        suggested_action: str,
    ) -> dict[str, Any]:
        return {
            "conflict_type": conflict_type,
            "severity": severity,
            "resource_id": resource_id,
            "resource_name": resource_name,
            "related_dispatch_order_ids": related_dispatch_order_ids,
            "message": message,
            "suggested_action": suggested_action,
            "context": {},
        }

    @staticmethod
    def _impact_item(
        state: dict[str, Any],
        *,
        impact_type: str,
        severity: str,
        message: str,
    ) -> dict[str, Any]:
        return {
            "dispatch_order_id": state["order_id"],
            "flight_id": state.get("flight_id"),
            "impact_type": impact_type,
            "severity": severity,
            "message": message,
            "original_start_time": state["original_start_time"].isoformat()
            if state.get("original_start_time")
            else None,
            "original_end_time": state["original_end_time"].isoformat() if state.get("original_end_time") else None,
            "projected_start_time": state["projected_start_time"].isoformat()
            if state.get("projected_start_time")
            else None,
            "projected_end_time": state["projected_end_time"].isoformat() if state.get("projected_end_time") else None,
        }

    @staticmethod
    def _equipment_ids(order: Any) -> set[str]:
        result: set[str] = set()
        for item in getattr(order, "equipment_list", None) or []:
            value = str(getattr(item, "id", None) or item or "").strip()
            if value:
                result.add(value)
        return result

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

    @staticmethod
    def _deduplicate_conflicts(items: list[dict[str, Any]]) -> list[dict[str, Any]]:
        unique: dict[tuple[str, str, str], dict[str, Any]] = {}
        for item in items:
            key = (
                str(item.get("conflict_type") or ""),
                str(item.get("resource_id") or ""),
                ",".join(sorted(str(value) for value in item.get("related_dispatch_order_ids") or [])),
            )
            current = unique.get(key)
            if current is None or str(item.get("severity") or "") == "high":
                unique[key] = item
        return list(unique.values())

    @staticmethod
    def _deduplicate_impacts(items: list[dict[str, Any]]) -> list[dict[str, Any]]:
        unique: dict[tuple[str, str], dict[str, Any]] = {}
        for item in items:
            key = (str(item.get("dispatch_order_id") or ""), str(item.get("impact_type") or ""))
            unique[key] = item
        return list(unique.values())

    @staticmethod
    def _deduplicate_recommendations(items: list[dict[str, Any]]) -> list[dict[str, Any]]:
        unique: dict[tuple[str, str], dict[str, Any]] = {}
        for item in items:
            key = (str(item.get("dispatch_order_id") or ""), str(item.get("action") or ""))
            unique[key] = item
        return list(unique.values())

    @staticmethod
    def _risk_level(
        *,
        conflict_count: int,
        impacted_count: int,
        stand_impacted: int,
        equipment_impacted: int,
    ) -> str:
        if stand_impacted > 0 or conflict_count >= 5 or impacted_count >= 6:
            return "critical"
        if equipment_impacted > 0 or conflict_count >= 3 or impacted_count >= 4:
            return "high"
        if conflict_count > 0 or impacted_count > 0:
            return "medium"
        return "low"
