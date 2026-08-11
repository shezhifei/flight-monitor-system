"""Natural-language query tool definitions."""

from enum import StrEnum
from typing import Any

from .base import BaseToolDefinition, OperationLevel, ToolCategory, build_openai_tools


class QueryToolName(StrEnum):
    QUERY = "QUERY"
    SEARCH_FLIGHTS_ADVANCED = "search_flights_advanced"
    COUNT_FLIGHTS_BY_STATUS = "count_flights_by_status"
    GET_DELAYED_FLIGHTS = "get_delayed_flights"
    GET_FLIGHTS_BY_TIME_RANGE = "get_flights_by_time_range"
    GET_ABNORMAL_FLIGHTS = "get_abnormal_flights"
    GET_TURNAROUND_STATS = "get_turnaround_stats"
    GENERATE_FLIGHT_HISTORY_REPORT = "generate_flight_history_report"
    GENERATE_FLIGHT_EVENT_JOURNEY = "generate_flight_event_journey"


QUERY_TOOL_DEFINITIONS: list[BaseToolDefinition] = [
    BaseToolDefinition(
        name=QueryToolName.QUERY.value,
        description=(
            "通用查询工具（仅在其他专用查询工具均不匹配时作为最后手段使用）。"
            "支持意图：search（搜索列表）、aggregate（聚合统计）、timeseries（时间趋势）、compare（对比分析）、detail（单条明细）。"
            "优先使用专用工具：查某航班用 get_flight_details/search_flights_by_number，查延误用 get_delayed_flights，"
            "查异常用 list_anomalies，按状态统计用 count_flights_by_status。"
        ),
        parameters={
            "intent": {
                "type": "string",
                "description": "查询意图类型",
                "enum": ["search", "aggregate", "timeseries", "compare", "detail"],
            },
            "dataset": {
                "type": "string",
                "description": "数据集名称，支持：flights|alerts|tasks|ops",
                "enum": ["flights", "alerts", "tasks", "ops"],
            },
            "time_range": {
                "type": "object",
                "description": "时间范围",
                "properties": {
                    "from": {"type": "string", "description": "起始时间，ISO8601格式"},
                    "to": {"type": "string", "description": "结束时间，ISO8601格式"},
                },
            },
            "filters": {"type": "object", "description": "过滤条件键值对"},
            "metrics": {"type": "array", "items": {"type": "string"}, "description": "度量指标列表"},
            "group_by": {"type": "array", "items": {"type": "string"}, "description": "分组字段列表"},
            "limit": {"type": "integer", "description": "返回最大数量，默认50"},
        },
        required_params=["intent"],
        category=ToolCategory.QUERY,
        operation_level=OperationLevel.READ,
    ),
    BaseToolDefinition(
        name=QueryToolName.SEARCH_FLIGHTS_ADVANCED.value,
        description=(
            "按多种条件组合搜索航班列表（支持状态、日期范围、航司、异常、延误等过滤）。"
            "适用场景：用户问'今天南航有哪些航班'、'延误超过30分钟的航班'、'异常航班列表'等。"
            "不适用：查某个具体航班号请用 search_flights_by_number；仅需统计数量请用 count_flights_by_status。"
        ),
        parameters={
            "status": {"type": "string", "description": "航班状态过滤，如 delayed, scheduled, boarding, departed"},
            "date": {"type": "string", "description": "指定日期 YYYY-MM-DD"},
            "date_from": {"type": "string", "description": "起始日期 YYYY-MM-DD"},
            "date_to": {"type": "string", "description": "结束日期 YYYY-MM-DD"},
            "airline_code": {"type": "string", "description": "航司代码，如 CA、MU、CZ"},
            "has_open_anomaly": {"type": "boolean", "description": "是否仅查询存在 open/acknowledged 异常的航班"},
            "delay_minutes_gt": {"type": "integer", "description": "延误分钟数下限"},
            "data_scope": {"type": "string", "description": "数据范围：auto | active | archive | all"},
            "sort_by": {
                "type": "string",
                "description": "排序字段：auto | scheduled_departure | delay_minutes | status | airline_code | archived_at",
            },
            "sort_order": {"type": "string", "description": "排序方向：auto | asc | desc"},
            "limit": {"type": "integer", "description": "返回最大数量"},
        },
        required_params=[],
        category=ToolCategory.QUERY,
        operation_level=OperationLevel.READ,
    ),
    BaseToolDefinition(
        name=QueryToolName.COUNT_FLIGHTS_BY_STATUS.value,
        description=(
            "按状态分组统计航班数量（如多少个延误、多少个已登机等）。"
            "适用场景：用户问'今天有几个延误'、'各状态航班数量'、'航班状态统计'等。"
            "不适用：需要航班明细列表请用 search_flights_advanced。"
        ),
        parameters={
            "date": {"type": "string", "description": "指定日期 YYYY-MM-DD"},
            "date_from": {"type": "string", "description": "起始日期 YYYY-MM-DD"},
            "date_to": {"type": "string", "description": "结束日期 YYYY-MM-DD"},
            "data_scope": {"type": "string", "description": "数据范围：auto | active | archive | all"},
        },
        required_params=[],
        category=ToolCategory.QUERY,
        operation_level=OperationLevel.READ,
    ),
    BaseToolDefinition(
        name=QueryToolName.GET_DELAYED_FLIGHTS.value,
        description=(
            "查询延误航班列表，可设定最小延误时间阈值。"
            "适用场景：用户问'有没有延误航班'、'延误超过1小时的航班'、'哪些航班晚点了'等。"
            "不适用：查询异常告警请用 list_anomalies；查询航班各状态数量请用 count_flights_by_status。"
        ),
        parameters={
            "min_delay_minutes": {"type": "integer", "description": "最小延误分钟数，不填则返回所有延误航班"},
            "date": {"type": "string", "description": "指定日期 YYYY-MM-DD"},
            "date_from": {"type": "string", "description": "起始日期 YYYY-MM-DD"},
            "date_to": {"type": "string", "description": "结束日期 YYYY-MM-DD"},
            "data_scope": {"type": "string", "description": "数据范围：auto | active | archive | all"},
            "sort_by": {
                "type": "string",
                "description": "排序字段：auto | scheduled_departure | delay_minutes | status | airline_code | archived_at",
            },
            "sort_order": {"type": "string", "description": "排序方向：auto | asc | desc"},
            "limit": {"type": "integer", "description": "返回最大数量"},
        },
        required_params=[],
        category=ToolCategory.QUERY,
        operation_level=OperationLevel.READ,
    ),
    BaseToolDefinition(
        name=QueryToolName.GET_FLIGHTS_BY_TIME_RANGE.value,
        description=(
            "按计划起飞时间范围查询航班列表。"
            "适用场景：用户问'下午2点到4点有哪些航班'、'未来2小时的航班'等。"
            "不适用：按航班号查找请用 search_flights_by_number；仅需延误航班请用 get_delayed_flights。"
        ),
        parameters={
            "start_time": {"type": "string", "description": "起始时间，ISO 8601格式"},
            "end_time": {"type": "string", "description": "结束时间，ISO 8601格式"},
            "data_scope": {"type": "string", "description": "数据范围：auto | active | archive | all"},
            "sort_by": {
                "type": "string",
                "description": "排序字段：auto | scheduled_departure | delay_minutes | status | airline_code | archived_at",
            },
            "sort_order": {"type": "string", "description": "排序方向：auto | asc | desc"},
            "limit": {"type": "integer", "description": "返回最大数量"},
        },
        required_params=["start_time", "end_time"],
        category=ToolCategory.QUERY,
        operation_level=OperationLevel.READ,
    ),
    BaseToolDefinition(
        name=QueryToolName.GET_ABNORMAL_FLIGHTS.value,
        description=(
            "查询存在 open/acknowledged 异常的航班列表，按日期筛选。"
            "适用场景：用户问'今天有没有异常航班'。"
            "区别于 list_anomalies：本工具查的是航班粒度异常摘要，list_anomalies 查的是异常事件明细。"
        ),
        parameters={
            "date": {"type": "string", "description": "指定日期 YYYY-MM-DD"},
            "date_from": {"type": "string", "description": "起始日期 YYYY-MM-DD"},
            "date_to": {"type": "string", "description": "结束日期 YYYY-MM-DD"},
            "data_scope": {"type": "string", "description": "数据范围：auto | active | archive | all"},
            "sort_by": {
                "type": "string",
                "description": "排序字段：auto | scheduled_departure | delay_minutes | status | airline_code | archived_at",
            },
            "sort_order": {"type": "string", "description": "排序方向：auto | asc | desc"},
            "limit": {"type": "integer", "description": "返回最大数量"},
        },
        required_params=[],
        category=ToolCategory.QUERY,
        operation_level=OperationLevel.READ,
    ),
    BaseToolDefinition(
        name=QueryToolName.GET_TURNAROUND_STATS.value,
        description=(
            "获取过站保障统计数据（过站时间、准点率等指标）。"
            "适用场景：用户问'今天的过站效率'、'过站统计'、'平均过站时间'等。"
        ),
        parameters={
            "date": {"type": "string", "description": "指定日期 YYYY-MM-DD"},
            "date_from": {"type": "string", "description": "起始日期 YYYY-MM-DD"},
            "date_to": {"type": "string", "description": "结束日期 YYYY-MM-DD"},
            "data_scope": {"type": "string", "description": "数据范围：auto | active | archive | all"},
        },
        required_params=[],
        category=ToolCategory.QUERY,
        operation_level=OperationLevel.READ,
    ),
    BaseToolDefinition(
        name=QueryToolName.GENERATE_FLIGHT_HISTORY_REPORT.value,
        description=(
            "基于AI生成航班历史报告，包含事件时间线和分析。"
            "适用场景：用户需要某航班的详细历史回顾报告时使用。"
            "需提供 flight_id 或 flight_number 至少一个。"
        ),
        parameters={
            "flight_id": {"type": "string", "description": "系统航班ID"},
            "flight_number": {"type": "string", "description": "航班号，如 MU5123"},
            "hours": {"type": "integer", "description": "回溯小时数，默认24"},
            "incident_type": {"type": "string", "description": "事件类型关键词（可选），如 延误、备降"},
        },
        required_params=[],
        category=ToolCategory.QUERY,
        operation_level=OperationLevel.READ,
    ),
    BaseToolDefinition(
        name=QueryToolName.GENERATE_FLIGHT_EVENT_JOURNEY.value,
        description=(
            "基于AI生成航班事件旅程图，展示航班从计划到执行的完整事件链。"
            "适用场景：用户需要可视化某航班的事件流程时使用。"
            "需提供 flight_id 或 flight_number 至少一个。"
        ),
        parameters={
            "flight_id": {"type": "string", "description": "系统航班ID"},
            "flight_number": {"type": "string", "description": "航班号，如 MU5123"},
            "hours": {"type": "integer", "description": "回溯小时数，默认24"},
        },
        required_params=[],
        category=ToolCategory.QUERY,
        operation_level=OperationLevel.READ,
    ),
]


QUERY_TOOLS: list[dict[str, Any]] = build_openai_tools(QUERY_TOOL_DEFINITIONS)


def get_query_tools() -> list[dict[str, Any]]:
    return QUERY_TOOLS
