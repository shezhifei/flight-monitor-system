"""
航班AI工具定义

提供与OpenAI function calling兼容的航班查询工具schema定义。
这些工具允许AI通过自然语言查询航班信息（只读）。
"""

from enum import StrEnum
from typing import Any

from .base import BaseToolDefinition, OperationLevel, ToolCategory, build_openai_tools


class FlightToolName(StrEnum):
    """航班工具名称枚举"""

    GET_FLIGHT_DETAILS = "get_flight_details"
    SEARCH_FLIGHTS_BY_NUMBER = "search_flights_by_number"


# 航班工具定义列表
FLIGHT_TOOL_DEFINITIONS: list[BaseToolDefinition] = [
    BaseToolDefinition(
        name=FlightToolName.GET_FLIGHT_DETAILS.value,
        description=(
            "根据航班ID获取航班的完整详细信息，包括状态、时间、登机口、机位、保障进度等。"
            "适用场景：已知航班ID（系统内部ID），需要查看该航班的全量信息。"
            "不适用：如果只有航班号（如CA1234），请先用 search_flights_by_number 搜索获取ID。"
        ),
        parameters={"flight_id": {"type": "string", "description": "航班的唯一ID（系统内部ID，非航班号）"}},
        required_params=["flight_id"],
        category=ToolCategory.FLIGHT,
        operation_level=OperationLevel.READ,
    ),
    BaseToolDefinition(
        name=FlightToolName.SEARCH_FLIGHTS_BY_NUMBER.value,
        description=(
            "根据航班号搜索航班信息。输入航班号如 CA1234、MU5678 即可查询。"
            "适用场景：用户提到了具体航班号时优先使用此工具。"
            "不适用：多条件组合搜索请用 search_flights_advanced；查延误航班请用 get_delayed_flights。"
        ),
        parameters={"flight_number": {"type": "string", "description": "航班号，如 CA1234、MU5678"}},
        required_params=["flight_number"],
        category=ToolCategory.FLIGHT,
        operation_level=OperationLevel.READ,
    ),
]


# OpenAI格式的工具列表
FLIGHT_TOOLS: list[dict[str, Any]] = build_openai_tools(FLIGHT_TOOL_DEFINITIONS)


def get_flight_tools() -> list[dict[str, Any]]:
    """
    获取航班工具列表

    Returns:
        OpenAI格式的工具定义列表
    """
    return FLIGHT_TOOLS


__all__ = [
    "FLIGHT_TOOLS",
    "FLIGHT_TOOL_DEFINITIONS",
    "FlightToolName",
    "get_flight_tools",
]
