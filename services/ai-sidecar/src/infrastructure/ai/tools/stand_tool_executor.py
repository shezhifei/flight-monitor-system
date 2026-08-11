"""机位工具执行器。"""

from typing import Any

from .base import BaseToolExecutor, ToolCategory, ToolExecutionError, ToolExecutionStatus
from .stand_tools import StandToolName


class StandToolExecutor(BaseToolExecutor):
    """将机位工具调用路由到 stand repository。"""

    def __init__(self, stand_repository: Any = None, default_user: str = "AI_Assistant"):
        super().__init__(default_user)
        self._repo = stand_repository

    def _register_handlers(self) -> None:
        self._handlers = {
            StandToolName.LIST_STANDS.value: self._handle_list_stands,
            StandToolName.GET_STAND_DETAILS.value: self._handle_get_stand_details,
        }

    def get_category(self) -> ToolCategory:
        return ToolCategory.STAND

    async def _handle_list_stands(self, args: dict[str, Any]) -> dict[str, Any]:
        self._ensure_service()
        items = await self._repo.find_all(
            terminal=args.get("terminal"),
            include_inactive=False,
        )
        return {
            "total": len(items),
            "items": [self._stand_to_dict(s) for s in items],
        }

    async def _handle_get_stand_details(self, args: dict[str, Any]) -> dict[str, Any]:
        self._ensure_service()
        stand_code = self._require_arg(args, "stand_code")
        item = await self._repo.find_by_code(stand_code)
        if not item:
            raise ToolExecutionError(
                f"未找到机位: {stand_code}",
                ToolExecutionStatus.NOT_FOUND,
            )
        return self._stand_to_dict(item)

    def _ensure_service(self) -> None:
        if not self._repo:
            raise ToolExecutionError(
                "机位服务未初始化",
                ToolExecutionStatus.INTERNAL_ERROR,
            )

    @staticmethod
    def _stand_to_dict(stand: Any) -> dict[str, Any]:
        return {
            "id": stand.id,
            "code": stand.code,
            "name": getattr(stand, "name", None),
            "terminal": getattr(stand, "terminal", None),
            "area": getattr(stand, "area", None),
            "stand_type": getattr(stand, "stand_type", None),
            "size_category": getattr(stand, "size_category", None),
            "is_active": getattr(stand, "is_active", True),
        }
