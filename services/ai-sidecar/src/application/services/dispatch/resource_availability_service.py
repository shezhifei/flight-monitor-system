"""Resource availability evaluation for dispatch scheduling."""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime
from typing import Any

from src.domain.models.dispatch import DispatchLockLevel, EquipmentStatus, ScheduleSource, TeamStatus


@dataclass
class ResourceAvailability:
    resource_type: str
    resource_id: str
    available: bool
    schedule_source: ScheduleSource
    reason: str
    reasons: list[str] = field(default_factory=list)
    lock_level: DispatchLockLevel = DispatchLockLevel.OPTIMIZABLE
    score_breakdown: dict[str, float] = field(default_factory=dict)
    metadata: dict[str, Any] = field(default_factory=dict)


class ResourceAvailabilityService:
    """Centralized availability checks for teams and equipment."""

    DEFAULT_MIN_REST_MINUTES = 15

    def __init__(
        self,
        *,
        shift_instance_repo: Any | None = None,
        schedule_exception_repo: Any | None = None,
        team_member_repo: Any | None = None,
        team_repo: Any | None = None,
        order_repo: Any | None = None,
        order_member_repo: Any | None = None,
    ) -> None:
        self._shift_instance_repo = shift_instance_repo
        self._schedule_exception_repo = schedule_exception_repo
        self._team_member_repo = team_member_repo
        self._team_repo = team_repo
        self._order_repo = order_repo
        self._order_member_repo = order_member_repo

    async def evaluate_team(
        self,
        *,
        team: Any,
        planned_start_time: datetime,
        planned_end_time: datetime,
        terminal: str | None = None,
        exclude_order_id: str | None = None,
    ) -> ResourceAvailability:
        reasons: list[str] = []
        metadata: dict[str, Any] = {}
        lock_level = DispatchLockLevel.OPTIMIZABLE

        if not getattr(team, "is_active", True):
            return self._unavailable(
                resource_type="team",
                resource_id=str(getattr(team, "id", "") or ""),
                schedule_source=ScheduleSource.CURRENT_STATUS_FALLBACK,
                reason="班组已停用",
            )

        if terminal and getattr(team, "terminal", None) and str(team.terminal) != str(terminal):
            return self._unavailable(
                resource_type="team",
                resource_id=str(team.id),
                schedule_source=ScheduleSource.CURRENT_STATUS_FALLBACK,
                reason="班组不在目标航站楼值守",
            )

        instances = await self._find_shift_instances(
            resource_type="team",
            resource_id=str(team.id),
            planned_start_time=planned_start_time,
            planned_end_time=planned_end_time,
        )
        has_shift_instance = bool(instances)
        schedule_source = (
            ScheduleSource.SHIFT_INSTANCE if has_shift_instance else ScheduleSource.CURRENT_STATUS_FALLBACK
        )

        if has_shift_instance:
            active_instance = self._pick_covering_instance(instances, planned_start_time, planned_end_time)
            if active_instance is None:
                return self._unavailable(
                    resource_type="team",
                    resource_id=str(team.id),
                    schedule_source=schedule_source,
                    reason="目标时间窗没有排班实例覆盖",
                )
            metadata["shift_instance_id"] = str(getattr(active_instance, "id", "") or "")
            min_rest_minutes = int(getattr(active_instance, "min_rest_minutes", None) or self.DEFAULT_MIN_REST_MINUTES)
            metadata["min_rest_minutes"] = min_rest_minutes
            max_continuous_minutes = getattr(active_instance, "max_continuous_minutes", None)
            if max_continuous_minutes is not None:
                span_minutes = max(
                    0,
                    int((planned_end_time - planned_start_time).total_seconds() // 60),
                )
                if span_minutes > int(max_continuous_minutes):
                    return self._unavailable(
                        resource_type="team",
                        resource_id=str(team.id),
                        schedule_source=schedule_source,
                        reason="任务时长超过班组连续作业上限",
                    )
        else:
            current_status = getattr(team, "current_status", None)
            current_value = getattr(current_status, "value", current_status)
            if str(current_value or "") != TeamStatus.ON_DUTY.value:
                return self._unavailable(
                    resource_type="team",
                    resource_id=str(team.id),
                    schedule_source=schedule_source,
                    reason="班组当前不在岗，且无排班实例兜底",
                )
            reasons.append("无排班实例，已回退到 current_status=on_duty")
            metadata["fallback"] = True
            min_rest_minutes = self.DEFAULT_MIN_REST_MINUTES

        member_ids = await self._load_member_user_ids(str(team.id))
        if member_ids:
            leave_records = await self._find_leave_records(
                user_ids=member_ids,
                team_id=str(team.id),
                planned_start_time=planned_start_time,
                planned_end_time=planned_end_time,
            )
            if leave_records:
                return self._unavailable(
                    resource_type="team",
                    resource_id=str(team.id),
                    schedule_source=schedule_source,
                    reason="班组成员存在请休假冲突",
                    metadata={"leave_user_ids": [item.user_id for item in leave_records]},
                )

            rest_violation = await self._check_member_rest(
                user_ids=member_ids,
                planned_start_time=planned_start_time,
                min_rest_minutes=min_rest_minutes,
            )
            if rest_violation:
                return self._unavailable(
                    resource_type="team",
                    resource_id=str(team.id),
                    schedule_source=schedule_source,
                    reason="班组成员未满足最小休息时间",
                    metadata=rest_violation,
                )

        overlaps = await self._find_team_overlaps(
            team_id=str(team.id),
            planned_start_time=planned_start_time,
            planned_end_time=planned_end_time,
            exclude_order_id=exclude_order_id,
        )
        if overlaps:
            related_ids = [str(getattr(item, "id", "") or "") for item in overlaps]
            return self._unavailable(
                resource_type="team",
                resource_id=str(team.id),
                schedule_source=schedule_source,
                reason="班组在目标时间窗已有派工占用",
                metadata={"overlapping_order_ids": related_ids},
            )

        lock_rules = await self._find_lock_rules(
            team_id=str(team.id),
            planned_start_time=planned_start_time,
            planned_end_time=planned_end_time,
        )
        if lock_rules:
            lock_level = max(
                (item.lock_level for item in lock_rules),
                key=self._lock_rank,
            )
            if lock_level in {DispatchLockLevel.FROZEN, DispatchLockLevel.MANUAL_LOCK}:
                return self._unavailable(
                    resource_type="team",
                    resource_id=str(team.id),
                    schedule_source=schedule_source,
                    reason="班组存在冻结/人工锁定规则",
                    metadata={"lock_rule_ids": [item.id for item in lock_rules]},
                    lock_level=lock_level,
                )

        reason = reasons[0] if reasons else "班组在目标时间窗可用"
        score_breakdown = {
            "availability": 100.0,
            "fallback_penalty": -8.0 if not has_shift_instance else 0.0,
            "terminal_match": 10.0 if terminal and getattr(team, "terminal", None) == terminal else 0.0,
            "member_ready": 12.0 if member_ids else 6.0,
        }
        return ResourceAvailability(
            resource_type="team",
            resource_id=str(team.id),
            available=True,
            schedule_source=schedule_source,
            reason=reason,
            reasons=reasons,
            lock_level=lock_level,
            score_breakdown=score_breakdown,
            metadata=metadata,
        )

    async def evaluate_equipment(
        self,
        *,
        equipment: Any,
        planned_start_time: datetime,
        planned_end_time: datetime,
        terminal: str | None = None,
        exclude_order_id: str | None = None,
    ) -> ResourceAvailability:
        if not getattr(equipment, "is_active", True):
            return self._unavailable(
                resource_type="equipment",
                resource_id=str(getattr(equipment, "id", "") or ""),
                schedule_source=ScheduleSource.CURRENT_STATUS_FALLBACK,
                reason="设备已停用",
            )
        status = getattr(equipment, "status", None)
        status_value = getattr(status, "value", status)
        if str(status_value or "") != EquipmentStatus.AVAILABLE.value:
            return self._unavailable(
                resource_type="equipment",
                resource_id=str(equipment.id),
                schedule_source=ScheduleSource.CURRENT_STATUS_FALLBACK,
                reason="设备当前不可用",
            )
        if terminal and getattr(equipment, "terminal", None) and str(equipment.terminal) != str(terminal):
            return self._unavailable(
                resource_type="equipment",
                resource_id=str(equipment.id),
                schedule_source=ScheduleSource.CURRENT_STATUS_FALLBACK,
                reason="设备不在目标航站楼",
            )

        downtimes = await self._find_equipment_downtimes(
            equipment_ids=[str(equipment.id)],
            planned_start_time=planned_start_time,
            planned_end_time=planned_end_time,
        )
        if downtimes:
            return self._unavailable(
                resource_type="equipment",
                resource_id=str(equipment.id),
                schedule_source=ScheduleSource.CURRENT_STATUS_FALLBACK,
                reason="设备停机窗口与任务时间冲突",
                metadata={"downtime_ids": [item.id for item in downtimes]},
            )

        if self._order_repo is not None and hasattr(self._order_repo, "find_equipment_conflicts"):
            conflicts = await self._order_repo.find_equipment_conflicts(
                equipment_ids=[str(equipment.id)],
                window_start=planned_start_time,
                window_end=planned_end_time,
                exclude_order_id=exclude_order_id,
            )
            if conflicts:
                return self._unavailable(
                    resource_type="equipment",
                    resource_id=str(equipment.id),
                    schedule_source=ScheduleSource.CURRENT_STATUS_FALLBACK,
                    reason="设备在目标时间窗已有占用",
                    metadata={"conflicting_order_ids": [item.get("dispatch_order_id") for item in conflicts]},
                )

        return ResourceAvailability(
            resource_type="equipment",
            resource_id=str(equipment.id),
            available=True,
            schedule_source=ScheduleSource.CURRENT_STATUS_FALLBACK,
            reason="设备在目标时间窗可用",
            score_breakdown={"availability": 100.0},
        )

    async def evaluate_employee(
        self,
        *,
        employee: Any,
        planned_start_time: datetime,
        planned_end_time: datetime,
        terminal: str | None = None,
        exclude_order_id: str | None = None,
        ignore_dispatch_overlaps: bool = False,
    ) -> ResourceAvailability:
        user_id = str(getattr(employee, "id", "") or getattr(employee, "user_id", "") or "").strip()
        if not user_id:
            return self._unavailable(
                resource_type="employee",
                resource_id="",
                schedule_source=ScheduleSource.CURRENT_STATUS_FALLBACK,
                reason="人员标识缺失",
            )

        reasons: list[str] = []
        metadata: dict[str, Any] = {}

        instances = await self._find_shift_instances(
            resource_type="employee",
            resource_id=user_id,
            planned_start_time=planned_start_time,
            planned_end_time=planned_end_time,
        )
        has_shift_instance = bool(instances)
        schedule_source = (
            ScheduleSource.SHIFT_INSTANCE if has_shift_instance else ScheduleSource.CURRENT_STATUS_FALLBACK
        )

        source_team = None
        min_rest_minutes = self.DEFAULT_MIN_REST_MINUTES
        if has_shift_instance:
            active_instance = self._pick_covering_instance(instances, planned_start_time, planned_end_time)
            if active_instance is None:
                return self._unavailable(
                    resource_type="employee",
                    resource_id=user_id,
                    schedule_source=schedule_source,
                    reason="目标时间窗没有个人排班实例覆盖",
                )
            metadata["shift_instance_id"] = str(getattr(active_instance, "id", "") or "")
            min_rest_minutes = int(getattr(active_instance, "min_rest_minutes", None) or self.DEFAULT_MIN_REST_MINUTES)
        else:
            source_team = await self._resolve_employee_fallback_team(user_id=user_id, terminal=terminal)
            if source_team is None:
                return self._unavailable(
                    resource_type="employee",
                    resource_id=user_id,
                    schedule_source=schedule_source,
                    reason="人员当前不在岗，且无个人排班实例兜底",
                )
            reasons.append("无个人排班实例，已回退到换班归属班组 on_duty 判定")
            metadata["fallback"] = True
            metadata["source_team_id"] = str(getattr(source_team, "id", "") or "")
            metadata["source_team_name"] = getattr(source_team, "name", None)

        leave_records = await self._find_leave_records(
            user_ids=[user_id],
            team_id=str(getattr(source_team, "id", "") or "") if source_team else "",
            planned_start_time=planned_start_time,
            planned_end_time=planned_end_time,
        )
        if leave_records:
            return self._unavailable(
                resource_type="employee",
                resource_id=user_id,
                schedule_source=schedule_source,
                reason="人员存在请休假冲突",
                metadata={"leave_user_ids": [item.user_id for item in leave_records]},
            )

        rest_violation = await self._check_member_rest(
            user_ids=[user_id],
            planned_start_time=planned_start_time,
            min_rest_minutes=min_rest_minutes,
        )
        if rest_violation:
            return self._unavailable(
                resource_type="employee",
                resource_id=user_id,
                schedule_source=schedule_source,
                reason="人员未满足最小休息时间",
                metadata=rest_violation,
            )

        if not ignore_dispatch_overlaps:
            overlaps = await self._find_employee_overlaps(
                individual_user_id=user_id,
                planned_start_time=planned_start_time,
                planned_end_time=planned_end_time,
                exclude_order_id=exclude_order_id,
            )
            if overlaps:
                return self._unavailable(
                    resource_type="employee",
                    resource_id=user_id,
                    schedule_source=schedule_source,
                    reason="人员在目标时间窗已有派工占用",
                    metadata={"overlapping_order_ids": [str(getattr(item, "id", "") or "") for item in overlaps]},
                )

        reason = reasons[0] if reasons else "人员在目标时间窗可用"
        return ResourceAvailability(
            resource_type="employee",
            resource_id=user_id,
            available=True,
            schedule_source=schedule_source,
            reason=reason,
            reasons=reasons,
            score_breakdown={
                "availability": 100.0,
                "fallback_penalty": -8.0 if not has_shift_instance else 0.0,
            },
            metadata=metadata,
        )

    async def list_team_availability(
        self,
        *,
        teams: list[Any],
        planned_start_time: datetime,
        planned_end_time: datetime,
        terminal: str | None = None,
    ) -> list[ResourceAvailability]:
        results: list[ResourceAvailability] = []
        for team in teams:
            results.append(
                await self.evaluate_team(
                    team=team,
                    planned_start_time=planned_start_time,
                    planned_end_time=planned_end_time,
                    terminal=terminal,
                )
            )
        return results

    async def list_employee_availability(
        self,
        *,
        employees: list[Any],
        planned_start_time: datetime,
        planned_end_time: datetime,
        terminal: str | None = None,
        exclude_order_id: str | None = None,
        ignore_dispatch_overlaps: bool = False,
    ) -> list[ResourceAvailability]:
        results: list[ResourceAvailability] = []
        for employee in employees:
            results.append(
                await self.evaluate_employee(
                    employee=employee,
                    planned_start_time=planned_start_time,
                    planned_end_time=planned_end_time,
                    terminal=terminal,
                    exclude_order_id=exclude_order_id,
                    ignore_dispatch_overlaps=ignore_dispatch_overlaps,
                )
            )
        return results

    def _unavailable(
        self,
        *,
        resource_type: str,
        resource_id: str,
        schedule_source: ScheduleSource,
        reason: str,
        metadata: dict[str, Any] | None = None,
        lock_level: DispatchLockLevel = DispatchLockLevel.OPTIMIZABLE,
    ) -> ResourceAvailability:
        return ResourceAvailability(
            resource_type=resource_type,
            resource_id=resource_id,
            available=False,
            schedule_source=schedule_source,
            reason=reason,
            reasons=[reason],
            lock_level=lock_level,
            metadata=metadata or {},
        )

    @staticmethod
    def _lock_rank(lock_level: DispatchLockLevel) -> int:
        order = {
            DispatchLockLevel.OPTIMIZABLE: 1,
            DispatchLockLevel.ACTIVE: 2,
            DispatchLockLevel.FROZEN: 3,
            DispatchLockLevel.MANUAL_LOCK: 4,
        }
        return order.get(lock_level, 0)

    @staticmethod
    def _pick_covering_instance(
        instances: list[Any], planned_start_time: datetime, planned_end_time: datetime
    ) -> Any | None:
        for instance in instances:
            if (
                getattr(instance, "start_time", None) <= planned_start_time
                and getattr(instance, "end_time", None) >= planned_end_time
            ):
                return instance
        return instances[0] if instances else None

    async def _find_shift_instances(
        self,
        *,
        resource_type: str,
        resource_id: str,
        planned_start_time: datetime,
        planned_end_time: datetime,
    ) -> list[Any]:
        if self._shift_instance_repo is None:
            return []
        finder = getattr(self._shift_instance_repo, "find_for_resource_window", None)
        if not callable(finder):
            return []
        return await finder(
            resource_type=resource_type,
            resource_id=resource_id,
            window_start=planned_start_time,
            window_end=planned_end_time,
        )

    async def _load_member_user_ids(self, team_id: str) -> list[str]:
        if self._team_member_repo is None:
            return []
        finder = getattr(self._team_member_repo, "find_by_team", None)
        if not callable(finder):
            return []
        try:
            members = await finder(team_id, include_inactive=False)
        except TypeError:
            members = await finder(team_id)
        return [
            str(getattr(item, "user_id", "") or "").strip()
            for item in (members or [])
            if str(getattr(item, "user_id", "") or "").strip()
        ]

    async def _find_leave_records(
        self,
        *,
        user_ids: list[str],
        team_id: str,
        planned_start_time: datetime,
        planned_end_time: datetime,
    ) -> list[Any]:
        if self._schedule_exception_repo is None:
            return []
        finder = getattr(self._schedule_exception_repo, "find_leave_records", None)
        if not callable(finder):
            return []
        return await finder(
            user_ids=user_ids,
            team_id=team_id,
            window_start=planned_start_time,
            window_end=planned_end_time,
        )

    async def _find_equipment_downtimes(
        self,
        *,
        equipment_ids: list[str],
        planned_start_time: datetime,
        planned_end_time: datetime,
    ) -> list[Any]:
        if self._schedule_exception_repo is None:
            return []
        finder = getattr(self._schedule_exception_repo, "find_equipment_downtimes", None)
        if not callable(finder):
            return []
        return await finder(
            equipment_ids=equipment_ids,
            window_start=planned_start_time,
            window_end=planned_end_time,
        )

    async def _find_lock_rules(
        self,
        *,
        team_id: str,
        planned_start_time: datetime,
        planned_end_time: datetime,
    ) -> list[Any]:
        if self._schedule_exception_repo is None:
            return []
        finder = getattr(self._schedule_exception_repo, "find_lock_rules", None)
        if not callable(finder):
            return []
        return await finder(
            team_id=team_id,
            window_start=planned_start_time,
            window_end=planned_end_time,
        )

    async def _find_team_overlaps(
        self,
        *,
        team_id: str,
        planned_start_time: datetime,
        planned_end_time: datetime,
        exclude_order_id: str | None,
    ) -> list[Any]:
        if self._order_repo is None:
            return []
        finder = getattr(self._order_repo, "find_overlapping_orders", None)
        if not callable(finder):
            return []
        return await finder(
            window_start=planned_start_time,
            window_end=planned_end_time,
            team_id=team_id,
            exclude_order_id=exclude_order_id,
        )

    async def _check_member_rest(
        self,
        *,
        user_ids: list[str],
        planned_start_time: datetime,
        min_rest_minutes: int,
    ) -> dict[str, Any] | None:
        if self._order_member_repo is None:
            return None
        finder = getattr(self._order_member_repo, "find_latest_checkout_for_user", None)
        if not callable(finder):
            return None
        minimum_gap = max(0, int(min_rest_minutes))
        for user_id in user_ids:
            latest = await finder(user_id=user_id, before=planned_start_time, max_gap_hours=24.0)
            if not latest:
                continue
            checkout_time = latest.get("check_out_time")
            if checkout_time is None:
                continue
            gap_minutes = (planned_start_time - checkout_time).total_seconds() / 60.0
            if gap_minutes < minimum_gap:
                return {
                    "user_id": user_id,
                    "latest_checkout_order_id": latest.get("dispatch_order_id"),
                    "gap_minutes": round(max(0.0, gap_minutes), 2),
                    "required_rest_minutes": minimum_gap,
                }
        return None

    async def _find_employee_overlaps(
        self,
        *,
        individual_user_id: str,
        planned_start_time: datetime,
        planned_end_time: datetime,
        exclude_order_id: str | None,
    ) -> list[Any]:
        if self._order_repo is None:
            return []
        finder = getattr(self._order_repo, "find_overlapping_orders", None)
        if not callable(finder):
            return []
        return await finder(
            window_start=planned_start_time,
            window_end=planned_end_time,
            individual_user_id=individual_user_id,
            exclude_order_id=exclude_order_id,
        )

    async def _resolve_employee_fallback_team(
        self,
        *,
        user_id: str,
        terminal: str | None,
    ) -> Any | None:
        memberships = await self._load_user_team_memberships(user_id)
        if not memberships or self._team_repo is None:
            return None
        for membership in memberships:
            team_id = str(getattr(membership, "team_id", "") or "").strip()
            if not team_id:
                continue
            team = await self._team_repo.find_by_id(team_id)
            if team is None or not getattr(team, "is_active", True):
                continue
            if terminal and getattr(team, "terminal", None) and str(team.terminal) != str(terminal):
                continue
            current_status = getattr(team, "current_status", None)
            current_value = getattr(current_status, "value", current_status)
            if str(current_value or "") == TeamStatus.ON_DUTY.value:
                return team
        return None

    async def _load_user_team_memberships(self, user_id: str) -> list[Any]:
        if self._team_member_repo is None:
            return []
        finder = getattr(self._team_member_repo, "find_by_user", None)
        if not callable(finder):
            return []
        return await finder(user_id)
