"""Rolling-horizon optimizer for dispatch scheduling."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime
from typing import Any

from src.domain.models.dispatch import DispatchLockLevel

from .dispatch_optimizer import AvailableTeam, DispatchTask, ILPDispatchOptimizer
from .dispatch_shared import Position


@dataclass(frozen=True)
class RollingOptimizationResult:
    success: bool
    assignments: list[dict[str, Any]]
    frozen_order_ids: list[str]
    total_cost: float
    solver_time_ms: float
    is_optimal: bool
    unassigned_order_ids: list[str]


class RollingHorizonOptimizer:
    """Build optimization inputs from an order window and apply freeze policy."""

    DEFAULT_NEAR_START_FREEZE_MINUTES = 15

    def __init__(self, *, optimizer: ILPDispatchOptimizer | None = None) -> None:
        self._optimizer = optimizer or ILPDispatchOptimizer()

    async def optimize_window(
        self,
        *,
        orders: list[Any],
        teams: list[Any],
        freeze_order_ids: list[str] | None = None,
        current_time: datetime,
        time_limit_seconds: float,
    ) -> RollingOptimizationResult:
        explicit_frozen_ids: set[str] = {str(item).strip() for item in (freeze_order_ids or []) if str(item).strip()}
        frozen_orders, candidate_orders = self._classify_orders(
            orders=orders,
            current_time=current_time,
            explicit_frozen_ids=explicit_frozen_ids,
        )

        tasks: list[DispatchTask] = []
        for order in candidate_orders:
            stand_position = self._extract_position(order)
            if stand_position is None:
                continue
            team_type_id = getattr(order, "team_type_id", None)
            workflow_context = getattr(order, "workflow_context", {}) or {}
            if not team_type_id:
                team_type_id = workflow_context.get("team_type_id")
            team_type_ids = workflow_context.get("team_type_ids") or []
            if team_type_id and team_type_id not in team_type_ids:
                team_type_ids = [team_type_id, *team_type_ids]
            normalized_team_type_ids = [str(item).strip() for item in team_type_ids if str(item).strip()]
            if not normalized_team_type_ids:
                continue
            tasks.append(
                DispatchTask(
                    id=str(getattr(order, "id", "") or ""),
                    flight_id=str(getattr(order, "flight_id", "") or ""),
                    task_type=str(getattr(order, "task_type", "") or ""),
                    stand_position=stand_position,
                    planned_start=order.planned_start_time,
                    planned_end=order.planned_end_time,
                    required_team_type_ids=normalized_team_type_ids,
                )
            )

        if not tasks or not teams:
            return RollingOptimizationResult(
                success=True,
                assignments=[],
                frozen_order_ids=[str(getattr(item, "id", "") or "") for item in frozen_orders],
                total_cost=0.0,
                solver_time_ms=0.0,
                is_optimal=True,
                unassigned_order_ids=[task.id for task in tasks],
            )

        frozen_available_from = self._build_team_available_from(frozen_orders, current_time)
        available_teams: list[AvailableTeam] = []
        for team in teams:
            position = self._extract_position(team)
            if position is None:
                continue
            available_teams.append(
                AvailableTeam(
                    id=str(getattr(team, "id", "") or ""),
                    name=str(getattr(team, "name", "") or ""),
                    team_type_id=str(getattr(team, "team_type_id", "") or ""),
                    position=position,
                    available_from=frozen_available_from.get(str(getattr(team, "id", "") or ""), current_time),
                )
            )

        if not available_teams:
            return RollingOptimizationResult(
                success=False,
                assignments=[],
                frozen_order_ids=[str(getattr(item, "id", "") or "") for item in frozen_orders],
                total_cost=0.0,
                solver_time_ms=0.0,
                is_optimal=False,
                unassigned_order_ids=[task.id for task in tasks],
            )

        result = await self._optimizer.optimize(tasks, available_teams, time_limit_seconds)
        assignments = [
            {
                "dispatch_order_id": assignment.task_id,
                "team_id": assignment.team_id,
                "travel_minutes": assignment.cost,
            }
            for assignment in result.assignments
        ]
        return RollingOptimizationResult(
            success=bool(getattr(result, "success", True)),
            assignments=assignments,
            frozen_order_ids=[str(getattr(item, "id", "") or "") for item in frozen_orders],
            total_cost=float(getattr(result, "total_cost", 0.0) or 0.0),
            solver_time_ms=float(getattr(result, "solver_time_ms", 0.0) or 0.0),
            is_optimal=bool(getattr(result, "is_optimal", False)),
            unassigned_order_ids=list(getattr(result, "unassigned_tasks", []) or []),
        )

    def _classify_orders(
        self,
        *,
        orders: list[Any],
        current_time: datetime,
        explicit_frozen_ids: set[str],
    ) -> tuple[list[Any], list[Any]]:
        frozen_orders: list[Any] = []
        candidate_orders: list[Any] = []
        for order in orders:
            order_id = str(getattr(order, "id", "") or "")
            if order_id in explicit_frozen_ids:
                frozen_orders.append(order)
                continue
            if self._should_freeze(order, current_time=current_time):
                frozen_orders.append(order)
                continue
            candidate_orders.append(order)
        return frozen_orders, candidate_orders

    def _should_freeze(self, order: Any, *, current_time: datetime) -> bool:
        status = str(getattr(getattr(order, "status", None), "value", getattr(order, "status", "")) or "")
        if status in {"in_progress", "completed", "cancelled"}:
            return True
        if self._uses_personalized_assignment(order):
            return True
        lock_level = getattr(order, "lock_level", DispatchLockLevel.OPTIMIZABLE)
        lock_value = getattr(lock_level, "value", lock_level)
        if lock_value in {DispatchLockLevel.FROZEN.value, DispatchLockLevel.MANUAL_LOCK.value}:
            return True
        planned_start_time = getattr(order, "planned_start_time", None)
        if planned_start_time is not None and planned_start_time <= current_time:
            return True
        return bool(
            planned_start_time is not None
            and (planned_start_time - current_time).total_seconds() <= self.DEFAULT_NEAR_START_FREEZE_MINUTES * 60
        )

    @staticmethod
    def _extract_position(entity: Any) -> Position | None:
        lat = getattr(entity, "current_position_lat", None)
        lng = getattr(entity, "current_position_lng", None)
        if lat is None or lng is None:
            lat = getattr(entity, "position_lat", None)
            lng = getattr(entity, "position_lng", None)
        if lat is None or lng is None:
            stand_position = getattr(entity, "stand_position", None)
            if stand_position is not None:
                return stand_position
            return None
        return Position(lat=float(lat), lng=float(lng))

    @staticmethod
    def _build_team_available_from(frozen_orders: list[Any], current_time: datetime) -> dict[str, datetime]:
        result: dict[str, datetime] = {}
        for order in frozen_orders:
            end_time = (
                getattr(order, "actual_end_time", None) or getattr(order, "planned_end_time", None) or current_time
            )
            for team_id in RollingHorizonOptimizer._extract_source_team_ids(order):
                previous = result.get(team_id)
                if previous is None or end_time > previous:
                    result[team_id] = end_time
        return result

    @staticmethod
    def _extract_source_team_ids(order: Any) -> list[str]:
        team_ids: list[str] = []
        direct_team_id = str(getattr(order, "team_id", "") or "").strip()
        if direct_team_id:
            team_ids.append(direct_team_id)

        task_crew = getattr(order, "task_crew", None) or {}
        for team_id in task_crew.get("source_team_ids") or []:
            normalized = str(team_id or "").strip()
            if normalized:
                team_ids.append(normalized)
        for member in task_crew.get("members") or []:
            normalized = str((member or {}).get("source_team_id") or "").strip()
            if normalized:
                team_ids.append(normalized)

        for member in getattr(order, "members", None) or []:
            normalized = str(getattr(member, "source_team_id", "") or "").strip()
            if normalized:
                team_ids.append(normalized)

        return list(dict.fromkeys(team_ids))

    @staticmethod
    def _uses_personalized_assignment(order: Any) -> bool:
        if str(getattr(order, "individual_user_id", "") or "").strip():
            return True
        if str(getattr(order, "department_rule_version", "") or "").strip():
            return True

        task_crew = getattr(order, "task_crew", None) or {}
        if task_crew.get("members"):
            return True
        if getattr(order, "crew_requirement_snapshot", None):
            return True

        for member in getattr(order, "members", None) or []:
            if str(getattr(member, "slot_code", "") or "").strip():
                return True
            if str(getattr(member, "qualification_code", "") or "").strip():
                return True
            source_type = getattr(getattr(member, "source_type", None), "value", getattr(member, "source_type", None))
            if str(source_type or "").strip() == "individual":
                return True
        return False
