"""
处置建议工具定义

提供与OpenAI function calling兼容的异常处置建议工具schema定义。
"""

from enum import StrEnum
from typing import Any

from .base import BaseToolDefinition, OperationLevel, ToolCategory, build_openai_tools


class AdvisorToolName(StrEnum):
    """建议工具名称枚举"""

    GET_HANDLING_RECOMMENDATION = "get_handling_recommendation"


ADVISOR_TOOL_DEFINITIONS: list[BaseToolDefinition] = [
    BaseToolDefinition(
        name=AdvisorToolName.GET_HANDLING_RECOMMENDATION.value,
        description="根据事件描述，结合规范文件和历史案例，提供处置建议。适用于突发异常事件的快速决策支持。",
        parameters={
            "incident_description": {
                "type": "string",
                "description": "事件描述（自然语言），例如：'旅客拒绝登机并情绪激动'",
            },
            "flight_id": {"type": "string", "description": "关联航班ID（可选，用于获取航班上下文）"},
            "urgency": {
                "type": "string",
                "description": "紧急程度",
                "enum": ["低", "中", "高", "紧急"],
                "default": "中",
            },
        },
        required_params=["incident_description"],
        category=ToolCategory.ADVISOR,
        operation_level=OperationLevel.READ,
    )
]

ADVISOR_TOOLS: list[dict[str, Any]] = build_openai_tools(ADVISOR_TOOL_DEFINITIONS)


def get_advisor_tools() -> list[dict[str, Any]]:
    return ADVISOR_TOOLS


__all__ = [
    "ADVISOR_TOOLS",
    "ADVISOR_TOOL_DEFINITIONS",
    "AdvisorToolName",
    "get_advisor_tools",
]
