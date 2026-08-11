"""
AI工具基类定义

提供工具定义和执行器的基类，所有特定工具应继承这些基类。
"""

import asyncio
import inspect
import json
import time
from abc import ABC, abstractmethod
from collections.abc import Callable, Mapping
from dataclasses import dataclass, field
from datetime import datetime
from enum import StrEnum
from typing import Any, Optional, TypeVar

from src.infrastructure.common.exceptions import LLM_EXCEPTIONS
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)

TValidated = TypeVar("TValidated")


class ToolCategory(StrEnum):
    """工具类别"""

    FLIGHT = "flight"
    FLIGHT_EVENT = "flight_event"
    TODO = "todo"
    SYSTEM = "system"
    CUSTOM = "custom"
    BUSINESS_CASE = "business_case"
    REPORT = "report"
    ADVISOR = "advisor"
    ANOMALY = "anomaly"
    QUERY = "query"
    TEAM = "team"
    EQUIPMENT = "equipment"
    STAND = "stand"
    DISPATCH_QUERY = "dispatch_query"


class OperationLevel(StrEnum):
    """工具操作级别（用于人机协作边界控制）。"""

    READ = "l0_read"
    WORKSPACE_WRITE = "l1_workspace_write"
    ASSISTED_WRITE = "l2_assisted_write"
    CRITICAL_WRITE = "l3_critical_write"


class InvocationMode(StrEnum):
    """工具调用模式。"""

    USER_REQUESTED = "user_requested"
    AGENT_AUTONOMOUS = "agent_autonomous"

    def allows(self, operation_level: OperationLevel) -> bool:
        """判断当前调用模式是否允许指定级别的操作。"""
        if self == InvocationMode.USER_REQUESTED:
            return operation_level in {
                OperationLevel.READ,
                OperationLevel.WORKSPACE_WRITE,
                OperationLevel.ASSISTED_WRITE,
            }

        if self == InvocationMode.AGENT_AUTONOMOUS:
            return operation_level in {
                OperationLevel.READ,
                OperationLevel.WORKSPACE_WRITE,
            }

        return False


class ToolExecutionStatus(StrEnum):
    """工具执行状态"""

    SUCCESS = "success"
    PENDING_APPROVAL = "pending_approval"
    ERROR = "error"
    VALIDATION_ERROR = "validation_error"
    NOT_FOUND = "not_found"
    PERMISSION_DENIED = "permission_denied"
    TIMEOUT = "timeout"


_STATUS_DEFAULT_CODE: dict[ToolExecutionStatus, str] = {
    ToolExecutionStatus.SUCCESS: "TOOL_SUCCESS",
    ToolExecutionStatus.PENDING_APPROVAL: "TOOL_PENDING_APPROVAL",
    ToolExecutionStatus.ERROR: "TOOL_EXECUTION_ERROR",
    ToolExecutionStatus.VALIDATION_ERROR: "TOOL_VALIDATION_ERROR",
    ToolExecutionStatus.NOT_FOUND: "TOOL_NOT_FOUND",
    ToolExecutionStatus.PERMISSION_DENIED: "TOOL_PERMISSION_DENIED",
    ToolExecutionStatus.TIMEOUT: "TOOL_TIMEOUT",
}

_STATUS_DEFAULT_MESSAGE: dict[ToolExecutionStatus, str] = {
    ToolExecutionStatus.SUCCESS: "tool executed successfully",
    ToolExecutionStatus.PENDING_APPROVAL: "tool execution requires human approval",
    ToolExecutionStatus.ERROR: "tool execution failed",
    ToolExecutionStatus.VALIDATION_ERROR: "tool arguments validation failed",
    ToolExecutionStatus.NOT_FOUND: "requested resource was not found",
    ToolExecutionStatus.PERMISSION_DENIED: "permission denied for tool execution",
    ToolExecutionStatus.TIMEOUT: "tool execution timed out",
}

_STATUS_DEFAULT_RECOVERABLE: dict[ToolExecutionStatus, bool] = {
    ToolExecutionStatus.SUCCESS: False,
    ToolExecutionStatus.PENDING_APPROVAL: True,
    ToolExecutionStatus.ERROR: True,
    ToolExecutionStatus.VALIDATION_ERROR: True,
    ToolExecutionStatus.NOT_FOUND: True,
    ToolExecutionStatus.PERMISSION_DENIED: True,
    ToolExecutionStatus.TIMEOUT: True,
}

_STATUS_DEFAULT_RETRYABLE: dict[ToolExecutionStatus, bool] = {
    ToolExecutionStatus.SUCCESS: False,
    ToolExecutionStatus.PENDING_APPROVAL: False,
    ToolExecutionStatus.ERROR: True,
    ToolExecutionStatus.VALIDATION_ERROR: False,
    ToolExecutionStatus.NOT_FOUND: False,
    ToolExecutionStatus.PERMISSION_DENIED: False,
    ToolExecutionStatus.TIMEOUT: True,
}

_STATUS_DEFAULT_SEVERITY: dict[ToolExecutionStatus, str] = {
    ToolExecutionStatus.SUCCESS: "success",
    ToolExecutionStatus.PENDING_APPROVAL: "warning",
    ToolExecutionStatus.ERROR: "error",
    ToolExecutionStatus.VALIDATION_ERROR: "warning",
    ToolExecutionStatus.NOT_FOUND: "warning",
    ToolExecutionStatus.PERMISSION_DENIED: "error",
    ToolExecutionStatus.TIMEOUT: "error",
}


@dataclass
class ToolExecutionResult:
    """工具执行结果"""

    tool_call_id: str
    tool_name: str
    status: ToolExecutionStatus
    result: Any | None = None
    error_message: str | None = None
    execution_time_ms: float = 0.0
    code: str | None = None
    message: str | None = None
    recoverable: bool | None = None
    retryable: bool | None = None
    severity: str | None = None
    metadata: dict[str, Any] = field(default_factory=dict)

    def __post_init__(self) -> None:
        defaults = self._semantic_defaults()
        if self.code is None:
            self.code = defaults["code"]
        if self.message is None:
            self.message = defaults["message"]
        if self.recoverable is None:
            self.recoverable = defaults["recoverable"]
        if self.retryable is None:
            self.retryable = defaults["retryable"]
        if self.severity is None:
            self.severity = defaults["severity"]

    def _semantic_defaults(self) -> dict[str, Any]:
        fallback_message = self.error_message or _STATUS_DEFAULT_MESSAGE[self.status]
        return {
            "code": _STATUS_DEFAULT_CODE[self.status],
            "message": fallback_message,
            "recoverable": _STATUS_DEFAULT_RECOVERABLE[self.status],
            "retryable": _STATUS_DEFAULT_RETRYABLE[self.status],
            "severity": _STATUS_DEFAULT_SEVERITY[self.status],
        }

    def to_message(self) -> dict[str, Any]:
        """转换为AI消息格式"""
        if self.status == ToolExecutionStatus.SUCCESS:
            content = json.dumps(self.result, ensure_ascii=False, default=str)
        elif self.status == ToolExecutionStatus.PENDING_APPROVAL:
            content = json.dumps(
                {
                    "status": self.status.value,
                    "result": self.result,
                    "message": self.message or self.error_message,
                    "code": self.code,
                    "recoverable": self.recoverable,
                    "retryable": self.retryable,
                },
                ensure_ascii=False,
                default=str,
            )
        else:
            content = json.dumps(
                {
                    "error": self.error_message,
                    "status": self.status.value,
                    "code": self.code,
                    "message": self.message or self.error_message,
                    "recoverable": self.recoverable,
                    "retryable": self.retryable,
                },
                ensure_ascii=False,
            )

        return {"role": "tool", "tool_call_id": self.tool_call_id, "content": content}

    def to_contract_payload(self, execution_id: str | None = None) -> dict[str, Any]:
        """转换为 API/SSE 语义契约载荷。"""
        return {
            "success": self.status == ToolExecutionStatus.SUCCESS,
            "status": self.status.value,
            "code": self.code,
            "message": self.message or self.error_message or "",
            "recoverable": bool(self.recoverable),
            "retryable": bool(self.retryable),
            "execution_id": execution_id,
            "tool_name": self.tool_name,
            "severity": self.severity,
            "approval_required": self.status == ToolExecutionStatus.PENDING_APPROVAL,
            "approval_id": self._extract_approval_id(),
            "data": self.result,
            "error": self.error_message,
            "meta": {
                "duration_ms": int(self.execution_time_ms or 0),
                "contract_version": "2.0",
                **(self.metadata or {}),
            },
        }

    def _extract_approval_id(self) -> str | None:
        if self.status != ToolExecutionStatus.PENDING_APPROVAL:
            return None
        if not isinstance(self.result, dict):
            return None
        action_id = self.result.get("action_id")
        if action_id is None:
            return None
        normalized = str(action_id).strip()
        return normalized or None


class ToolExecutionError(Exception):
    """工具执行错误"""

    def __init__(self, message: str, status: ToolExecutionStatus = ToolExecutionStatus.ERROR):
        super().__init__(message)
        self.status = status


@dataclass
class BaseToolDefinition:
    """工具定义基类"""

    name: str
    description: str
    parameters: dict[str, Any]
    version: str = "1.0.0"
    required_params: list[str] = field(default_factory=list)
    category: Optional["ToolCategory"] = None
    operation_level: OperationLevel = OperationLevel.READ
    side_effect: bool = False

    def to_openai_schema(self) -> dict[str, Any]:
        """生成 OpenAI function calling 格式的工具 Schema"""
        return {
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": {"type": "object", "properties": self.parameters, "required": self.required_params},
            },
        }


class BaseToolExecutor(ABC):
    """
    工具执行器基类

    提供工具调用执行的统一接口和辅助方法。
    子类需要实现 _get_handlers 方法以注册工具处理器。
    """

    def __init__(self, default_user: str = "AI_Assistant", max_concurrency: int = 5):
        """
        初始化执行器

        Args:
            default_user: 默认操作用户名
            max_concurrency: 最大并发执行数
        """
        self.default_user = default_user
        self.max_concurrency = max_concurrency
        self._handlers: dict[str, Callable] = {}
        self._service = None  # 通用服务引用
        self._register_handlers()

    def _ensure_service(self) -> None:
        """确保服务已初始化，子类可覆盖以自定义错误消息"""
        if self._service is None:
            raise ToolExecutionError(f"{self.get_category().value} 服务未初始化", ToolExecutionStatus.ERROR)

    def set_service(self, service) -> None:
        """设置关联服务"""
        self._service = service

    @staticmethod
    def _extract_value(obj: Any) -> Any:
        """提取对象的 .value 属性或返回对象本身。"""
        return obj.value if hasattr(obj, "value") else obj

    @staticmethod
    def _extract_nested_value(obj: Any) -> Any:
        """提取嵌套值对象（obj.value.value）并优雅降级。"""
        if obj is None:
            return None
        first = obj.value if hasattr(obj, "value") else obj
        return first.value if hasattr(first, "value") else first

    @staticmethod
    def _to_iso(dt: datetime | None) -> str | None:
        """格式化 datetime 为 ISO 字符串。"""
        return dt.isoformat() if dt else None

    @staticmethod
    def _unwrap(obj: Any, accessor: str) -> Any:
        """访问聚合对象（如 get_flight/get_todo），否则返回原对象。"""
        return getattr(obj, accessor)() if hasattr(obj, accessor) else obj

    def _require_arg(
        self,
        args: Mapping[str, Any],
        name: str,
        message: str | None = None,
    ) -> Any:
        """验证必填参数存在且非空字符串。"""
        value = args.get(name)
        if value is None or (isinstance(value, str) and not value.strip()):
            raise ToolExecutionError(
                message or f"缺少必需参数: {name}",
                ToolExecutionStatus.VALIDATION_ERROR,
            )
        return value

    def _validate_args(
        self,
        model_cls: Callable[..., TValidated],
        args: Mapping[str, Any],
        message_prefix: str = "参数验证失败",
    ) -> TValidated:
        """使用模型验证参数并统一转换错误。"""
        try:
            return model_cls(**dict(args))
        except Exception as exc:
            raise ToolExecutionError(
                f"{message_prefix}: {exc}",
                ToolExecutionStatus.VALIDATION_ERROR,
            ) from exc

    def _parse_iso_datetime(
        self,
        value: Any,
        field_label: str,
        include_hint: bool = False,
    ) -> datetime | None:
        """解析 ISO 8601 时间字符串并统一返回验证错误。"""
        if value in (None, ""):
            return None
        if isinstance(value, datetime):
            return value
        if not isinstance(value, str):
            raise ToolExecutionError(
                f"无效的{field_label}格式: {value}",
                ToolExecutionStatus.VALIDATION_ERROR,
            )
        try:
            return datetime.fromisoformat(value)
        except ValueError as exc:
            hint = "，请使用ISO 8601格式" if include_hint else ""
            raise ToolExecutionError(
                f"无效的{field_label}格式: {value}{hint}",
                ToolExecutionStatus.VALIDATION_ERROR,
            ) from exc

    @staticmethod
    def _success_response(message: str | None = None, **payload: Any) -> dict[str, Any]:
        """构造标准成功响应。"""
        response = {"success": True, **payload}
        if message is not None:
            response["message"] = message
        return response

    @staticmethod
    def _status_response(
        success: bool,
        success_message: str,
        failure_message: str,
        **payload: Any,
    ) -> dict[str, Any]:
        """构造带 success 标志的统一响应。"""
        return {
            "success": success,
            **payload,
            "message": success_message if success else failure_message,
        }

    async def _safe_call(
        self,
        operation: Callable[[], Any],
        warning_message: str,
        default: Any = None,
    ) -> Any:
        """执行操作并在异常时记录 warning，返回默认值。"""
        try:
            result = operation()
            if inspect.isawaitable(result):
                return await result
            return result
        except Exception as exc:  # noqa: BLE001 - safe_call wraps arbitrary operation
            logger.warning(f"{warning_message}: {exc}")
            return default

    async def _run_ai_task(
        self,
        prompt: str,
        ai_entity: Any,
        error_message: str,
        fallback_builder: Callable[[Exception | None], str],
    ) -> str:
        """执行 AI 任务，失败或未配置时返回降级内容。"""
        if not ai_entity:
            return fallback_builder(None)

        try:
            response = await ai_entity.execute_task(prompt)
            return response.content if hasattr(response, "content") else str(response)
        except LLM_EXCEPTIONS as exc:
            logger.error(f"{error_message}: {exc}")
            return fallback_builder(exc)

    @abstractmethod
    def _register_handlers(self) -> None:
        """
        注册工具处理器

        子类必须实现此方法，将工具名称映射到处理函数。
        Example:
            self._handlers = {
                "tool_name": self._handle_tool_name,
            }
        """

    @abstractmethod
    def get_category(self) -> ToolCategory:
        """返回此执行器处理的工具类别"""

    async def execute_tool_call(
        self, tool_call_id: str, tool_name: str, arguments: str | dict[str, Any]
    ) -> ToolExecutionResult:
        """
        执行单个工具调用

        Args:
            tool_call_id: 工具调用ID
            tool_name: 工具名称
            arguments: 工具参数（JSON字符串或字典）

        Returns:
            工具执行结果
        """
        start_time = time.time()

        try:
            # 解析参数
            if isinstance(arguments, str):
                try:
                    args = json.loads(arguments)
                except json.JSONDecodeError as e:
                    raise ToolExecutionError(f"无法解析工具参数: {e}", ToolExecutionStatus.VALIDATION_ERROR) from e
            else:
                args = arguments

            # 获取处理器
            handler = self._handlers.get(tool_name)
            if not handler:
                raise ToolExecutionError(f"未知的工具: {tool_name}", ToolExecutionStatus.VALIDATION_ERROR)

            # 执行处理器
            result = await handler(args)

            execution_time = (time.time() - start_time) * 1000
            logger.info(f"工具 '{tool_name}' 执行成功，耗时 {execution_time:.2f}ms")

            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                status=ToolExecutionStatus.SUCCESS,
                result=result,
                execution_time_ms=execution_time,
            )

        except ToolExecutionError as e:
            execution_time = (time.time() - start_time) * 1000
            logger.warning(f"工具 '{tool_name}' 执行失败: {e}")
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                status=e.status,
                error_message=str(e),
                execution_time_ms=execution_time,
            )
        except TimeoutError as e:
            execution_time = (time.time() - start_time) * 1000
            logger.warning(f"工具 '{tool_name}' 执行超时: {e}")
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                status=ToolExecutionStatus.TIMEOUT,
                error_message=str(e),
                execution_time_ms=execution_time,
            )
        except Exception as e:  # noqa: BLE001 - top-level tool execution handler must catch all errors
            execution_time = (time.time() - start_time) * 1000
            logger.error(f"工具 '{tool_name}' 执行异常: {e}")
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                status=ToolExecutionStatus.ERROR,
                error_message=f"执行异常: {e!s}",
                execution_time_ms=execution_time,
            )

    async def execute_tool_calls(self, tool_calls: list[dict[str, Any]]) -> list[ToolExecutionResult]:
        """
        批量执行工具调用

        Args:
            tool_calls: 工具调用列表

        Returns:
            执行结果列表
        """
        semaphore = asyncio.Semaphore(self.max_concurrency or 5)

        async def _run(tc: dict[str, Any]) -> ToolExecutionResult:
            async with semaphore:
                tool_call_id = tc.get("id", "unknown")
                function = tc.get("function", {})
                tool_name = function.get("name", "unknown")
                arguments = function.get("arguments", "{}")
                return await self.execute_tool_call(tool_call_id, tool_name, arguments)

        return await asyncio.gather(*(_run(tc) for tc in tool_calls))

    def get_handler_names(self) -> list[str]:
        """获取此执行器支持的所有工具名称"""
        return list(self._handlers.keys())


def build_openai_tools(definitions: list[BaseToolDefinition]) -> list[dict[str, Any]]:
    """将工具定义列表转换为 OpenAI schema 列表。"""
    return [definition.to_openai_schema() for definition in definitions]


__all__ = [
    "BaseToolDefinition",
    "BaseToolExecutor",
    "InvocationMode",
    "OperationLevel",
    "ToolCategory",
    "ToolExecutionError",
    "ToolExecutionResult",
    "ToolExecutionStatus",
    "build_openai_tools",
]
