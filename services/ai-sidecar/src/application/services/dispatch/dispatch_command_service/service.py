"""Dispatch command application service implementation.

Encapsulates write-side dispatch order operations so routes no longer call
repositories directly.
"""

from __future__ import annotations

import asyncio
from contextlib import asynccontextmanager
from datetime import datetime, timedelta
from enum import Enum
from typing import Any

from src.application.services.anomaly.alert_service import AlertChannel, AlertLevel
from src.application.services.async_todo_service import AssignTodoCommand, CreateTodoCommand
from src.application.services.async_unit_of_work import AsyncUnitOfWork
from src.application.services.dispatch.dispatch_shared import DispatchCalculator
from src.domain.exceptions.base import BusinessRuleException, EntityNotFoundException, SystemException
from src.domain.models.anomaly import Anomaly, AnomalySeverity, AnomalyType
from src.domain.models.dispatch import (
    AssigneeType,
    DispatchOrder,
    DispatchOrderMember,
    DispatchOrderStatus,
    DispatchPublicationState,
    DispatchSourceType,
    DispatchType,
    LegScope,
    MemberRole,
)
from src.domain.utils.time_utils import utc_now
from src.infrastructure.common.exceptions import POSTGRES_EXCEPTIONS
from src.infrastructure.logging.core import get_logger
from src.shared.id_generator import generate_id

from .models import ACTION_LOG_MAP, CHECKIN_DISTANCE_THRESHOLD_METERS

logger = get_logger(__name__)


class DispatchCommandApplicationService:
    """Write-side service for dispatch order lifecycle commands."""

    CHECKIN_DISTANCE_THRESHOLD_METERS = CHECKIN_DISTANCE_THRESHOLD_METERS
    ACTION_LOG_MAP = ACTION_LOG_MAP

    def __init__(
        self,
        *,
        order_repo: Any,
        member_repo: Any,
        team_member_repo: Any,
        stand_repo: Any | None = None,
        team_repo: Any | None = None,
        anomaly_repo: Any | None = None,
        db_pool: Any | None = None,
        collaboration_recorder: Any | None = None,
        notification_port: Any | None = None,
        alert_service: Any | None = None,
        sse_hub: Any | None = None,
        checklist_service: Any | None = None,
        dispatch_chat_service: Any | None = None,
        dispatch_rule_service: Any | None = None,
        temporary_task_template_repo: Any | None = None,
        dispatch_service: Any | None = None,
        todo_service: Any | None = None,
        metrics_service: Any | None = None,
    ) -> None:
        self._order_repo = order_repo
        self._member_repo = member_repo
        self._team_member_repo = team_member_repo
        self._stand_repo = stand_repo
        self._team_repo = team_repo
        self._anomaly_repo = anomaly_repo
        self._db_pool = db_pool
        self._collaboration_recorder = collaboration_recorder
        self._notification_port = notification_port
        self._alert_service = alert_service
        self._sse_hub = sse_hub
        self._checklist_service = checklist_service
        self._dispatch_chat_service = dispatch_chat_service
        self._dispatch_rule_service = dispatch_rule_service
        self._temporary_task_template_repo = temporary_task_template_repo
        self._dispatch_service = dispatch_service
        self._todo_service = todo_service
        self._metrics_service = metrics_service
        self._dispatch_conflict_service = None
        self._travel_stats_repo = None

    def set_checklist_service(self, checklist_service: Any | None) -> None:
        self._checklist_service = checklist_service

    def set_dispatch_chat_service(self, dispatch_chat_service: Any | None) -> None:
        self._dispatch_chat_service = dispatch_chat_service

    def set_dispatch_rule_service(self, dispatch_rule_service: Any | None) -> None:
        self._dispatch_rule_service = dispatch_rule_service

    def set_dispatch_service(self, dispatch_service: Any | None) -> None:
        self._dispatch_service = dispatch_service

    def set_todo_service(self, todo_service: Any | None) -> None:
        self._todo_service = todo_service

    def set_metrics_service(self, metrics_service: Any | None) -> None:
        self._metrics_service = metrics_service

    async def _refresh_dispatch_chat_deprecation(self, flight_id: str) -> None:
        service = self._dispatch_chat_service
        normalized_flight_id = str(flight_id or "").strip()
        if service is None or not normalized_flight_id:
            return

        refresh = getattr(service, "refresh_group_deprecation_for_flight", None)
        if not callable(refresh):
            return

        try:
            await refresh(flight_id=normalized_flight_id)
        except TypeError:
            await refresh(normalized_flight_id)
        except Exception as exc:  # noqa: BLE001 - best-effort side effect must not abort main flow
            logger.warning(
                "dispatch chat deprecation refresh failed flight_id=%s: %s",
                normalized_flight_id,
                exc,
            )

    def set_checkin_support(self, *, stand_repo: Any | None) -> None:
        self._stand_repo = stand_repo

    def set_issue_reporting_support(
        self,
        *,
        anomaly_repo: Any | None,
        team_repo: Any | None,
        notification_port: Any | None,
        alert_service: Any | None,
        sse_hub: Any | None,
    ) -> None:
        self._anomaly_repo = anomaly_repo
        self._team_repo = team_repo
        self._notification_port = notification_port
        self._alert_service = alert_service
        self._sse_hub = sse_hub

    def set_dispatch_conflict_service(self, dispatch_conflict_service: Any | None) -> None:
        self._dispatch_conflict_service = dispatch_conflict_service

    def set_travel_stats_repo(self, travel_stats_repo: Any | None) -> None:
        self._travel_stats_repo = travel_stats_repo

    def _increment_metric(self, key: str, value: int = 1) -> None:
        metrics_service = self._metrics_service
        if metrics_service is None:
            return
        increment_counter = getattr(metrics_service, "increment_counter", None)
        if callable(increment_counter):
            try:
                increment_counter(key, value)
            except Exception as exc:  # pragma: no cover - best effort side effect  # noqa: BLE001 - best-effort side effect must not abort main flow
                logger.warning("dispatch metric increment failed key=%s: %s", key, exc)

    async def _resolve_followup_owner(self, order: DispatchOrder) -> str | None:
        normalized_team_id = str(getattr(order, "team_id", "") or "").strip()
        team_repo = self._team_repo
        if normalized_team_id and team_repo is not None:
            try:
                team = await team_repo.find_by_id(normalized_team_id)
            except TypeError:
                team = await team_repo.find_by_id(normalized_team_id, load_members=True)
            if team is not None:
                leader_id = str(getattr(team, "leader_id", "") or "").strip()
                if leader_id:
                    return leader_id

        if normalized_team_id and self._team_member_repo is not None:
            try:
                members = await self._team_member_repo.find_by_team(normalized_team_id)
            except POSTGRES_EXCEPTIONS as exc:
                logger.warning("team_member_lookup_failed team_id=%s", normalized_team_id, exc_info=exc)
                members = []
            for member in members or []:
                if getattr(member, "is_active", True) is False:
                    continue
                role = (
                    str(getattr(getattr(member, "role", None), "value", getattr(member, "role", "")) or "")
                    .strip()
                    .lower()
                )
                if role == MemberRole.LEADER.value:
                    user_id = str(getattr(member, "user_id", "") or "").strip()
                    if user_id:
                        return user_id

        for member in list(getattr(order, "members", []) or []):
            if getattr(member, "is_active", True) is False:
                continue
            role = (
                str(getattr(getattr(member, "role", None), "value", getattr(member, "role", "")) or "").strip().lower()
            )
            if role == MemberRole.LEADER.value:
                user_id = str(getattr(member, "user_id", "") or "").strip()
                if user_id:
                    return user_id

        fallback_user_id = str(getattr(order, "dispatched_by", "") or "").strip()
        return fallback_user_id or None

    async def _find_open_followup_todo(
        self,
        *,
        source_type: str,
        source_id: str,
    ) -> Any | None:
        todo_service = self._todo_service
        if todo_service is None:
            return None

        list_by_source = getattr(todo_service, "list_todos_by_source", None)
        if not callable(list_by_source):
            return None

        try:
            items = await list_by_source(source_type=source_type, source_id=source_id, limit=20)
        except Exception as exc:  # pragma: no cover - best effort side effect  # noqa: BLE001 - best-effort side effect must not abort main flow
            logger.warning(
                "dispatch followup todo lookup failed source_type=%s source_id=%s: %s",
                source_type,
                source_id,
                exc,
            )
            return None

        for aggregate in items or []:
            todo = aggregate.get_todo()
            status = str(getattr(getattr(todo, "status", None), "value", getattr(todo, "status", "")) or "").strip()
            if status in {"已完成", "已取消"}:
                continue
            return aggregate
        return None

    async def _ensure_followup_todo(
        self,
        *,
        order: DispatchOrder,
        actor_id: str,
        source_type: str,
        title: str,
        description: str,
        priority: str,
        due_date: datetime | None,
        tags: list[str],
    ) -> dict[str, Any] | None:
        todo_service = self._todo_service
        if todo_service is None:
            return None

        owner_user_id = await self._resolve_followup_owner(order)
        existing = await self._find_open_followup_todo(
            source_type=source_type,
            source_id=str(order.id),
        )
        created = False
        if existing is None:
            try:
                todo_id = await todo_service.create_todo(
                    CreateTodoCommand(
                        title=title,
                        description=description,
                        priority=priority,
                        category="工作",
                        due_date=due_date,
                        estimated_duration=10,
                        tags=tags,
                        source_type=source_type,
                        source_id=str(order.id),
                        created_by=actor_id or "system",
                    )
                )
            except Exception as exc:  # pragma: no cover - best effort side effect  # noqa: BLE001 - best-effort side effect must not abort main flow
                logger.warning(
                    "dispatch followup todo creation failed order_id=%s source_type=%s: %s",
                    order.id,
                    source_type,
                    exc,
                )
                return None
            aggregate = await todo_service.get_todo(todo_id)
            if aggregate is None:
                return None
            existing = aggregate
            created = True

        todo = existing.get_todo()
        current_assignee = str(getattr(todo, "assigned_to", "") or "").strip() or None
        if owner_user_id and current_assignee != owner_user_id:
            try:
                await todo_service.assign_todo(
                    AssignTodoCommand(
                        todo_id=todo.todo_id,
                        assignee=owner_user_id,
                        assigned_by=actor_id or "system",
                    )
                )
                current_assignee = owner_user_id
            except Exception as exc:  # pragma: no cover - best effort side effect  # noqa: BLE001 - best-effort side effect must not abort main flow
                logger.warning(
                    "dispatch followup todo assign failed order_id=%s todo_id=%s assignee=%s: %s",
                    order.id,
                    todo.todo_id.value,
                    owner_user_id,
                    exc,
                )

        return {
            "todo_id": todo.todo_id.value,
            "created": created,
            "assigned_to": current_assignee,
        }

    @asynccontextmanager
    async def _write_scope(self):
        if AsyncUnitOfWork.get_current_connection() is not None or self._db_pool is None:
            yield AsyncUnitOfWork.get_current_connection()
            return

        async with AsyncUnitOfWork(self._db_pool):
            yield AsyncUnitOfWork.get_current_connection()

    async def _append_order_log_with_event(
        self,
        *,
        flight_id: str,
        dispatch_order_id: str,
        action: str,
        event_type: str,
        actor_id: str | None,
        details: dict[str, Any] | None,
        correlation_id: str | None,
        conn,
    ) -> None:
        log_id = generate_id()
        event_id = generate_id()
        if self._collaboration_recorder is not None:
            await self._collaboration_recorder.record_event(
                flight_id=flight_id,
                dispatch_order_id=dispatch_order_id,
                event_type=event_type,
                actor_user_id=actor_id,
                correlation_id=correlation_id,
                payload=dict(details or {}),
                source_table="dispatch_order_logs",
                source_record_id=log_id,
                event_id=event_id,
                conn=conn,
            )
        try:
            await self._order_repo.append_log(
                dispatch_order_id=dispatch_order_id,
                action=action,
                actor_id=actor_id,
                details=details,
                conn=conn,
                log_id=log_id,
                event_id=event_id,
            )
        except TypeError:
            await self._order_repo.append_log(
                dispatch_order_id=dispatch_order_id,
                action=action,
                actor_id=actor_id,
                details=details,
            )

    @staticmethod
    def _payload_value(payload: dict[str, Any], key: str, default: Any = None) -> Any:
        value = payload.get(key, default)
        return default if value is None else value

    @staticmethod
    def _validate_qr_code(order_id: str, qr_code: str | None) -> bool:
        if not qr_code:
            return True
        normalized = str(qr_code).strip()
        return normalized in {
            order_id,
            f"dispatch:{order_id}",
            f"dispatch_order:{order_id}",
        }

    @staticmethod
    def _extract_action_time(raw: Any) -> datetime:
        if isinstance(raw, datetime):
            return raw
        if isinstance(raw, str):
            text = raw.strip()
            if text:
                if text.endswith("Z"):
                    text = text[:-1] + "+00:00"
                try:
                    return datetime.fromisoformat(text)
                except ValueError:
                    pass
        return utc_now()

    @staticmethod
    def _normalize_assignee_type(raw: Any) -> AssigneeType:
        candidate = raw
        if isinstance(candidate, AssigneeType):
            return candidate
        if isinstance(candidate, Enum):
            candidate = candidate.value

        text = str(candidate or "").strip().lower()
        if "." in text:
            text = text.rsplit(".", maxsplit=1)[-1]

        return AssigneeType(text or AssigneeType.TEAM.value)

    @staticmethod
    def _ensure_order_execution_published(order: DispatchOrder, *, action_label: str) -> None:
        publication_state = getattr(
            getattr(order, "publication_state", None), "value", getattr(order, "publication_state", None)
        )
        if str(publication_state or "").strip() == DispatchPublicationState.PREPUBLISHED.value:
            raise BusinessRuleException(message=f"预发布工单尚未正式发布，不能{action_label}")

    @staticmethod
    def _serialize_crew_requirement_snapshot(requirement_version: Any) -> list[dict[str, Any]]:
        return [
            {
                "slot_code": item.slot_code,
                "qualification_code": item.qualification_code,
                "min_level_code": item.min_level_code,
                "required_count": item.required_count,
                "must_be_distinct": item.must_be_distinct,
                "exclusive_group": item.exclusive_group,
                "remarks": item.remarks,
            }
            for item in (getattr(requirement_version, "requirements", None) or [])
        ]

    @staticmethod
    def _serialize_equipment_requirement_snapshot(requirement_version: Any) -> list[dict[str, Any]]:
        return [
            {
                "slot_code": item.slot_code,
                "equipment_type_id": item.equipment_type_id,
                "equipment_type_code": item.equipment_type_code,
                "required_count": item.required_count,
                "must_be_distinct": item.must_be_distinct,
                "requires_driver": item.requires_driver,
                "driver_qualification_code": item.driver_qualification_code,
                "driver_min_level_code": item.driver_min_level_code,
                "remarks": item.remarks,
            }
            for item in (getattr(requirement_version, "equipment_requirements", None) or [])
        ]

    @staticmethod
    def _serialize_template_crew_requirement_snapshot(template: Any) -> list[dict[str, Any]]:
        return [
            {
                "slot_code": item.slot_code,
                "qualification_code": item.qualification_code,
                "min_level_code": item.min_level_code,
                "required_count": item.required_count,
                "must_be_distinct": item.must_be_distinct,
                "exclusive_group": item.exclusive_group,
                "remarks": item.remarks,
            }
            for item in (getattr(template, "crew_requirements", None) or [])
        ]

    @staticmethod
    def _serialize_template_equipment_requirement_snapshot(template: Any) -> list[dict[str, Any]]:
        return [
            {
                "slot_code": item.slot_code,
                "equipment_type_id": item.equipment_type_id,
                "equipment_type_code": item.equipment_type_code,
                "required_count": item.required_count,
                "must_be_distinct": item.must_be_distinct,
                "requires_driver": item.requires_driver,
                "driver_qualification_code": item.driver_qualification_code,
                "driver_min_level_code": item.driver_min_level_code,
                "remarks": item.remarks,
            }
            for item in (getattr(template, "equipment_requirements", None) or [])
        ]

    async def _resolve_task_definition(
        self,
        payload: dict[str, Any],
    ) -> tuple[str, str, list[dict[str, Any]], list[dict[str, Any]], str | None, str | None]:
        department_id = str(payload.get("department_id") or "").strip()
        task_type = str(payload.get("task_type") or "").strip()
        template_code = str(payload.get("temporary_task_template_code") or "").strip()
        crew_requirement_snapshot = list(payload.get("crew_requirement_snapshot") or [])
        equipment_requirement_snapshot = list(payload.get("equipment_requirement_snapshot") or [])
        department_rule_version = payload.get("department_rule_version")
        resolved_template_code: str | None = None

        if template_code:
            if not department_id:
                raise BusinessRuleException(message="临时任务模板创建必须指定 department_id")
            if self._temporary_task_template_repo is None:
                raise SystemException(message="临时任务模板仓储未配置")
            template = await self._temporary_task_template_repo.find_by_code(department_id, template_code)
            if template is None or not bool(getattr(template, "is_active", True)):
                raise BusinessRuleException(message=f"临时任务模板 {template_code} 不存在或未启用")
            resolved_template_code = template_code
            if not task_type:
                task_type = str(getattr(template, "task_type", "") or "").strip()
            if not crew_requirement_snapshot:
                crew_requirement_snapshot = self._serialize_template_crew_requirement_snapshot(template)
            if not equipment_requirement_snapshot:
                equipment_requirement_snapshot = self._serialize_template_equipment_requirement_snapshot(template)
            if not department_rule_version:
                department_rule_version = f"temporary-template:{getattr(template, 'id', template_code)}"
            if not crew_requirement_snapshot:
                raise BusinessRuleException(message=f"临时任务模板 {template_code} 缺少人员资质要求")
            if not equipment_requirement_snapshot:
                raise BusinessRuleException(message=f"临时任务模板 {template_code} 缺少设备类型要求")

        if crew_requirement_snapshot and equipment_requirement_snapshot and task_type:
            return (
                department_id,
                task_type,
                crew_requirement_snapshot,
                equipment_requirement_snapshot,
                department_rule_version,
                resolved_template_code,
            )

        if not department_id or not task_type:
            raise BusinessRuleException(message="必须指定 task_type 或 temporary_task_template_code")
        if self._dispatch_rule_service is None:
            raise SystemException(message="派工规则服务未配置，无法按作业类型自动带出人员与设备需求")

        requirement_version = await self._dispatch_rule_service.get_published_requirement(department_id, task_type)
        if requirement_version is None:
            raise BusinessRuleException(message=f"作业类型 {task_type} 缺少已发布作业类型规则")

        if not crew_requirement_snapshot:
            crew_requirement_snapshot = self._serialize_crew_requirement_snapshot(requirement_version)
            if not crew_requirement_snapshot:
                raise BusinessRuleException(message=f"作业类型 {task_type} 缺少人员资质要求")
        if not equipment_requirement_snapshot:
            equipment_requirement_snapshot = self._serialize_equipment_requirement_snapshot(requirement_version)
            if not equipment_requirement_snapshot:
                raise BusinessRuleException(message=f"作业类型 {task_type} 缺少设备类型要求")
        if not department_rule_version:
            department_rule_version = getattr(requirement_version, "id", None)
        return (
            department_id,
            task_type,
            crew_requirement_snapshot,
            equipment_requirement_snapshot,
            department_rule_version,
            resolved_template_code,
        )

    async def create_order(self, payload: dict[str, Any], *, actor_id: str) -> DispatchOrder:
        team_id = payload.get("team_id")
        individual_user_id = payload.get("individual_user_id")
        assignee_type_raw = payload.get("assignee_type")
        normalized_assignee_raw = assignee_type_raw
        if normalized_assignee_raw is None and individual_user_id:
            normalized_assignee_raw = AssigneeType.INDIVIDUAL.value
        elif normalized_assignee_raw is None:
            normalized_assignee_raw = AssigneeType.TEAM.value
        try:
            assignee_type = self._normalize_assignee_type(normalized_assignee_raw)
        except ValueError as exc:
            raise BusinessRuleException(message=f"无效派工类型: {normalized_assignee_raw}") from exc

        has_explicit_assignee = bool(team_id or individual_user_id)
        if has_explicit_assignee:
            if assignee_type == AssigneeType.TEAM and not team_id:
                raise BusinessRuleException(message="班组派工必须指定 team_id")
            if assignee_type == AssigneeType.INDIVIDUAL and not individual_user_id:
                raise BusinessRuleException(message="个人派工必须指定 individual_user_id")

        (
            department_id,
            task_type,
            crew_requirement_snapshot,
            equipment_requirement_snapshot,
            department_rule_version,
            resolved_template_code,
        ) = await self._resolve_task_definition(payload)

        workflow_context = dict(payload.get("workflow_context") or {})
        if payload.get("location") is not None:
            workflow_context["manual_location"] = payload.get("location")
        if payload.get("remarks") is not None:
            workflow_context["manual_remarks"] = payload.get("remarks")
        if payload.get("priority") is not None:
            workflow_context["manual_priority"] = payload.get("priority")
        if resolved_template_code:
            workflow_context["temporary_task_template_code"] = resolved_template_code

        publication_state = DispatchPublicationState(
            str(payload.get("publication_state") or DispatchPublicationState.PUBLISHED.value)
        )
        has_inline_assignment = bool(payload.get("task_crew") or payload.get("equipment_assignment"))
        initial_status = (
            DispatchOrderStatus.ASSIGNED
            if (
                publication_state == DispatchPublicationState.PUBLISHED
                and (has_explicit_assignee or has_inline_assignment)
            )
            else DispatchOrderStatus.PENDING
        )

        order = DispatchOrder(
            id=generate_id(),
            flight_id=str(self._payload_value(payload, "flight_id", "")),
            task_type=task_type,
            department_id=department_id,
            stand_id=payload.get("stand_id"),
            assignee_type=assignee_type,
            team_id=team_id,
            individual_user_id=individual_user_id,
            planned_start_time=payload.get("planned_start_time"),
            planned_end_time=payload.get("planned_end_time"),
            status=initial_status,
            dispatch_type=DispatchType.MANUAL,
            dispatched_at=utc_now() if initial_status == DispatchOrderStatus.ASSIGNED else None,
            dispatched_by=(actor_id or None) if initial_status == DispatchOrderStatus.ASSIGNED else None,
            publication_state=publication_state,
            source_type=DispatchSourceType(str(payload.get("source_type") or DispatchSourceType.MANUAL.value)),
            leg_scope=LegScope(str(payload.get("leg_scope") or LegScope.NONE.value)),
            department_rule_version=department_rule_version,
            crew_requirement_snapshot=crew_requirement_snapshot,
            equipment_requirement_snapshot=equipment_requirement_snapshot,
            task_crew=dict(payload.get("task_crew") or {}),
            equipment_assignment=list(payload.get("equipment_assignment") or []),
            lock_level=("manual_lock" if bool(payload.get("manual_lock", False)) else "optimizable"),
            workflow_context=workflow_context,
        )
        correlation_id = generate_id()
        async with self._write_scope() as conn:
            try:
                saved = await self._order_repo.save(order, conn=conn)
            except TypeError:
                saved = await self._order_repo.save(order)

            if assignee_type == AssigneeType.TEAM and team_id:
                team_members = await self._team_member_repo.find_by_team(team_id)
                members = [
                    DispatchOrderMember(
                        id=generate_id(),
                        dispatch_order_id=saved.id,
                        user_id=member.user_id,
                        role=member.role,
                        source_type=AssigneeType.TEAM,
                        source_team_id=team_id,
                    )
                    for member in team_members
                ]
                if members:
                    await asyncio.gather(*(self._member_repo.save(item) for item in members))

            await self._append_order_log_with_event(
                flight_id=saved.flight_id,
                dispatch_order_id=saved.id,
                action="created",
                event_type="order_created",
                actor_id=actor_id,
                details={
                    "assignee_type": saved.assignee_type.value,
                    "team_id": saved.team_id,
                    "individual_user_id": saved.individual_user_id,
                    "temporary_task_template_code": resolved_template_code,
                },
                correlation_id=correlation_id,
                conn=conn,
            )

            should_prepare_for_optimization = (
                not has_explicit_assignee
                and not has_inline_assignment
                and not bool(payload.get("manual_lock", False))
                and publication_state == DispatchPublicationState.PUBLISHED
                and self._dispatch_service is not None
            )
            if should_prepare_for_optimization:
                preparation_result = await self._dispatch_service.prepare_order_for_publication(saved)
                saved.qualification_gap = list(preparation_result.get("qualification_gap") or [])
                saved.equipment_gap = list(preparation_result.get("equipment_gap") or [])
                if preparation_result.get("reason"):
                    saved.availability_reason = preparation_result.get("reason")
                try:
                    saved = await self._order_repo.save(saved, conn=conn)
                except TypeError:
                    saved = await self._order_repo.save(saved)

        dispatch_chat_service = self._dispatch_chat_service
        if dispatch_chat_service is not None:
            sync_group = getattr(dispatch_chat_service, "sync_group_for_dispatch_order_id", None)
            if callable(sync_group):
                try:
                    await sync_group(saved.id)
                except Exception as exc:  # pragma: no cover - best effort side effect  # noqa: BLE001 - best-effort side effect must not abort main flow
                    logger.warning(f"dispatch chat sync failed after order creation order_id={saved.id}: {exc}")

        return saved

    async def accept_order(self, order_id: str, payload: dict[str, Any], *, actor_id: str) -> dict[str, Any]:
        if not actor_id:
            raise SystemException(message="未登录")

        order = await self._order_repo.find_by_id(order_id)
        if not order:
            raise EntityNotFoundException(entity_type="资源", entity_id="派工单不存在")
        self._ensure_order_execution_published(order, action_label="接单")

        if order.status not in {DispatchOrderStatus.ASSIGNED, DispatchOrderStatus.IN_PROGRESS}:
            raise BusinessRuleException(message=f"当前状态不允许接单: {order.status.value}")
        if not order.can_be_started_by(actor_id):
            raise SystemException(message="无权接收此派工单")

        client_action_id = str(payload.get("client_action_id") or "").strip() or None
        note = payload.get("note")

        if client_action_id and await self._order_repo.has_logged_action(
            order_id,
            "accepted",
            client_action_id=client_action_id,
        ):
            return {
                "success": True,
                "status": "duplicate",
                "message": "重复接单请求已忽略",
            }

        if await self._order_repo.has_logged_action(order_id, "accepted", actor_id=actor_id):
            return {
                "success": True,
                "status": "accepted",
                "message": "已接单",
            }

        async with self._write_scope() as conn:
            await self._append_order_log_with_event(
                flight_id=order.flight_id,
                dispatch_order_id=order_id,
                action="accepted",
                event_type="order_accepted",
                actor_id=actor_id,
                details={
                    "note": note,
                    "client_action_id": client_action_id,
                    "accepted_at": utc_now().isoformat(),
                },
                correlation_id=client_action_id or generate_id(),
                conn=conn,
            )
        return {
            "success": True,
            "status": "accepted",
            "message": "接单成功",
        }

    async def start_order(self, order_id: str, payload: dict[str, Any], *, actor_id: str) -> dict[str, Any]:
        if not actor_id:
            raise SystemException(message="未登录")

        order = await self._order_repo.find_by_id(order_id)
        if not order:
            raise EntityNotFoundException(entity_type="资源", entity_id="派工单不存在")
        self._ensure_order_execution_published(order, action_label="开始执行")
        if order.status == DispatchOrderStatus.IN_PROGRESS:
            if not order.can_be_completed_by(actor_id):
                raise SystemException(message="无权操作此派工单")
            return {
                "message": "派工单已在执行中",
                "actual_start_time": order.actual_start_time.isoformat()
                if order.actual_start_time
                else utc_now().isoformat(),
                "compat_alias": True,
            }
        if order.status != DispatchOrderStatus.ASSIGNED:
            raise BusinessRuleException(message=f"派工单状态不正确，当前状态: {order.status.value}")
        if not order.can_be_started_by(actor_id):
            raise SystemException(message="无权操作此派工单")

        if hasattr(self._order_repo, "has_logged_action"):
            accepted = await self._order_repo.has_logged_action(order_id, "accepted", actor_id=actor_id)
            if not accepted:
                raise BusinessRuleException(message="请先接单再开始执行")

        member = await self._member_repo.find_by_order_and_user(order_id, actor_id)
        if member is None or member.check_in_time is None:
            raise BusinessRuleException(message="请先完成签到再开始执行")

        actual_start = payload.get("actual_start_time") or utc_now()
        await self._start_order_runtime(
            order=order,
            order_id=order_id,
            actor_id=actor_id,
            actual_start=actual_start,
            reason="manual_start",
        )

        return {
            "message": "派工单已开始执行",
            "actual_start_time": actual_start.isoformat(),
        }

    async def _start_order_runtime(
        self,
        *,
        order,
        order_id: str,
        actor_id: str,
        actual_start: datetime,
        reason: str,
    ) -> None:
        event_id = generate_id()
        log_id = generate_id()
        correlation_id = generate_id()
        async with self._write_scope() as conn:
            if self._collaboration_recorder is not None:
                await self._collaboration_recorder.record_event(
                    flight_id=order.flight_id,
                    dispatch_order_id=order_id,
                    event_type="order_started",
                    actor_user_id=actor_id,
                    correlation_id=correlation_id,
                    payload={"actual_start_time": actual_start.isoformat(), "reason": reason},
                    source_table="dispatch_order_logs",
                    source_record_id=log_id,
                    event_id=event_id,
                    conn=conn,
                )
            try:
                success = await self._order_repo.start_order(
                    order_id,
                    actual_start,
                    actor_id,
                    conn=conn,
                    event_id=event_id,
                    log_id=log_id,
                )
            except TypeError:
                success = await self._order_repo.start_order(order_id, actual_start, actor_id)
        if not success:
            raise SystemException(message="操作失败")

    async def complete_order(self, order_id: str, payload: dict[str, Any], *, actor_id: str) -> dict[str, Any]:
        if not actor_id:
            raise SystemException(message="未登录")

        order = await self._order_repo.find_by_id(order_id)
        if not order:
            raise EntityNotFoundException(entity_type="资源", entity_id="派工单不存在")
        self._ensure_order_execution_published(order, action_label="完工")
        if order.status != DispatchOrderStatus.IN_PROGRESS:
            raise BusinessRuleException(message=f"派工单状态不正确，当前状态: {order.status.value}")
        if not order.can_be_completed_by(actor_id):
            raise SystemException(message="无权操作此派工单")

        checklist_service = self._checklist_service
        gate: dict[str, Any] = {
            "enforced": False,
            "ready": True,
            "blocking_issues": [],
            "soft_missing_count": 0,
            "can_soft_complete": True,
            "required_total": 0,
            "completed_required": 0,
            "template_version": None,
        }
        if checklist_service is not None:
            gate = await checklist_service.evaluate_completion_gate(
                dispatch_order_id=order_id,
                task_type=order.task_type,
            )
            if gate.get("enforced") and not gate.get("can_soft_complete", gate.get("ready", True)):
                self._increment_metric("dispatch.order.complete.blocked")
                raise BusinessRuleException(
                    message="关键安全检查未完成，无法完工",
                ).with_details(
                    blocking_issues=gate.get("blocking_issues", []),
                    pending_required_items=gate.get("pending_required_items", []),
                    failed_required_items=gate.get("failed_required_items", []),
                    soft_missing_count=int(gate.get("soft_missing_count") or 0),
                    can_soft_complete=bool(gate.get("can_soft_complete", True)),
                    required_total=gate.get("required_total", 0),
                    completed_required=gate.get("completed_required", 0),
                    template_version=gate.get("template_version"),
                )

        actual_end = payload.get("actual_end_time") or utc_now()
        if order.actual_start_time is not None and actual_end < order.actual_start_time:
            raise BusinessRuleException(message="actual_end_time 不能早于 actual_start_time")
        completion_notes = payload.get("completion_notes")
        completion_mode = "soft_complete" if int(gate.get("soft_missing_count") or 0) > 0 else "hard_complete"
        followup_required = completion_mode == "soft_complete"
        event_id = generate_id()
        log_id = generate_id()
        async with self._write_scope() as conn:
            if self._collaboration_recorder is not None:
                await self._collaboration_recorder.record_event(
                    flight_id=order.flight_id,
                    dispatch_order_id=order_id,
                    event_type="order_completed",
                    actor_user_id=actor_id,
                    correlation_id=generate_id(),
                    payload={
                        "actual_end_time": actual_end.isoformat(),
                        "completion_notes": completion_notes,
                        "completion_mode": completion_mode,
                    },
                    source_table="dispatch_order_logs",
                    source_record_id=log_id,
                    event_id=event_id,
                    conn=conn,
                )
            try:
                success = await self._order_repo.complete_order(
                    order_id,
                    actual_end,
                    actor_id,
                    completion_notes,
                    conn=conn,
                    event_id=event_id,
                    log_id=log_id,
                )
            except TypeError:
                success = await self._order_repo.complete_order(order_id, actual_end, actor_id, completion_notes)
        if not success:
            raise SystemException(message="操作失败")

        await self._refresh_dispatch_chat_deprecation(order.flight_id)

        followup_todo: dict[str, Any] | None = None
        if followup_required:
            self._increment_metric("dispatch.order.complete.soft")
            followup_todo = await self._ensure_followup_todo(
                order=order,
                actor_id=actor_id,
                source_type="dispatch_soft_followup",
                title=f"补录安全检查 - {order.id}",
                description=(
                    f"派工单 {order.id} 已软闭环完工，请班组长补核常规安全项。"
                    f" 待补项: {', '.join(list(gate.get('pending_routine_items') or [])[:8]) or '无'};"
                    f" 失败项: {', '.join(list(gate.get('failed_routine_items') or [])[:8]) or '无'}。"
                ),
                priority="高",
                due_date=actual_end + timedelta(hours=4),
                tags=["dispatch", "team_lead_followup", "soft_completion"],
            )
            if hasattr(self._order_repo, "append_log"):
                async with self._write_scope() as conn:
                    await self._append_order_log_with_event(
                        flight_id=order.flight_id,
                        dispatch_order_id=order_id,
                        action="soft_completion_followup_created",
                        event_type="order_soft_followup_created",
                        actor_id=actor_id,
                        details={
                            "owner_role": "team_lead",
                            "soft_missing_count": int(gate.get("soft_missing_count") or 0),
                            "pending_routine_items": list(gate.get("pending_routine_items") or []),
                            "failed_routine_items": list(gate.get("failed_routine_items") or []),
                            "todo_id": (followup_todo or {}).get("todo_id"),
                            "assigned_to": (followup_todo or {}).get("assigned_to"),
                        },
                        correlation_id=payload.get("client_action_id") or generate_id(),
                        conn=conn,
                    )

        return {
            "message": "派工单已完成",
            "actual_end_time": actual_end.isoformat(),
            "completion_mode": completion_mode,
            "followup_required": followup_required,
            "followup_owner_role": "team_lead" if followup_required else None,
            "followup_todo_id": (followup_todo or {}).get("todo_id"),
        }

    async def report_estimated_completion(
        self, order_id: str, payload: dict[str, Any], *, actor_id: str
    ) -> dict[str, Any]:
        if not actor_id:
            raise SystemException(message="未登录")

        order = await self._order_repo.find_by_id(order_id)
        if not order:
            raise EntityNotFoundException(entity_type="资源", entity_id="派工单不存在")
        self._ensure_order_execution_published(order, action_label="回报预计完成时间")
        if order.status != DispatchOrderStatus.IN_PROGRESS:
            raise BusinessRuleException(message=f"仅作业中派工单可回报预计完成时间，当前状态: {order.status.value}")
        if not order.can_be_completed_by(actor_id):
            raise SystemException(message="无权回报此派工单预计完成时间")

        client_action_id = str(payload.get("client_action_id") or "").strip() or None
        if client_action_id and await self._order_repo.has_logged_action(
            order_id,
            "estimated_completion_reported",
            client_action_id=client_action_id,
        ):
            existing_time = payload.get("estimated_completion_time") or utc_now()
            serialized_time = existing_time.isoformat() if hasattr(existing_time, "isoformat") else str(existing_time)
            return {
                "dispatch_order_id": order_id,
                "estimated_completion_time": serialized_time,
                "estimated_completion_reported_at": utc_now().isoformat(),
                "estimated_completion_reported_by": actor_id,
                "note": payload.get("note"),
                "has_conflicts": False,
                "suggestions": [],
                "status": "duplicate",
                "message": "重复预计完成时间回报已忽略",
            }

        estimated_completion_time = payload.get("estimated_completion_time")
        if estimated_completion_time is None:
            raise BusinessRuleException(message="estimated_completion_time 必填")
        if order.actual_start_time is not None and estimated_completion_time < order.actual_start_time:
            raise BusinessRuleException(message="estimated_completion_time 不能早于 actual_start_time")

        note = str(payload.get("note") or "").strip() or None
        event_id = generate_id()
        log_id = generate_id()
        async with self._write_scope() as conn:
            await self._append_order_log_with_event(
                flight_id=order.flight_id,
                dispatch_order_id=order_id,
                action="estimated_completion_reported",
                event_type="dispatch_eta_reported",
                actor_id=actor_id,
                details={
                    "estimated_completion_time": estimated_completion_time.isoformat(),
                    "note": note,
                    "client_action_id": client_action_id,
                },
                correlation_id=client_action_id or generate_id(),
                conn=conn,
            )
            success = await self._order_repo.report_estimated_completion(
                order_id,
                estimated_completion_time,
                actor_id,
                note,
                conn=conn,
                event_id=event_id,
                log_id=log_id,
            )
        if not success:
            raise SystemException(message="预计完成时间回报失败")

        suggestions_payload: list[dict[str, Any]] = []
        conflict_service = self._dispatch_conflict_service
        if conflict_service is not None:
            try:
                horizon_start = min(filter(None, [order.actual_start_time, order.planned_start_time, utc_now()]))
                base_end = order.planned_end_time or estimated_completion_time
                horizon_end = max(estimated_completion_time, base_end)
                suggestions = await conflict_service.replan(
                    window_start=horizon_start,
                    window_end=horizon_end + (horizon_end - horizon_start),
                    strategy="balanced",
                    apply_changes=False,
                    max_suggestions=20,
                )
                suggestions_payload = [
                    {
                        "dispatch_order_id": item.dispatch_order_id,
                        "reason": item.reason,
                        "original_start_time": item.original_start_time,
                        "original_end_time": item.original_end_time,
                        "suggested_start_time": item.suggested_start_time,
                        "suggested_end_time": item.suggested_end_time,
                        "related_dispatch_order_id": item.related_dispatch_order_id,
                        "impact_score": item.impact_score,
                    }
                    for item in suggestions
                    if item.dispatch_order_id != order_id
                ]
            except (
                Exception  # noqa: BLE001
            ) as exc:  # pragma: no cover
                logger.warning(f"Failed to build ETA replan suggestions for order {order_id}: {exc}")

        recipient_ids = set()
        if order.dispatched_by and order.dispatched_by != actor_id:
            recipient_ids.add(order.dispatched_by)
        if order.individual_user_id and order.individual_user_id != actor_id:
            recipient_ids.add(order.individual_user_id)
        team_repo = self._team_repo
        if order.team_id and team_repo is not None:
            team = await team_repo.find_by_id(order.team_id)
            if team and team.leader_id and team.leader_id != actor_id:
                recipient_ids.add(team.leader_id)

        notification_port = self._notification_port
        if notification_port is not None and recipient_ids:
            try:
                await asyncio.gather(
                    *(
                        notification_port.notify_user(
                            user_id=recipient_id,
                            title="作业预计超时回报",
                            body=(
                                f"派工单 {order_id} 回报预计完成时间 {estimated_completion_time.isoformat()}。"
                                f"请关注后续接续工单安排。"
                            ),
                            category="dispatch",
                            severity="warning",
                            flight_id=order.flight_id,
                            dispatch_order_id=order_id,
                            related_entity_type="dispatch_order",
                            related_entity_id=order_id,
                        )
                        for recipient_id in recipient_ids
                    ),
                    return_exceptions=True,
                )
            except (
                Exception  # noqa: BLE001
            ) as exc:  # pragma: no cover
                logger.warning(f"Failed to send ETA notifications for order {order_id}: {exc}")

        return {
            "dispatch_order_id": order_id,
            "estimated_completion_time": estimated_completion_time.isoformat(),
            "estimated_completion_reported_at": utc_now().isoformat(),
            "estimated_completion_reported_by": actor_id,
            "note": note,
            "has_conflicts": bool(suggestions_payload),
            "suggestions": suggestions_payload,
            "message": "预计完成时间已回报",
        }

    async def cancel_order(
        self,
        order_id: str,
        payload: dict[str, Any],
        *,
        actor_id: str,
        is_privileged: bool = False,
    ) -> dict[str, Any]:
        order = await self._order_repo.find_by_id(order_id)
        if not order:
            raise EntityNotFoundException(entity_type="资源", entity_id="派工单不存在")
        if order.status in [DispatchOrderStatus.COMPLETED, DispatchOrderStatus.CANCELLED]:
            raise BusinessRuleException(message="该派工单无法取消")
        if not is_privileged and not order.can_be_started_by(actor_id):
            raise SystemException(message="仅管理员、调度管理员或该派工执行人可取消")

        enforce_actor_assignment = not is_privileged
        event_id = generate_id()
        log_id = generate_id()
        try:
            async with self._write_scope() as conn:
                if self._collaboration_recorder is not None:
                    await self._collaboration_recorder.record_event(
                        flight_id=order.flight_id,
                        dispatch_order_id=order_id,
                        event_type="order_cancelled",
                        actor_user_id=actor_id,
                        correlation_id=event_id,
                        payload={"reason": str(payload.get("reason") or "").strip() or None},
                        source_table="dispatch_order_logs",
                        source_record_id=log_id,
                        event_id=event_id,
                        conn=conn,
                    )
                success = await self._order_repo.update_status(
                    order_id,
                    "cancelled",
                    actor_id,
                    enforce_actor_assignment=enforce_actor_assignment,
                    conn=conn,
                    event_id=event_id,
                    log_id=log_id,
                )
        except TypeError:
            success = await self._order_repo.update_status(order_id, "cancelled", actor_id)

        if not success:
            if enforce_actor_assignment:
                raise SystemException(message="仅管理员、调度管理员或该派工执行人可取消")
            raise SystemException(message="操作失败")

        await self._refresh_dispatch_chat_deprecation(order.flight_id)

        str(payload.get("reason") or "").strip()
        return {"message": "派工单已取消"}

    async def report_issue(self, order_id: str, payload: dict[str, Any], *, actor_id: str) -> dict[str, Any]:
        if not actor_id:
            raise SystemException(message="未登录")

        order = await self._order_repo.find_by_id(order_id)
        if not order:
            raise EntityNotFoundException(entity_type="资源", entity_id="派工单不存在")

        anomaly_repo = self._anomaly_repo
        if anomaly_repo is None:
            raise SystemException(message="异常仓储不可用")

        if not order.can_be_started_by(actor_id):
            raise SystemException(message="无权上报此派工单异常")

        client_action_id = str(payload.get("client_action_id") or "").strip() or None
        if client_action_id and await self._order_repo.has_logged_action(
            order_id,
            "issue_reported",
            client_action_id=client_action_id,
        ):
            return {
                "success": True,
                "status": "duplicate",
                "message": "重复异常上报已忽略",
            }

        severity_raw = str(payload.get("severity") or "medium")
        try:
            severity = AnomalySeverity(severity_raw)
        except ValueError as exc:
            raise BusinessRuleException(message=f"无效异常级别: {severity_raw}") from exc

        issue_type_raw = str(payload.get("issue_type") or AnomalyType.DISPATCH_ISSUE.value).strip().lower()
        known_anomaly_types = {item.value for item in AnomalyType}
        anomaly_type = (
            AnomalyType(issue_type_raw) if issue_type_raw in known_anomaly_types else AnomalyType.DISPATCH_ISSUE
        )

        input_mode = str(payload.get("input_mode") or "text").strip().lower() or "text"
        title = str(payload.get("title") or "").strip()
        description = str(payload.get("description") or "").strip() or None
        note = str(payload.get("note") or "").strip() or None
        attachments = [str(item).strip() for item in list(payload.get("attachments") or []) if str(item).strip()]
        voice_attachment_id = str(payload.get("voice_attachment_id") or "").strip() or None
        if voice_attachment_id and voice_attachment_id not in attachments:
            attachments.append(voice_attachment_id)

        resolved_title = title or description or note
        if not resolved_title:
            if input_mode == "photo":
                resolved_title = "现场图片异常首报"
            elif input_mode == "voice":
                resolved_title = "现场语音异常首报"
            else:
                resolved_title = "现场异常首报"

        anomaly = Anomaly(
            anomaly_id=generate_id(),
            flight_id=order.flight_id,
            anomaly_type=anomaly_type,
            severity=severity,
            title=resolved_title,
            description=description,
            context_data={
                "dispatch_order_id": order_id,
                "reported_by": actor_id,
                "issue_type": issue_type_raw,
                "note": note,
                "attachments": attachments,
                "voice_attachment_id": voice_attachment_id,
                "input_mode": input_mode,
                "minimal_first_report": True,
                "position": {
                    "lat": payload.get("lat"),
                    "lng": payload.get("lng"),
                },
            },
        )
        created = await anomaly_repo.create_anomaly(anomaly)
        self._increment_metric(f"dispatch.issue_reported.{input_mode}")

        async with self._write_scope() as conn:
            await self._append_order_log_with_event(
                flight_id=order.flight_id,
                dispatch_order_id=order_id,
                action="issue_reported",
                event_type="order_issue_reported",
                actor_id=actor_id,
                details={
                    "anomaly_id": created.anomaly_id,
                    "severity": severity.value,
                    "title": resolved_title,
                    "input_mode": input_mode,
                    "client_action_id": client_action_id,
                },
                correlation_id=client_action_id or generate_id(),
                conn=conn,
            )

        sse_hub = self._sse_hub
        if sse_hub is not None:
            broadcast = getattr(sse_hub, "broadcast_to_topic", None)
            if callable(broadcast):
                try:
                    await broadcast(
                        "anomaly_alerts",
                        {
                            "event": "dispatch_issue_reported",
                            "anomaly_id": created.anomaly_id,
                            "dispatch_order_id": order_id,
                            "flight_id": order.flight_id,
                            "severity": severity.value,
                            "title": created.title,
                            "timestamp": utc_now().isoformat(),
                        },
                    )
                except Exception as exc:  # pragma: no cover - best effort side effect  # noqa: BLE001 - best-effort side effect must not abort main flow
                    logger.warning(f"Failed to broadcast dispatch issue SSE event for order {order_id}: {exc}")

        alert_service = self._alert_service
        if severity in {AnomalySeverity.HIGH, AnomalySeverity.CRITICAL} and alert_service is not None:
            level = AlertLevel.CRITICAL if severity == AnomalySeverity.CRITICAL else AlertLevel.ERROR
            channels = [AlertChannel.LOG]
            if severity == AnomalySeverity.CRITICAL:
                channels.append(AlertChannel.EMAIL)
            try:
                await alert_service.send_alert_async(
                    title=f"Dispatch issue reported: {created.title}",
                    message=f"dispatch_order={order_id}, flight={order.flight_id}, severity={severity.value}",
                    level=level,
                    channels=channels,
                    metadata={
                        "dispatch_order_id": order_id,
                        "anomaly_id": created.anomaly_id,
                        "reported_by": actor_id,
                    },
                )
            except Exception as exc:  # pragma: no cover - best effort side effect  # noqa: BLE001 - best-effort side effect must not abort main flow
                logger.warning(f"Failed to send dispatch issue alert for order {order_id}: {exc}")

        recipient_ids = set()
        if order.individual_user_id and order.individual_user_id != actor_id:
            recipient_ids.add(order.individual_user_id)
        if order.dispatched_by and order.dispatched_by != actor_id:
            recipient_ids.add(order.dispatched_by)

        team_repo = self._team_repo
        if order.team_id and team_repo is not None:
            team = await team_repo.find_by_id(order.team_id)
            if team and team.leader_id and team.leader_id != actor_id:
                recipient_ids.add(team.leader_id)

        notification_port = self._notification_port
        if (
            notification_port is not None
            and recipient_ids
            and severity in {AnomalySeverity.HIGH, AnomalySeverity.CRITICAL}
        ):
            try:
                notification_results = await asyncio.gather(
                    *(
                        notification_port.notify_user(
                            user_id=recipient_id,
                            title="Dispatch issue requires attention",
                            body=(
                                f"Dispatch order {order_id} reported issue: {created.title}. "
                                f"Severity: {severity.value}."
                            ),
                            category="dispatch",
                            severity="warning" if severity == AnomalySeverity.HIGH else "critical",
                            flight_id=order.flight_id,
                            dispatch_order_id=order_id,
                            related_entity_type="anomaly",
                            related_entity_id=created.anomaly_id,
                        )
                        for recipient_id in recipient_ids
                    ),
                    return_exceptions=True,
                )
                for notify_exc in notification_results:
                    if isinstance(notify_exc, Exception):
                        logger.warning(f"Failed to send dispatch issue notification for order {order_id}: {notify_exc}")
            except Exception as exc:  # pragma: no cover - best effort side effect  # noqa: BLE001 - best-effort side effect must not abort main flow
                logger.warning(f"Dispatch issue notification fanout failed for order {order_id}: {exc}")

        return {
            "success": True,
            "message": "异常已上报",
            "data": {
                "anomaly_id": created.anomaly_id,
                "dispatch_order_id": order_id,
                "severity": severity.value,
                "input_mode": input_mode,
                "title": created.title,
            },
        }

    async def submit_safety_checklist_item(
        self,
        order_id: str,
        item_code: str,
        payload: dict[str, Any],
        *,
        actor_id: str,
    ) -> dict[str, Any]:
        if not actor_id:
            raise SystemException(message="未登录")

        checklist_service = self._checklist_service
        if checklist_service is None:
            raise SystemException(message="安全检查清单服务不可用")

        order = await self._order_repo.find_by_id(order_id)
        if not order:
            raise EntityNotFoundException(entity_type="资源", entity_id="派工单不存在")
        self._ensure_order_execution_published(order, action_label="签到")

        if order.status in {DispatchOrderStatus.COMPLETED, DispatchOrderStatus.CANCELLED}:
            raise BusinessRuleException(message="当前状态不可提交安全检查清单")

        if order.status not in {DispatchOrderStatus.ASSIGNED, DispatchOrderStatus.IN_PROGRESS}:
            raise BusinessRuleException(message=f"当前状态不可提交清单，当前状态: {order.status.value}")

        if not order.can_be_completed_by(actor_id):
            raise SystemException(message="无权操作此派工单安全检查清单")

        try:
            record = await checklist_service.submit_item_result(
                dispatch_order_id=order_id,
                task_type=order.task_type,
                item_code=item_code,
                result=payload.get("result"),
                note=payload.get("note"),
                checked_by=actor_id,
                handled_on_site=bool(payload.get("handled_on_site", False)),
            )
        except ValueError as exc:
            logger.warning(
                f"invalid safety checklist item submission for dispatch_order_id={order_id}, "
                f"item_code={item_code}: {exc}"
            )
            raise BusinessRuleException(message="invalid safety checklist item result") from exc

        if hasattr(self._order_repo, "append_log"):
            async with self._write_scope() as conn:
                await self._order_repo.append_log(
                    dispatch_order_id=order_id,
                    action="safety_checklist_item",
                    actor_id=actor_id,
                    details={
                        "item_code": item_code,
                        "result": payload.get("result"),
                        "note": payload.get("note"),
                        "template_version": record.get("template_version"),
                    },
                    conn=conn,
                )

        return record

    async def submit_safety_checklist_batch(
        self,
        order_id: str,
        payload: dict[str, Any],
        *,
        actor_id: str,
    ) -> dict[str, Any]:
        if not actor_id:
            raise SystemException(message="未登录")

        checklist_service = self._checklist_service
        if checklist_service is None:
            raise SystemException(message="安全检查清单服务不可用")

        order = await self._order_repo.find_by_id(order_id)
        if not order:
            raise EntityNotFoundException(entity_type="资源", entity_id="派工单不存在")
        self._ensure_order_execution_published(order, action_label="批量安全检查")

        if order.status in {DispatchOrderStatus.COMPLETED, DispatchOrderStatus.CANCELLED}:
            raise BusinessRuleException(message="当前状态不可提交安全检查清单")

        if order.status not in {DispatchOrderStatus.ASSIGNED, DispatchOrderStatus.IN_PROGRESS}:
            raise BusinessRuleException(message=f"当前状态不可提交清单，当前状态: {order.status.value}")

        if not order.can_be_completed_by(actor_id):
            raise SystemException(message="无权操作此派工单安全检查清单")

        try:
            result = await checklist_service.submit_batch_results(
                dispatch_order_id=order_id,
                task_type=order.task_type,
                items=list(payload.get("items") or []),
                checked_by=actor_id,
            )
        except ValueError as exc:
            logger.warning(f"invalid safety checklist batch submission for dispatch_order_id={order_id}: {exc}")
            raise BusinessRuleException(message="invalid safety checklist batch submission") from exc

        if hasattr(self._order_repo, "append_log"):
            async with self._write_scope() as conn:
                await self._order_repo.append_log(
                    dispatch_order_id=order_id,
                    action="safety_checklist_batch",
                    actor_id=actor_id,
                    details={
                        "submitted_count": int(result.get("submitted_count") or 0),
                        "blocking_issues": list((result.get("gate") or {}).get("blocking_issues") or []),
                        "soft_missing_count": int((result.get("gate") or {}).get("soft_missing_count") or 0),
                        "template_version": (result.get("gate") or {}).get("template_version"),
                    },
                    conn=conn,
                )

        return result

    async def checkin_order(self, order_id: str, payload: dict[str, Any], *, actor_id: str) -> dict[str, Any]:
        if not actor_id:
            raise SystemException(message="未登录")

        order = await self._order_repo.find_by_id(order_id)
        if not order:
            raise EntityNotFoundException(entity_type="资源", entity_id="派工单不存在")
        self._ensure_order_execution_published(order, action_label="签到")

        if order.status in {DispatchOrderStatus.COMPLETED, DispatchOrderStatus.CANCELLED}:
            raise BusinessRuleException(message="当前状态不可签到")

        if not order.can_be_started_by(actor_id):
            raise SystemException(message="无权操作此派工单")

        qr_code = payload.get("qr_code")

        lat = payload.get("lat")
        lng = payload.get("lng")
        if (lat is None) != (lng is None):
            raise BusinessRuleException(message="位置参数需要同时提供 lat 与 lng")

        client_action_id = str(payload.get("client_action_id") or "").strip() or None
        if client_action_id and hasattr(self._order_repo, "has_logged_action"):
            duplicate = await self._order_repo.has_logged_action(
                order_id,
                "checkin",
                client_action_id=client_action_id,
            )
            if duplicate:
                return {
                    "message": "重复签到请求已忽略",
                    "status": "duplicate",
                }

        distance_m: float | None = None
        verification_status = "pending_verification"
        verification_source = "manual"
        if lat is not None and lng is not None and order.stand_id and self._stand_repo is not None:
            stand = await self._stand_repo.find_by_id(order.stand_id)
            if stand is not None:
                distance_m = DispatchCalculator.haversine_distance(
                    float(lat),
                    float(lng),
                    float(stand.position_lat),
                    float(stand.position_lng),
                )
                if distance_m <= self.CHECKIN_DISTANCE_THRESHOLD_METERS:
                    verification_status = "verified"
                    verification_source = "geo"

        if verification_status != "verified" and qr_code and self._validate_qr_code(order_id, qr_code):
            verification_status = "verified"
            verification_source = "qr"

        member = await self._member_repo.find_by_order_and_user(order_id, actor_id)
        if member is None and order.assignee_type == AssigneeType.INDIVIDUAL and order.individual_user_id == actor_id:
            member = DispatchOrderMember(
                id=generate_id(),
                dispatch_order_id=order_id,
                user_id=actor_id,
                role=MemberRole.MEMBER,
                source_type=AssigneeType.INDIVIDUAL,
                source_team_id=None,
            )
            await self._member_repo.save(member)

        if not member:
            raise EntityNotFoundException(entity_type="资源", entity_id="您不是此派工单的成员")

        if member.check_in_time:
            return {
                "message": "您已到场",
                "status": "already_checked_in",
                "check_in_time": member.check_in_time.isoformat(),
                "verification_status": verification_status,
                "verification_source": verification_source,
                "auto_started": order.status == DispatchOrderStatus.IN_PROGRESS,
            }

        member.check_in_time = utc_now()
        auto_started = False
        async with self._write_scope() as conn:
            await self._member_repo.save(member)

            if hasattr(self._order_repo, "append_log"):
                await self._append_order_log_with_event(
                    flight_id=order.flight_id,
                    dispatch_order_id=order_id,
                    action="checkin",
                    event_type="order_checked_in",
                    actor_id=actor_id,
                    details={
                        "client_action_id": client_action_id,
                        "qr_code": qr_code,
                        "lat": lat,
                        "lng": lng,
                        "accuracy_m": payload.get("accuracy_m"),
                        "distance_to_stand_m": distance_m,
                        "note": payload.get("note"),
                        "verification_status": verification_status,
                        "verification_source": verification_source,
                    },
                    correlation_id=client_action_id or generate_id(),
                    conn=conn,
                )
            if order.status == DispatchOrderStatus.ASSIGNED:
                auto_started = True
        if auto_started:
            await self._start_order_runtime(
                order=order,
                order_id=order_id,
                actor_id=actor_id,
                actual_start=member.check_in_time,
                reason="auto_start_after_checkin",
            )

        followup_todo: dict[str, Any] | None = None
        if verification_status == "pending_verification":
            self._increment_metric("dispatch.order.arrival.pending_verification")
            followup_todo = await self._ensure_followup_todo(
                order=order,
                actor_id=actor_id,
                source_type="dispatch_arrival_verification",
                title=f"补核到场记录 - {order.id}",
                description=(
                    f"派工单 {order.id} 到场记录未通过自动核验。"
                    f" 请班组长在 2 小时内补核到场来源与现场真实性。"
                    f" 来源: {verification_source}; 距离: "
                    f"{round(distance_m, 2) if distance_m is not None else '未记录'} 米。"
                ),
                priority="高",
                due_date=member.check_in_time + timedelta(hours=2),
                tags=["dispatch", "team_lead_followup", "arrival_verification"],
            )
            if hasattr(self._order_repo, "append_log"):
                async with self._write_scope() as conn:
                    await self._append_order_log_with_event(
                        flight_id=order.flight_id,
                        dispatch_order_id=order_id,
                        action="arrival_verification_followup_created",
                        event_type="order_arrival_verification_followup_created",
                        actor_id=actor_id,
                        details={
                            "verification_status": verification_status,
                            "verification_source": verification_source,
                            "distance_to_stand_m": round(distance_m, 2) if distance_m is not None else None,
                            "todo_id": (followup_todo or {}).get("todo_id"),
                            "assigned_to": (followup_todo or {}).get("assigned_to"),
                            "due_at": (member.check_in_time + timedelta(hours=2)).isoformat(),
                        },
                        correlation_id=client_action_id or generate_id(),
                        conn=conn,
                    )

        return {
            "message": "到场成功",
            "check_in_time": member.check_in_time.isoformat(),
            "distance_to_stand_m": round(distance_m, 2) if distance_m is not None else None,
            "verification_status": verification_status,
            "verification_source": verification_source,
            "auto_started": auto_started,
            "order_status": "in_progress" if auto_started else order.status.value,
            "followup_todo_id": (followup_todo or {}).get("todo_id"),
        }

    async def checkout_order(
        self,
        order_id: str,
        payload: dict[str, Any],
        *,
        actor_id: str,
    ) -> dict[str, Any]:
        """用户主动签退。全部 active member 签退后自动完结工单。"""
        if not actor_id:
            raise SystemException(message="未登录")

        order = await self._order_repo.find_by_id(order_id)
        if not order:
            raise EntityNotFoundException(entity_type="资源", entity_id="派工单不存在")
        self._ensure_order_execution_published(order, action_label="签退")

        if order.status not in (DispatchOrderStatus.IN_PROGRESS, DispatchOrderStatus.ASSIGNED):
            raise BusinessRuleException(message="仅进行中或已派发的工单可以签退")

        client_action_id = self._payload_value(payload, "client_action_id")

        # 重复性检查
        if client_action_id and hasattr(self._order_repo, "has_logged_action"):
            duplicate = await self._order_repo.has_logged_action(
                order_id,
                "checkout",
                client_action_id=client_action_id,
            )
            if duplicate:
                return {"message": "重复签退请求已忽略", "status": "duplicate"}

        member = await self._member_repo.find_by_order_and_user(order_id, actor_id)
        if not member:
            raise EntityNotFoundException(entity_type="资源", entity_id="您不是此派工单的成员")

        if member.check_out_time:
            return {
                "message": "您已签退",
                "status": "already_checked_out",
                "check_out_time": member.check_out_time.isoformat(),
            }

        recorded_at = self._extract_action_time(payload.get("recorded_at"))
        checkout_time = recorded_at if recorded_at is not None else utc_now()
        member.check_out_time = checkout_time

        travel_info: dict[str, Any] | None = None
        async with self._write_scope() as conn:
            await self._member_repo.save(member)

            # 记录移动时间：查找上一个已完工工单的签退记录
            travel_info = await self._record_travel_time_on_checkin(
                user_id=actor_id,
                current_order=order,
                checkin_time=member.check_in_time,
            )

            if hasattr(self._order_repo, "append_log"):
                await self._append_order_log_with_event(
                    flight_id=order.flight_id,
                    dispatch_order_id=order_id,
                    action="checkout",
                    event_type="order_checked_out",
                    actor_id=actor_id,
                    details={
                        "client_action_id": client_action_id,
                        "lat": payload.get("lat"),
                        "lng": payload.get("lng"),
                        "note": payload.get("note"),
                        "travel_info": travel_info,
                    },
                    correlation_id=client_action_id or generate_id(),
                    conn=conn,
                )

        # 检查是否全员签退 → 自动完结工单
        auto_completed = await self._try_auto_complete_on_all_checkout(order_id, actor_id)

        result: dict[str, Any] = {
            "message": "签退成功",
            "check_out_time": checkout_time.isoformat(),
            "auto_completed": auto_completed,
        }
        if travel_info:
            result["travel_from_order_id"] = travel_info.get("from_order_id")
            result["travel_from_stand_code"] = travel_info.get("from_stand_code")
            result["travel_minutes"] = travel_info.get("travel_minutes")
        return result

    async def _try_auto_complete_on_all_checkout(
        self,
        order_id: str,
        actor_id: str,
    ) -> bool:
        """Check if all active members have checked out; if so, auto-complete."""
        members = await self._member_repo.find_by_order(order_id)
        if not members:
            return False
        all_checked_out = all(m.check_out_time is not None for m in members)
        if not all_checked_out:
            return False

        try:
            await self.complete_order(
                order_id,
                {"completion_notes": "全员签退自动完结"},
                actor_id=actor_id,
            )
            return True
        except Exception as exc:  # noqa: BLE001 - best-effort side effect must not abort main flow
            logger.warning("auto_complete_order_failed order_id=%s", order_id, exc_info=exc)
            return False

    async def _record_travel_time_on_checkin(
        self,
        *,
        user_id: str,
        current_order: Any,
        checkin_time: datetime | None,
    ) -> dict[str, Any] | None:
        """If we can find a previous checkout and the current order has check_in_time,
        calculate and record travel time between the two stands."""
        if checkin_time is None or self._travel_stats_repo is None:
            return None
        if not current_order.stand_id:
            return None

        find_fn = getattr(self._member_repo, "find_latest_checkout_for_user", None)
        if not callable(find_fn):
            return None

        prev = await find_fn(user_id, checkin_time)
        if prev is None:
            return None

        prev_checkout_time = prev.get("check_out_time")
        prev_stand_id = prev.get("stand_id")
        if prev_checkout_time is None or prev_stand_id is None:
            return None
        if prev_stand_id == current_order.stand_id:
            return None

        travel_minutes = (checkin_time - prev_checkout_time).total_seconds() / 60.0
        if travel_minutes <= 0 or travel_minutes > 240:
            return None

        try:
            await self._travel_stats_repo.record_travel(
                from_stand_id=prev_stand_id,
                to_stand_id=current_order.stand_id,
                travel_minutes=travel_minutes,
            )
        except POSTGRES_EXCEPTIONS as exc:
            logger.warning("record_travel failed: %s", exc)

        return {
            "from_order_id": prev.get("dispatch_order_id"),
            "from_stand_code": prev.get("stand_code"),
            "travel_minutes": round(travel_minutes, 2),
        }

    async def sync_mobile_actions(self, payload: dict[str, Any], *, actor_id: str) -> dict[str, Any]:
        if not actor_id:
            raise SystemException(message="未登录")

        actions = list(payload.get("actions") or [])
        results: list[dict[str, Any]] = []
        applied = 0
        duplicates = 0
        failed = 0

        for action in actions:
            action_type = str(action.get("action_type") or "").strip()
            dispatch_order_id = str(action.get("dispatch_order_id") or "").strip()
            client_action_id = str(action.get("client_action_id") or "").strip()
            action_payload = action.get("payload") or {}

            try:
                action_log = self.ACTION_LOG_MAP.get(action_type, action_type)
                if await self._order_repo.has_logged_action(
                    dispatch_order_id,
                    action_log,
                    client_action_id=client_action_id or None,
                ):
                    duplicates += 1
                    results.append(
                        {
                            "client_action_id": client_action_id,
                            "dispatch_order_id": dispatch_order_id,
                            "action_type": action_type,
                            "status": "duplicate",
                            "message": "重复动作已忽略",
                            "server_timestamp": utc_now(),
                        }
                    )
                    continue

                if action_type == "accept":
                    await self.accept_order(
                        dispatch_order_id,
                        {
                            "note": str(action_payload.get("note") or "") or None,
                            "client_action_id": client_action_id or None,
                        },
                        actor_id=actor_id,
                    )
                elif action_type == "checkin":
                    await self.checkin_order(
                        dispatch_order_id,
                        {
                            "qr_code": action_payload.get("qr_code"),
                            "lat": action_payload.get("lat"),
                            "lng": action_payload.get("lng"),
                            "accuracy_m": action_payload.get("accuracy_m"),
                            "note": action_payload.get("note"),
                            "client_action_id": client_action_id or None,
                        },
                        actor_id=actor_id,
                    )
                elif action_type == "start":
                    await self.start_order(
                        dispatch_order_id,
                        {
                            "actual_start_time": self._extract_action_time(
                                action_payload.get("actual_start_time") or action.get("action_timestamp")
                            )
                        },
                        actor_id=actor_id,
                    )
                elif action_type == "complete":
                    await self.complete_order(
                        dispatch_order_id,
                        {
                            "actual_end_time": self._extract_action_time(
                                action_payload.get("actual_end_time") or action.get("action_timestamp")
                            ),
                            "completion_notes": action_payload.get("completion_notes"),
                        },
                        actor_id=actor_id,
                    )
                elif action_type == "report_issue":
                    await self.report_issue(
                        dispatch_order_id,
                        {
                            "title": str(action_payload.get("title") or "现场异常上报"),
                            "description": action_payload.get("description"),
                            "severity": action_payload.get("severity", "medium"),
                            "issue_type": action_payload.get("issue_type", "dispatch_issue"),
                            "note": action_payload.get("note"),
                            "lat": action_payload.get("lat"),
                            "lng": action_payload.get("lng"),
                            "attachments": list(action_payload.get("attachments") or []),
                            "client_action_id": client_action_id or None,
                        },
                        actor_id=actor_id,
                    )
                elif action_type == "eta_report":
                    await self.report_estimated_completion(
                        dispatch_order_id,
                        {
                            "estimated_completion_time": self._extract_action_time(
                                action_payload.get("estimated_completion_time") or action.get("action_timestamp")
                            ),
                            "note": action_payload.get("note"),
                            "client_action_id": client_action_id or None,
                        },
                        actor_id=actor_id,
                    )
                elif action_type == "checkout":
                    await self.checkout_order(
                        dispatch_order_id,
                        {
                            "lat": action_payload.get("lat"),
                            "lng": action_payload.get("lng"),
                            "note": action_payload.get("note"),
                            "client_action_id": client_action_id or None,
                            "recorded_at": self._extract_action_time(
                                action_payload.get("recorded_at") or action.get("action_timestamp")
                            ),
                        },
                        actor_id=actor_id,
                    )
                else:
                    raise BusinessRuleException(message=f"不支持的动作类型: {action_type}")

                applied += 1
                results.append(
                    {
                        "client_action_id": client_action_id,
                        "dispatch_order_id": dispatch_order_id,
                        "action_type": action_type,
                        "status": "applied",
                        "message": "补传成功",
                        "server_timestamp": utc_now(),
                    }
                )
            except Exception as exc:  # noqa: BLE001 - recovery handler must catch all errors
                failed += 1
                results.append(
                    {
                        "client_action_id": client_action_id,
                        "dispatch_order_id": dispatch_order_id,
                        "action_type": action_type,
                        "status": "failed",
                        "message": getattr(exc, "message", None) or f"补传失败: {exc}",
                        "server_timestamp": utc_now(),
                    }
                )

        return {
            "total": len(actions),
            "applied": applied,
            "duplicates": duplicates,
            "failed": failed,
            "results": results,
        }
