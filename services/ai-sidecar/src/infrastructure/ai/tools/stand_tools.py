"""机位查询工具定义。"""

from enum import StrEnum
from typing import Any

from .base import BaseToolDefinition, OperationLevel, ToolCategory, build_openai_tools


class StandToolName(StrEnum):
    LIST_STANDS = "list_stands"
    GET_STAND_DETAILS = "get_stand_details"


STAND_TOOL_DEFINITIONS: list[BaseToolDefinition] = [
    BaseToolDefinition(
        name=StandToolName.LIST_STANDS.value,
        description=("查询机位/停机位列表，支持按航站楼筛选。适用场景：用户问'有哪些机位'、'T2的机位列表'等。"),
        parameters={
            "terminal": {
                "type": "string",
                "description": "航站楼过滤，如 T1、T2",
            },
        },
        required_params=[],
        category=ToolCategory.STAND,
        operation_level=OperationLevel.READ,
    ),
    BaseToolDefinition(
        name=StandToolName.GET_STAND_DETAILS.value,
        description=(
            "根据机位编号查询机位详细信息（位置、类型、大小等级）。"
            "适用场景：用户问'A12机位什么情况'、'这个机位能停大飞机吗'等。"
        ),
        parameters={
            "stand_code": {
                "type": "string",
                "description": "机位编号，如 A12、B03",
            },
        },
        required_params=["stand_code"],
        category=ToolCategory.STAND,
        operation_level=OperationLevel.READ,
    ),
]

STAND_TOOLS: list[dict[str, Any]] = build_openai_tools(STAND_TOOL_DEFINITIONS)


def get_stand_tools() -> list[dict[str, Any]]:
    return STAND_TOOLS
