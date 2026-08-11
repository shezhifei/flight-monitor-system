"""
报告生成工具定义

提供与OpenAI function calling兼容的报告生成工具schema定义。
"""

from enum import StrEnum
from typing import Any

from .base import BaseToolDefinition, OperationLevel, ToolCategory, build_openai_tools


class ReportToolName(StrEnum):
    """报告工具名称枚举"""

    GENERATE_INCIDENT_REPORT = "generate_incident_report"


REPORT_TOOL_DEFINITIONS: list[BaseToolDefinition] = [
    BaseToolDefinition(
        name=ReportToolName.GENERATE_INCIDENT_REPORT.value,
        description="根据航班ID生成异常事件报告，包含时间线、备注摘要和处置建议。用于事后分析和归档。",
        parameters={
            "flight_id": {"type": "string", "description": "航班ID（系统内部ID）"},
            "incident_type": {
                "type": "string",
                "description": "事件类型",
                "enum": ["延误", "返航", "备降", "机械故障", "旅客事件", "其他"],
            },
            "time_range_hours": {"type": "integer", "description": "回溯时间范围（小时），默认24小时", "default": 24},
            "include_remarks": {"type": "boolean", "description": "是否包含备注信息", "default": True},
        },
        required_params=["flight_id"],
        category=ToolCategory.REPORT,
        operation_level=OperationLevel.WORKSPACE_WRITE,
    )
]

REPORT_TOOLS: list[dict[str, Any]] = build_openai_tools(REPORT_TOOL_DEFINITIONS)


def get_report_tools() -> list[dict[str, Any]]:
    return REPORT_TOOLS


__all__ = [
    "REPORT_TOOLS",
    "REPORT_TOOL_DEFINITIONS",
    "ReportToolName",
    "get_report_tools",
]
