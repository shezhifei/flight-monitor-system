"""
AIP Application - 统一入口

整合所有 AIP 组件，提供统一的 AI 应用接口。
支持 Legacy、AIP 和双轨三种运行模式。
"""

from __future__ import annotations

from collections.abc import Awaitable, Callable
from dataclasses import dataclass
from enum import StrEnum
from typing import Any

from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class AIPMode(StrEnum):
    """AIP 运行模式"""

    AIP_ONLY = "aip_only"
    LEGACY_ONLY = "legacy_only"
    DUAL = "dual"


@dataclass
class AIPRuntimeConfig:
    """AIP 运行时配置"""

    ontology_enabled: bool = True
    object_acl_enabled: bool = True
    hitl_enabled: bool = True
    action_approval_default: bool = True
    max_action_depth: int = 3
    max_context_objects: int = 10
    cache_enabled: bool = True
    cache_ttl_seconds: int = 300
    mode: AIPMode = AIPMode.DUAL
    legacy_fallback: bool = True
    migration_progress: float = 0.0


class AIPApplication:
    """
    AIP 应用主类

    整合 Ontology、Function Registry、ACL、Action Executor、ToolBridge 等组件，
    提供统一的 AI 交互接口。

    支持三种运行模式：
    - AIP_ONLY: 仅使用 AIP 模式
    - LEGACY_ONLY: 仅使用 Legacy 工具模式
    - DUAL: 双轨并行，自动回退
    """

    def __init__(self, config: AIPRuntimeConfig | None = None):
        self.config = config or AIPRuntimeConfig()
        self._initialized = False

        self._ontology_registry = None
        self._function_registry = None
        self._object_acl = None
        self._action_executor = None
        self._context_bridge = None
        self._legacy_adapter = None
        self._tool_bridge = None

        self._action_handlers: dict[str, Callable] = {}

    async def initialize(self) -> None:
        """初始化 AIP 应用"""
        if self._initialized:
            return

        logger.info(f"Initializing AIP Application (mode: {self.config.mode.value})...")

        await self._init_ontology()
        await self._init_function_registry()
        await self._init_security()
        await self._init_action_executor()
        await self._init_context_bridge()
        await self._init_action_handlers()

        if self.config.mode != AIPMode.AIP_ONLY:
            await self._init_tool_bridge()

        self._initialized = True
        logger.info(f"AIP Application initialized successfully (mode: {self.config.mode.value})")

    async def _init_ontology(self) -> None:
        """初始化 Ontology"""
        if not self.config.ontology_enabled:
            logger.info("Ontology disabled, skipping initialization")
            return

        from ..ontology.schema import get_ontology_registry

        self._ontology_registry = get_ontology_registry()
        logger.info(f"Ontology loaded: {len(self._ontology_registry.get_schema('default').objects)} objects")

    async def _init_function_registry(self) -> None:
        """初始化 Function Registry"""
        from .function_registry import get_aip_registry

        self._function_registry = get_aip_registry()

        if not self._function_registry.is_initialized:
            self._function_registry.initialize_from_ontology()

        logger.info(f"Function Registry loaded: {len(self._function_registry.get_all())} functions")

    async def _init_security(self) -> None:
        """初始化安全层"""
        if not self.config.object_acl_enabled:
            logger.info("Object ACL disabled, skipping initialization")
            return

        from ..ontology.security import get_object_acl

        self._object_acl = get_object_acl()

        self._object_acl.grant(
            principal_type="role",
            principal_id="admin",
            object_type="*",
            permission="admin",
            description="Admin role has full access",
        )

        self._object_acl.grant(
            principal_type="role",
            principal_id="operator",
            object_type="*",
            permission="read",
            description="Operator role can read all objects",
        )

        self._object_acl.grant(
            principal_type="role",
            principal_id="operator",
            object_type="*",
            permission="execute",
            description="Operator role can execute actions",
        )

        logger.info("Object ACL initialized")

    async def _init_action_executor(self) -> None:
        """初始化 Action Executor"""
        if not self.config.hitl_enabled:
            logger.info("HITL disabled, using direct execution")
            self._action_executor = None
            return

        from ..tools.pending_actions import get_pending_action_store
        from .action_executor import AIPActionExecutor

        pending_store = get_pending_action_store()

        self._action_executor = AIPActionExecutor(
            ontology_registry=self._ontology_registry,
            object_acl=self._object_acl,
            pending_action_store=pending_store,
            action_handlers=self._action_handlers,
        )

        logger.info("Action Executor initialized")

    async def _init_context_bridge(self) -> None:
        """初始化 Context Bridge"""
        if not self.config.ontology_enabled:
            logger.info("Context Bridge disabled (Ontology disabled)")
            return

        from .context_bridge import OntologyContextBridge

        self._context_bridge = OntologyContextBridge(ontology_registry=self._ontology_registry)

        logger.info("Context Bridge initialized")

    async def _init_tool_bridge(self) -> None:
        """初始化 Tool Bridge（双轨模式）"""
        try:
            from src.infrastructure.ai.tools.registry import get_tool_registry

            from .tool_bridge import BridgeMode, initialize_tool_bridge

            tool_registry = get_tool_registry()

            bridge_mode_map = {
                AIPMode.DUAL: BridgeMode.DUAL,
                AIPMode.AIP_ONLY: BridgeMode.AIP_ONLY,
                AIPMode.LEGACY_ONLY: BridgeMode.LEGACY_ONLY,
            }

            self._tool_bridge = await initialize_tool_bridge(
                tool_registry=tool_registry, aip_app=self, mode=bridge_mode_map.get(self.config.mode, BridgeMode.DUAL)
            )

            logger.info(f"Tool Bridge initialized (mode: {self._tool_bridge.get_mode().value})")

        except Exception as exc:  # noqa: BLE001 - optional bridge; failure must not block AIP init
            logger.warning("Failed to initialize Tool Bridge", exc_info=exc)
            self._tool_bridge = None

    async def _init_action_handlers(self) -> None:
        """初始化 Action Handlers"""
        from .action_handlers import register_all_handlers

        register_all_handlers(self)
        logger.info("Action Handlers initialized")

    def register_action_handler(
        self, object_type: str, action: str, handler: Callable[[str, dict[str, Any]], Awaitable[dict[str, Any]]]
    ) -> None:
        """注册 Action 处理器"""
        key = f"{object_type}.{action}"
        self._action_handlers[key] = handler

        if self._action_executor:
            self._action_executor.register_handler(object_type, action, handler)

        logger.info(f"Registered action handler: {key}")

    def set_mode(self, mode: AIPMode) -> None:
        """设置运行模式"""
        if self.config.mode == mode:
            return

        logger.info(f"Switching AIP mode: {self.config.mode.value} -> {mode.value}")
        self.config.mode = mode

        if self._tool_bridge:
            from .tool_bridge import BridgeMode

            bridge_mode_map = {
                AIPMode.DUAL: BridgeMode.DUAL,
                AIPMode.AIP_ONLY: BridgeMode.AIP_ONLY,
                AIPMode.LEGACY_ONLY: BridgeMode.LEGACY_ONLY,
            }
            self._tool_bridge.set_mode(bridge_mode_map.get(mode, BridgeMode.DUAL))

    def get_mode(self) -> AIPMode:
        """获取当前运行模式"""
        return self.config.mode

    async def execute_action(
        self,
        principal: str,
        object_type: str,
        object_id: str,
        action: str,
        parameters: dict[str, Any],
        invocation_mode: str = "user_requested",
    ) -> dict[str, Any]:
        """
        执行 Action

        根据运行模式决定执行路径：
        - AIP_ONLY: 直接使用 AIP Action Executor
        - LEGACY_ONLY: 使用 Tool Bridge 的 Legacy 执行
        - DUAL: 优先 AIP，失败时回退到 Legacy

        Args:
            principal: 执行主体
            object_type: 对象类型
            object_id: 对象ID
            action: Action 名称
            parameters: Action 参数
            invocation_mode: 调用模式

        Returns:
            执行结果
        """
        if not self._initialized:
            await self.initialize()

        if self.config.mode == AIPMode.LEGACY_ONLY:
            return await self._execute_legacy(action, parameters, {"principal": principal}, principal)

        try:
            return await self._execute_aip(principal, object_type, object_id, action, parameters, invocation_mode)
        except Exception as exc:
            logger.error(
                "AIP execution failed action=%s object_type=%s object_id=%s",
                action,
                object_type,
                object_id,
                exc_info=exc,
            )

            if self.config.mode == AIPMode.DUAL and self.config.legacy_fallback:
                logger.info("Falling back to Legacy mode")
                return await self._execute_legacy(action, parameters, {"principal": principal}, principal)

            return {
                "status": "error",
                "error": "action_failed",
                "mode": "aip",
                "fallback_available": self.config.mode == AIPMode.DUAL,
            }

    async def _execute_aip(
        self,
        principal: str,
        object_type: str,
        object_id: str,
        action: str,
        parameters: dict[str, Any],
        invocation_mode: str,
    ) -> dict[str, Any]:
        """通过 AIP 模式执行"""
        if not self._action_executor:
            return {"status": "error", "error": "AIP Action Executor not initialized", "mode": "aip"}

        result = await self._action_executor.execute(
            principal=principal,
            object_type=object_type,
            object_id=object_id,
            action=action,
            parameters=parameters,
            invocation_mode=invocation_mode,
        )

        result_dict = result.to_dict()
        result_dict["mode"] = "aip"
        return result_dict

    async def _execute_legacy(
        self, tool_name: str, parameters: dict[str, Any], user_context: dict[str, Any], principal: str
    ) -> dict[str, Any]:
        """通过 Legacy 模式执行"""
        if not self._tool_bridge:
            return {"status": "error", "error": "Tool Bridge not initialized", "mode": "legacy"}

        return await self._tool_bridge.execute_with_fallback(
            tool_name=tool_name, arguments=parameters, user_context=user_context, principal=principal
        )

    def get_tools_for_user(
        self, user_id: str, user_roles: list[str], object_types: list[str] | None = None
    ) -> list[dict[str, Any]]:
        """
        获取用户可用的工具列表

        根据运行模式返回不同的工具集。

        Args:
            user_id: 用户ID
            user_roles: 用户角色列表
            object_types: 限制的对象类型

        Returns:
            工具 Schema 列表
        """
        if not self._initialized:
            logger.warning("AIP not initialized, returning empty tools")
            return []

        if self.config.mode == AIPMode.LEGACY_ONLY:
            if self._tool_bridge:
                return self._tool_bridge.get_tools_for_llm(user_id, user_roles)
            return []

        if self.config.mode == AIPMode.AIP_ONLY:
            return self._function_registry.get_tool_schemas(
                user_id=user_id, user_roles=user_roles, object_types=object_types
            )

        if self._tool_bridge:
            from .tool_bridge import BridgeMode

            return self._tool_bridge.get_tools_for_llm(user_id, user_roles, mode=BridgeMode.DUAL)

        return self._function_registry.get_tool_schemas(
            user_id=user_id, user_roles=user_roles, object_types=object_types
        )

    def build_system_prompt(self, object_types: list[str] | None = None) -> str:
        """
        构建系统提示词

        Args:
            object_types: 包含的对象类型

        Returns:
            系统提示词
        """
        if not self._context_bridge:
            return ""

        return self._context_bridge.build_system_prompt(
            object_types=object_types, include_actions=True, include_relationships=True
        )

    def get_object_schema(self, object_type: str) -> dict[str, Any] | None:
        """获取对象 Schema"""
        if not self._ontology_registry:
            return None

        obj = self._ontology_registry.get_object(object_type, "default")
        return obj.to_schema_dict() if obj else None

    def get_object_actions(self, object_type: str) -> list[dict[str, Any]]:
        """获取对象的所有 Actions"""
        if not self._ontology_registry:
            return []

        actions = self._ontology_registry.get_object_actions(object_type, "default")
        return [a.to_schema_dict() for a in actions]

    def check_permission(
        self, principal: str, object_type: str, object_id: str | None, permission: str
    ) -> dict[str, Any]:
        """检查权限"""
        if not self._object_acl:
            return {"allowed": True, "reason": "ACL not initialized"}

        from ..ontology.security import Permission

        try:
            perm = Permission(permission.upper())
        except ValueError:
            perm = Permission.READ

        result = self._object_acl.check_permission(
            principal=principal, object_type=object_type, object_id=object_id, permission=perm
        )

        return {"allowed": result.allowed, "reason": result.reason, "requires_approval": result.requires_approval}

    def get_migration_status(self) -> dict[str, Any]:
        """获取迁移状态"""
        status = {
            "mode": self.config.mode.value,
            "migration_progress": self.config.migration_progress,
            "initialized": self._initialized,
        }

        if self._tool_bridge:
            status["bridge"] = self._tool_bridge.get_migration_status()

        return status

    def get_metrics(self) -> dict[str, Any]:
        """获取运行指标"""
        metrics = {
            "mode": self.config.mode.value,
            "initialized": self._initialized,
            "function_count": len(self._function_registry.get_all()) if self._function_registry else 0,
            "handler_count": len(self._action_handlers),
        }

        if self._tool_bridge:
            bridge_metrics = self._tool_bridge.get_metrics()
            metrics["bridge_metrics"] = {
                "total_calls": bridge_metrics.total_calls,
                "aip_calls": bridge_metrics.aip_calls,
                "legacy_calls": bridge_metrics.legacy_calls,
                "fallback_calls": bridge_metrics.fallback_calls,
                "failed_calls": bridge_metrics.failed_calls,
            }

        return metrics

    def adapt_legacy_tools(self, tool_definitions: list[Any]) -> int:
        """
        适配 Legacy Tools

        Args:
            tool_definitions: 现有的 ToolDefinition 列表

        Returns:
            成功适配的数量
        """
        if not self._legacy_adapter:
            from .legacy_adapter import LegacyToolAdapter

            self._legacy_adapter = LegacyToolAdapter(
                aip_registry=self._function_registry, ontology_registry=self._ontology_registry
            )

        adapted = self._legacy_adapter.register_legacy_tools_batch(tool_definitions)
        return len(adapted)

    @property
    def ontology_registry(self):
        return self._ontology_registry

    @property
    def function_registry(self):
        return self._function_registry

    @property
    def object_acl(self):
        return self._object_acl

    @property
    def action_executor(self):
        return self._action_executor

    @property
    def tool_bridge(self):
        return self._tool_bridge

    @property
    def is_initialized(self) -> bool:
        return self._initialized


_aip_app: AIPApplication | None = None


def get_aip_app() -> AIPApplication:
    """获取全局 AIP Application 实例"""
    global _aip_app
    if _aip_app is None:
        _aip_app = AIPApplication()
    return _aip_app


async def initialize_aip_app(config: AIPRuntimeConfig | None = None) -> AIPApplication:
    """初始化并返回全局 AIP Application"""
    global _aip_app
    _aip_app = AIPApplication(config)
    await _aip_app.initialize()
    return _aip_app


async def initialize_aip_from_config(ai_config: Any) -> AIPApplication:
    """从 AIConfig 初始化 AIP Application"""
    from config.ai_config import AIPMode as ConfigAIPMode

    runtime_config = AIPRuntimeConfig()

    if hasattr(ai_config, "aip") and ai_config.aip:
        aip_config = ai_config.aip
        runtime_config.ontology_enabled = aip_config.ontology_enabled
        runtime_config.object_acl_enabled = aip_config.object_acl_enabled
        runtime_config.hitl_enabled = aip_config.hitl_enabled
        runtime_config.action_approval_default = aip_config.action_approval_default
        runtime_config.max_action_depth = aip_config.max_action_depth
        runtime_config.max_context_objects = aip_config.max_context_objects
        runtime_config.cache_enabled = aip_config.cache_enabled
        runtime_config.cache_ttl_seconds = aip_config.cache_ttl_seconds
        runtime_config.legacy_fallback = aip_config.legacy_fallback
        runtime_config.migration_progress = aip_config.migration_progress

        mode_value = aip_config.mode
        if isinstance(mode_value, str):
            mode_map = {
                "aip_only": AIPMode.AIP_ONLY,
                "legacy_only": AIPMode.LEGACY_ONLY,
                "dual": AIPMode.DUAL,
            }
            runtime_config.mode = mode_map.get(mode_value, AIPMode.DUAL)
        elif hasattr(mode_value, "value"):
            if mode_value == ConfigAIPMode.AIP_ONLY:
                runtime_config.mode = AIPMode.AIP_ONLY
            elif mode_value == ConfigAIPMode.LEGACY_ONLY:
                runtime_config.mode = AIPMode.LEGACY_ONLY
            else:
                runtime_config.mode = AIPMode.DUAL

    return await initialize_aip_app(runtime_config)


__all__ = [
    "AIPApplication",
    "AIPMode",
    "AIPRuntimeConfig",
    "get_aip_app",
    "initialize_aip_app",
    "initialize_aip_from_config",
]
