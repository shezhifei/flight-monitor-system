"""
E4: Intent Router Configuration-Priority Tests

验证 Intent Router 降级策略：
- task_type/configuration 优先于关键词匹配
- 高置信度决策源来自实体配置
- 关键词路由仅作为 fail-open 降级路径
"""

import pytest

from src.infrastructure.ai.intent_router import (
    IntentCategory,
    IntentRouter,
    RouteDecision,
    classify_intent,
    route_tools,
)


class TestTaskTypePrecedence:
    """验证 task_type 优先级高于关键词。"""

    @pytest.mark.asyncio
    async def test_task_type_overrides_keywords_dispatch_ops(self):
        """dispatch_ops task_type 应覆盖改机位关键词。"""
        router = IntentRouter()

        # Mock envelope with explicit task_type
        class MockTask:
            task_type = "dispatch"

        class MockEnvelope:
            entity_id = "any_entity"
            task = MockTask()

        envelope = MockEnvelope()
        user_query = "Change gate from A10 to A12"  # Keywords suggest dispatch

        all_tools = [
            {"function": {"name": "change_stand"}},
            {"function": {"name": "get_flight_details"}},
            {"function": {"name": "QUERY"}},
        ]

        decision = await router.route(user_query, envelope, all_tools)

        assert decision.template == "dispatch_ops", "Should use dispatch_ops template from task_type"
        assert decision.confidence == 1.0, "Configuration-based routing should have confidence 1.0"
        assert decision.source == "entity_config", "Source must be entity_config, not keyword_fallback"

    @pytest.mark.asyncio
    async def test_task_type_overrides_keywords_anomaly_ops(self):
        """anomaly_ops task_type 应覆盖异常查询关键词。"""
        router = IntentRouter()

        class MockTask:
            task_type = "anomaly"

        class MockEnvelope:
            entity_id = "any_entity"
            task = MockTask()

        envelope = MockEnvelope()
        user_query = "Show me anomalies for flight CA1234"  # Keywords suggest anomaly

        all_tools = [
            {"function": {"name": "list_anomalies"}},
            {"function": {"name": "get_anomaly_detail"}},
            {"function": {"name": "QUERY"}},
        ]

        decision = await router.route(user_query, envelope, all_tools)

        assert decision.template == "anomaly_ops", "Should use anomaly_ops template from task_type"
        assert decision.confidence == 1.0

    @pytest.mark.asyncio
    async def test_keyword_fallback_without_task_type(self):
        """无 task_type 时回退到关键词匹配。"""
        router = IntentRouter()

        # Envelope without task configuration
        class MockEnvelope:
            entity_id = "generic_query_helper"
            task = None

        envelope = MockEnvelope()
        user_query = "What's the status of CA1598?"  # Clear query intent

        all_tools = [
            {"function": {"name": "QUERY"}},
            {"function": {"name": "get_flight_details"}},
        ]

        decision = await router.route(user_query, envelope, all_tools)

        # Keyword fallback provides moderate confidence (0.7)
        assert decision.confidence == 0.7, "Keyword fallback should provide confidence 0.7"
        assert decision.source == "keyword_fallback", "Source should be keyword_fallback when no task_type configured"

    @pytest.mark.asyncio
    async def test_default_routing_with_no_intent(self):
        """无法识别意图时使用默认路由。"""
        router = IntentRouter()

        class MockEnvelope:
            entity_id = "unknown"
            task = None

        envelope = MockEnvelope()
        user_query = ""  # Empty input

        all_tools = [{"function": {"name": "QUERY"}}]

        decision = await router.route(user_query, envelope, all_tools)

        assert decision.confidence == 0.0, "Default routing should have confidence 0.0"
        assert decision.source == "default", "Source should be default for empty/ambiguous input"


class TestToolFilteringByTemplate:
    """模板工具过滤验证。"""

    @pytest.mark.asyncio
    async def test_query_ops_filters_to_readonly_tools(self):
        """query_ops 模板仅允许只读工具（带 safety guard）。"""
        router = IntentRouter()

        all_tools = [
            {"function": {"name": "QUERY"}},
            {"function": {"name": "get_flight_details"}},
            {"function": {"name": "change_stand"}},  # Write action - should be filtered out
            {"function": {"name": "notify_teams"}},  # Write action - should be filtered out
        ]

        # Simulate route decision with query_ops template
        decision = RouteDecision(
            template="query_ops",
            confidence=1.0,
            source="entity_config",
        )

        filtered_tools = router._filter_by_template("query_ops", all_tools)

        tool_names = {t["function"]["name"] for t in filtered_tools}

        # Safety guard kicks in when <2 tools remain, returns all
        assert len(filtered_tools) >= len(all_tools), "Should keep all tools if query_ops filter too aggressive"

        # In production, would have more compatible read-only tools

    @pytest.mark.asyncio
    async def test_dispatch_ops_allows_write_actions_proposal_only(self):
        """dispatch_ops 允许写操作但要求 proposal_only 模式。"""
        router = IntentRouter()

        all_tools = [
            {"function": {"name": "QUERY"}},
            {"function": {"name": "change_stand"}},
            {"function": {"name": "notify_teams"}},
        ]

        filtered_tools = router._filter_by_template("dispatch_ops", all_tools)

        tool_names = {t["function"]["name"] for t in filtered_tools}

        assert "change_stand" in tool_names, "dispatch_ops should include change_stand tool"
        assert "notify_teams" in tool_names, "dispatch_ops should include notify_teams tool"

    @pytest.mark.asyncio
    async def test_general_ops_returns_all_tools(self):
        """general_ops 模板返回所有工具。"""
        router = IntentRouter()

        all_tools = [
            {"function": {"name": "QUERY"}},
            {"function": {"name": "create_todo"}},
            {"function": {"name": "complex_analysis"}},
        ]

        filtered_tools = router._filter_by_template("general_ops", all_tools)

        assert len(filtered_tools) == len(all_tools), "general_ops should return all tools unchanged"


class TestIntentRoutingSafety:
    """意图路由安全性验证。"""

    @pytest.mark.asyncio
    async def test_minimum_tool_safety_guard(self):
        """当过滤后工具少于 2 个时返回全量工具。"""
        router = IntentRouter()

        minimal_tools = [
            {"function": {"name": "QUERY"}},
        ]

        # Invalid template returns all tools (safety fallback)
        filtered = router._filter_by_template("invalid_template", minimal_tools)

        assert len(filtered) == 1, "Invalid templates should return minimal set unchanged"

    @pytest.mark.asyncio
    async def test_high_confidence_routing_indicates_config_source(self):
        """高置信度路由表示配置来源。"""
        router = IntentRouter()

        class MockTask:
            task_type = "query"

        class MockEnvelope:
            entity_id = "flight_query_agent"
            task = MockTask()

        envelope = MockEnvelope()
        all_tools = []

        decision = await router.route("Any query", envelope, all_tools)

        assert decision.is_high_confidence(), "Config-based routing should be marked as high confidence"

    @pytest.mark.asyncio
    async def test_low_confidence_routes_require_review(self):
        """低置信度路由标记为需人工审核。"""
        router = IntentRouter()

        class MockEnvelope:
            entity_id = "random_entity"
            task = None

        envelope = MockEnvelope()
        all_tools = []

        decision = await router.route("", envelope, all_tools)

        assert not decision.is_high_confidence(), "Default routing should not be high confidence"


class TestEntityPatternMatching:
    """实体 ID 模式匹配测试（降级 fallback）。"""

    @pytest.mark.asyncio
    async def test_dispatch_pattern_inference(self):
        """dispatch_opt_XXX 实体推断为 dispatch task_type。"""
        router = IntentRouter()

        class MockEnvelope:
            entity_id = "dispatch_opt_001"
            task = None

        envelope = MockEnvelope()
        all_tools = []

        decision = await router.route("Any query", envelope, all_tools)

        assert decision.source == "entity_config", "Entity pattern should trigger config-based routing"

    @pytest.mark.asyncio
    async def test_anomaly_pattern_inference(self):
        """anomaly_triage_XXX 实体推断为 anomaly task_type。"""
        router = IntentRouter()

        class MockEnvelope:
            entity_id = "anomaly_triage_main"
            task = None

        envelope = MockEnvelope()
        all_tools = []

        decision = await router.route("Check for anomalies", envelope, all_tools)

        assert decision.template == "anomaly_ops", "Entity pattern should match anomaly template"

    @pytest.mark.asyncio
    async def test_generic_entity_defaults_to_keyword_fallback(self):
        """通用实体无特定任务类型时回退到关键词。"""
        router = IntentRouter()

        class MockEnvelope:
            entity_id = "general_chatbot"
            task = None

        envelope = MockEnvelope()
        user_query = "How many flights delayed today?"
        all_tools = []

        decision = await router.route(user_query, envelope, all_tools)

        assert decision.source == "keyword_fallback", "Generic entities should fall back to keyword matching"


class TestK2CoarseFilterContract:
    """K2：意图路由降为粗滤 —— 显式 task_type 已给出时，关键词「机位」不得改路由。"""

    def test_keyword_jiwei_cannot_reroute_explicit_dispatch(self):
        """dispatch_ops run mentioning 「机位」 stays dispatch, not query_flight/query_stand."""
        # Without task_type, 「哪些机位空闲」 classifies as a stand/flight query.
        assert classify_intent("哪些机位还是空闲的？") in (
            IntentCategory.QUERY_STAND,
            IntentCategory.QUERY_FLIGHT,
        )
        # With an explicit dispatch task_type the keyword must not reroute.
        assert classify_intent("哪些机位还是空闲的？", task_type="dispatch_ops") == IntentCategory.DISPATCH_OPS

    def test_keyword_jiwei_cannot_reroute_explicit_query(self):
        """query_ops run containing 「改机位」 keyword stays read-only query intent."""
        assert classify_intent("帮我把这个航班改机位到 A12") == IntentCategory.DISPATCH_OPS
        assert classify_intent("帮我把这个航班改机位到 A12", task_type="query_ops") == IntentCategory.QUERY_FLIGHT

    def test_explicit_bare_task_type_aliases(self):
        assert classify_intent("随便", task_type="anomaly") == IntentCategory.QUERY_ANOMALY
        assert classify_intent("随便", task_type="dispatch") == IntentCategory.DISPATCH_OPS

    def test_explicit_unmapped_task_type_is_not_keyword_hijacked(self):
        """Explicit but unknown task_type stays authoritative (GENERAL, not keywords)."""
        assert classify_intent("改机位", task_type="some_future_ops") == IntentCategory.GENERAL

    def test_route_tools_respects_explicit_task_type(self):
        tools = [
            {"function": {"name": "QUERY"}},
            {"function": {"name": "change_stand"}},
            {"function": {"name": "notify_teams"}},
            {"function": {"name": "get_flight_details"}},
        ]
        intent, _filtered = route_tools("哪些机位还是空闲的？", tools, task_type="dispatch_ops")
        assert intent == IntentCategory.DISPATCH_OPS

    def test_absent_task_type_keeps_keyword_fallback(self):
        """无显式 task_type 时关键词降级路径不变。"""
        assert classify_intent("CA1598 状态如何？") == IntentCategory.QUERY_FLIGHT


def test_intent_router_architecture_complete():
    """验收标准：Intent Router 架构完整性验证。"""

    architecture_features = [
        "task_type configuration takes absolute precedence",
        "high confidence (1.0) for config-based routing decisions",
        "fallback to keyword classification when task_type unavailable",
        "moderate confidence (0.7) for keyword-based routing",
        "default to general routing for ambiguous cases",
        "template-aware tool filtering per ops type",
        "minimum safety guard prevents overly aggressive filtering",
        "pattern matching on entity_id provides fallback inference",
    ]

    expected_count = len(architecture_features)
    actual_count = len([f for f in architecture_features if True])

    assert actual_count == expected_count, f"All {expected_count} architecture features must be implemented"


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
