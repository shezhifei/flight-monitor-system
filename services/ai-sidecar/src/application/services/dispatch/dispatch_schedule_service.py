"""Schedule planning application service."""

from __future__ import annotations

from datetime import datetime
from types import SimpleNamespace
from typing import Any

from src.domain.exceptions.base import EntityNotFoundException
from src.domain.models.dispatch import (
    DispatchLockLevel,
    DispatchLockRule,
    EquipmentDowntime,
    LeaveRecord,
    ShiftInstance,
    ShiftTemplate,
)
from src.shared.id_generator import generate_id


class DispatchScheduleService:
    """CRUD and availability queries for schedule planning."""

    def __init__(
        self,
        *,
        shift_template_repo: Any,
        shift_instance_repo: Any,
        schedule_exception_repo: Any,
        team_repo: Any,
        team_member_repo: Any,
        equipment_repo: Any,
        availability_service: Any,
    ) -> None:
        self._shift_template_repo = shift_template_repo
        self._shift_instance_repo = shift_instance_repo
        self._schedule_exception_repo = schedule_exception_repo
        self._team_repo = team_repo
        self._team_member_repo = team_member_repo
        self._equipment_repo = equipment_repo
        self._availability_service = availability_service

    async def list_templates(
        self,
        *,
        resource_type: str | None = None,
        resource_id: str | None = None,
        enabled: bool | None = None,
        limit: int = 100,
    ) -> list[ShiftTemplate]:
        return await self._shift_template_repo.find_all(
            resource_type=resource_type,
            resource_id=resource_id,
            enabled=enabled,
            limit=limit,
            offset=0,
        )

    async def create_template(self, payload: dict[str, Any]) -> ShiftTemplate:
        template = ShiftTemplate(
            id=generate_id(),
            name=str(payload.get("name") or "").strip(),
            resource_type=str(payload.get("resource_type") or "").strip(),
            resource_id=str(payload.get("resource_id") or "").strip(),
            terminal=payload.get("terminal"),
            start_time_local=str(payload.get("start_time_local") or "08:00"),
            end_time_local=str(payload.get("end_time_local") or "16:00"),
            weekdays=list(payload.get("weekdays") or []),
            max_continuous_minutes=payload.get("max_continuous_minutes"),
            min_rest_minutes=payload.get("min_rest_minutes"),
            enabled=bool(payload.get("enabled", True)),
        )
        return await self._shift_template_repo.save(template)

    async def list_instances(
        self,
        *,
        resource_type: str | None = None,
        resource_id: str | None = None,
        window_start: datetime | None = None,
        window_end: datetime | None = None,
        limit: int = 200,
    ) -> list[ShiftInstance]:
        return await self._shift_instance_repo.find_all(
            resource_type=resource_type,
            resource_id=resource_id,
            window_start=window_start,
            window_end=window_end,
            limit=limit,
            offset=0,
        )

    async def create_instance(self, payload: dict[str, Any]) -> ShiftInstance:
        instance = ShiftInstance(
            id=generate_id(),
            template_id=payload.get("template_id"),
            resource_type=str(payload.get("resource_type") or "").strip(),
            resource_id=str(payload.get("resource_id") or "").strip(),
            terminal=payload.get("terminal"),
            start_time=payload.get("start_time"),
            end_time=payload.get("end_time"),
            status=str(payload.get("status") or "scheduled"),
            max_continuous_minutes=payload.get("max_continuous_minutes"),
            min_rest_minutes=payload.get("min_rest_minutes"),
        )
        return await self._shift_instance_repo.save(instance)

    async def list_exceptions(
        self,
        *,
        window_start: datetime | None = None,
        window_end: datetime | None = None,
        limit: int = 200,
    ) -> list[dict[str, Any]]:
        return await self._schedule_exception_repo.list_exceptions(
            window_start=window_start,
            window_end=window_end,
            limit=limit,
        )

    async def create_exception(self, payload: dict[str, Any]) -> dict[str, Any]:
        exception_type = str(payload.get("exception_type") or "").strip()
        if exception_type == "leave":
            record = await self._schedule_exception_repo.save_leave_record(
                LeaveRecord(
                    id=generate_id(),
                    user_id=str(payload.get("user_id") or "").strip(),
                    team_id=payload.get("team_id"),
                    start_time=payload.get("start_time"),
                    end_time=payload.get("end_time"),
                    reason=payload.get("reason"),
                    status=str(payload.get("status") or "approved"),
                )
            )
            return {
                "id": record.id,
                "exception_type": "leave",
                "resource_id": record.user_id,
                "team_id": record.team_id,
                "start_time": record.start_time,
                "end_time": record.end_time,
                "status": record.status,
                "reason": record.reason,
            }
        if exception_type == "equipment_downtime":
            downtime = await self._schedule_exception_repo.save_equipment_downtime(
                EquipmentDowntime(
                    id=generate_id(),
                    equipment_id=str(payload.get("equipment_id") or "").strip(),
                    start_time=payload.get("start_time"),
                    end_time=payload.get("end_time"),
                    reason=payload.get("reason"),
                    status=str(payload.get("status") or "scheduled"),
                )
            )
            return {
                "id": downtime.id,
                "exception_type": "equipment_downtime",
                "resource_id": downtime.equipment_id,
                "start_time": downtime.start_time,
                "end_time": downtime.end_time,
                "status": downtime.status,
                "reason": downtime.reason,
            }
        if exception_type == "dispatch_lock":
            rule = await self._schedule_exception_repo.save_lock_rule(
                DispatchLockRule(
                    id=generate_id(),
                    dispatch_order_id=payload.get("dispatch_order_id"),
                    flight_id=payload.get("flight_id"),
                    team_id=payload.get("team_id"),
                    lock_level=DispatchLockLevel(str(payload.get("lock_level") or DispatchLockLevel.MANUAL_LOCK.value)),
                    start_time=payload.get("start_time"),
                    end_time=payload.get("end_time"),
                    reason=payload.get("reason"),
                )
            )
            return {
                "id": rule.id,
                "exception_type": "dispatch_lock",
                "resource_id": rule.team_id or rule.flight_id or rule.dispatch_order_id,
                "dispatch_order_id": rule.dispatch_order_id,
                "start_time": rule.start_time,
                "end_time": rule.end_time,
                "status": rule.lock_level.value,
                "reason": rule.reason,
            }
        raise EntityNotFoundException(entity_type="排班异常", entity_id="不支持的异常类型")

    async def get_availability(
        self,
        *,
        resource_type: str,
        planned_start_time: datetime,
        planned_end_time: datetime,
        terminal: str | None = None,
        resource_ids: list[str] | None = None,
    ) -> list[dict[str, Any]]:
        normalized_resource_ids = [str(item).strip() for item in (resource_ids or []) if str(item).strip()]
        if resource_type == "team":
            teams = await self._team_repo.find_all(
                include_inactive=False,
                team_type_id=None,
                terminal=terminal,
                limit=max(1, len(normalized_resource_ids) or 200),
                offset=0,
            )
            if normalized_resource_ids:
                teams = [item for item in teams if str(getattr(item, "id", "") or "") in normalized_resource_ids]
            availability = await self._availability_service.list_team_availability(
                teams=teams,
                planned_start_time=planned_start_time,
                planned_end_time=planned_end_time,
                terminal=terminal,
            )
            return [self._availability_to_dict(item) for item in availability]
        if resource_type == "equipment":
            equipment = await self._equipment_repo.find_all(
                include_inactive=False,
                equipment_type_id=None,
                terminal=terminal,
                status=None,
                limit=max(1, len(normalized_resource_ids) or 200),
                offset=0,
            )
            if normalized_resource_ids:
                equipment = [
                    item for item in equipment if str(getattr(item, "id", "") or "") in normalized_resource_ids
                ]
            results = []
            for item in equipment:
                results.append(
                    self._availability_to_dict(
                        await self._availability_service.evaluate_equipment(
                            equipment=item,
                            planned_start_time=planned_start_time,
                            planned_end_time=planned_end_time,
                            terminal=terminal,
                        )
                    )
                )
            return results
        if resource_type == "employee":
            employees = await self._list_employee_profiles(normalized_resource_ids)
            availability = await self._availability_service.list_employee_availability(
                employees=employees,
                planned_start_time=planned_start_time,
                planned_end_time=planned_end_time,
                terminal=terminal,
            )
            return [self._availability_to_dict(item) for item in availability]
        raise EntityNotFoundException(entity_type="排班资源", entity_id="不支持的资源类型")

    async def _list_employee_profiles(self, resource_ids: list[str]) -> list[Any]:
        normalized_ids = [str(item).strip() for item in resource_ids if str(item).strip()]
        if normalized_ids:
            return [SimpleNamespace(id=user_id) for user_id in normalized_ids]

        if self._team_member_repo is None or not hasattr(self._team_member_repo, "list_active_users"):
            return []
        rows = await self._team_member_repo.list_active_users()
        return [
            SimpleNamespace(
                id=str(item.get("user_id") or "").strip(),
                username=item.get("username"),
            )
            for item in rows
            if str(item.get("user_id") or "").strip()
        ]

    @staticmethod
    def _availability_to_dict(item: Any) -> dict[str, Any]:
        return {
            "resource_type": item.resource_type,
            "resource_id": item.resource_id,
            "available": bool(item.available),
            "schedule_source": getattr(
                getattr(item, "schedule_source", None), "value", getattr(item, "schedule_source", None)
            ),
            "reason": item.reason,
            "reasons": list(item.reasons or []),
            "lock_level": getattr(getattr(item, "lock_level", None), "value", getattr(item, "lock_level", None)),
            "score_breakdown": dict(item.score_breakdown or {}),
            "metadata": dict(item.metadata or {}),
        }
