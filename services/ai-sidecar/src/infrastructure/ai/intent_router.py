"""
意图预路由器 (Intent Pre-Router)

在 LLM 调用之前，基于规则对用户输入进行轻量级意图分类，
动态裁剪工具集，减少 LLM 面对的工具数量，提高选择准确率并节省 token。

**重要决策**: E4 - task_type/configuration 优先于关键词匹配
    - 当实体配置明确指定 task_type 时，直接使用（置信度 1.0）
    - 关键词路由仅作为 fail-open 降级路径（置信度 0.7）

**K2 收敛**: 意图路由降为粗滤。显式 task_type 已给出时，关键词
（如「机位」）一律不得改写分类 —— classify_intent / route_tools
接受 ``task_type`` 参数，显式值直接映射为粗类别，跳过全部关键词规则。
"""

import re
from dataclasses import dataclass, field
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
    
    # Task template aliases (maps to ops templates)
    TASK_TYPE_ALIAS = {
        "query_ops": QUERY_FLIGHT,
        "anomaly_ops": QUERY_ANOMALY,
        "dispatch_ops": DISPATCH_OPS,
    }


# 预编译正则
_FLIGHT_NUMBER_RE = re.compile(r"\b[A-Za-z]{2}\d{3,4}\b")

# K2: explicit task_type → coarse intent category. Anything the caller tells us
# explicitly is authoritative; keywords may only fill the gap when it is absent.
_TASK_TYPE_TO_INTENT: dict[str, str] = {
    "query": IntentCategory.QUERY_FLIGHT,
    "query_ops": IntentCategory.QUERY_FLIGHT,
    "anomaly": IntentCategory.QUERY_ANOMALY,
    "anomaly_ops": IntentCategory.QUERY_ANOMALY,
    "dispatch": IntentCategory.DISPATCH_OPS,
    "dispatch_ops": IntentCategory.DISPATCH_OPS,
}


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


# ============================================================================
# Task Type Based Routing (E4 Implementation)
# ============================================================================

@dataclass
class RouteDecision:
    """路由决策结果。"""
    
    template: str                        # ops template name ("query_ops", "dispatch_ops")
    confidence: float                    # 0.0-1.0 confidence score
    source: str                          # "entity_config" | "keyword_fallback" | "default"
    intent_category: str | None = None   # Legacy intent category (optional)
    filtered_tools: list[dict[str, Any]] | None = None
    
    def is_high_confidence(self) -> bool:
        """High confidence routing decisions."""
        return self.confidence >= 0.9


class IntentRouter:
    """意图路由器 - 支持配置优先和关键词降级策略。"""
    
    def __init__(self):
        self._keyword_router = KeywordRouter()
    
    async def route(
        self,
        user_query: str,
        envelope: Any | None = None,  # Can be dict, object, or None
        all_tools: list[dict[str, Any]] | None = None,
    ) -> RouteDecision:
        """
        路由决策：task_type 优先级高于关键词匹配。
        
        Args:
            user_query: User input text
            envelope: Optional context envelope (object with attributes or dict)
                     Can be None for keyword-only routing
            all_tools: Full tool set for filtering. Defaults to empty list.
            
        Returns:
            RouteDecision with template, confidence, and filtered tools
        """
        if all_tools is None:
            all_tools = []
        # Step 1: Extract task_type from entity configuration (highest priority)
        task_type = await self._extract_task_type(envelope)
        
        if task_type:
            logger.info(f"[E4] Using configured task_type: {task_type} (confidence=1.0)")
            
            # Map to ops template
            template = f"{task_type}_ops"
            filtered_tools = self._filter_by_template(template, all_tools)
            
            return RouteDecision(
                template=template,
                confidence=1.0,
                source="entity_config",
                filtered_tools=filtered_tools,
            )
        
        # Step 2: Fallback to keyword-based classification (fail-open)
        intent_category = self._keyword_router.classify(user_query)
        
        if intent_category != IntentCategory.GENERAL:
            logger.debug(f"[E4] Keyword fallback: {intent_category} (confidence=0.7)")
            template = f"{intent_category}_ops"
            filtered_tools = self._filter_by_template(template, all_tools)
            
            return RouteDecision(
                template=template,
                confidence=0.7,
                source="keyword_fallback",
                intent_category=intent_category,
                filtered_tools=filtered_tools,
            )
        
        # Step 3: Default - no filtering
        logger.debug("[E4] Default routing: GENERAL (confidence=0.0)")
        return RouteDecision(
            template="general_ops",
            confidence=0.0,
            source="default",
            filtered_tools=all_tools,
        )
    
    async def _extract_task_type(self, envelope: Any | None) -> str | None:
        """Extract task_type from envelope or entity config."""
        # Check envelope.task.task_type first
        if hasattr(envelope, 'task') and envelope.task:
            task_type = getattr(envelope.task, 'task_type', None)
            if task_type:
                return task_type
        
        # Fallback: extract from entity_id pattern (last resort)
        # e.g., "dispatch_opt_001" → "dispatch"
        entity_id = getattr(envelope, 'entity_id', '')
        
        # Pattern matching for entity types
        patterns = {
            'dispatch': ['dispatch_opt', 'dispatch_optimizer'],
            'anomaly': ['anomaly_triage', 'anomaly_investigator'],
            'query': ['flight_query', 'status_checker'],
        }
        
        for task_prefix, aliases in patterns.items():
            if any(alias in entity_id.lower() for alias in aliases):
                logger.warning(f"[E4] Inferring task_type={task_prefix} from entity_id (low reliability)")
                return task_prefix
        
        return None
    
    def _filter_by_template(
        self, 
        template: str, 
        all_tools: list[dict[str, Any]],
    ) -> list[dict[str, Any]]:
        """Filter tools based on ops template requirements."""
        
        # Define tool sets per template
        TEMPLATE_TOOLS = {
            "query_ops": ["QUERY"],  # Read-only only
            "anomaly_ops": ["QUERY", "list_anomalies", "get_anomaly_detail"],
            "dispatch_ops": [
                "QUERY", "list_anomalies", 
                "change_stand", "notify_teams",  # Write actions require proposal_only
            ],
            "general_ops": [],  # No filtering - return all tools
        }
        
        allowed_names = set(TEMPLATE_TOOLS.get(template, []))
        if not allowed_names:
            # Empty list means 'no filtering' for general_ops
            if template == "general_ops":
                return all_tools
            return all_tools
        
        filtered = [
            tool for tool in all_tools 
            if tool.get("function", {}).get("name", "") in allowed_names
        ]
        
        # Safety check: fall back to full set if too few tools
        if len(filtered) < 2:
            logger.warning(
                f"Template '{template}' filtered to {len(filtered)} tools, "
                f"falling back to full tool set"
            )
            return all_tools
        
        logger.debug(f"[E4] Template '{template}' → {len(filtered)}/{len(all_tools)} tools")
        return filtered


class KeywordRouter:
    """Legacy keyword-based intent classifier (used as fallback)."""
    
    def classify(self, user_input: str) -> str:
        """Classify intent using keyword rules."""
        if not user_input or user_input.isspace():
            return IntentCategory.GENERAL
        
        text = user_input.lower().strip()
        
        # Flight number detection
        if _FLIGHT_NUMBER_RE.search(user_input):
            return IntentCategory.QUERY_FLIGHT
        
        # Match against rules (existing logic)
        for keywords, category in _INTENT_RULES:
            for keyword in keywords:
                if keyword in text:
                    return category
        
        return IntentCategory.GENERAL


def classify_intent(user_input: str, *, task_type: str | None = None) -> str:
    """
    基于关键词规则对用户输入进行意图分类（K2 起仅为粗滤）。

    Args:
        user_input: User input text
        task_type: Explicit task_type from the envelope/entity config. When
            given, keyword rules are skipped entirely so a stray keyword
            (e.g. 「机位」) can never reroute an explicitly-typed run.

    Returns:
        Intent classification string
    """
    if task_type:
        normalized = str(task_type).strip().lower()
        coarse = _TASK_TYPE_TO_INTENT.get(normalized)
        if coarse is not None:
            logger.debug("[K2] Explicit task_type=%s → coarse intent %s (keywords skipped)", task_type, coarse)
            return coarse
        # Explicit but unmapped: keep it authoritative — do NOT let keywords
        # invent a route. GENERAL means "no intent-based filtering" downstream.
        logger.debug("[K2] Explicit task_type=%s has no coarse mapping; staying GENERAL", task_type)
        return IntentCategory.GENERAL

    if not user_input or user_input.isspace():
        return IntentCategory.GENERAL

    text = user_input.lower().strip()

    # Detect flight number pattern (e.g., CA1234, MU5678, CZ3022)
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

    If intent is GENERAL or unrecognized, return full tool set.
    Otherwise, only return the tool subset associated with that intent.

    Args:
        intent: Intent classification
        all_tools: Full tool list (OpenAI schema format)

    Returns:
        Filtered tool list
    """
    if intent == IntentCategory.GENERAL:
        return all_tools

    allowed_names = set(_INTENT_TOOL_MAP.get(intent, []))
    if not allowed_names:
        return all_tools

    filtered = [tool for tool in all_tools if tool.get("function", {}).get("name", "") in allowed_names]

    # Safety guarantee: if filtered tools are too few (<2), fall back to full set
    if len(filtered) < 2:
        logger.warning(f"Intent '{intent}' filtered to {len(filtered)} tools, falling back to full tool set")
        return all_tools

    logger.debug(f"Intent routing: '{intent}' → {len(filtered)}/{len(all_tools)} tools")
    return filtered


def route_tools(
    user_input: str,
    all_tools: list[dict[str, Any]],
    *,
    task_type: str | None = None,
) -> tuple[str, list[dict[str, Any]]]:
    """
    One-step intent classification + tool filtering.

    Args:
        user_input: User input text
        all_tools: Full tool list
        task_type: Explicit task_type; when given, keywords cannot change the
            classification (K2 coarse-filter contract).

    Returns:
        (intent, filtered_tools) tuple
    """
    intent = classify_intent(user_input, task_type=task_type)
    filtered = filter_tools_by_intent(intent, all_tools)
    return intent, filtered
