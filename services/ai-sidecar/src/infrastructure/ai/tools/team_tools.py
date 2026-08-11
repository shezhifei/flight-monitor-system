"""班组查询工具定义。"""

from enum import StrEnum
from typing import Any

from .base import BaseToolDefinition, OperationLevel, ToolCategory, build_openai_tools


class TeamToolName(StrEnum):
    LIST_TEAMS = "list_teams"
    GET_TEAM_DETAILS = "get_team_details"
    GET_AVAILABLE_TEAMS = "get_available_teams"


TEAM_TOOL_DEFINITIONS: list[BaseToolDefinition] = [
    BaseToolDefinition(
        name=TeamToolName.LIST_TEAMS.value,
        description=(
            "查询班组列表，支持按状态、类型、航站楼筛选。"
            "适用场景：用户问'哪些班组在岗'、'今天有多少班组'、'休息中的班组'等。"
            "不适用：查可派工班组请用 get_available_teams。"
        ),
        parameters={
            "status": {
                "type": "string",
                "description": "班组状态过滤：on_duty（在岗）| off_duty（下班）| break（休息中）",
                "enum": ["on_duty", "off_duty", "break"],
            },
            "team_type_id": {
                "type": "string",
                "description": "班组类型ID过滤",
            },
            "terminal": {
                "type": "string",
                "description": "航站楼过滤，如 T1、T2",
            },
        },
        required_params=[],
        category=ToolCategory.TEAM,
        operation_level=OperationLevel.READ,
    ),
    BaseToolDefinition(
        name=TeamToolName.GET_TEAM_DETAILS.value,
        description=(
            "查询单个班组的详细信息，包括成员列表、班组长、当前位置等。"
            "适用场景：用户问'这个班组有几个人'、'班组长是谁'等。"
            "前提：需先通过 list_teams 或 get_available_teams 获取班组ID或名称。"
        ),
        parameters={
            "team_id": {
                "type": "string",
                "description": "班组ID",
            },
            "team_name": {
                "type": "string",
                "description": "班组名称（模糊匹配，与 team_id 二选一）",
            },
        },
        required_params=[],
        category=ToolCategory.TEAM,
        operation_level=OperationLevel.READ,
    ),
    BaseToolDefinition(
        name=TeamToolName.GET_AVAILABLE_TEAMS.value,
        description=(
            "查询当前可派工（空闲）的班组列表。"
            "适用场景：用户问'有没有空闲班组'、'哪个班组能去保障'、'加油班组还有空闲的吗'等。"
        ),
        parameters={
            "team_type_id": {
                "type": "string",
                "description": "班组类型ID过滤，如加油组、清洁组的类型ID",
            },
            "terminal": {
                "type": "string",
                "description": "航站楼过滤",
            },
        },
        required_params=[],
        category=ToolCategory.TEAM,
        operation_level=OperationLevel.READ,
    ),
]

TEAM_TOOLS: list[dict[str, Any]] = build_openai_tools(TEAM_TOOL_DEFINITIONS)


def get_team_tools() -> list[dict[str, Any]]:
    return TEAM_TOOLS
