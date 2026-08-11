"""
LangChain 工具链适配器层

将系统中原有的 ToolRegistry、审批流拦截、权限体系
转化为现成可用的 LangChain BaseTool，供 LangGraph 直接驱动。
"""

import json
import re
from typing import Any, Literal

from langchain_core.tools import BaseTool
from pydantic import BaseModel, ConfigDict, Field, create_model

from src.infrastructure.ai.tools.base import (
    BaseToolDefinition,
    InvocationMode,
    ToolExecutionResult,
    ToolExecutionStatus,
)
from src.infrastructure.ai.tools.registry import ToolRegistry
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)

_JSON_SCHEMA_SCALAR_TYPES: dict[str, Any] = {
    "string": str,
    "integer": int,
    "number": float,
    "boolean": bool,
}


class RequiresApprovalException(Exception):
    """
    业务异常哨兵：工具触发了高危操作拦截，需要挂起状态机等待审批。
    会被外层的 Graph 框架 (如 prebuilt ToolNode 的包被层) 捕获，
    进而转化为 LangGraph 的 NodeInterrupt 或特定的状态分支。
    """

    def __init__(self, action_id: str, tool_name: str, message: str):
        super().__init__(message)
        self.action_id = action_id
        self.tool_name = tool_name
        self.message = message


class PermissionDeniedException(Exception):
    """
    业务异常哨兵：越权调用。
    """

    pass


class ToolValidationFailure(Exception):
    def __init__(self, code: str, message: str, retryable: bool = True):
        super().__init__(message)
        self.code, self.message, self.retryable = code, message, retryable


class ToolResourceNotFoundFailure(Exception):
    def __init__(self, code: str, message: str, retryable: bool = True):
        super().__init__(message)
        self.code, self.message, self.retryable = code, message, retryable


def _create_pydantic_model_from_schema(name: str, schema: dict[str, Any]) -> type[BaseModel]:
    """将 OpenAI 风格的 JSON schema 参数描述转化为 Pydantic Model (可选适配)"""
    if not isinstance(schema, dict):
        schema = {}

    properties = schema.get("properties")
    if not isinstance(properties, dict):
        properties = {}

    required_fields = {str(item).strip() for item in (schema.get("required") or []) if str(item).strip()}

    model_fields: dict[str, Any] = {}
    for field_name, field_schema in properties.items():
        normalized_field_name = str(field_name).strip()
        if not normalized_field_name:
            continue

        resolved_schema = field_schema if isinstance(field_schema, dict) else {}
        annotation = _schema_to_python_type(
            model_name=f"{name}_{normalized_field_name}",
            schema=resolved_schema,
        )
        is_required = normalized_field_name in required_fields

        if is_required:
            default_value = ...
        else:
            default_value = resolved_schema.get("default", None)

        field_description = resolved_schema.get("description")
        if field_description is not None:
            default = Field(default=default_value, description=str(field_description))
        else:
            default = default_value

        model_fields[normalized_field_name] = (annotation, default)

    return create_model(
        _normalize_model_name(name),
        __config__=ConfigDict(extra="forbid"),
        **model_fields,
    )


def _normalize_model_name(name: str) -> str:
    normalized = re.sub(r"[^0-9a-zA-Z_]+", "_", str(name or "").strip()) or "ToolArgs"
    if normalized[0].isdigit():
        normalized = f"ToolArgs_{normalized}"
    return normalized


def _resolve_schema_types(schema: dict[str, Any]) -> list[str]:
    raw_type = schema.get("type")
    if isinstance(raw_type, str):
        return [raw_type]
    if isinstance(raw_type, list):
        return [str(item).strip() for item in raw_type if str(item).strip()]
    if isinstance(schema.get("properties"), dict):
        return ["object"]
    if isinstance(schema.get("items"), dict):
        return ["array"]
    return []


def _schema_to_python_type(model_name: str, schema: dict[str, Any]) -> Any:
    normalized_schema = schema if isinstance(schema, dict) else {}
    schema_types = _resolve_schema_types(normalized_schema)
    non_null_types = [item for item in schema_types if item != "null"]
    is_nullable = "null" in schema_types

    enum_values = normalized_schema.get("enum")
    if isinstance(enum_values, list) and enum_values:
        non_null_values = tuple(value for value in enum_values if value is not None)
        if non_null_values:
            annotation: Any = Literal.__getitem__(non_null_values)
            return annotation | None if is_nullable else annotation

    if "array" in non_null_types:
        item_schema = normalized_schema.get("items")
        item_annotation = _schema_to_python_type(
            model_name=f"{model_name}_item",
            schema=item_schema if isinstance(item_schema, dict) else {},
        )
        annotation = list[item_annotation]
        return annotation | None if is_nullable else annotation

    if "object" in non_null_types:
        if isinstance(normalized_schema.get("properties"), dict):
            annotation = _create_pydantic_model_from_schema(model_name, normalized_schema)
        else:
            annotation = dict[str, Any]
        return annotation | None if is_nullable else annotation

    for json_type in non_null_types:
        mapped = _JSON_SCHEMA_SCALAR_TYPES.get(json_type)
        if mapped is not None:
            return mapped | None if is_nullable else mapped

    return Any


class GovernedLangChainTool(BaseTool):
    """
    带业务治理约束的 LangChain 工具。
    拦截了 LangChain 原生的执行链路，强行灌入我们系统的 ToolRegistry 和权限上下文。
    """

    name: str = ""
    description: str = ""
    args_schema: type[BaseModel] | None = None
    model_config = ConfigDict(arbitrary_types_allowed=True)

    # 绕过 pydantic 的直接校验，后续通过 inject 的字段运行
    _registry: ToolRegistry
    _definition: BaseToolDefinition
    _user_id: str | None = None
    _user_roles: list[str] | None = None
    _invocation_mode: InvocationMode = InvocationMode.AGENT_AUTONOMOUS

    def __init__(
        self,
        registry: ToolRegistry,
        definition: BaseToolDefinition,
        user_id: str | None = None,
        user_roles: list[str] | None = None,
        invocation_mode: InvocationMode = InvocationMode.AGENT_AUTONOMOUS,
        **kwargs,
    ):
        tool_schema = {
            "type": "object",
            "properties": dict(definition.parameters or {}),
            "required": list(definition.required_params or []),
        }
        args_schema = _create_pydantic_model_from_schema(
            f"{definition.name}_args",
            tool_schema,
        )
        super().__init__(name=definition.name, description=definition.description, args_schema=args_schema, **kwargs)
        self._registry = registry
        self._definition = definition
        self._user_id = user_id
        self._user_roles = user_roles
        self._invocation_mode = invocation_mode

    def _run(self, *args: Any, **kwargs: Any) -> Any:
        # LangChain 默认在同步链执行 _run
        raise NotImplementedError("GovernedLangChainTool 仅支持异步运行 (_arun)")

    async def _arun(self, *args: Any, **kwargs: Any) -> Any:
        """异步执行"""
        import uuid

        # 组装 JSON 参数传递给旧版的 execute_tool_call
        tool_call_id = f"call_{uuid.uuid4().hex[:8]}"
        arguments_json = json.dumps(kwargs, ensure_ascii=False)

        logger.debug(f"LangChain Tool Adapter routing {self.name} -> ToolRegistry")

        result: ToolExecutionResult = await self._registry.execute_tool_call(
            tool_call_id=tool_call_id,
            tool_name=self.name,
            arguments=arguments_json,
            user_id=self._user_id,
            user_roles=self._user_roles,
            invocation_mode=self._invocation_mode,
        )

        status = result.status

        if status == ToolExecutionStatus.PENDING_APPROVAL:
            # 关键设计：将其转化为异常，阻断 Graph 直接往下走
            action_id = result.result.get("action_id") if isinstance(result.result, dict) else "unknown"
            raise RequiresApprovalException(
                action_id=action_id, tool_name=self.name, message=result.error_message or "需要人工审批"
            )

        if status == ToolExecutionStatus.PERMISSION_DENIED:
            raise PermissionDeniedException(result.error_message or "权限不足")

        if status == ToolExecutionStatus.VALIDATION_ERROR:
            raise ToolValidationFailure(code=result.code, message=result.error_message or "")

        if status == ToolExecutionStatus.NOT_FOUND:
            raise ToolResourceNotFoundFailure(code=result.code, message=result.error_message or "")

        if status in [ToolExecutionStatus.ERROR, ToolExecutionStatus.TIMEOUT]:
            # 出错时不应挂起，而是返回错误信息，让 LLM 进行 Reflection (反思) 和重试
            return f"工具执行失败 ({status.value}): {result.error_message}"

        # ToolExecutionStatus.SUCCESS
        if isinstance(result.result, (dict, list)):
            return json.dumps(result.result, ensure_ascii=False)
        return str(result.result)


class ToolAdapterFactory:
    """
    负责将系统注册表中的工具集导出为 LangChain 格式的工具数组。
    """

    def __init__(self, registry: ToolRegistry):
        self._registry = registry

    @staticmethod
    def create_adapted_tool(
        tool_name: str,
        registry: ToolRegistry,
        *,
        user_id: str | None = None,
        user_roles: list[str] | None = None,
        invocation_mode: InvocationMode = InvocationMode.AGENT_AUTONOMOUS,
    ) -> BaseTool:
        registry_entry = registry._tools.get(tool_name)
        if registry_entry is None:
            raise KeyError(f"Tool '{tool_name}' is not registered")

        definition, _category = registry_entry
        return GovernedLangChainTool(
            registry=registry,
            definition=definition,
            user_id=user_id,
            user_roles=user_roles,
            invocation_mode=invocation_mode,
        )

    def export_tools(
        self,
        user_id: str | None = None,
        user_roles: list[str] | None = None,
        invocation_mode: InvocationMode = InvocationMode.AGENT_AUTONOMOUS,
    ) -> list[BaseTool]:
        """
        导出绑定了特定上下问（用户、权限）的工具数组，用于传递给 llm.bind_tools()
        """
        lc_tools = []
        for tool_name, (_definition, _category) in self._registry._tools.items():
            # 提前做好权限筛查，未授权直接不装配到 LangChain 里
            if not self._registry._is_allowed_by_user(tool_name, user_id, user_roles):
                continue

            if not self._registry._is_allowed_by_invocation_mode(tool_name, invocation_mode):
                continue

            lc_tool = self.create_adapted_tool(
                tool_name=tool_name,
                registry=self._registry,
                user_id=user_id,
                user_roles=user_roles,
                invocation_mode=invocation_mode,
            )
            lc_tools.append(lc_tool)

        return lc_tools
