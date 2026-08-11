"""
意图预路由器 (Intent Pre-Router)

在 LLM 调用之前，基于规则对用户输入进行轻量级意图分类，
动态裁剪工具集，减少 LLM 面对的工具数量，提高选择准确率并节省 token。
"""

import re
from typing import Any

from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class IntentCategory:
    """意图分类常量"""

    QUERY_FLIGHT = "query_flight"
    QUERY_ANOMALY = "query_anomaly"
    QUERY_STATS = "query_stats"
    DISPATCH_OPS = "dispatch_ops"
    TODO_MGMT = "todo_mgmt"
    REPORT = "report"
    ADVISOR = "advisor"
    BUSINESS_CASE = "business_case"
    QUERY_TEAM = "query_team"
    QUERY_EQUIPMENT = "query_equipment"
    QUERY_STAND = "query_stand"
    QUERY_DISPATCH = "query_dispatch"
    GENERAL = "general"


# 预编译正则
_FLIGHT_NUMBER_RE = re.compile(r"\b[A-Za-z]{2}\d{3,4}\b")

# 意图分类规则：(关键词列表, 意图类别)
# 按优先级排列，先匹配到的优先
_INTENT_RULES: list[tuple[list[str], str]] = [
    # 调度操作（高危操作优先识别）
    (
        ["改机位", "换机位", "变更机位", "更换机位", "调机位", "change_stand", "通知班组", "通知车队", "notify"],
        IntentCategory.DISPATCH_OPS,
    ),
    # 处置建议
    (
        ["怎么处理", "怎么办", "处置建议", "应急方案", "处理方案", "旅客拒绝", "旅客投诉", "recommendation", "建议"],
        IntentCategory.ADVISOR,
    ),
    # 报告生成
    (
        ["报告", "report", "事件报告", "历史报告", "事件旅程", "生成报告", "event journey"],
        IntentCategory.REPORT,
    ),
    # 异常/告警查询
    (
        [
            "异常",
            "告警",
            "冲突",
            "机位冲突",
            "anomaly",
            "anomalies",
            "KPI恶化",
            "派工问题",
            "gate_stand",
            "alert",
            "风险",
        ],
        IntentCategory.QUERY_ANOMALY,
    ),
    # 统计类查询
    (
        [
            "统计",
            "多少",
            "几个",
            "数量",
            "总共",
            "合计",
            "比例",
            "占比",
            "过站",
            "turnaround",
            "效率",
            "准点率",
            "count",
        ],
        IntentCategory.QUERY_STATS,
    ),
    # 航班查询
    (
        [
            "航班",
            "flight",
            "延误",
            "晚点",
            "delay",
            "起飞",
            "降落",
            "到达",
            "登机",
            "出发",
            "机位",
            "登机口",
            "stand",
            "gate",
            "CA",
            "MU",
            "CZ",
            "HU",
            "ZH",
            "FM",
            "SC",
            "3U",
            "8L",
        ],
        IntentCategory.QUERY_FLIGHT,
    ),
    # 待办事项
    (
        ["待办", "任务", "todo", "事项", "清单", "完成", "进度", "分配", "指派", "创建任务"],
        IntentCategory.TODO_MGMT,
    ),
    # 业务事项
    (
        ["业务事项", "business_case", "工作流", "审批"],
        IntentCategory.BUSINESS_CASE,
    ),
    # 班组查询
    (
        ["班组", "在岗", "班组长", "空闲班组", "车组", "清洁组", "加油组", "值班", "谁在", "哪个组", "班组成员"],
        IntentCategory.QUERY_TEAM,
    ),
    # 设备/车辆查询
    (
        ["设备", "加油车", "摆渡车", "拖车", "客梯车", "GPU", "污水车", "行李车", "登机梯", "电源车", "车辆"],
        IntentCategory.QUERY_EQUIPMENT,
    ),
    # 机位查询
    (
        ["空机位", "远机位", "近机位", "停机位", "空位", "机位列表", "哪些机位"],
        IntentCategory.QUERY_STAND,
    ),
    # 派工单查询
    (
        ["派工", "派工单", "保障谁", "谁在保障", "谁保障", "dispatch", "保障任务"],
        IntentCategory.QUERY_DISPATCH,
    ),
]

# 每个意图类别对应允许的工具名称集合
_INTENT_TOOL_MAP: dict[str, list[str]] = {
    IntentCategory.QUERY_FLIGHT: [
        "search_flights_by_number",
        "get_flight_details",
        "search_flights_advanced",
        "get_delayed_flights",
        "get_flights_by_time_range",
        "get_abnormal_flights",
        "QUERY",
        "filter_flights",
    ],
    IntentCategory.QUERY_ANOMALY: [
        "list_anomalies",
        "get_anomaly_detail",
        "get_anomaly_stats",
        # 异常可能需要关联航班信息
        "search_flights_by_number",
        "get_flight_details",
    ],
    IntentCategory.QUERY_STATS: [
        "count_flights_by_status",
        "get_turnaround_stats",
        "get_anomaly_stats",
        "get_todo_stats",
        "search_flights_advanced",
        "QUERY",
    ],
    IntentCategory.DISPATCH_OPS: [
        "filter_flights",
        "change_stand",
        "notify_teams",
        "search_flights_by_number",
        "get_flight_details",
        "list_anomalies",
    ],
    IntentCategory.TODO_MGMT: [
        "create_todo",
        "get_todo",
        "list_todos",
        "update_todo",
        "complete_todo",
        "cancel_todo",
        "assign_todo",
        "update_progress",
        "search_todos",
        "get_todo_stats",
        "spawn_subtodo",
    ],
    IntentCategory.REPORT: [
        "generate_incident_report",
        "generate_flight_history_report",
        "generate_flight_event_journey",
        "search_flights_by_number",
        "get_flight_details",
    ],
    IntentCategory.ADVISOR: [
        "get_handling_recommendation",
        "search_flights_by_number",
        "get_flight_details",
        "list_anomalies",
        "get_anomaly_detail",
    ],
    IntentCategory.BUSINESS_CASE: [
        "create_business_case",
        "list_business_cases",
        "get_business_case",
        "update_business_case",
        "search_flights_by_number",
        "get_flight_details",
    ],
    IntentCategory.QUERY_TEAM: [
        "list_teams",
        "get_team_details",
        "get_available_teams",
        "notify_teams",
    ],
    IntentCategory.QUERY_EQUIPMENT: [
        "list_equipment",
        "get_available_equipment",
        "list_equipment_types",
    ],
    IntentCategory.QUERY_STAND: [
        "list_stands",
        "get_stand_details",
        "filter_flights",
        "change_stand",
    ],
    IntentCategory.QUERY_DISPATCH: [
        "list_dispatch_orders",
        "get_dispatch_order",
        "get_dispatch_by_flight",
        "get_dispatch_by_team",
        "search_flights_by_number",
        "get_flight_details",
    ],
}


def classify_intent(user_input: str) -> str:
    """
    基于关键词规则对用户输入进行意图分类。

    Args:
        user_input: 用户输入文本

    Returns:
        意图分类字符串
    """
    if not user_input or user_input.isspace():
        return IntentCategory.GENERAL

    text = user_input.lower().strip()

    # 检测航班号模式（如 CA1234, MU5678, CZ3022）
    if _FLIGHT_NUMBER_RE.search(user_input):
        return IntentCategory.QUERY_FLIGHT

    for keywords, category in _INTENT_RULES:
        for keyword in keywords:
            if keyword in text:
                return category

    return IntentCategory.GENERAL


def filter_tools_by_intent(
    intent: str,
    all_tools: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    """
    根据意图分类裁剪工具集。

    如果意图为 GENERAL 或无法识别，返回全量工具。
    否则只返回该意图关联的工具子集。

    Args:
        intent: 意图分类
        all_tools: 全量工具列表（OpenAI schema格式）

    Returns:
        裁剪后的工具列表
    """
    if intent == IntentCategory.GENERAL:
        return all_tools

    allowed_names = set(_INTENT_TOOL_MAP.get(intent, []))
    if not allowed_names:
        return all_tools

    filtered = [tool for tool in all_tools if tool.get("function", {}).get("name", "") in allowed_names]

    # 安全保障：如果过滤后工具太少（<2），回退到全量
    if len(filtered) < 2:
        logger.warning(f"Intent '{intent}' filtered to {len(filtered)} tools, falling back to full tool set")
        return all_tools

    logger.debug(f"Intent routing: '{intent}' → {len(filtered)}/{len(all_tools)} tools")
    return filtered


def route_tools(
    user_input: str,
    all_tools: list[dict[str, Any]],
) -> tuple[str, list[dict[str, Any]]]:
    """
    一步完成意图分类 + 工具裁剪。

    Args:
        user_input: 用户输入文本
        all_tools: 全量工具列表

    Returns:
        (intent, filtered_tools) 元组
    """
    intent = classify_intent(user_input)
    filtered = filter_tools_by_intent(intent, all_tools)
    return intent, filtered
