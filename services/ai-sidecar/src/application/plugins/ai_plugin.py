"""AI 系统插件 — 将 AI 初始化逻辑封装为可插拔模块。

AI 组件不是系统核心功能，初始化失败应优雅降级而非中断启动。
本插件将 ~120 行的 try/except 初始化代码从 DI 容器提取为独立模块，
使 DI 容器不再关心 AI 内部实现细节。

使用方式::

    AIPlugin.install(container, config_manager)
    # 或
    AIPlugin.uninstall(container)
"""

from __future__ import annotations

import os
import traceback
from typing import TYPE_CHECKING, Any

from src.infrastructure.logging.core import get_logger

if TYPE_CHECKING:
    from src.di.container import DIContainer

logger = get_logger(__name__)


# AI 服务属性名 — 初始化失败时统一置 None
_AI_ATTR_NAMES = (
    "ai_config_store",
    "ai_context_manager",
    "ai_conversation_manager",
    "ai_execution_repository",
    "ai_pending_action_store",
    "ai_rate_limiter",
    "ai_executor",
    "todo_agent_service",
    "todo_graph_pilot_snapshot_service",
    "todo_graph_pilot_ops_service",
    "nl_query_service",
    "smart_monitor",
    "flight_insight_service",
    "ai_entity_manager",
    "ai_batch_service",
)


class AIPlugin:
    """AI 子系统的安装 / 卸载入口。"""

    @staticmethod
    def install(container: DIContainer, config_manager: Any) -> bool:
        """初始化全部 AI 组件，失败时安全降级。

        Returns:
            True 如果 AI 初始化成功，False 如果降级为 None。
        """
        from src.application.services.ai.feature_flags import resolve_ai_feature_flags
        from src.application.services.ai.nl_query_service import NLQueryService
        from src.application.services.ai.todo_agent_service import TodoAgentService
        from src.application.services.ai.todo_graph_pilot_ops_service import (
            TodoGraphPilotOpsService,
            TodoGraphPilotSnapshotService,
        )
        from src.application.services.flight.flight_insight_service import FlightInsightService
        from src.infrastructure.ai.agent_execution_repository import PostgresAgentExecutionRepository
        from src.infrastructure.ai.ai_manager_factory import AIManagerFactory
        from src.infrastructure.ai.rate_limiter import RateLimiter
        from src.infrastructure.ai.services.smart_monitor import SmartMonitor
        from src.infrastructure.ai.todo_agent_executor import TodoAgentExecutor
        from src.infrastructure.ai.tools import (
            PostgresPendingActionStore,
            set_pending_action_store,
        )

        strict_envs = {"production", "prod", "staging", "stage"}
        flight_read_service = container.flight_query_service or container.async_flight_service

        try:
            ai_config = config_manager.get_dict("ai", {})
            feature_overrides = resolve_ai_feature_flags(config_manager=config_manager)

            # 1. Config Store
            container.ai_config_store = AIManagerFactory.create_config_store(
                ai_config,
                async_db_connection=container.async_connection_pool,
            )

            # 2. Conversation Manager
            container.ai_context_manager, container.ai_conversation_manager = AIManagerFactory.create_ai_managers(
                ai_config,
                async_db_connection=container.async_connection_pool,
            )

            # 3. Execution Repository & Pending Action Store
            container.ai_execution_repository = PostgresAgentExecutionRepository(container.async_connection_pool)
            container.ai_pending_action_store = PostgresPendingActionStore(container.async_connection_pool)
            set_pending_action_store(container.ai_pending_action_store)

            # 4. Rate Limiter
            container.ai_rate_limiter = RateLimiter(rpm=60, tpm=100000)

            # 5. Executor
            container.ai_executor = TodoAgentExecutor(
                config_store=container.ai_config_store,
                execution_repo=container.ai_execution_repository,
                rate_limiter=container.ai_rate_limiter,
                conversation_manager=container.ai_conversation_manager,
                notification_port=container.notification_port,
                feature_overrides=feature_overrides,
                max_concurrent=10,
            )
            # 5.1 Flight Insight
            container.flight_insight_service = FlightInsightService(
                history_service=container.history_service,
                flight_service=flight_read_service,
                ai_config_store=container.ai_config_store,
            )

            # 5.2 AI Entity and Batch Services
            from src.infrastructure.ai.services.ai_entity_manager import AIEntityManager
            from src.infrastructure.ai.services.batch_service import BatchOperationService

            container.ai_entity_manager = AIEntityManager()
            container.ai_batch_service = BatchOperationService(
                entity_manager=container.ai_entity_manager,
                max_concurrency=config_manager.get_int("ai.batch.max_concurrency", 5),
            )

            # 6. Tool Registry
            AIPlugin._register_tools(container, config_manager)

            # 6.1 Permission policy
            if container._runtime_env in strict_envs:
                strict_tool_permissions = True
            else:
                strict_tool_permissions = config_manager.get_bool(
                    "ai.security.strict_tool_permissions",
                    config_manager.get_bool("ai.security.strict.tool.permissions", False),
                )

            registry = container.tool_registry
            registry.permission_manager.set_default_allow(not strict_tool_permissions)
            registry.set_require_user_context(strict_tool_permissions)
            logger.info(
                f"AI tool permission mode configured: strict={strict_tool_permissions}, env={container._runtime_env}"
            )

            # 7. Agent Services
            container.todo_agent_service = TodoAgentService(
                executor=container.ai_executor,
                execution_repo=container.ai_execution_repository,
                config_store=container.ai_config_store,
                rate_limiter=container.ai_rate_limiter,
                todo_service=container.async_todo_service,
                business_case_service=container.async_business_case_service,
            )
            container.todo_graph_pilot_snapshot_service = TodoGraphPilotSnapshotService(
                agent_service=container.todo_agent_service,
                tool_registry=registry,
            )
            container.todo_graph_pilot_ops_service = TodoGraphPilotOpsService(
                snapshot_service=container.todo_graph_pilot_snapshot_service,
                config_store=container.ai_config_store,
                alert_service=container.alert_service,
                sse_hub=container.sse_hub,
                config_manager=config_manager,
            )
            container.nl_query_service = NLQueryService(
                conversation_manager=container.ai_conversation_manager,
                tool_registry=registry,
                ai_config_store=container.ai_config_store,
                flight_service=flight_read_service,
                notification_port=container.notification_port,
                feature_overrides=feature_overrides,
                db_pool=container.async_connection_pool,
            )

            # 8. Smart Monitor
            container.smart_monitor = SmartMonitor(
                config_path="config/smart_monitor_config.yaml",
                flight_service=flight_read_service,
                sse_hub=container.sse_hub,
                ai_entity=None,
            )

            return True

        except Exception as exc:  # noqa: BLE001 - top-level AI init must degrade gracefully
            logger.error(f"Failed to initialize AI components: {exc}")
            logger.error(traceback.format_exc())
            AIPlugin.uninstall(container)
            return False

    @staticmethod
    def uninstall(container: DIContainer) -> None:
        """将所有 AI 属性重置为 None，确保降级安全。"""
        from src.infrastructure.ai.tools import PendingActionStore, set_pending_action_store

        for attr_name in _AI_ATTR_NAMES:
            setattr(container, attr_name, None)
        set_pending_action_store(PendingActionStore())

    @staticmethod
    def _register_tools(container: DIContainer, config_manager: Any) -> None:
        """注册所有 AI 工具定义和执行器。"""
        from src.infrastructure.ai.tools import (
            ADVISOR_TOOL_DEFINITIONS,
            ANOMALY_TOOL_DEFINITIONS,
            DISPATCH_COMMAND_DEFINITIONS,
            DISPATCH_QUERY_TOOL_DEFINITIONS,
            EQUIPMENT_TOOL_DEFINITIONS,
            REPORT_TOOL_DEFINITIONS,
            SQL_QUERY_TOOL_DEFINITIONS,
            STAND_TOOL_DEFINITIONS,
            TEAM_TOOL_DEFINITIONS,
            TODO_TOOL_DEFINITIONS,
            AdvisorToolExecutor,
            AnomalyToolExecutor,
            BusinessCaseToolExecutor,
            DispatchCommandExecutor,
            DispatchQueryExecutor,
            EquipmentToolExecutor,
            ReportToolExecutor,
            SimpleKnowledgeBase,
            SQLQueryReadOnlyExecutor,
            StandToolExecutor,
            TeamToolExecutor,
            TodoToolExecutor,
            get_business_case_tools,
        )
        from src.infrastructure.ai.tools.registry import set_tool_registry

        registry = container.tool_registry
        flight_read_service = container.flight_query_service or container.async_flight_service
        registry.clear()
        set_tool_registry(registry)

        sql_read_tool_enabled = config_manager.get_bool(
            "ai.feature_flags.sql_read_tool_enabled",
            config_manager.get_bool("ai.feature.flags.sql.read.tool.enabled", True),
        )

        # 工具定义
        registry.register_tools(TODO_TOOL_DEFINITIONS)
        registry.register_tools(get_business_case_tools())
        registry.register_tools(REPORT_TOOL_DEFINITIONS)
        registry.register_tools(ADVISOR_TOOL_DEFINITIONS)
        registry.register_tools(ANOMALY_TOOL_DEFINITIONS)
        if sql_read_tool_enabled:
            registry.register_tools(SQL_QUERY_TOOL_DEFINITIONS)
        registry.register_tools(DISPATCH_COMMAND_DEFINITIONS)
        registry.register_tools(TEAM_TOOL_DEFINITIONS)
        registry.register_tools(DISPATCH_QUERY_TOOL_DEFINITIONS)
        registry.register_tools(EQUIPMENT_TOOL_DEFINITIONS)
        registry.register_tools(STAND_TOOL_DEFINITIONS)

        # 执行器
        registry.register_executor(TodoToolExecutor(todo_service=container.async_todo_service))
        registry.register_executor(
            BusinessCaseToolExecutor(business_case_service=container.async_business_case_service)
        )
        registry.register_executor(AnomalyToolExecutor(anomaly_repository=container.anomaly_repository))
        if sql_read_tool_enabled:
            allowed_relations = config_manager.get_list(
                "ai.query_db.allowed_relations",
                config_manager.get_list("ai.query.db.allowed.relations", []),
            )
            if not allowed_relations:
                raw_relations = (
                    config_manager.get_str("ai.query_db.allowed_relations", "").strip()
                    or config_manager.get_str("ai.query.db.allowed.relations", "").strip()
                )
                if raw_relations:
                    allowed_relations = [item.strip() for item in raw_relations.split(",") if item and item.strip()]

            statement_timeout_ms = max(
                100,
                config_manager.get_int(
                    "ai.query_db.statement_timeout_ms",
                    config_manager.get_int("ai.query.db.statement.timeout.ms", 5000),
                ),
            )
            default_max_rows = max(
                1,
                config_manager.get_int(
                    "ai.query_db.default_max_rows",
                    config_manager.get_int("ai.query.db.default.max.rows", 200),
                ),
            )
            hard_max_rows = max(
                default_max_rows,
                config_manager.get_int(
                    "ai.query_db.hard_max_rows",
                    config_manager.get_int("ai.query.db.hard.max.rows", 500),
                ),
            )
            registry.register_executor(
                SQLQueryReadOnlyExecutor(
                    db_pool=container.ai_query_connection_pool or container.async_connection_pool,
                    allowed_relations=allowed_relations,
                    statement_timeout_ms=statement_timeout_ms,
                    default_max_rows=default_max_rows,
                    hard_max_rows=hard_max_rows,
                )
            )
        registry.register_executor(
            DispatchCommandExecutor(
                flight_service=flight_read_service,
                dispatch_service=container.dispatch_service,
                notification_service=container.notification_service,
            )
        )
        registry.register_executor(
            ReportToolExecutor(
                history_service=container.history_service,
                flight_service=flight_read_service,
                ai_entity=None,
            )
        )

        pageindex_api_key = (
            config_manager.get_str("ai.pageindex.api_key", "").strip() or os.getenv("PAGEINDEX_API_KEY", "").strip()
        )
        knowledge_base = SimpleKnowledgeBase(
            base_path="knowledge_base",
            db_pool=container.async_connection_pool,
            pageindex_api_key=pageindex_api_key,
        )
        registry.register_executor(
            AdvisorToolExecutor(
                knowledge_base=knowledge_base,
                flight_service=flight_read_service,
                ai_entity=None,
            )
        )

        # 地服工具执行器
        registry.register_executor(
            TeamToolExecutor(
                team_repository=getattr(container, "dispatch_team_repository", None),
                member_repository=getattr(container, "dispatch_team_member_repository", None),
            )
        )
        registry.register_executor(
            DispatchQueryExecutor(
                dispatch_order_repository=getattr(container, "dispatch_order_repository", None),
            )
        )
        registry.register_executor(
            EquipmentToolExecutor(
                equipment_repository=getattr(container, "dispatch_equipment_repository", None),
                equipment_type_repository=getattr(container, "dispatch_equipment_type_repository", None),
            )
        )
        registry.register_executor(
            StandToolExecutor(
                stand_repository=getattr(container, "dispatch_stand_repository", None),
            )
        )
