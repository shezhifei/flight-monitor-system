"""派工单查询工具定义。"""

from enum import StrEnum
from typing import Any

from .base import BaseToolDefinition, OperationLevel, ToolCategory, build_openai_tools


class DispatchQueryToolName(StrEnum):
    LIST_DISPATCH_ORDERS = "list_dispatch_orders"
    GET_DISPATCH_ORDER = "get_dispatch_order"
    GET_DISPATCH_BY_FLIGHT = "get_dispatch_by_flight"
    GET_DISPATCH_BY_TEAM = "get_dispatch_by_team"


DISPATCH_QUERY_TOOL_DEFINITIONS: list[BaseToolDefinition] = [
    BaseToolDefinition(
        name=DispatchQueryToolName.LIST_DISPATCH_ORDERS.value,
        description=(
            "查询派工单列表，支持按状态、部门筛选。"
            "适用场景：用户问'当前有多少派工单'、'进行中的派工'、'今天的派工情况'等。"
            "不适用：查某航班的派工请用 get_dispatch_by_flight。"
        ),
        parameters={
            "status": {
                "type": "string",
                "description": "派工单状态过滤：pending（待分配）| assigned（已分配）| in_progress（执行中）| completed（已完成）| cancelled（已取消）",
                "enum": ["pending", "assigned", "in_progress", "completed", "cancelled"],
            },
            "department": {
                "type": "string",
                "description": "部门/科室过滤",
            },
            "limit": {"type": "integer", "description": "返回最大数量，默认50"},
        },
        required_params=[],
        category=ToolCategory.DISPATCH_QUERY,
        operation_level=OperationLevel.READ,
    ),
    BaseToolDefinition(
        name=DispatchQueryToolName.GET_DISPATCH_ORDER.value,
        description=("查询单个派工单的详细信息，包括分配班组、执行状态、时间线。需提供派工单ID。"),
        parameters={
            "order_id": {
                "type": "string",
                "description": "派工单ID",
            },
        },
        required_params=["order_id"],
        category=ToolCategory.DISPATCH_QUERY,
        operation_level=OperationLevel.READ,
    ),
    BaseToolDefinition(
        name=DispatchQueryToolName.GET_DISPATCH_BY_FLIGHT.value,
        description=(
            "查询某航班的所有派工单。"
            "适用场景：用户问'谁在保障CA1234'、'这个航班的派工'、'哪个班组在保障这个航班'等。"
            "需提供航班ID（系统内部ID）。"
        ),
        parameters={
            "flight_id": {
                "type": "string",
                "description": "航班ID（系统内部ID）",
            },
        },
        required_params=["flight_id"],
        category=ToolCategory.DISPATCH_QUERY,
        operation_level=OperationLevel.READ,
    ),
    BaseToolDefinition(
        name=DispatchQueryToolName.GET_DISPATCH_BY_TEAM.value,
        description=("查询某班组的派工记录。适用场景：用户问'这个班组今天保障了几个'、'清洁一组的任务'等。"),
        parameters={
            "team_id": {
                "type": "string",
                "description": "班组ID",
            },
            "status": {
                "type": "string",
                "description": "状态过滤",
                "enum": ["pending", "assigned", "in_progress", "completed", "cancelled"],
            },
        },
        required_params=["team_id"],
        category=ToolCategory.DISPATCH_QUERY,
        operation_level=OperationLevel.READ,
    ),
]

DISPATCH_QUERY_TOOLS: list[dict[str, Any]] = build_openai_tools(DISPATCH_QUERY_TOOL_DEFINITIONS)


def get_dispatch_query_tools() -> list[dict[str, Any]]:
    return DISPATCH_QUERY_TOOLS
