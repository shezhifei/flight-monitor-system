"""Dispatch conflict validation and emergency replan service."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timedelta
from typing import Any, ClassVar

from src.domain.models.dispatch import AssigneeType, DispatchLockLevel
from src.domain.utils.time_utils import utc_now

SEVERITY_ORDER = {
    "critical": 4,
    "high": 3,
    "medium": 2,
    "low": 1,
}


@dataclass
class ReplanSuggestion:
    dispatch_order_id: str
    reason: str
    suggestion_type: str | None
    order_class: str | None
    original_start_time: datetime | None
    original_end_time: datetime | None
    suggested_start_time: datetime | None
    suggested_end_time: datetime | None
    related_dispatch_order_id: str | None
    impact_score: float
    current_assignment: dict[str, Any] | None = None
    suggested_assignment: dict[str, Any] | None = None
    lateness_minutes: int = 0
    travel_minutes: int = 0


class DispatchConflictService:
    """Provide conflict checks for dispatch scheduling flows."""

    ACTIVE_STATUSES: ClassVar[list[str]] = ["pending", "assigned", "in_progress"]

    def __init__(
        self,
        order_repo: Any,
        team_type_repo: Any | None = None,
        team_repo: Any | None = None,
        resource_availability_service: Any | None = None,
    ):
        self._order_repo = order_repo
        self._team_type_repo = team_type_repo
        self._team_repo = team_repo
        self._resource_availability_service = resource_availability_service

    @staticmethod
    def _order_interval(order: Any, fallback_now: datetime | None = None) -> tuple[datetime, datetime]:
        baseline_now = fallback_now or utc_now()
        start = (
            order.actual_start_time
            or order.planned_start_time
            or order.dispatched_at
            or order.created_at
            or baseline_now
        )
        end = order.actual_end_time or order.planned_end_time
        if end is None:
            end = start + timedelta(minutes=15)
        if end < start:
            end = start
        return start, end

    @staticmethod
    def _build_conflict(
        *,
        conflict_type: str,
        severity: str,
        resource_id: str | None,
        resource_name: str | None,
        related_dispatch_order_ids: list[str],
        message: str,
        suggested_action: str | None,
        context: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        return {
            "conflict_type": conflict_type,
            "severity": severity,
            "resource_id": resource_id,
            "resource_name": resource_name,
            "related_dispatch_order_ids": related_dispatch_order_ids,
            "message": message,
            "suggested_action": suggested_action,
            "context": context or {},
        }

    @staticmethod
    def _order_member_user_ids(order: Any) -> list[str]:
        user_ids: list[str] = []
        direct_user_id = str(getattr(order, "individual_user_id", "") or "").strip()
        if direct_user_id:
            user_ids.append(direct_user_id)

        for member in getattr(order, "members", None) or []:
            if getattr(member, "is_active", True) is False:
                continue
            normalized = str(getattr(member, "user_id", "") or "").strip()
            if normalized:
                user_ids.append(normalized)

        task_crew = getattr(order, "task_crew", None) or {}
        for member in task_crew.get("members") or []:
            normalized = str((member or {}).get("user_id") or "").strip()
            if normalized:
                user_ids.append(normalized)

        return list(dict.fromkeys(user_ids))

    @staticmethod
    def _deduplicate_conflicts(items: list[dict[str, Any]]) -> list[dict[str, Any]]:
        unique: dict[tuple[str, str, str], dict[str, Any]] = {}
        for item in items:
            related = item.get("related_dispatch_order_ids") or []
            key = (
                str(item.get("conflict_type") or ""),
                str(item.get("resource_id") or ""),
                ",".join(sorted(str(value) for value in related)),
            )
            current = unique.get(key)
            if current is None:
                unique[key] = item
                continue
            if SEVERITY_ORDER.get(str(item.get("severity")), 0) > SEVERITY_ORDER.get(str(current.get("severity")), 0):
                unique[key] = item

        result = list(unique.values())
        result.sort(
            key=lambda item: (
                -SEVERITY_ORDER.get(str(item.get("severity")), 0),
                str(item.get("conflict_type") or ""),
            )
        )
        return result

    async def validate_candidate(
        self,
        *,
        planned_start_time: datetime,
        planned_end_time: datetime,
        team_id: str | None = None,
        individual_user_id: str | None = None,
        stand_id: str | None = None,
        equipment_ids: list[str] | None = None,
        exclude_order_id: str | None = None,
    ) -> list[dict[str, Any]]:
        """Validate candidate assignment and return detailed conflicts."""
        conflicts: list[dict[str, Any]] = []

        overlaps = await self._order_repo.find_overlapping_orders(
            window_start=planned_start_time,
            window_end=planned_end_time,
            team_id=team_id,
            individual_user_id=individual_user_id,
            stand_id=stand_id,
            exclude_order_id=exclude_order_id,
        )

        for order in overlaps:
            if team_id and order.team_id == team_id:
                conflicts.append(
                    self._build_conflict(
                        conflict_type="team_overlap",
                        severity="high",
                        resource_id=team_id,
                        resource_name=order.team_name,
                        related_dispatch_order_ids=[order.id],
                        message="班组在目标时间段已被占用",
                        suggested_action="更换班组或调整任务时间",
                        context={"conflict_order_status": order.status.value},
                    )
                )

            if individual_user_id and order.individual_user_id == individual_user_id:
                matched_user_ids = {individual_user_id}
            elif individual_user_id:
                matched_user_ids = {
                    user_id for user_id in self._order_member_user_ids(order) if user_id == individual_user_id
                }
            else:
                matched_user_ids = set()
            if matched_user_ids:
                conflicts.append(
                    self._build_conflict(
                        conflict_type="individual_overlap",
                        severity="high",
                        resource_id=individual_user_id,
                        resource_name=order.individual_username,
                        related_dispatch_order_ids=[order.id],
                        message="人员在目标时间段已有派工任务",
                        suggested_action="更换人员或调整任务时间",
                        context={
                            "conflict_order_status": order.status.value,
                            "matched_user_ids": sorted(matched_user_ids),
                        },
                    )
                )

            if stand_id and order.stand_id == stand_id:
                conflicts.append(
                    self._build_conflict(
                        conflict_type="stand_overlap",
                        severity="medium",
                        resource_id=stand_id,
                        resource_name=order.stand_code,
                        related_dispatch_order_ids=[order.id],
                        message="机位同时间段存在保障任务重叠",
                        suggested_action="确认机位占用计划并调整时间窗口",
                        context={"conflict_order_status": order.status.value},
                    )
                )

        normalized_equipment_ids = [str(item).strip() for item in (equipment_ids or []) if str(item).strip()]
        if normalized_equipment_ids and hasattr(self._order_repo, "find_equipment_conflicts"):
            equipment_conflicts = await self._order_repo.find_equipment_conflicts(
                equipment_ids=normalized_equipment_ids,
                window_start=planned_start_time,
                window_end=planned_end_time,
                exclude_order_id=exclude_order_id,
            )
            for row in equipment_conflicts:
                equipment_id = row.get("equipment_id")
                conflicts.append(
                    self._build_conflict(
                        conflict_type="equipment_overlap",
                        severity="high",
                        resource_id=str(equipment_id) if equipment_id else None,
                        resource_name=None,
                        related_dispatch_order_ids=[str(row.get("dispatch_order_id"))],
                        message="设备在目标时间段已被占用",
                        suggested_action="更换设备或调整任务时间",
                        context={},
                    )
                )

        return self._deduplicate_conflicts(conflicts)

    async def list_conflicts(
        self,
        *,
        window_start: datetime,
        window_end: datetime,
        limit: int = 500,
    ) -> list[dict[str, Any]]:
        """Compute current conflicts from active orders in a window."""
        orders = await self._order_repo.find_orders_in_window(
            window_start=window_start,
            window_end=window_end,
            statuses=self.ACTIVE_STATUSES,
        )
        fallback_now = utc_now()

        conflicts: list[dict[str, Any]] = []
        for index, left in enumerate(orders):
            left_start, left_end = self._order_interval(left, fallback_now)
            for right in orders[index + 1 :]:
                right_start, right_end = self._order_interval(right, fallback_now)
                if left_end < right_start or right_end < left_start:
                    continue

                if left.team_id and right.team_id and left.team_id == right.team_id:
                    conflicts.append(
                        self._build_conflict(
                            conflict_type="team_overlap",
                            severity="high",
                            resource_id=left.team_id,
                            resource_name=left.team_name or right.team_name,
                            related_dispatch_order_ids=[left.id, right.id],
                            message="班组在同一时间段被重复分配",
                            suggested_action="优先调整后创建工单的开始时间",
                            context={},
                        )
                    )

                if (
                    left.individual_user_id
                    and right.individual_user_id
                    and left.individual_user_id == right.individual_user_id
                ):
                    conflicts.append(
                        self._build_conflict(
                            conflict_type="individual_overlap",
                            severity="high",
                            resource_id=left.individual_user_id,
                            resource_name=left.individual_username or right.individual_username,
                            related_dispatch_order_ids=[left.id, right.id],
                            message="同一人员在同一时间段被重复分配",
                            suggested_action="更换执行人或错峰执行",
                            context={},
                        )
                    )

                overlapping_user_ids = sorted(
                    set(self._order_member_user_ids(left)).intersection(self._order_member_user_ids(right))
                )
                if overlapping_user_ids:
                    resource_id = overlapping_user_ids[0]
                    if not (
                        left.individual_user_id
                        and right.individual_user_id
                        and left.individual_user_id == right.individual_user_id
                    ):
                        conflicts.append(
                            self._build_conflict(
                                conflict_type="person_time_overlap",
                                severity="high",
                                resource_id=resource_id,
                                resource_name=None,
                                related_dispatch_order_ids=[left.id, right.id],
                                message="同一成员在同一时间段参与了多个任务编组",
                                suggested_action="更换成员、重组编组或错峰执行",
                                context={"matched_user_ids": overlapping_user_ids},
                            )
                        )

                if left.stand_id and right.stand_id and left.stand_id == right.stand_id:
                    conflicts.append(
                        self._build_conflict(
                            conflict_type="stand_overlap",
                            severity="medium",
                            resource_id=left.stand_id,
                            resource_name=left.stand_code or right.stand_code,
                            related_dispatch_order_ids=[left.id, right.id],
                            message="机位时间窗口重叠",
                            suggested_action="核对作业类型依赖关系后再调整窗口",
                            context={},
                        )
                    )

                if len(conflicts) >= limit:
                    return self._deduplicate_conflicts(conflicts)[:limit]

        return self._deduplicate_conflicts(conflicts)[:limit]

    async def replan(
        self,
        *,
        window_start: datetime,
        window_end: datetime,
        strategy: str,
        apply_changes: bool,
        max_suggestions: int,
    ) -> list[ReplanSuggestion]:
        """Generate optional replan suggestions and apply them if requested."""
        buffer_minutes = {
            "stability": 10,
            "balanced": 5,
            "efficiency": 0,
        }.get(strategy, 5)
        min_duration = timedelta(minutes=5)

        orders = await self._order_repo.find_orders_in_window(
            window_start=window_start,
            window_end=window_end,
            statuses=self.ACTIVE_STATUSES,
        )
        fallback_now = utc_now()
        order_by_id = {order.id: order for order in orders}

        grouped: dict[tuple[str, str], list[Any]] = {}
        for order in orders:
            if order.team_id:
                grouped.setdefault(("team", order.team_id), []).append(order)
            for user_id in self._order_member_user_ids(order):
                grouped.setdefault(("user", user_id), []).append(order)
            if order.stand_id:
                grouped.setdefault(("stand", order.stand_id), []).append(order)

        suggestions_by_order: dict[str, ReplanSuggestion] = {}

        for _, group_orders in grouped.items():
            group_orders.sort(key=lambda current: self._order_interval(current, fallback_now)[0])
            for idx in range(1, len(group_orders)):
                previous = group_orders[idx - 1]
                current = group_orders[idx]
                _, previous_end = self._order_interval(previous, fallback_now)
                current_start, current_end = self._order_interval(current, fallback_now)

                target_start = previous_end + timedelta(minutes=buffer_minutes)
                if current_start >= target_start:
                    continue

                reassignment_candidate = await self._build_reassignment_suggestion(
                    current=current,
                    previous=previous,
                    planned_start_time=current_start,
                    planned_end_time=current_end,
                )
                if reassignment_candidate is not None:
                    existing = suggestions_by_order.get(current.id)
                    if existing is None or reassignment_candidate.impact_score <= existing.impact_score:
                        suggestions_by_order[current.id] = reassignment_candidate
                    continue

                duration = max(current_end - current_start, min_duration)
                suggested_start = target_start
                suggested_end = suggested_start + duration
                impact_minutes = max(0.0, (suggested_start - current_start).total_seconds() / 60.0)

                existing = suggestions_by_order.get(current.id)
                candidate = ReplanSuggestion(
                    dispatch_order_id=current.id,
                    reason="resource_time_overlap",
                    suggestion_type="assigned_conflict_resolution",
                    order_class="assigned_conflict",
                    original_start_time=current.planned_start_time,
                    original_end_time=current.planned_end_time,
                    suggested_start_time=suggested_start,
                    suggested_end_time=suggested_end,
                    related_dispatch_order_id=previous.id,
                    impact_score=round(impact_minutes, 2),
                    current_assignment=self._assignment_for_order(current),
                    suggested_assignment=self._assignment_for_order(current),
                    lateness_minutes=round(impact_minutes),
                )
                if existing is None or candidate.impact_score > existing.impact_score:
                    suggestions_by_order[current.id] = candidate

        suggestions = sorted(
            suggestions_by_order.values(),
            key=lambda item: item.impact_score,
            reverse=True,
        )[:max_suggestions]

        if apply_changes and suggestions:
            updated_orders: list[Any] = []
            pending_logs: list[dict[str, Any]] = []
            for suggestion in suggestions:
                order = order_by_id.get(suggestion.dispatch_order_id)
                if order is None:
                    continue
                order.planned_start_time = suggestion.suggested_start_time
                order.planned_end_time = suggestion.suggested_end_time
                self._apply_suggested_assignment(order, suggestion.suggested_assignment)
                updated_orders.append(order)
                pending_logs.append(
                    {
                        "dispatch_order_id": order.id,
                        "action": "replanned",
                        "actor_id": None,
                        "details": {
                            "strategy": strategy,
                            "original_start_time": suggestion.original_start_time.isoformat()
                            if suggestion.original_start_time
                            else None,
                            "original_end_time": suggestion.original_end_time.isoformat()
                            if suggestion.original_end_time
                            else None,
                            "suggested_start_time": suggestion.suggested_start_time.isoformat()
                            if suggestion.suggested_start_time
                            else None,
                            "suggested_end_time": suggestion.suggested_end_time.isoformat()
                            if suggestion.suggested_end_time
                            else None,
                            "suggested_assignment": suggestion.suggested_assignment,
                        },
                    }
                )

            save_batch = getattr(self._order_repo, "save_batch", None)
            if callable(save_batch):
                await save_batch(updated_orders)
            else:
                for order in updated_orders:
                    await self._order_repo.save(order)

            append_logs = getattr(self._order_repo, "append_logs", None)
            if callable(append_logs):
                await append_logs(pending_logs)
            elif hasattr(self._order_repo, "append_log"):
                for log_item in pending_logs:
                    await self._order_repo.append_log(
                        dispatch_order_id=log_item["dispatch_order_id"],
                        action=log_item["action"],
                        actor_id=log_item["actor_id"],
                        details=log_item["details"],
                    )

        return suggestions

    async def summarize_replan(
        self,
        suggestions: list[ReplanSuggestion],
    ) -> dict[str, Any]:
        affected_flights = set()
        changed_orders = set()
        reassigned_orders = 0
        delayed_orders = 0
        added_delay_minutes = 0.0
        requires_manual_confirmation = False

        for suggestion in suggestions:
            try:
                order = await self._order_repo.find_by_id(suggestion.dispatch_order_id, load_members=False)
            except TypeError:
                order = await self._order_repo.find_by_id(suggestion.dispatch_order_id)
            if order is not None and getattr(order, "flight_id", None):
                affected_flights.add(str(order.flight_id))
            changed_orders.add(suggestion.dispatch_order_id)
            current_assignment = suggestion.current_assignment or {}
            suggested_assignment = suggestion.suggested_assignment or {}
            if current_assignment != suggested_assignment:
                reassigned_orders += 1
            if (
                suggestion.original_start_time is not None
                and suggestion.suggested_start_time is not None
                and suggestion.suggested_start_time > suggestion.original_start_time
            ):
                delayed_orders += 1
                added_delay_minutes += max(
                    0.0,
                    (suggestion.suggested_start_time - suggestion.original_start_time).total_seconds() / 60.0,
                )
            if suggestion.impact_score >= 15 or suggestion.suggestion_type == "assigned_conflict_resolution":
                requires_manual_confirmation = True

        risk_level = "low"
        if added_delay_minutes >= 60 or reassigned_orders >= 5:
            risk_level = "critical"
        elif added_delay_minutes >= 30 or reassigned_orders >= 3:
            risk_level = "high"
        elif added_delay_minutes > 0 or reassigned_orders > 0:
            risk_level = "medium"

        return {
            "impact_summary": {
                "affected_flights": len(affected_flights),
                "changed_orders": len(changed_orders),
                "reassigned_orders": reassigned_orders,
                "delayed_orders": delayed_orders,
                "added_delay_minutes": round(added_delay_minutes, 2),
            },
            "changed_orders": sorted(changed_orders),
            "risk_level": risk_level,
            "requires_manual_confirmation": requires_manual_confirmation,
        }

    async def _build_reassignment_suggestion(
        self,
        *,
        current: Any,
        previous: Any,
        planned_start_time: datetime,
        planned_end_time: datetime,
    ) -> ReplanSuggestion | None:
        if self._team_type_repo is None or self._team_repo is None or self._resource_availability_service is None:
            return None
        current_lock_level = getattr(
            getattr(current, "lock_level", None),
            "value",
            getattr(current, "lock_level", DispatchLockLevel.OPTIMIZABLE.value),
        )
        if str(current_lock_level) in {
            DispatchLockLevel.FROZEN.value,
            DispatchLockLevel.MANUAL_LOCK.value,
        }:
            return None
        if not getattr(current, "task_type", None):
            return None
        team_types = await self._team_type_repo.find_by_task_type(current.task_type)
        team_type_ids = [str(item.id) for item in (team_types or []) if getattr(item, "id", None)]
        if not team_type_ids:
            return None
        teams = await self._team_repo.find_available_for_dispatch(
            team_type_ids=team_type_ids,
            terminal=getattr(current, "terminal", None),
        )
        for team in teams or []:
            team_id = str(getattr(team, "id", "") or "")
            if team_id == str(getattr(current, "team_id", "") or ""):
                continue
            availability = await self._resource_availability_service.evaluate_team(
                team=team,
                planned_start_time=planned_start_time,
                planned_end_time=planned_end_time,
                terminal=getattr(current, "terminal", None),
                exclude_order_id=str(getattr(current, "id", "") or ""),
            )
            if not availability.available:
                continue
            return ReplanSuggestion(
                dispatch_order_id=current.id,
                reason="resource_reassignment",
                suggestion_type="assigned_conflict_resolution",
                order_class="assigned_conflict",
                original_start_time=current.planned_start_time,
                original_end_time=current.planned_end_time,
                suggested_start_time=current.planned_start_time,
                suggested_end_time=current.planned_end_time,
                related_dispatch_order_id=previous.id,
                impact_score=1.0,
                current_assignment=self._assignment_for_order(current),
                suggested_assignment={
                    **self._assignment_for_order(current),
                    "assignee_type": AssigneeType.TEAM.value,
                    "team_id": team_id,
                },
            )
        return None

    @staticmethod
    def _assignment_for_order(order: Any) -> dict[str, Any]:
        assignee_type = getattr(getattr(order, "assignee_type", None), "value", getattr(order, "assignee_type", None))
        equipment_list = getattr(order, "equipment_list", None) or []
        equipment_ids = [
            str(getattr(item, "id", "") or "") for item in equipment_list if str(getattr(item, "id", "") or "")
        ]
        members = getattr(order, "members", None) or []
        member_user_ids = [
            str(getattr(item, "user_id", "") or "") for item in members if str(getattr(item, "user_id", "") or "")
        ]
        if not member_user_ids:
            task_crew = getattr(order, "task_crew", None) or {}
            member_user_ids = [
                str((item or {}).get("user_id") or "")
                for item in task_crew.get("members") or []
                if str((item or {}).get("user_id") or "")
            ]
        return {
            "assignee_type": assignee_type,
            "team_id": getattr(order, "team_id", None),
            "individual_user_id": getattr(order, "individual_user_id", None),
            "equipment_ids": equipment_ids,
            "member_user_ids": member_user_ids,
            "department_rule_version": getattr(order, "department_rule_version", None),
            "crew_requirement_snapshot": getattr(order, "crew_requirement_snapshot", None) or [],
            "qualification_gap": getattr(order, "qualification_gap", None) or [],
            "task_crew": getattr(order, "task_crew", None) or {},
        }

    @staticmethod
    def _apply_suggested_assignment(order: Any, assignment: dict[str, Any] | None) -> None:
        if not assignment:
            return
        assignee_type = str(assignment.get("assignee_type") or "").strip()
        if assignee_type == AssigneeType.TEAM.value:
            order.assignee_type = AssigneeType.TEAM
            order.team_id = assignment.get("team_id")
            order.individual_user_id = None
        elif assignee_type == AssigneeType.INDIVIDUAL.value:
            order.assignee_type = AssigneeType.INDIVIDUAL
            order.individual_user_id = assignment.get("individual_user_id")
            order.team_id = None
        order.department_rule_version = assignment.get("department_rule_version")
        order.crew_requirement_snapshot = assignment.get("crew_requirement_snapshot") or []
        order.qualification_gap = assignment.get("qualification_gap") or []
        order.task_crew = assignment.get("task_crew") or {}

    async def cascade_delay_preview(
        self,
        *,
        flight_id: str,
        delayed_task_type: str,
        delay_minutes: float,
        scheduled_departure: datetime | None = None,
    ) -> dict[str, Any]:
        """Preview how a single task type delay cascades through subsequent task types.

        Returns a dict with:
          - delayed_task_type: the task type that was delayed
          - delay_minutes: input delay
          - cascaded_task_types: list of dicts with task_type, original_start, original_end,
            projected_start, projected_end, shift_minutes
          - departure_impact_minutes: estimated departure delay (0 if no impact)
        """
        orders = await self._order_repo.find_by_flight(flight_id)
        if not orders:
            return {
                "delayed_task_type": delayed_task_type,
                "delay_minutes": delay_minutes,
                "cascaded_task_types": [],
                "departure_impact_minutes": 0,
            }

        fallback_now = utc_now()

        # Sort by planned start time (sequence)
        orders.sort(key=lambda o: o.planned_start_time or o.created_at or fallback_now)

        # Find the index of the delayed task type
        delayed_idx = None
        for idx, order in enumerate(orders):
            if order.task_type == delayed_task_type:
                delayed_idx = idx
                break

        if delayed_idx is None:
            return {
                "delayed_task_type": delayed_task_type,
                "delay_minutes": delay_minutes,
                "cascaded_task_types": [],
                "departure_impact_minutes": 0,
            }

        delta = timedelta(minutes=delay_minutes)
        cascaded: list[dict[str, Any]] = []

        # The delayed task type itself
        source = orders[delayed_idx]
        source_start, source_end = self._order_interval(source, fallback_now)
        new_source_end = source_end + delta

        cascaded.append(
            {
                "task_type": source.task_type,
                "task_type_name": getattr(source, "task_type_name", None) or source.task_type,
                "original_start": source_start.isoformat(),
                "original_end": source_end.isoformat(),
                "projected_start": source_start.isoformat(),
                "projected_end": new_source_end.isoformat(),
                "shift_minutes": round(delay_minutes, 2),
            }
        )

        # Cascade to subsequent task_types
        previous_projected_end = new_source_end
        for order in orders[delayed_idx + 1 :]:
            o_start, o_end = self._order_interval(order, fallback_now)
            duration = o_end - o_start

            if o_start < previous_projected_end:
                # This task type is pushed
                shift = previous_projected_end - o_start
                shift_minutes = shift.total_seconds() / 60.0
                projected_start = previous_projected_end
                projected_end = projected_start + duration
            else:
                # Enough gap, no cascade
                shift_minutes = 0.0
                projected_start = o_start
                projected_end = o_end

            cascaded.append(
                {
                    "task_type": order.task_type,
                    "task_type_name": getattr(order, "task_type_name", None) or order.task_type,
                    "original_start": o_start.isoformat(),
                    "original_end": o_end.isoformat(),
                    "projected_start": projected_start.isoformat(),
                    "projected_end": projected_end.isoformat(),
                    "shift_minutes": round(shift_minutes, 2),
                }
            )

            if shift_minutes > 0:
                previous_projected_end = projected_end
            else:
                previous_projected_end = o_end

        # Estimate departure impact
        departure_impact = 0.0
        if scheduled_departure and previous_projected_end > scheduled_departure:
            departure_impact = (previous_projected_end - scheduled_departure).total_seconds() / 60.0

        return {
            "delayed_task_type": delayed_task_type,
            "delay_minutes": delay_minutes,
            "cascaded_task_types": cascaded,
            "departure_impact_minutes": round(max(departure_impact, 0.0), 2),
        }
