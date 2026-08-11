"""设备工具执行器。"""

from typing import Any

from .base import BaseToolExecutor, ToolCategory, ToolExecutionError, ToolExecutionStatus
from .equipment_tools import EquipmentToolName


class EquipmentToolExecutor(BaseToolExecutor):
    """将设备工具调用路由到 equipment repository。"""

    def __init__(
        self,
        equipment_repository: Any = None,
        equipment_type_repository: Any = None,
        default_user: str = "AI_Assistant",
    ):
        super().__init__(default_user)
        self._equip_repo = equipment_repository
        self._type_repo = equipment_type_repository

    def _register_handlers(self) -> None:
        self._handlers = {
            EquipmentToolName.LIST_EQUIPMENT.value: self._handle_list_equipment,
            EquipmentToolName.GET_AVAILABLE_EQUIPMENT.value: self._handle_get_available,
            EquipmentToolName.LIST_EQUIPMENT_TYPES.value: self._handle_list_types,
        }

    def get_category(self) -> ToolCategory:
        return ToolCategory.EQUIPMENT

    async def _handle_list_equipment(self, args: dict[str, Any]) -> dict[str, Any]:
        self._ensure_service()
        items = await self._equip_repo.find_all(
            include_inactive=False,
            equipment_type_id=args.get("equipment_type_id"),
            terminal=args.get("terminal"),
            status=args.get("status"),
        )
        return {
            "total": len(items),
            "items": [self._equip_to_dict(e) for e in items],
        }

    async def _handle_get_available(self, args: dict[str, Any]) -> dict[str, Any]:
        self._ensure_service()
        items = await self._equip_repo.find_available_for_dispatch(
            equipment_type_id=args.get("equipment_type_id"),
            terminal=args.get("terminal"),
        )
        return {
            "total": len(items),
            "items": [self._equip_to_dict(e) for e in items],
        }

    async def _handle_list_types(self, args: dict[str, Any]) -> dict[str, Any]:
        if not self._type_repo:
            raise ToolExecutionError(
                "设备类型服务未初始化",
                ToolExecutionStatus.INTERNAL_ERROR,
            )
        types = await self._type_repo.find_all(include_inactive=False)
        return {
            "total": len(types),
            "items": [
                {
                    "id": t.id,
                    "name": t.name,
                    "code": getattr(t, "code", None),
                }
                for t in types
            ],
        }

    def _ensure_service(self) -> None:
        if not self._equip_repo:
            raise ToolExecutionError(
                "设备服务未初始化",
                ToolExecutionStatus.INTERNAL_ERROR,
            )

    @staticmethod
    def _equip_to_dict(equip: Any) -> dict[str, Any]:
        return {
            "id": equip.id,
            "code": getattr(equip, "code", None),
            "name": getattr(equip, "name", None),
            "status": equip.status.value if hasattr(equip.status, "value") else str(getattr(equip, "status", "")),
            "terminal": getattr(equip, "terminal", None),
            "equipment_type": getattr(equip, "equipment_type_name", None)
            or (equip.equipment_type.name if getattr(equip, "equipment_type", None) else None),
            "current_stand_id": getattr(equip, "current_stand_id", None),
        }
