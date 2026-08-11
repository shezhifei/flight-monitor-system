"""EP-08: AI 调度操作性工具执行器。

实现 filter_flights / change_stand / notify_teams 三个 handler。
审批拦截由 ToolRegistry 统一处理；本执行器仅负责真实副作用执行。
"""

from typing import Any

from src.infrastructure.logging.core import get_logger

from .base import BaseToolExecutor, ToolCategory, ToolExecutionError, ToolExecutionStatus
from .dispatch_command_tools import DispatchCommandToolName

logger = get_logger(__name__)


class DispatchCommandExecutor(BaseToolExecutor):
    """Execute dispatch command tool calls."""

    def __init__(
        self,
        flight_service: Any = None,
        dispatch_service: Any = None,
        notification_service: Any = None,
        default_user: str = "AI_Assistant",
    ):
        super().__init__(default_user)
        self._service = flight_service
        self._dispatch_service = dispatch_service
        self._notification_service = notification_service

    def _register_handlers(self) -> None:
        self._handlers = {
            DispatchCommandToolName.FILTER_FLIGHTS.value: self._handle_filter_flights,
            DispatchCommandToolName.CHANGE_STAND.value: self._handle_change_stand,
            DispatchCommandToolName.NOTIFY_TEAMS.value: self._handle_notify_teams,
        }

    def get_category(self) -> ToolCategory:
        return ToolCategory.FLIGHT

    async def _handle_filter_flights(self, args: dict[str, Any]) -> dict[str, Any]:
        """Filter flights by keyword / status / stand (read-only)."""
        self._ensure_service()
        limit = min(int(args.get("limit") or 20), 100)

        filters: dict[str, Any] = {}
        if args.get("flight_number"):
            filters["flight_number"] = args["flight_number"]
        if args.get("status"):
            filters["status"] = args["status"]
        if args.get("stand_id"):
            filters["stand_id"] = args["stand_id"]

        try:
            if hasattr(self._service, "search_flights"):
                flights = await self._service.search_flights(**filters, limit=limit)
            elif hasattr(self._service, "get_flights"):
                flights = await self._service.get_flights(limit=limit, **filters)
            else:
                flights = []
        except Exception as exc:  # noqa: BLE001 - service call fallback must catch all errors
            logger.warning(f"filter_flights fallback due to: {exc}")
            flights = []

        result_items = []
        for f in flights[:limit]:
            result_items.append(
                {
                    "flight_id": getattr(f, "id", None) or getattr(f, "flight_id", None),
                    "flight_number": getattr(f, "flight_number", None),
                    "status": str(getattr(f, "flight_status", None) or ""),
                    "stand_id": getattr(f, "stand_id", None),
                    "gate": getattr(f, "gate", None),
                    "scheduled_departure": self._to_iso(getattr(f, "scheduled_departure", None)),
                }
            )

        return {
            "count": len(result_items),
            "flights": result_items,
        }

    async def _handle_change_stand(self, args: dict[str, Any]) -> dict[str, Any]:
        """Apply stand change side effect after approval gate has passed."""
        self._ensure_service()

        flight_id = str(self._require_arg(args, "flight_id")).strip()
        new_stand_id = str(self._require_arg(args, "new_stand_id")).strip()
        reason = str(args.get("reason") or "AI 建议变更机位").strip()
        if not flight_id or not new_stand_id:
            raise ToolExecutionError("flight_id and new_stand_id are required", ToolExecutionStatus.VALIDATION_ERROR)

        if not hasattr(self._service, "assign_stand"):
            raise ToolExecutionError(
                "flight service does not support assign_stand",
                ToolExecutionStatus.ERROR,
            )

        try:
            await self._service.assign_stand(
                flight_id=flight_id,
                stand_number=new_stand_id,
                assigned_by=self.default_user,
            )
        except TypeError:
            # 兼容部分服务旧签名（位置参数）
            await self._service.assign_stand(flight_id, new_stand_id, self.default_user)
        except Exception as exc:
            raise ToolExecutionError(
                f"failed to assign stand for flight {flight_id}: {exc}",
                ToolExecutionStatus.ERROR,
            ) from exc

        return {
            "status": "executed",
            "flight_id": flight_id,
            "new_stand_id": new_stand_id,
            "reason": reason,
            "affected_rows": 1,
            "side_effects": [
                {
                    "type": "flight_stand_changed",
                    "flight_id": flight_id,
                    "new_stand_id": new_stand_id,
                }
            ],
            "message": f"flight {flight_id} stand changed to {new_stand_id}",
        }

    @staticmethod
    def _normalize_priority(priority: Any) -> str:
        normalized = str(priority or "normal").strip().lower()
        if normalized in {"urgent", "high", "normal"}:
            return normalized
        return "normal"

    async def _notify_single_target(
        self,
        *,
        target_id: str,
        title: str,
        body: str,
        severity: str,
        flight_id: str,
        payload: dict[str, Any],
    ) -> None:
        service = self._notification_service
        if service is None:
            raise ToolExecutionError("notification service is not initialized", ToolExecutionStatus.ERROR)

        if hasattr(service, "notify_user"):
            await service.notify_user(
                user_id=target_id,
                title=title,
                body=body,
                category="dispatch",
                severity=severity,
                related_entity_type="flight" if flight_id else None,
                related_entity_id=flight_id or None,
            )
            return

        if hasattr(service, "send"):
            try:
                await service.send(
                    user_id=target_id,
                    title=title,
                    body=body,
                    category="dispatch",
                    severity=severity,
                    related_entity_type="flight" if flight_id else None,
                    related_entity_id=flight_id or None,
                )
                return
            except TypeError:
                # 兼容旧接口签名
                await service.send(target_id, title, body)
                return

        if hasattr(service, "notify_ai_event"):
            event_payload = dict(payload)
            event_payload["target_id"] = target_id
            await service.notify_ai_event("dispatch_notify_teams", event_payload)
            return

        raise ToolExecutionError(
            "notification service does not expose notify_user/send interface",
            ToolExecutionStatus.ERROR,
        )

    async def _handle_notify_teams(self, args: dict[str, Any]) -> dict[str, Any]:
        """Deliver team notifications once approval gate has passed."""
        if self._notification_service is None:
            raise ToolExecutionError("notification service is not initialized", ToolExecutionStatus.ERROR)

        message = str(self._require_arg(args, "message")).strip()
        team_ids = [str(team_id).strip() for team_id in (args.get("team_ids") or []) if str(team_id).strip()]
        priority = self._normalize_priority(args.get("priority"))
        flight_id = str(args.get("flight_id") or "").strip()
        target_desc = f"班组 {', '.join(team_ids)}" if team_ids else "所有相关班组"
        title = f"调度通知{f'（航班 {flight_id}）' if flight_id else ''}"
        severity = "high" if priority in {"high", "urgent"} else "info"
        payload = {
            "team_ids": team_ids,
            "message": message,
            "priority": priority,
            "flight_id": flight_id or None,
        }

        delivered = 0
        if team_ids:
            for team_id in team_ids:
                await self._notify_single_target(
                    target_id=team_id,
                    title=title,
                    body=message,
                    severity=severity,
                    flight_id=flight_id,
                    payload=payload,
                )
                delivered += 1
        elif hasattr(self._notification_service, "notify_ai_event"):
            await self._notification_service.notify_ai_event("dispatch_notify_teams", payload)
            delivered = 1
        else:
            raise ToolExecutionError(
                "notify_teams requires team_ids when broadcast adapter is unavailable",
                ToolExecutionStatus.VALIDATION_ERROR,
            )

        return {
            "status": "executed",
            "team_ids": team_ids,
            "priority": priority,
            "flight_id": flight_id or None,
            "target": target_desc,
            "delivered": delivered,
            "affected_rows": delivered,
            "side_effects": [
                {
                    "type": "teams_notified",
                    "team_ids": team_ids,
                    "priority": priority,
                    "flight_id": flight_id or None,
                    "delivered": delivered,
                }
            ],
            "message": f"notification delivered to {target_desc}",
        }
