"""
AIP Action Handler 注册器

将 Ontology Actions 绑定到实际的业务服务实现。
作为 AIP 应用与现有业务逻辑之间的适配层。

使用方式:
    from src.infrastructure.ai.aip.action_handlers import register_all_handlers

    app = get_aip_app()
    register_all_handlers(app)
"""

from __future__ import annotations

from collections.abc import Awaitable, Callable
from typing import Any

from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)

HandlerFunc = Callable[[str, dict[str, Any]], Awaitable[dict[str, Any]]]


class ActionHandlerRegistry:
    """Action Handler 注册表"""

    def __init__(self):
        self._handlers: dict[str, HandlerFunc] = {}
        self._initialized = False

    def register(self, object_type: str, action: str, handler: HandlerFunc) -> None:
        """注册 Action Handler"""
        key = f"{object_type}.{action}"
        self._handlers[key] = handler
        logger.debug(f"Registered handler: {key}")

    def get(self, object_type: str, action: str) -> HandlerFunc | None:
        """获取 Action Handler"""
        key = f"{object_type}.{action}"
        return self._handlers.get(key)

    def has_handler(self, object_type: str, action: str) -> bool:
        """检查是否有 Handler"""
        return self.get(object_type, action) is not None

    @property
    def all_handlers(self) -> dict[str, HandlerFunc]:
        return self._handlers.copy()

    @property
    def is_initialized(self) -> bool:
        return self._initialized

    def mark_initialized(self) -> None:
        self._initialized = True


_handler_registry: ActionHandlerRegistry | None = None


def get_handler_registry() -> ActionHandlerRegistry:
    """获取全局 Handler 注册表"""
    global _handler_registry
    if _handler_registry is None:
        _handler_registry = ActionHandlerRegistry()
    return _handler_registry


async def _handle_flight_change_stand(flight_id: str, params: dict[str, Any]) -> dict[str, Any]:
    """
    Flight.change_stand Handler

    将航班从一个机位变更到另一个机位。

    Args:
        flight_id: 航班ID
        params: 包含 new_stand, reason 等

    Returns:
        执行结果
    """
    try:
        from src.application.services.flight.flight_command_gateway import FlightCommandGateway

        gateway = FlightCommandGateway()
        new_stand = params.get("new_stand")
        reason = params.get("reason", "AI requested change")

        if not new_stand:
            return {"success": False, "error": "new_stand is required"}

        await gateway.update_flight_stand(flight_id=flight_id, new_stand=new_stand, reason=reason)

        return {
            "success": True,
            "flight_id": flight_id,
            "new_stand": new_stand,
            "message": f"Flight {flight_id} stand changed to {new_stand}",
        }

    except ImportError:
        logger.error("FlightCommandGateway not available")
        raise RuntimeError("FlightCommandGateway not available") from None
    except Exception as exc:
        logger.error("Failed to change stand for flight %s", flight_id, exc_info=exc)
        return {"success": False, "error": "action_failed"}


async def _handle_flight_delay_flight(flight_id: str, params: dict[str, Any]) -> dict[str, Any]:
    """
    Flight.delay_flight Handler

    标记航班延误。

    Args:
        flight_id: 航班ID
        params: 包含 delay_minutes, reason 等

    Returns:
        执行结果
    """
    try:
        from src.application.services.flight.flight_command_gateway import FlightCommandGateway

        gateway = FlightCommandGateway()
        delay_minutes = params.get("delay_minutes", 0)
        reason = params.get("reason", "AI recorded delay")

        await gateway.update_flight_delay(flight_id=flight_id, delay_minutes=delay_minutes, reason=reason)

        return {
            "success": True,
            "flight_id": flight_id,
            "delay_minutes": delay_minutes,
            "message": f"Flight {flight_id} delayed by {delay_minutes} minutes",
        }

    except ImportError:
        logger.error("FlightCommandGateway not available")
        raise RuntimeError("FlightCommandGateway not available") from None
    except Exception as exc:
        logger.error("Failed to delay flight %s", flight_id, exc_info=exc)
        return {"success": False, "error": "action_failed"}


async def _handle_flight_assign_team(flight_id: str, params: dict[str, Any]) -> dict[str, Any]:
    """
    Flight.assign_team Handler

    为航班分配保障班组。

    Args:
        flight_id: 航班ID
        params: 包含 team_id, role 等

    Returns:
        执行结果
    """
    try:
        from src.application.services.dispatch.dispatch_command_service import DispatchCommandService

        service = DispatchCommandService()
        team_id = params.get("team_id")
        role = params.get("role", "handler")

        if not team_id:
            return {"success": False, "error": "team_id is required"}

        await service.assign_team_to_flight(flight_id=flight_id, team_id=team_id, role=role)

        return {
            "success": True,
            "flight_id": flight_id,
            "team_id": team_id,
            "role": role,
            "message": f"Team {team_id} assigned to flight {flight_id}",
        }

    except ImportError:
        logger.error("DispatchCommandService not available")
        raise RuntimeError("DispatchCommandService not available") from None
    except Exception as exc:
        logger.error("Failed to assign team to flight %s", flight_id, exc_info=exc)
        return {"success": False, "error": "action_failed"}


async def _handle_flight_update_status(flight_id: str, params: dict[str, Any]) -> dict[str, Any]:
    """
    Flight.update_status Handler

    更新航班状态。

    Args:
        flight_id: 航班ID
        params: 包含 status 等

    Returns:
        执行结果
    """
    try:
        from src.application.services.flight.flight_command_gateway import FlightCommandGateway

        gateway = FlightCommandGateway()
        status = params.get("status")

        if not status:
            return {"success": False, "error": "status is required"}

        await gateway.update_flight_status(flight_id=flight_id, new_status=status)

        return {
            "success": True,
            "flight_id": flight_id,
            "status": status,
            "message": f"Flight {flight_id} status updated to {status}",
        }

    except ImportError:
        logger.error("FlightCommandGateway not available")
        raise RuntimeError("FlightCommandGateway not available") from None
    except Exception as exc:
        logger.error("Failed to update status for flight %s", flight_id, exc_info=exc)
        return {"success": False, "error": "action_failed"}


async def _handle_flight_mark_arrived(flight_id: str, params: dict[str, Any]) -> dict[str, Any]:
    """Flight.mark_arrived Handler"""
    try:
        from src.application.services.flight.flight_command_gateway import FlightCommandGateway

        gateway = FlightCommandGateway()
        actual_arrival = params.get("actual_arrival")

        await gateway.mark_flight_arrived(flight_id=flight_id, actual_arrival=actual_arrival)

        return {
            "success": True,
            "flight_id": flight_id,
            "status": "arrived",
            "message": f"Flight {flight_id} marked as arrived",
        }

    except Exception as exc:
        logger.error("Failed to mark flight %s as arrived", flight_id, exc_info=exc)
        return {"success": False, "error": "action_failed"}


async def _handle_flight_mark_departed(flight_id: str, params: dict[str, Any]) -> dict[str, Any]:
    """Flight.mark_departed Handler"""
    try:
        from src.application.services.flight.flight_command_gateway import FlightCommandGateway

        gateway = FlightCommandGateway()
        actual_departure = params.get("actual_departure")

        await gateway.mark_flight_departed(flight_id=flight_id, actual_departure=actual_departure)

        return {
            "success": True,
            "flight_id": flight_id,
            "status": "departed",
            "message": f"Flight {flight_id} marked as departed",
        }

    except Exception as exc:
        logger.error("Failed to mark flight %s as departed", flight_id, exc_info=exc)
        return {"success": False, "error": "action_failed"}


async def _handle_stand_occupy(stand_id: str, params: dict[str, Any]) -> dict[str, Any]:
    """
    Stand.occupy Handler

    占用机位（航班停靠）。

    Args:
        stand_id: 机位ID
        params: 包含 flight_id 等

    Returns:
        执行结果
    """
    try:
        flight_id = params.get("flight_id")
        if not flight_id:
            return {"success": False, "error": "flight_id is required"}

        logger.info(f"Stand {stand_id} occupied by flight {flight_id}")

        return {
            "success": True,
            "stand_id": stand_id,
            "flight_id": flight_id,
            "status": "occupied",
            "message": f"Stand {stand_id} occupied by flight {flight_id}",
        }

    except (KeyError, TypeError, ValueError, OSError) as exc:
        logger.error("Failed to occupy stand %s", stand_id, exc_info=exc)
        return {"success": False, "error": "action_failed"}


async def _handle_stand_release(stand_id: str, params: dict[str, Any]) -> dict[str, Any]:
    """Stand.release Handler"""
    try:
        logger.info(f"Stand {stand_id} released")

        return {
            "success": True,
            "stand_id": stand_id,
            "status": "available",
            "message": f"Stand {stand_id} released and available",
        }

    except (KeyError, TypeError, ValueError, OSError) as exc:
        logger.error("Failed to release stand %s", stand_id, exc_info=exc)
        return {"success": False, "error": "action_failed"}


async def _handle_stand_reserve(stand_id: str, params: dict[str, Any]) -> dict[str, Any]:
    """Stand.reserve Handler"""
    try:
        flight_id = params.get("flight_id")
        start_time = params.get("start_time")
        end_time = params.get("end_time")

        logger.info(f"Stand {stand_id} reserved for flight {flight_id}")

        return {
            "success": True,
            "stand_id": stand_id,
            "flight_id": flight_id,
            "start_time": start_time,
            "end_time": end_time,
            "status": "reserved",
            "message": f"Stand {stand_id} reserved",
        }

    except (KeyError, TypeError, ValueError, OSError) as exc:
        logger.error("Failed to reserve stand %s", stand_id, exc_info=exc)
        return {"success": False, "error": "action_failed"}


async def _handle_stand_close(stand_id: str, params: dict[str, Any]) -> dict[str, Any]:
    """Stand.close Handler"""
    try:
        reason = params.get("reason", "Maintenance")
        expected_reopen = params.get("expected_reopen")

        logger.info(f"Stand {stand_id} closed: {reason}")

        return {
            "success": True,
            "stand_id": stand_id,
            "status": "closed",
            "reason": reason,
            "expected_reopen": expected_reopen,
            "message": f"Stand {stand_id} closed: {reason}",
        }

    except (KeyError, TypeError, ValueError, OSError) as exc:
        logger.error("Failed to close stand %s", stand_id, exc_info=exc)
        return {"success": False, "error": "action_failed"}


async def _handle_stand_update_status(stand_id: str, params: dict[str, Any]) -> dict[str, Any]:
    """Stand.update_status Handler"""
    try:
        status = params.get("status")
        if not status:
            return {"success": False, "error": "status is required"}

        logger.info(f"Stand {stand_id} status updated to {status}")

        return {
            "success": True,
            "stand_id": stand_id,
            "status": status,
            "message": f"Stand {stand_id} status updated to {status}",
        }

    except (KeyError, TypeError, ValueError, OSError) as exc:
        logger.error("Failed to update stand %s status", stand_id, exc_info=exc)
        return {"success": False, "error": "action_failed"}


async def _handle_team_assign_flight(team_id: str, params: dict[str, Any]) -> dict[str, Any]:
    """Team.assign_flight Handler"""
    try:
        flight_id = params.get("flight_id")
        task_type = params.get("task_type", "general")

        if not flight_id:
            return {"success": False, "error": "flight_id is required"}

        logger.info(f"Team {team_id} assigned to flight {flight_id} (task: {task_type})")

        return {
            "success": True,
            "team_id": team_id,
            "flight_id": flight_id,
            "task_type": task_type,
            "message": f"Team {team_id} assigned to flight {flight_id}",
        }

    except (KeyError, TypeError, ValueError, OSError) as exc:
        logger.error("Failed to assign flight to team %s", team_id, exc_info=exc)
        return {"success": False, "error": "action_failed"}


async def _handle_team_update_status(team_id: str, params: dict[str, Any]) -> dict[str, Any]:
    """Team.update_status Handler"""
    try:
        status = params.get("status")
        if not status:
            return {"success": False, "error": "status is required"}

        logger.info(f"Team {team_id} status updated to {status}")

        return {
            "success": True,
            "team_id": team_id,
            "status": status,
            "message": f"Team {team_id} status updated to {status}",
        }

    except (KeyError, TypeError, ValueError, OSError) as exc:
        logger.error("Failed to update team %s status", team_id, exc_info=exc)
        return {"success": False, "error": "action_failed"}


async def _handle_team_change_location(team_id: str, params: dict[str, Any]) -> dict[str, Any]:
    """Team.change_location Handler"""
    try:
        new_location = params.get("new_location")
        if not new_location:
            return {"success": False, "error": "new_location is required"}

        logger.info(f"Team {team_id} location changed to {new_location}")

        return {
            "success": True,
            "team_id": team_id,
            "location": new_location,
            "message": f"Team {team_id} location updated to {new_location}",
        }

    except (KeyError, TypeError, ValueError, OSError) as exc:
        logger.error("Failed to update team %s location", team_id, exc_info=exc)
        return {"success": False, "error": "action_failed"}


async def _handle_anomaly_acknowledge(anomaly_id: str, params: dict[str, Any]) -> dict[str, Any]:
    """Anomaly.acknowledge Handler"""
    try:
        from src.application.services.anomaly.anomaly_detection_service import AnomalyDetectionService

        service = AnomalyDetectionService()
        acknowledged_by = params.get("acknowledged_by", "AI")

        await service.acknowledge_anomaly(anomaly_id=anomaly_id, acknowledged_by=acknowledged_by)

        return {
            "success": True,
            "anomaly_id": anomaly_id,
            "status": "acknowledged",
            "acknowledged_by": acknowledged_by,
            "message": f"Anomaly {anomaly_id} acknowledged",
        }

    except ImportError:
        logger.error("AnomalyDetectionService not available")
        raise RuntimeError("AnomalyDetectionService not available") from None
    except Exception as exc:
        logger.error("Failed to acknowledge anomaly %s", anomaly_id, exc_info=exc)
        return {"success": False, "error": "action_failed"}


async def _handle_anomaly_assign_team(anomaly_id: str, params: dict[str, Any]) -> dict[str, Any]:
    """Anomaly.assign_team Handler"""
    try:
        team_id = params.get("team_id")
        if not team_id:
            return {"success": False, "error": "team_id is required"}

        logger.info(f"Anomaly {anomaly_id} assigned to team {team_id}")

        return {
            "success": True,
            "anomaly_id": anomaly_id,
            "team_id": team_id,
            "message": f"Anomaly {anomaly_id} assigned to team {team_id}",
        }

    except (KeyError, TypeError, ValueError, OSError) as exc:
        logger.error("Failed to assign team to anomaly %s", anomaly_id, exc_info=exc)
        return {"success": False, "error": "action_failed"}


async def _handle_anomaly_resolve(anomaly_id: str, params: dict[str, Any]) -> dict[str, Any]:
    """Anomaly.resolve Handler"""
    try:
        resolution_notes = params.get("resolution_notes", "")
        resolved_by = params.get("resolved_by", "AI")

        logger.info(f"Anomaly {anomaly_id} resolved by {resolved_by}")

        return {
            "success": True,
            "anomaly_id": anomaly_id,
            "status": "resolved",
            "resolved_by": resolved_by,
            "resolution_notes": resolution_notes,
            "message": f"Anomaly {anomaly_id} resolved",
        }

    except (KeyError, TypeError, ValueError, OSError) as exc:
        logger.error("Failed to resolve anomaly %s", anomaly_id, exc_info=exc)
        return {"success": False, "error": "action_failed"}


async def _handle_anomaly_escalate(anomaly_id: str, params: dict[str, Any]) -> dict[str, Any]:
    """Anomaly.escalate Handler"""
    try:
        escalation_reason = params.get("escalation_reason")
        escalate_to = params.get("escalate_to", "supervisor")

        if not escalation_reason:
            return {"success": False, "error": "escalation_reason is required"}

        logger.info(f"Anomaly {anomaly_id} escalated to {escalate_to}: {escalation_reason}")

        return {
            "success": True,
            "anomaly_id": anomaly_id,
            "status": "escalated",
            "escalation_reason": escalation_reason,
            "escalate_to": escalate_to,
            "message": f"Anomaly {anomaly_id} escalated",
        }

    except (KeyError, TypeError, ValueError, OSError) as exc:
        logger.error("Failed to escalate anomaly %s", anomaly_id, exc_info=exc)
        return {"success": False, "error": "action_failed"}


async def _handle_todo_create(todo_id: str, params: dict[str, Any]) -> dict[str, Any]:
    """Todo.create Handler"""
    try:
        from src.application.services.async_todo_service import AsyncTodoService

        service = AsyncTodoService()
        title = params.get("title")
        description = params.get("description", "")
        priority = params.get("priority", "medium")
        assignee_id = params.get("assignee_id")
        due_date = params.get("due_date")

        if not title:
            return {"success": False, "error": "title is required"}

        result = await service.create_todo(
            title=title, description=description, priority=priority, assignee_id=assignee_id, due_date=due_date
        )

        return {
            "success": True,
            "todo_id": result.get("id", todo_id),
            "title": title,
            "status": "pending",
            "message": f"Todo '{title}' created",
        }

    except ImportError:
        logger.error("AsyncTodoService not available")
        raise RuntimeError("AsyncTodoService not available") from None
    except Exception as exc:
        logger.error("Failed to create todo", exc_info=exc)
        return {"success": False, "error": "action_failed"}


async def _handle_todo_complete(todo_id: str, params: dict[str, Any]) -> dict[str, Any]:
    """Todo.complete Handler"""
    try:
        from src.application.services.async_todo_service import AsyncTodoService

        service = AsyncTodoService()
        completion_notes = params.get("completion_notes", "")

        await service.complete_todo(todo_id=todo_id, completion_notes=completion_notes)

        return {
            "success": True,
            "todo_id": todo_id,
            "status": "completed",
            "message": f"Todo {todo_id} marked as completed",
        }

    except Exception as exc:
        logger.error("Failed to complete todo %s", todo_id, exc_info=exc)
        return {"success": False, "error": "action_failed"}


async def _handle_todo_assign(todo_id: str, params: dict[str, Any]) -> dict[str, Any]:
    """Todo.assign Handler"""
    try:
        assignee_id = params.get("assignee_id")
        if not assignee_id:
            return {"success": False, "error": "assignee_id is required"}

        logger.info(f"Todo {todo_id} assigned to user {assignee_id}")

        return {
            "success": True,
            "todo_id": todo_id,
            "assignee_id": assignee_id,
            "message": f"Todo {todo_id} assigned to {assignee_id}",
        }

    except (KeyError, TypeError, ValueError, OSError) as exc:
        logger.error("Failed to assign todo %s", todo_id, exc_info=exc)
        return {"success": False, "error": "action_failed"}


def register_all_handlers(app: Any) -> None:
    """
    注册所有 Action Handlers 到 AIPApplication

    Args:
        app: AIPApplication 实例
    """
    registry = get_handler_registry()

    handlers = {
        "Flight.change_stand": _handle_flight_change_stand,
        "Flight.delay_flight": _handle_flight_delay_flight,
        "Flight.assign_team": _handle_flight_assign_team,
        "Flight.update_status": _handle_flight_update_status,
        "Flight.mark_arrived": _handle_flight_mark_arrived,
        "Flight.mark_departed": _handle_flight_mark_departed,
        "Stand.occupy": _handle_stand_occupy,
        "Stand.release": _handle_stand_release,
        "Stand.reserve": _handle_stand_reserve,
        "Stand.close": _handle_stand_close,
        "Stand.update_status": _handle_stand_update_status,
        "Team.assign_flight": _handle_team_assign_flight,
        "Team.update_status": _handle_team_update_status,
        "Team.change_location": _handle_team_change_location,
        "Anomaly.acknowledge": _handle_anomaly_acknowledge,
        "Anomaly.assign_team": _handle_anomaly_assign_team,
        "Anomaly.resolve": _handle_anomaly_resolve,
        "Anomaly.escalate": _handle_anomaly_escalate,
        "Todo.create": _handle_todo_create,
        "Todo.complete": _handle_todo_complete,
        "Todo.assign": _handle_todo_assign,
    }

    for action_key, handler in handlers.items():
        object_type, action = action_key.split(".", 1)
        registry.register(object_type, action, handler)
        if hasattr(app, "register_action_handler"):
            app.register_action_handler(object_type, action, handler)

    registry.mark_initialized()
    logger.info(f"Registered {len(handlers)} action handlers")


def get_registered_handlers_count() -> int:
    """获取已注册的 Handler 数量"""
    return len(get_handler_registry().all_handlers)


__all__ = [
    "ActionHandlerRegistry",
    "get_handler_registry",
    "get_registered_handlers_count",
    "register_all_handlers",
]
