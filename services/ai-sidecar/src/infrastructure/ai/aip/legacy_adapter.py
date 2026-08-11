"""
Legacy Tool 到 AIP Function 适配器

将现有的 ToolDefinition/ToolExecutor 适配为 AIPFunction，
保持向后兼容的同时启用 AIP 模式。
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, ClassVar

from src.infrastructure.logging.core import get_logger

if TYPE_CHECKING:
    from ..ontology.schema import OntologyRegistry
    from .function_registry import AIPFunction, RiskLevel

logger = get_logger(__name__)


@dataclass
class ToolToFunctionMapping:
    """Tool 到 Function 的映射配置"""

    tool_name: str
    object_type: str
    action_name: str
    requires_approval: bool = False
    risk_level: str = "NORMAL"


class LegacyToolAdapter:
    """
    Legacy Tool 适配器

    将现有的 BaseToolDefinition 和 BaseToolExecutor
    适配为 AIPFunction 并注册到 AIPFunctionRegistry。
    """

    TOOL_CATEGORY_TO_OBJECT_TYPE: ClassVar[dict[str, str]] = {
        "flight": "Flight",
        "stand": "Stand",
        "team": "Team",
        "equipment": "Equipment",
        "anomaly": "Anomaly",
        "todo": "Todo",
        "dispatch_query": "Dispatch",
        "flight_event": "Flight",
        "business_case": "BusinessCase",
        "report": "Report",
    }

    def __init__(self, aip_registry: Any, ontology_registry: OntologyRegistry):
        self._aip_registry = aip_registry
        self._ontology = ontology_registry
        self._mappings: dict[str, ToolToFunctionMapping] = {}

    def register_legacy_tool(
        self, tool_def: Any, custom_mapping: ToolToFunctionMapping | None = None
    ) -> AIPFunction | None:
        """
        将 Legacy Tool 注册为 AIPFunction

        Args:
            tool_def: 现有的 BaseToolDefinition 实例
            custom_mapping: 自定义映射配置

        Returns:
            注册的 AIPFunction，如果失败则返回 None
        """
        from ..tools.base import OperationLevel
        from .function_registry import AIPFunction, FunctionCategory, RiskLevel

        if custom_mapping:
            mapping = custom_mapping
        else:
            mapping = self._infer_mapping(tool_def)

        if not mapping:
            logger.warning(f"Cannot infer mapping for tool: {tool_def.name}")
            return None

        parameters_schema = self._convert_parameters_schema(tool_def.parameters)

        risk_level = self._map_operation_level_to_risk(getattr(tool_def, "operation_level", OperationLevel.READ))

        if mapping.requires_approval:
            risk_level = RiskLevel.MEDIUM

        aip_function = AIPFunction(
            name=mapping.action_name,
            category=FunctionCategory.OBJECT_ACTION,
            object_type=mapping.object_type,
            action_name=mapping.action_name,
            description=tool_def.description,
            parameters_schema=parameters_schema,
            requires_approval=mapping.requires_approval or risk_level in (RiskLevel.HIGH, RiskLevel.CRITICAL),
            risk_level=risk_level,
        )

        self._aip_registry.register(aip_function)
        self._mappings[tool_def.name] = mapping

        logger.info(f"Registered legacy tool '{tool_def.name}' as AIP function '{aip_function.name}'")

        return aip_function

    def register_legacy_tools_batch(
        self, tool_definitions: list[Any], custom_mappings: dict[str, ToolToFunctionMapping] | None = None
    ) -> list[AIPFunction]:
        """批量注册 Legacy Tools"""
        results = []

        for tool_def in tool_definitions:
            custom_map = custom_mappings.get(tool_def.name) if custom_mappings else None
            result = self.register_legacy_tool(tool_def, custom_map)
            if result:
                results.append(result)

        return results

    def get_executor_bridge(self, tool_executor: Any) -> ExecutorBridge:
        """
        获取执行器桥接器

        Args:
            tool_executor: 现有的 BaseToolExecutor 实例

        Returns:
            ExecutorBridge 实例
        """
        return ExecutorBridge(tool_executor=tool_executor, mapping_registry=self._mappings)

    def _infer_mapping(self, tool_def: Any) -> ToolToFunctionMapping | None:
        """从 Tool 定义推断映射"""
        tool_name = tool_def.name.lower()

        for category_prefix, object_type in self.TOOL_CATEGORY_TO_OBJECT_TYPE.items():
            if tool_name.startswith(category_prefix) or category_prefix in tool_name:
                action_name = tool_name.replace(f"{category_prefix}_", "").replace("_", "_")

                if (
                    tool_name.startswith("change_")
                    or "update" in tool_name
                    or "delete" in tool_name
                    or "remove" in tool_name
                ):
                    requires_approval = True
                else:
                    requires_approval = False

                return ToolToFunctionMapping(
                    tool_name=tool_def.name,
                    object_type=object_type,
                    action_name=f"{object_type}.{action_name}",
                    requires_approval=requires_approval,
                )

        return ToolToFunctionMapping(tool_name=tool_def.name, object_type="Unknown", action_name=tool_def.name)

    @staticmethod
    def _convert_parameters_schema(parameters: dict[str, Any]) -> dict[str, Any]:
        """转换参数 Schema"""
        if "properties" in parameters:
            return parameters

        converted = {"type": "object", "properties": {}, "required": []}

        for key, value in parameters.items():
            if isinstance(value, dict):
                converted["properties"][key] = value
            else:
                converted["properties"][key] = {"type": "string"}

        return converted

    @staticmethod
    def _map_operation_level_to_risk(operation_level: Any) -> RiskLevel:
        """将操作级别映射到风险等级"""
        from ..tools.base import OperationLevel
        from .function_registry import RiskLevel

        level_map = {
            OperationLevel.READ: RiskLevel.LOW,
            OperationLevel.WORKSPACE_WRITE: RiskLevel.NORMAL,
            OperationLevel.ASSISTED_WRITE: RiskLevel.MEDIUM,
            OperationLevel.CRITICAL_WRITE: RiskLevel.HIGH,
        }

        if hasattr(operation_level, "value"):
            operation_level = operation_level.value

        return level_map.get(operation_level, RiskLevel.NORMAL)


class ExecutorBridge:
    """
    执行器桥接器

    将 AIP Action 调用转发到现有的 ToolExecutor 执行，
    处理参数转换和结果封装。
    """

    def __init__(self, tool_executor: Any, mapping_registry: dict[str, ToolToFunctionMapping]):
        self._executor = tool_executor
        self._mappings = mapping_registry

    async def execute_object_action(
        self, action_name: str, parameters: dict[str, Any], tool_call_id: str | None = None
    ) -> dict[str, Any]:
        """
        执行对象 Action

        Args:
            action_name: Action 名称（如 "Flight.change_stand"）
            parameters: Action 参数
            tool_call_id: 工具调用 ID

        Returns:
            执行结果
        """
        if "." in action_name:
            action_name = action_name.split(".", 1)[1]

        if not tool_call_id:
            tool_call_id = f"aip_{action_name}"

        try:
            result = await self._executor.execute_tool_call(
                tool_call_id=tool_call_id, tool_name=action_name, arguments=json.dumps(parameters)
            )

            return self._convert_result(result)

        except Exception as e:  # noqa: BLE001 - executor bridge must return structured error
            logger.error(f"Executor bridge error for action '{action_name}': {e}")
            return {"success": False, "error": str(e), "action": action_name}

    def _convert_result(self, result: Any) -> dict[str, Any]:
        """转换执行结果为统一格式"""
        from ..tools.base import ToolExecutionStatus

        if hasattr(result, "status"):
            status = result.status
            is_success = status == ToolExecutionStatus.SUCCESS
        else:
            is_success = True
            status = ToolExecutionStatus.SUCCESS

        return {
            "success": is_success,
            "status": str(status.value) if hasattr(status, "value") else str(status),
            "result": getattr(result, "result", None),
            "error": getattr(result, "error_message", None),
            "tool_name": getattr(result, "tool_name", None),
        }


class AIPToolAdapter:
    """
    AIP Function 到 Tool 的适配器

    将 AIPFunction 适配为 LLM 可调用的 Tool 格式，
    用于需要返回给 LLM 的工具列表。
    """

    def __init__(self, aip_registry: Any):
        self._aip_registry = aip_registry

    def get_tools_for_llm(
        self, user_id: str, user_roles: list[str], object_types: list[str] | None = None
    ) -> list[dict[str, Any]]:
        """
        获取 LLM 可用的工具列表

        Args:
            user_id: 用户 ID
            user_roles: 用户角色列表
            object_types: 限制的对象类型

        Returns:
            OpenAI 格式的工具列表
        """
        return self._aip_registry.get_tool_schemas(user_id=user_id, user_roles=user_roles, object_types=object_types)

    def enrich_tool_with_context(
        self, tool_schema: dict[str, Any], object_type: str, include_description: bool = True
    ) -> dict[str, Any]:
        """
        使用 Ontology 上下文丰富工具定义

        Args:
            tool_schema: 基础工具 Schema
            object_type: 对象类型

        Returns:
            丰富后的工具定义
        """
        enriched = tool_schema.copy()

        schema = self._aip_registry._ontology.get_object(object_type, "default")
        if not schema:
            return enriched

        if include_description:
            func_def = enriched.get("function", {})
            original_desc = func_def.get("description", "")

            enriched["function"]["description"] = f"{original_desc}\n\nObject: {schema.name}\n{schema.description}"

        return enriched


__all__ = [
    "AIPToolAdapter",
    "ExecutorBridge",
    "LegacyToolAdapter",
    "ToolToFunctionMapping",
]
