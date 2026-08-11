"""设备查询工具定义。"""

from enum import StrEnum
from typing import Any

from .base import BaseToolDefinition, OperationLevel, ToolCategory, build_openai_tools


class EquipmentToolName(StrEnum):
    LIST_EQUIPMENT = "list_equipment"
    GET_AVAILABLE_EQUIPMENT = "get_available_equipment"
    LIST_EQUIPMENT_TYPES = "list_equipment_types"


EQUIPMENT_TOOL_DEFINITIONS: list[BaseToolDefinition] = [
    BaseToolDefinition(
        name=EquipmentToolName.LIST_EQUIPMENT.value,
        description=(
            "查询设备/车辆列表，支持按状态、类型、航站楼筛选。"
            "适用场景：用户问'加油车有几辆'、'哪辆拖车在维修'、'设备列表'等。"
            "不适用：查可用设备请用 get_available_equipment 更高效。"
        ),
        parameters={
            "status": {
                "type": "string",
                "description": "设备状态过滤：available（可用）| in_use（使用中）| maintenance（维修中）| retired（报废）",
                "enum": ["available", "in_use", "maintenance", "retired"],
            },
            "equipment_type_id": {
                "type": "string",
                "description": "设备类型ID过滤",
            },
            "terminal": {
                "type": "string",
                "description": "航站楼过滤",
            },
        },
        required_params=[],
        category=ToolCategory.EQUIPMENT,
        operation_level=OperationLevel.READ,
    ),
    BaseToolDefinition(
        name=EquipmentToolName.GET_AVAILABLE_EQUIPMENT.value,
        description=(
            "查询当前可用（空闲）的设备/车辆列表。"
            "适用场景：用户问'加油车有空的吗'、'摆渡车够不够'、'有没有可用的客梯车'等。"
        ),
        parameters={
            "equipment_type_id": {
                "type": "string",
                "description": "设备类型ID过滤",
            },
            "terminal": {
                "type": "string",
                "description": "航站楼过滤",
            },
        },
        required_params=[],
        category=ToolCategory.EQUIPMENT,
        operation_level=OperationLevel.READ,
    ),
    BaseToolDefinition(
        name=EquipmentToolName.LIST_EQUIPMENT_TYPES.value,
        description=(
            "查询所有设备/车辆类型列表（如加油车、摆渡车、拖车、客梯车等）。"
            "适用场景：用户需要了解有哪些设备类型、获取类型ID时使用。"
        ),
        parameters={},
        required_params=[],
        category=ToolCategory.EQUIPMENT,
        operation_level=OperationLevel.READ,
    ),
]

EQUIPMENT_TOOLS: list[dict[str, Any]] = build_openai_tools(EQUIPMENT_TOOL_DEFINITIONS)


def get_equipment_tools() -> list[dict[str, Any]]:
    return EQUIPMENT_TOOLS
