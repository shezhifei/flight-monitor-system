"""
AIP 工具桥接器 - Legacy 与 AIP 模式集成

提供 Legacy ToolRegistry 到 AIP 模式的双向桥接：
1. Legacy → AIP: 将现有工具适配为 AIP Functions
2. AIP → Legacy: 支持回退到现有工具执行器

使用方式:
    # 初始化桥接器
    bridge = ToolBridge(tool_registry, aip_app)
    await bridge.initialize()

    # 启用双轨模式
    bridge.enable_dual_mode()

    # 执行带回退的操作
    result = await bridge.execute_with_fallback(...)
"""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass
from enum import StrEnum
from typing import TYPE_CHECKING, Any

from src.domain.utils.time_utils import utc_now
from src.infrastructure.ai.tools.base import (
    BaseToolDefinition,
)
from src.infrastructure.ai.tools.registry import ToolRegistry
from src.infrastructure.logging.core import get_logger

if TYPE_CHECKING:
    from .function_registry import AIPFunction

logger = get_logger(__name__)


class BridgeMode(StrEnum):
    """桥接运行模式"""

    AIP_ONLY = "aip_only"
    LEGACY_ONLY = "legacy_only"
    DUAL = "dual"


class MigrationStatus(StrEnum):
    """迁移状态"""

    NOT_STARTED = "not_started"
    IN_PROGRESS = "in_progress"
    COMPLETED = "completed"


@dataclass
class ToolMapping:
    """工具映射配置"""

    tool_name: str
    object_type: str
    action_name: str
    requires_approval: bool = False
    risk_level: str = "NORMAL"
    migration_status: MigrationStatus = MigrationStatus.NOT_STARTED
    custom_handler: Callable | None = None


TOOL_TO_OBJECT_MAPPING: dict[str, ToolMapping] = {
    "change_flight_stand": ToolMapping(
        tool_name="change_flight_stand",
        object_type="Flight",
        action_name="change_stand",
        requires_approval=True,
        risk_level="MEDIUM",
        migration_status=MigrationStatus.IN_PROGRESS,
    ),
    "delay_flight": ToolMapping(
        tool_name="delay_flight",
        object_type="Flight",
        action_name="delay_flight",
        requires_approval=False,
        risk_level="LOW",
        migration_status=MigrationStatus.IN_PROGRESS,
    ),
    "assign_team_to_flight": ToolMapping(
        tool_name="assign_team_to_flight",
        object_type="Flight",
        action_name="assign_team",
        requires_approval=True,
        risk_level="MEDIUM",
        migration_status=MigrationStatus.IN_PROGRESS,
    ),
    "update_flight_status": ToolMapping(
        tool_name="update_flight_status",
        object_type="Flight",
        action_name="update_status",
        requires_approval=True,
        risk_level="HIGH",
        migration_status=MigrationStatus.IN_PROGRESS,
    ),
    "mark_flight_arrived": ToolMapping(
        tool_name="mark_flight_arrived",
        object_type="Flight",
        action_name="mark_arrived",
        requires_approval=False,
        risk_level="LOW",
        migration_status=MigrationStatus.IN_PROGRESS,
    ),
    "mark_flight_departed": ToolMapping(
        tool_name="mark_flight_departed",
        object_type="Flight",
        action_name="mark_departed",
        requires_approval=False,
        risk_level="LOW",
        migration_status=MigrationStatus.IN_PROGRESS,
    ),
    "occupy_stand": ToolMapping(
        tool_name="occupy_stand",
        object_type="Stand",
        action_name="occupy",
        requires_approval=False,
        risk_level="LOW",
        migration_status=MigrationStatus.IN_PROGRESS,
    ),
    "release_stand": ToolMapping(
        tool_name="release_stand",
        object_type="Stand",
        action_name="release",
        requires_approval=False,
        risk_level="LOW",
        migration_status=MigrationStatus.IN_PROGRESS,
    ),
    "reserve_stand": ToolMapping(
        tool_name="reserve_stand",
        object_type="Stand",
        action_name="reserve",
        requires_approval=True,
        risk_level="MEDIUM",
        migration_status=MigrationStatus.IN_PROGRESS,
    ),
    "close_stand": ToolMapping(
        tool_name="close_stand",
        object_type="Stand",
        action_name="close",
        requires_approval=True,
        risk_level="MEDIUM",
        migration_status=MigrationStatus.IN_PROGRESS,
    ),
    "update_stand_status": ToolMapping(
        tool_name="update_stand_status",
        object_type="Stand",
        action_name="update_status",
        requires_approval=False,
        risk_level="LOW",
        migration_status=MigrationStatus.IN_PROGRESS,
    ),
    "assign_flight_to_team": ToolMapping(
        tool_name="assign_flight_to_team",
        object_type="Team",
        action_name="assign_flight",
        requires_approval=False,
        risk_level="LOW",
        migration_status=MigrationStatus.IN_PROGRESS,
    ),
    "update_team_status": ToolMapping(
        tool_name="update_team_status",
        object_type="Team",
        action_name="update_status",
        requires_approval=False,
        risk_level="LOW",
        migration_status=MigrationStatus.IN_PROGRESS,
    ),
    "change_team_location": ToolMapping(
        tool_name="change_team_location",
        object_type="Team",
        action_name="change_location",
        requires_approval=False,
        risk_level="LOW",
        migration_status=MigrationStatus.IN_PROGRESS,
    ),
    "acknowledge_anomaly": ToolMapping(
        tool_name="acknowledge_anomaly",
        object_type="Anomaly",
        action_name="acknowledge",
        requires_approval=False,
        risk_level="LOW",
        migration_status=MigrationStatus.IN_PROGRESS,
    ),
    "assign_team_to_anomaly": ToolMapping(
        tool_name="assign_team_to_anomaly",
        object_type="Anomaly",
        action_name="assign_team",
        requires_approval=False,
        risk_level="LOW",
        migration_status=MigrationStatus.IN_PROGRESS,
    ),
    "resolve_anomaly": ToolMapping(
        tool_name="resolve_anomaly",
        object_type="Anomaly",
        action_name="resolve",
        requires_approval=True,
        risk_level="MEDIUM",
        migration_status=MigrationStatus.IN_PROGRESS,
    ),
    "escalate_anomaly": ToolMapping(
        tool_name="escalate_anomaly",
        object_type="Anomaly",
        action_name="escalate",
        requires_approval=True,
        risk_level="HIGH",
        migration_status=MigrationStatus.IN_PROGRESS,
    ),
    "create_todo": ToolMapping(
        tool_name="create_todo",
        object_type="Todo",
        action_name="create",
        requires_approval=False,
        risk_level="LOW",
        migration_status=MigrationStatus.IN_PROGRESS,
    ),
    "complete_todo": ToolMapping(
        tool_name="complete_todo",
        object_type="Todo",
        action_name="complete",
        requires_approval=False,
        risk_level="LOW",
        migration_status=MigrationStatus.IN_PROGRESS,
    ),
    "assign_todo": ToolMapping(
        tool_name="assign_todo",
        object_type="Todo",
        action_name="assign",
        requires_approval=False,
        risk_level="LOW",
        migration_status=MigrationStatus.IN_PROGRESS,
    ),
}


@dataclass
class BridgeMetrics:
    """桥接器指标"""

    total_calls: int = 0
    aip_calls: int = 0
    legacy_calls: int = 0
    fallback_calls: int = 0
    failed_calls: int = 0
    pending_tools: int = 0
    migrated_tools: int = 0


class ToolBridge:
    """
    工具桥接器

    在 Legacy ToolRegistry 和 AIP 模式之间提供双向桥接：
    - 将 Legacy 工具适配为 AIP Functions
    - 支持双轨并行运行
    - 提供回退机制确保稳定性
    """

    def __init__(self, tool_registry: ToolRegistry, aip_app: Any, default_mode: BridgeMode = BridgeMode.DUAL):
        self._tool_registry = tool_registry
        self._aip_app = aip_app
        self._mode = default_mode
        self._initialized = False
        self._metrics = BridgeMetrics()
        self._mappings: dict[str, ToolMapping] = {}

    async def initialize(self) -> None:
        """初始化桥接器"""
        if self._initialized:
            return

        logger.info("Initializing ToolBridge...")

        self._load_mappings()
        await self._adapt_legacy_tools()

        self._metrics.pending_tools = len(self._mappings)
        self._initialized = True

        logger.info(f"ToolBridge initialized: {len(self._mappings)} tools mapped")

    def _load_mappings(self) -> None:
        """加载工具映射配置"""
        self._mappings = TOOL_TO_OBJECT_MAPPING.copy()

        for _tool_name, mapping in self._mappings.items():
            if mapping.migration_status == MigrationStatus.COMPLETED:
                self._metrics.migrated_tools += 1

    async def _adapt_legacy_tools(self) -> None:
        """将 Legacy 工具适配为 AIP Functions"""

        function_registry = self._aip_app.function_registry

        for tool_name, mapping in self._mappings.items():
            tool_def = self._find_tool_definition(tool_name)
            if not tool_def:
                logger.warning(f"Tool definition not found: {tool_name}")
                continue

            aip_function = self._create_aip_function_from_tool(tool_def, mapping)
            function_registry.register(aip_function)

            logger.debug(f"Adapted tool '{tool_name}' to AIP function '{aip_function.name}'")

    def _find_tool_definition(self, tool_name: str) -> BaseToolDefinition | None:
        """查找工具定义"""
        if tool_name in self._tool_registry._tools:
            definition, _ = self._tool_registry._tools[tool_name]
            return definition
        return None

    def _create_aip_function_from_tool(self, tool_def: BaseToolDefinition, mapping: ToolMapping) -> AIPFunction:
        """从工具定义创建 AIP Function"""
        from .function_registry import AIPFunction, FunctionCategory, RiskLevel

        risk_level_map = {
            "LOW": RiskLevel.LOW,
            "NORMAL": RiskLevel.NORMAL,
            "MEDIUM": RiskLevel.MEDIUM,
            "HIGH": RiskLevel.HIGH,
            "CRITICAL": RiskLevel.CRITICAL,
        }

        return AIPFunction(
            name=mapping.action_name,
            category=FunctionCategory.OBJECT_ACTION,
            object_type=mapping.object_type,
            action_name=mapping.action_name,
            description=tool_def.description,
            parameters_schema={
                "type": "object",
                "properties": tool_def.parameters,
                "required": tool_def.required_params,
            },
            requires_approval=mapping.requires_approval,
            risk_level=risk_level_map.get(mapping.risk_level, RiskLevel.NORMAL),
        )

    def set_mode(self, mode: BridgeMode) -> None:
        """设置桥接模式"""
        if mode == self._mode:
            return

        logger.info(f"Switching ToolBridge mode: {self._mode.value} -> {mode.value}")
        self._mode = mode

    def get_mode(self) -> BridgeMode:
        """获取当前模式"""
        return self._mode

    async def execute_with_fallback(
        self, tool_name: str, arguments: dict[str, Any], user_context: dict[str, Any], principal: str
    ) -> dict[str, Any]:
        """
        执行带回退的工具调用

        Args:
            tool_name: 工具名称
            arguments: 工具参数
            user_context: 用户上下文
            principal: 执行主体

        Returns:
            执行结果
        """
        self._metrics.total_calls += 1

        if self._mode == BridgeMode.LEGACY_ONLY:
            return await self._execute_legacy(tool_name, arguments, user_context, principal)

        if self._mode == BridgeMode.AIP_ONLY:
            return await self._execute_aip(tool_name, arguments, principal)

        try:
            result = await self._execute_aip(tool_name, arguments, principal)
            self._metrics.aip_calls += 1
            return result
        except Exception as exc:  # noqa: BLE001 - AIP fallback to legacy must catch all
            logger.warning(f"AIP execution failed for '{tool_name}': {exc}, falling back to legacy")
            self._metrics.fallback_calls += 1
            return await self._execute_legacy(tool_name, arguments, user_context, principal)

    async def _execute_aip(self, tool_name: str, arguments: dict[str, Any], principal: str) -> dict[str, Any]:
        """通过 AIP 模式执行"""
        mapping = self._mappings.get(tool_name)
        if not mapping:
            raise ValueError(f"No mapping found for tool: {tool_name}")

        result = await self._aip_app.execute_action(
            principal=principal,
            object_type=mapping.object_type,
            object_id=arguments.get(f"{mapping.object_type.lower()}_id", arguments.get("id", "")),
            action=mapping.action_name,
            parameters=arguments,
            invocation_mode="user_requested",
        )

        return {
            "mode": "aip",
            "status": result.get("status"),
            "result": result.get("result"),
            "pending_action_id": result.get("pending_action_id"),
            "change_preview": result.get("change_preview"),
            "error": result.get("error"),
        }

    async def _execute_legacy(
        self, tool_name: str, arguments: dict[str, Any], user_context: dict[str, Any], principal: str
    ) -> dict[str, Any]:
        """通过 Legacy 模式执行"""
        import json

        from src.infrastructure.ai.tools.pending_actions import get_pending_action_store

        self._metrics.legacy_calls += 1

        user_id = user_context.get("user_id", "")
        user_roles = user_context.get("roles", [])

        is_allowed = self._tool_registry._is_allowed_by_user(tool_name, user_id, user_roles)
        if not is_allowed:
            return {
                "mode": "legacy",
                "status": "permission_denied",
                "error": f"Permission denied for tool: {tool_name}",
            }

        tool_def, category = self._tool_registry._tools.get(tool_name, (None, None))
        if not tool_def or not category:
            return {"mode": "legacy", "status": "not_found", "error": f"Tool not found: {tool_name}"}

        operation_level = self._tool_registry._get_tool_operation_level(tool_name)
        invocation_mode = self._tool_registry._normalize_invocation_mode("user_requested")

        if not invocation_mode.allows(operation_level):
            pending_store = get_pending_action_store()
            action_id = f"legacy_{tool_name}_{int(utc_now().timestamp())}"

            await pending_store.create_action(
                tool_call_id=tool_name,
                tool_name=tool_name,
                arguments=json.dumps(arguments),
                operation_level=operation_level.value,
                invocation_mode="user_requested",
                requester_user_id=user_id,
                requester_user_roles=user_roles,
                reason="Legacy tool requires approval",
                entity_type=mapping.object_type if (mapping := self._mappings.get(tool_name)) else tool_name,
                entity_id=arguments.get("id", ""),
                before_snapshot={},
                after_snapshot={},
                json_patch=[],
            )

            return {
                "mode": "legacy",
                "status": "pending_approval",
                "pending_action_id": action_id,
            }

        executor = self._tool_registry._executors.get(category)
        if not executor:
            return {"mode": "legacy", "status": "no_executor", "error": f"No executor for category: {category.value}"}

        tool_result = await executor.execute_tool_call(
            tool_call_id=f"legacy_{tool_name}", tool_name=tool_name, arguments=arguments
        )

        return {
            "mode": "legacy",
            "status": tool_result.status.value,
            "result": tool_result.result,
            "error": tool_result.error_message,
        }

    def get_metrics(self) -> BridgeMetrics:
        """获取桥接器指标"""
        return self._metrics

    def get_migration_status(self) -> dict[str, Any]:
        """获取迁移状态"""
        return {
            "mode": self._mode.value,
            "total_mapped": len(self._mappings),
            "migrated": self._metrics.migrated_tools,
            "pending": self._metrics.pending_tools,
            "metrics": {
                "total_calls": self._metrics.total_calls,
                "aip_calls": self._metrics.aip_calls,
                "legacy_calls": self._metrics.legacy_calls,
                "fallback_calls": self._metrics.fallback_calls,
                "failed_calls": self._metrics.failed_calls,
            },
            "mappings": {
                name: {
                    "object_type": m.object_type,
                    "action_name": m.action_name,
                    "migration_status": m.migration_status.value,
                }
                for name, m in self._mappings.items()
            },
        }

    def mark_tool_migrated(self, tool_name: str) -> bool:
        """标记工具已迁移"""
        if tool_name not in self._mappings:
            return False

        self._mappings[tool_name].migration_status = MigrationStatus.COMPLETED
        self._metrics.migrated_tools += 1
        logger.info(f"Tool '{tool_name}' marked as migrated")

        return True

    def get_tools_for_llm(
        self, user_id: str, user_roles: list[str], mode: BridgeMode | None = None
    ) -> list[dict[str, Any]]:
        """
        获取 LLM 可用的工具列表

        根据模式返回不同的工具集：
        - AIP_ONLY: 仅返回 AIP Functions
        - LEGACY_ONLY: 仅返回 Legacy 工具
        - DUAL: 返回两者

        Args:
            user_id: 用户ID
            user_roles: 用户角色
            mode: 强制使用的模式

        Returns:
            工具列表
        """
        effective_mode = mode or self._mode

        if effective_mode == BridgeMode.AIP_ONLY:
            return self._aip_app.get_tools_for_user(user_id, user_roles)

        if effective_mode == BridgeMode.LEGACY_ONLY:
            return self._get_legacy_tools(user_id, user_roles)

        aip_tools = self._aip_app.get_tools_for_user(user_id, user_roles)
        legacy_tools = self._get_legacy_tools(user_id, user_roles)

        return aip_tools + legacy_tools

    def _get_legacy_tools(self, user_id: str, user_roles: list[str]) -> list[dict[str, Any]]:
        """获取 Legacy 工具列表"""
        tools = []
        normalized_roles = {r.lower() for r in user_roles}

        if "admin" in normalized_roles:
            for _tool_name, (tool_def, _) in self._tool_registry._tools.items():
                tools.append(tool_def.to_openai_schema())
            return tools

        for tool_name, (tool_def, _) in self._tool_registry._tools.items():
            if self._tool_registry._is_allowed_by_user(tool_name, user_id, user_roles):
                tools.append(tool_def.to_openai_schema())

        return tools


_bridge_instance: ToolBridge | None = None


def get_tool_bridge() -> ToolBridge | None:
    """获取全局工具桥接器实例"""
    return _bridge_instance


async def initialize_tool_bridge(
    tool_registry: ToolRegistry, aip_app: Any, mode: BridgeMode = BridgeMode.DUAL
) -> ToolBridge:
    """初始化全局工具桥接器"""
    global _bridge_instance
    _bridge_instance = ToolBridge(tool_registry, aip_app, mode)
    await _bridge_instance.initialize()
    return _bridge_instance


__all__ = [
    "TOOL_TO_OBJECT_MAPPING",
    "BridgeMetrics",
    "BridgeMode",
    "MigrationStatus",
    "ToolBridge",
    "ToolMapping",
    "get_tool_bridge",
    "initialize_tool_bridge",
]
