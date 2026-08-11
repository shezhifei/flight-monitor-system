"""
EP-08: AI 调度操作性工具定义

提供 filter_flights / change_stand / notify_teams 三个操作性工具，
允许 AI 通过对话调用实际调度 API。
"""

from enum import StrEnum
from typing import Any

from .base import BaseToolDefinition, OperationLevel, ToolCategory, build_openai_tools


class DispatchCommandToolName(StrEnum):
    FILTER_FLIGHTS = "filter_flights"
    CHANGE_STAND = "change_stand"
    NOTIFY_TEAMS = "notify_teams"


DISPATCH_COMMAND_DEFINITIONS: list[BaseToolDefinition] = [
    BaseToolDefinition(
        name=DispatchCommandToolName.FILTER_FLIGHTS.value,
        description=(
            "按条件筛选航班列表。支持航班号模糊匹配、状态过滤、机位过滤、"
            "时间范围过滤。返回符合条件的航班 ID 和摘要列表。"
        ),
        parameters={
            "flight_number": {
                "type": "string",
                "description": "航班号关键词（模糊匹配），如 CA, MU5678",
            },
            "status": {
                "type": "string",
                "description": "航班状态过滤，如 delayed, scheduled, boarding, departed",
            },
            "stand_id": {
                "type": "string",
                "description": "机位 ID 过滤",
            },
            "limit": {
                "type": "integer",
                "description": "返回最大数量，默认 20",
            },
        },
        required_params=[],
        category=ToolCategory.FLIGHT,
        operation_level=OperationLevel.READ,
    ),
    BaseToolDefinition(
        name=DispatchCommandToolName.CHANGE_STAND.value,
        description=("修改航班的机位分配。此操作需要人工确认后才会执行。调用后会返回待审批的操作 ID，需用户确认。"),
        parameters={
            "flight_id": {
                "type": "string",
                "description": "要修改机位的航班 ID",
            },
            "new_stand_id": {
                "type": "string",
                "description": "新机位 ID",
            },
            "reason": {
                "type": "string",
                "description": "变更原因说明",
            },
        },
        required_params=["flight_id", "new_stand_id"],
        category=ToolCategory.FLIGHT,
        operation_level=OperationLevel.ASSISTED_WRITE,
        side_effect=True,
    ),
    BaseToolDefinition(
        name=DispatchCommandToolName.NOTIFY_TEAMS.value,
        description=("向保障班组发送通知消息。可指定特定班组或广播给所有相关班组。此操作需要人工确认后才会执行。"),
        parameters={
            "team_ids": {
                "type": "array",
                "items": {"type": "string"},
                "description": "目标班组 ID 列表，留空则广播",
            },
            "message": {
                "type": "string",
                "description": "通知消息内容",
            },
            "priority": {
                "type": "string",
                "description": "通知优先级: normal, high, urgent",
            },
            "flight_id": {
                "type": "string",
                "description": "关联航班 ID（可选）",
            },
        },
        required_params=["message"],
        category=ToolCategory.FLIGHT,
        operation_level=OperationLevel.ASSISTED_WRITE,
        side_effect=True,
    ),
]

DISPATCH_COMMAND_TOOLS: list[dict[str, Any]] = build_openai_tools(DISPATCH_COMMAND_DEFINITIONS)


def get_dispatch_command_tools() -> list[dict[str, Any]]:
    return DISPATCH_COMMAND_TOOLS


__all__ = [
    "DISPATCH_COMMAND_DEFINITIONS",
    "DISPATCH_COMMAND_TOOLS",
    "DispatchCommandToolName",
    "get_dispatch_command_tools",
]
