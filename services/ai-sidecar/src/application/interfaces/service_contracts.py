"""Application service contracts for command/query segregation."""

from __future__ import annotations

from typing import Any, Protocol

from src.application.dto.anomaly_dtos import Anomaly, AnomalyRule, AnomalySummary
from src.application.dto.auth_dtos import RoleInfo, UserInfo
from src.application.dto.dispatch_dtos import (
    Department,
    DispatchOrder,
    Equipment,
    EquipmentType,
    Stand,
    TaskType,
    Team,
    TeamType,
)


class DispatchQueryService(Protocol):
    async def list_departments(
        self, *, include_inactive: bool = False, page: int = 1, page_size: int = 50
    ) -> list[Department]: ...

    async def get_department(self, department_id: str) -> Department | None: ...

    async def list_team_types(
        self, *, include_inactive: bool = False, page: int = 1, page_size: int = 50
    ) -> list[TeamType]: ...

    async def get_team_type(self, team_type_id: str) -> TeamType | None: ...

    async def list_teams(
        self,
        *,
        include_inactive: bool = False,
        team_type_id: str | None = None,
        terminal: str | None = None,
        page: int = 1,
        page_size: int = 50,
    ) -> list[Team]: ...

    async def get_team(self, team_id: str, *, load_members: bool = True) -> Team | None: ...

    async def list_team_members(self, team_id: str, *, include_inactive: bool = False) -> list[dict[str, Any]]: ...

    async def list_equipment_types(
        self, *, include_inactive: bool = False, page: int = 1, page_size: int = 50
    ) -> list[EquipmentType]: ...

    async def list_equipment(
        self,
        *,
        include_inactive: bool = False,
        equipment_type_id: str | None = None,
        terminal: str | None = None,
        status: str | None = None,
        page: int = 1,
        page_size: int = 50,
    ) -> list[Equipment]: ...

    async def get_equipment(self, equipment_id: str) -> Equipment | None: ...

    async def list_stands(
        self,
        *,
        terminal: str | None = None,
        include_inactive: bool = False,
        page: int = 1,
        page_size: int = 50,
    ) -> list[Stand]: ...

    async def get_stand(self, stand_id: str) -> Stand | None: ...

    async def list_task_types(
        self, *, category: str | None = None, page: int = 1, page_size: int = 100
    ) -> list[TaskType]: ...

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
    ) -> list[DispatchOrder]: ...

    async def get_order(
        self,
        order_id: str,
        *,
        department: str | None = None,
        load_members: bool = True,
    ) -> DispatchOrder | None: ...

    async def list_my_orders(self, user_id: str, *, status: str | None = None) -> list[dict[str, Any]]: ...

    async def get_timeline(
        self,
        *,
        view_mode: str = "flight",
        window_start: Any = None,
        window_end: Any = None,
        terminal: str | None = None,
        statuses: list[str] | None = None,
        source: str | None = None,
        include_cancelled: bool = False,
        is_admin: bool = False,
        department: str | None = None,
        current_time: Any = None,
    ) -> dict[str, Any]: ...

    async def get_order_timeline(self, order_id: str, *, limit: int = 200) -> dict[str, Any] | None: ...

    async def list_conflicts(self, *, window_start: Any, window_end: Any, limit: int = 200) -> list[dict[str, Any]]: ...

    async def cascade_delay_preview(
        self,
        *,
        flight_id: str,
        task_type: str,
        delay_minutes: float,
        scheduled_departure: Any = None,
    ) -> dict[str, Any]: ...


class DispatchCommandService(Protocol):
    async def create_order(self, payload: dict[str, Any], *, actor_id: str) -> dict[str, Any]: ...

    async def accept_order(self, order_id: str, payload: dict[str, Any], *, actor_id: str) -> dict[str, Any]: ...

    async def start_order(self, order_id: str, payload: dict[str, Any], *, actor_id: str) -> dict[str, Any]: ...

    async def complete_order(self, order_id: str, payload: dict[str, Any], *, actor_id: str) -> dict[str, Any]: ...

    async def cancel_order(self, order_id: str, payload: dict[str, Any], *, actor_id: str) -> dict[str, Any]: ...


class AnomalyQueryService(Protocol):
    async def list_anomalies(self, filters: dict[str, Any]) -> list[Anomaly]: ...

    async def get_anomaly(self, anomaly_id: str) -> Anomaly | None: ...

    async def list_rules(self, *, enabled_only: bool = False) -> list[AnomalyRule]: ...

    async def get_stats(self, *, start_date: Any = None, end_date: Any = None) -> AnomalySummary: ...


class AnomalyCommandService(Protocol):
    async def upsert_rule(self, payload: dict[str, Any]) -> dict[str, Any]: ...

    async def acknowledge(self, anomaly_id: str, payload: dict[str, Any], *, actor_id: str) -> dict[str, Any]: ...

    async def resolve(self, anomaly_id: str, payload: dict[str, Any], *, actor_id: str) -> dict[str, Any]: ...


class TodoCompletionPort(Protocol):
    async def on_todo_completed(self, todo_id: str) -> Any: ...


class AuthAdminQueryService(Protocol):
    async def list_users(self, filters: dict[str, Any]) -> list[UserInfo]: ...

    async def list_roles(self, filters: dict[str, Any]) -> list[RoleInfo]: ...

    async def list_permission_templates(self, filters: dict[str, Any]) -> list[dict[str, Any]]: ...


class AuthAdminCommandService(Protocol):
    async def create_role(self, payload: dict[str, Any], *, actor_id: str) -> dict[str, Any]: ...

    async def update_role(self, role_id: str, payload: dict[str, Any], *, actor_id: str) -> dict[str, Any]: ...

    async def delete_role(self, role_id: str, *, actor_id: str) -> dict[str, Any]: ...

    async def upsert_permission_template(self, payload: dict[str, Any], *, actor_id: str) -> dict[str, Any]: ...


class SSEHub(Protocol):
    """SSE hub contract — broadcast real-time events to subscribed clients.

    Used via duck typing across multiple services (smart_monitor,
    todo_chain, anomaly notify adapter, todo_graph_pilot_ops).
    """

    async def broadcast_to_topic(self, topic: str, payload: dict[str, Any]) -> None: ...

    async def publish(self, topic: str, payload: dict[str, Any]) -> None: ...
