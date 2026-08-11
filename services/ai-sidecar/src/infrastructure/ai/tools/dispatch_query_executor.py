"""派工单查询工具执行器。"""

from typing import Any

from .base import BaseToolExecutor, ToolCategory, ToolExecutionError, ToolExecutionStatus
from .dispatch_query_tools import DispatchQueryToolName


class DispatchQueryExecutor(BaseToolExecutor):
    """将派工单查询工具调用路由到 dispatch_order repository。"""

    def __init__(self, dispatch_order_repository: Any = None, default_user: str = "AI_Assistant"):
        super().__init__(default_user)
        self._repo = dispatch_order_repository

    def _register_handlers(self) -> None:
        self._handlers = {
            DispatchQueryToolName.LIST_DISPATCH_ORDERS.value: self._handle_list,
            DispatchQueryToolName.GET_DISPATCH_ORDER.value: self._handle_get,
            DispatchQueryToolName.GET_DISPATCH_BY_FLIGHT.value: self._handle_by_flight,
            DispatchQueryToolName.GET_DISPATCH_BY_TEAM.value: self._handle_by_team,
        }

    def get_category(self) -> ToolCategory:
        return ToolCategory.DISPATCH_QUERY

    async def _handle_list(self, args: dict[str, Any]) -> dict[str, Any]:
        self._ensure_service()
        limit = int(args.get("limit", 50) or 50)
        items = await self._repo.find_all(
            status=args.get("status"),
            department=args.get("department"),
            limit=max(1, min(limit, 200)),
        )
        return {
            "total": len(items),
            "items": [self._order_to_dict(o) for o in items],
        }

    async def _handle_get(self, args: dict[str, Any]) -> dict[str, Any]:
        self._ensure_service()
        order_id = self._require_arg(args, "order_id")
        item = await self._repo.find_by_id(order_id)
        if not item:
            raise ToolExecutionError(
                f"未找到派工单: {order_id}",
                ToolExecutionStatus.NOT_FOUND,
            )
        return self._order_to_dict(item)

    async def _handle_by_flight(self, args: dict[str, Any]) -> dict[str, Any]:
        self._ensure_service()
        flight_id = self._require_arg(args, "flight_id")
        items = await self._repo.find_by_flight(flight_id)
        return {
            "total": len(items),
            "items": [self._order_to_dict(o) for o in items],
        }

    async def _handle_by_team(self, args: dict[str, Any]) -> dict[str, Any]:
        self._ensure_service()
        team_id = self._require_arg(args, "team_id")
        items = await self._repo.find_by_team(
            team_id,
            status=args.get("status"),
        )
        return {
            "total": len(items),
            "items": [self._order_to_dict(o) for o in items],
        }

    def _ensure_service(self) -> None:
        if not self._repo:
            raise ToolExecutionError(
                "派工服务未初始化",
                ToolExecutionStatus.INTERNAL_ERROR,
            )

    @staticmethod
    def _order_to_dict(order: Any) -> dict[str, Any]:
        return {
            "id": order.id,
            "flight_id": getattr(order, "flight_id", None),
            "task_type": getattr(order, "task_type", None),
            "task_type_name": getattr(order, "task_type_name", None),
            "status": order.status.value if hasattr(order.status, "value") else str(order.status),
            "dispatch_type": getattr(order, "dispatch_type", None),
            "team_id": getattr(order, "team_id", None),
            "team_name": getattr(order, "team_name", None),
            "planned_start": order.planned_start_time.isoformat()
            if getattr(order, "planned_start_time", None)
            else None,
            "planned_end": order.planned_end_time.isoformat() if getattr(order, "planned_end_time", None) else None,
            "actual_start": order.actual_start_time.isoformat() if getattr(order, "actual_start_time", None) else None,
            "actual_end": order.actual_end_time.isoformat() if getattr(order, "actual_end_time", None) else None,
            "notes": getattr(order, "notes", None),
        }
