"""Anomaly-related AI tool definitions."""

from enum import StrEnum
from typing import Any

from .base import BaseToolDefinition, OperationLevel, ToolCategory, build_openai_tools


class AnomalyToolName(StrEnum):
    LIST_ANOMALIES = "list_anomalies"
    GET_ANOMALY_DETAIL = "get_anomaly_detail"
    GET_ANOMALY_STATS = "get_anomaly_stats"


ANOMALY_TOOL_DEFINITIONS: list[BaseToolDefinition] = [
    BaseToolDefinition(
        name=AnomalyToolName.LIST_ANOMALIES.value,
        description=(
            "列出当前系统中的异常告警（如机位冲突、KPI恶化、派工问题、AI风险等）。"
            "适用场景：用户问'有什么异常'、'当前告警'、'机位冲突有几个'等。"
            "不适用：查询航班信息请用 get_flight_details 或 search_flights_by_number；"
            "查询延误航班请用 get_delayed_flights。"
        ),
        parameters={
            "status": {
                "type": "string",
                "description": "异常状态过滤：open（未处理）| acknowledged（已确认）| resolved（已解决）",
                "enum": ["open", "acknowledged", "resolved"],
            },
            "anomaly_type": {
                "type": "string",
                "description": (
                    "异常类型过滤：service_node_timeout（服务节点超时）| gate_stand_conflict（机位冲突）"
                    "| kpi_degradation（KPI恶化）| ai_risk（AI风险）| dispatch_issue（派工问题）"
                ),
                "enum": ["service_node_timeout", "gate_stand_conflict", "kpi_degradation", "ai_risk", "dispatch_issue"],
            },
            "limit": {"type": "integer", "description": "返回最大数量，默认50"},
        },
        required_params=[],
        category=ToolCategory.ANOMALY,
        operation_level=OperationLevel.READ,
    ),
    BaseToolDefinition(
        name=AnomalyToolName.GET_ANOMALY_DETAIL.value,
        description=(
            "根据异常ID获取异常告警的详细信息，包括关联航班、严重程度、上下文数据等。"
            "适用场景：用户需要查看某条具体异常的完整信息时使用。"
            "前提：需要先通过 list_anomalies 获取到异常ID。"
        ),
        parameters={
            "anomaly_id": {"type": "string", "description": "异常告警的唯一ID"},
        },
        required_params=["anomaly_id"],
        category=ToolCategory.ANOMALY,
        operation_level=OperationLevel.READ,
    ),
    BaseToolDefinition(
        name=AnomalyToolName.GET_ANOMALY_STATS.value,
        description=(
            "获取异常告警的统计概览（按类型和状态分类的数量汇总）。"
            "适用场景：用户问'异常有多少'、'告警统计'、'今天的异常概况'等。"
            "不适用：需要异常明细列表请用 list_anomalies。"
        ),
        parameters={},
        required_params=[],
        category=ToolCategory.ANOMALY,
        operation_level=OperationLevel.READ,
    ),
]


ANOMALY_TOOLS: list[dict[str, Any]] = build_openai_tools(ANOMALY_TOOL_DEFINITIONS)


def get_anomaly_tools() -> list[dict[str, Any]]:
    return ANOMALY_TOOLS
