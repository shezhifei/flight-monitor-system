"""Dispatch query application service.

Read-side orchestration for dispatch-related APIs.
Routes should depend on this service instead of injecting repositories directly.
"""

from __future__ import annotations

from datetime import datetime
from typing import Any

from src.application.services.dispatch.dispatch_conflict_service import DispatchConflictService
from src.application.services.dispatch.dispatch_timeline_service import DispatchTimelineService
from src.domain.models.dispatch import (
    Department,
    DispatchOrder,
    Equipment,
    EquipmentType,
    Stand,
    TaskType,
    Team,
    TeamMember,
    TeamType,
)


class DispatchQueryCapabilityError(RuntimeError):
    """Raised when current repository implementation lacks required read capability."""


class DispatchQueryApplicationService:
    """Query-only service for dispatch and dispatch-order read use-cases."""

    def __init__(
        self,
        *,
        department_repo: Any,
        team_type_repo: Any,
        team_repo: Any,
        team_member_repo: Any,
        equipment_type_repo: Any,
        equipment_repo: Any,
        stand_repo: Any,
        task_type_repo: Any,
        order_repo: Any,
        shift_template_repo: Any | None = None,
        shift_instance_repo: Any | None = None,
        schedule_exception_repo: Any | None = None,
        availability_service: Any | None = None,
        timeline_service: DispatchTimelineService | None = None,
        conflict_service: DispatchConflictService | None = None,
    ) -> None:
        self._department_repo = department_repo
        self._team_type_repo = team_type_repo
        self._team_repo = team_repo
        self._team_member_repo = team_member_repo
        self._equipment_type_repo = equipment_type_repo
        self._equipment_repo = equipment_repo
        self._stand_repo = stand_repo
        self._task_type_repo = task_type_repo
        self._order_repo = order_repo
        self._shift_template_repo = shift_template_repo
        self._shift_instance_repo = shift_instance_repo
        self._schedule_exception_repo = schedule_exception_repo
        self._availability_service = availability_service
        self._timeline_service = timeline_service or DispatchTimelineService()
        self._conflict_service = conflict_service or DispatchConflictService(order_repo=order_repo)

    @staticmethod
    def _offset(page: int, page_size: int) -> int:
        safe_page = max(1, int(page or 1))
        safe_page_size = max(1, int(page_size or 1))
        return (safe_page - 1) * safe_page_size

    async def list_departments(
        self,
        *,
        include_inactive: bool = False,
        page: int = 1,
        page_size: int = 50,
    ) -> list[Department]:
        return await self._department_repo.find_all(
            include_inactive=include_inactive,
            limit=page_size,
            offset=self._offset(page, page_size),
        )

    async def get_department(self, department_id: str) -> Department | None:
        return await self._department_repo.find_by_id(department_id)

    async def list_team_types(
        self,
        *,
        include_inactive: bool = False,
        page: int = 1,
        page_size: int = 50,
    ) -> list[TeamType]:
        return await self._team_type_repo.find_all(
            include_inactive=include_inactive,
            limit=page_size,
            offset=self._offset(page, page_size),
        )

    async def get_team_type(self, team_type_id: str) -> TeamType | None:
        return await self._team_type_repo.find_by_id(team_type_id)

    async def list_teams(
        self,
        *,
        include_inactive: bool = False,
        team_type_id: str | None = None,
        terminal: str | None = None,
        page: int = 1,
        page_size: int = 50,
    ) -> list[Team]:
        return await self._team_repo.find_all(
            include_inactive=include_inactive,
            team_type_id=team_type_id,
            terminal=terminal,
            limit=page_size,
            offset=self._offset(page, page_size),
        )

    async def get_team(self, team_id: str, *, load_members: bool = True) -> Team | None:
        return await self._team_repo.find_by_id(team_id, load_members=load_members)

    async def list_team_members(self, team_id: str, *, include_inactive: bool = False) -> list[TeamMember]:
        return await self._team_member_repo.find_by_team(team_id, include_inactive=include_inactive)

    async def list_equipment_types(
        self,
        *,
        include_inactive: bool = False,
        page: int = 1,
        page_size: int = 50,
    ) -> list[EquipmentType]:
        return await self._equipment_type_repo.find_all(
            include_inactive=include_inactive,
            limit=page_size,
            offset=self._offset(page, page_size),
        )

    async def list_equipment(
        self,
        *,
        include_inactive: bool = False,
        equipment_type_id: str | None = None,
        terminal: str | None = None,
        status: str | None = None,
        page: int = 1,
        page_size: int = 50,
    ) -> list[Equipment]:
        return await self._equipment_repo.find_all(
            include_inactive=include_inactive,
            equipment_type_id=equipment_type_id,
            terminal=terminal,
            status=status,
            limit=page_size,
            offset=self._offset(page, page_size),
        )

    async def get_equipment(self, equipment_id: str) -> Equipment | None:
        return await self._equipment_repo.find_by_id(equipment_id)

    async def list_stands(
        self,
        *,
        terminal: str | None = None,
        include_inactive: bool = False,
        page: int = 1,
        page_size: int = 50,
    ) -> list[Stand]:
        return await self._stand_repo.find_all(
            terminal=terminal,
            include_inactive=include_inactive,
            limit=page_size,
            offset=self._offset(page, page_size),
        )

    async def get_stand(self, stand_id: str) -> Stand | None:
        return await self._stand_repo.find_by_id(stand_id)

    async def list_task_types(
        self,
        *,
        category: str | None = None,
        page: int = 1,
        page_size: int = 100,
    ) -> list[TaskType]:
        return await self._task_type_repo.find_all(
            category=category,
            limit=page_size,
            offset=self._offset(page, page_size),
        )

    async def list_orders(
        self,
        *,
        flight_id: str | None = None,
        team_id: str | None = None,
        status: str | None = None,
        source: str | None = None,
        page: int = 1,
        page_size: int = 20,
        department: str | None = None,
    ) -> list[DispatchOrder]:
        offset = self._offset(page, page_size)
        if flight_id:
            finder = getattr(self._order_repo, "find_by_flight_with_filters", None)
            if not callable(finder):
                raise DispatchQueryCapabilityError("dispatch_order_repo.find_by_flight_with_filters unavailable")
            return await finder(
                flight_id=flight_id,
                status=status,
                source=source,
                department=department,
                limit=page_size,
                offset=offset,
            )

        if team_id:
            try:
                return await self._order_repo.find_by_team(
                    team_id,
                    status=status,
                    source=source,
                    department=department,
                    limit=page_size,
                    offset=offset,
                )
            except TypeError as exc:
                raise DispatchQueryCapabilityError("dispatch_order_repo.find_by_team pagination unavailable") from exc

        return await self._order_repo.find_all(
            status=status,
            source=source,
            department=department,
            limit=page_size,
            offset=offset,
        )

    async def get_order(
        self,
        order_id: str,
        *,
        department: str | None = None,
        load_members: bool = True,
    ) -> DispatchOrder | None:
        return await self._order_repo.find_by_id(order_id, load_members=load_members, department=department)

    async def list_my_orders(self, user_id: str, *, status: str | None = None) -> list[DispatchOrder]:
        return await self._order_repo.find_by_user(user_id, status=status)

    async def get_timeline(
        self,
        *,
        view_mode: str = "flight",
        window_start: datetime | None = None,
        window_end: datetime | None = None,
        terminal: str | None = None,
        statuses: list[str] | None = None,
        source: str | None = None,
        include_cancelled: bool = False,
        is_admin: bool = False,
        department: str | None = None,
        current_time: datetime | None = None,
    ) -> dict[str, Any]:
        now = current_time
        resolved_window_start, resolved_window_end = self._timeline_service.resolve_window(
            window_start=window_start,
            window_end=window_end,
            current_time=now,
        )

        if not hasattr(self._order_repo, "find_timeline_rows"):
            raise DispatchQueryCapabilityError("dispatch_order_repo.find_timeline_rows unavailable")

        rows = await self._order_repo.find_timeline_rows(
            window_start=resolved_window_start,
            window_end=resolved_window_end,
            statuses=statuses or [],
            source=source,
            department=department,
            terminal=terminal,
            include_cancelled=include_cancelled,
        )
        return self._timeline_service.build_timeline_payload(
            rows=rows,
            view_mode=view_mode,
            is_admin=bool(is_admin),
            window_start=resolved_window_start,
            window_end=resolved_window_end,
            current_time=now,
        )

    async def get_order_timeline(self, order_id: str, *, limit: int = 200) -> dict[str, Any] | None:
        order = await self._order_repo.find_by_id(order_id, load_members=False)
        if not order:
            return None
        if not hasattr(self._order_repo, "list_logs"):
            raise DispatchQueryCapabilityError("dispatch_order_repo.list_logs unavailable")

        logs = await self._order_repo.list_logs(order_id, limit=limit)
        return {
            "dispatch_order_id": order_id,
            "items": logs,
            "total": len(logs),
        }

    async def list_conflicts(
        self,
        *,
        window_start: datetime,
        window_end: datetime,
        limit: int = 200,
    ) -> list[dict[str, Any]]:
        return await self._conflict_service.list_conflicts(
            window_start=window_start,
            window_end=window_end,
            limit=limit,
        )

    async def cascade_delay_preview(
        self,
        *,
        flight_id: str,
        task_type: str,
        delay_minutes: float,
        scheduled_departure: datetime | None = None,
    ) -> dict[str, Any]:
        return await self._conflict_service.cascade_delay_preview(
            flight_id=flight_id,
            delayed_task_type=task_type,
            delay_minutes=delay_minutes,
            scheduled_departure=scheduled_departure,
        )
