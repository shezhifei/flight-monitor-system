"""TODO Agent executor — main class."""

from __future__ import annotations

import asyncio
import inspect
import json
import logging
from collections.abc import Callable
from typing import Any

from src.domain.ai.agent_execution import (
    AgentExecution,
    AgentExecutionStatus,
)
from src.domain.ports.agent_runtime_port import (
    AgentTodoInfoDTO,
)
from src.domain.ports.notification_port import NotificationPort
from src.domain.utils.time_utils import utc_now
from src.infrastructure.ai.agent_execution_repository import AgentExecutionRepository
from src.infrastructure.ai.ai_entity import AIEntity, AIEntityConfig
from src.infrastructure.ai.config_store import AIConfigStoreInterface
from src.infrastructure.ai.conversation_manager import ConversationManager
from src.infrastructure.ai.execution.recovery import ExecutionRecovery
from src.infrastructure.ai.prompts import (
    DISTILLED_CONCLUSION_HINT,
    TASK_DESCRIPTION_TEMPLATE,
)
from src.infrastructure.ai.rate_limiter import RateLimiter
from src.infrastructure.ai.shared_context_pool import (
    SharedContextPool,
)
from src.infrastructure.ai.tools.base import InvocationMode
from src.infrastructure.ai.tools.registry import ToolRegistry, get_tool_registry

logger = logging.getLogger(__name__)


class _ExecutorCoreMixin:
    """Mixin for TodoAgentExecutor."""

    def __init__(
        self,
        config_store: AIConfigStoreInterface,
        execution_repo: AgentExecutionRepository,
        rate_limiter: RateLimiter | None = None,
        tool_registry: ToolRegistry | None = None,
        conversation_manager: ConversationManager | None = None,
        notification_port: NotificationPort | None = None,
        shared_context_pool: SharedContextPool | None = None,
        feature_overrides: dict[str, Any] | None = None,
        max_concurrent: int = 10,
    ):
        self._config_store = config_store
        self._execution_repo = execution_repo
        self._rate_limiter = rate_limiter or RateLimiter()
        self._tool_registry = tool_registry or get_tool_registry()
        self._conversation_manager = conversation_manager
        self._notification_port = notification_port
        self._shared_pool = shared_context_pool
        self._feature_overrides = dict(feature_overrides or {})
        self._semaphore = asyncio.Semaphore(max_concurrent)
        self._cancel_events: dict[str, asyncio.Event] = {}
        self._cancel_events_lock = asyncio.Lock()
        self._execution_started_at_iso: dict[str, str] = {}
        self._graph_resume_locks: dict[str, asyncio.Lock] = {}
        self._graph_resume_locks_lock = asyncio.Lock()

        self.recovery_mechanism = ExecutionRecovery()

        # 服务回调（由外部注入，用于创建子 TODO）
        self._todo_creator: Callable | None = None
        self._todo_loader: Callable | None = None
        self._todo_updater: Callable | None = None

        logger.info(f"TodoAgentExecutor initialized, max_concurrent={max_concurrent}")

    def set_todo_callbacks(self, creator: Callable, loader: Callable, updater: Callable) -> None:
        """设置 TODO 操作回调"""
        self._todo_creator = creator
        self._todo_loader = loader
        self._todo_updater = updater

    async def _emit_ai_event(self, event_type: str, run_id: str, todo_id: str, payload: dict):
        """Helper to emit SSE events"""
        try:
            real_payload = payload.copy() if isinstance(payload, dict) else {}
            status = str(real_payload.get("status") or self._default_status_for_event(event_type)).strip().lower()
            phase = str(real_payload.get("phase") or self._default_phase_for_event(event_type)).strip().lower()
            execution_started_at = self._execution_started_at_iso.get(run_id) or utc_now().isoformat()
            raw_timestamps = real_payload.get("timestamps") if isinstance(real_payload.get("timestamps"), dict) else {}
            started_at = (
                raw_timestamps.get("started_at")
                or real_payload.get("execution_started_at")
                or real_payload.get("started_at")
                or execution_started_at
            )
            ended_at = raw_timestamps.get("ended_at") or real_payload.get("ended_at")
            tool_name = real_payload.get("tool_name")
            if tool_name is None and isinstance(real_payload.get("tool"), dict):
                tool_name = real_payload.get("tool", {}).get("name")

            # Ensure ids are present and keep backward-compatible fields.
            real_payload.update(
                {
                    "event": str(real_payload.get("event") or event_type),
                    "execution_id": run_id,
                    "conversation_id": run_id,
                    "phase": phase,
                    "status": status,
                    "tool_name": tool_name,
                    "progress_pct": int(real_payload.get("progress_pct") or 0),
                    "recoverable": bool(
                        real_payload.get(
                            "recoverable",
                            status
                            in {
                                "timeout",
                                "error",
                                "validation_error",
                                "not_found",
                                "permission_denied",
                                "pending_approval",
                            },
                        )
                    ),
                    "retryable": bool(real_payload.get("retryable", status in {"timeout", "error"})),
                    "meta": {
                        **(real_payload.get("meta") or {}),
                        "contract_version": "2.0",
                    },
                    "execution_started_at": execution_started_at,
                    "timestamps": {
                        "started_at": str(started_at),
                        "ended_at": str(ended_at) if ended_at else None,
                    },
                    "agent_id": run_id,
                    "todo_id": todo_id,
                    "timestamp": utc_now().isoformat(),
                }
            )
            if self._notification_port:
                await self._notification_port.notify_ai_event(event_type, real_payload)
            else:
                logger.warning("Notification port not configured, skipping AI event")
        except Exception as e:  # noqa: BLE001 - best-effort AI event emission must not break the caller
            logger.warning(f"Failed to emit AI event: {e}")

    def _get_spawn_subtodo_tool(self) -> dict | None:
        """获取创建子 TODO 的工具定义"""
        return {
            "type": "function",
            "function": {
                "name": "spawn_subtodo",
                "description": "Create a sub-task (TODO) when the current task is too complex.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "title": {"type": "string"},
                        "description": {"type": "string"},
                        "entity_id": {"type": "string"},
                        "depends_on": {"type": "array", "items": {"type": "string"}},
                        "priority": {"type": "string", "enum": ["高", "中", "低"]},
                    },
                    "required": ["title"],
                },
            },
        }

    async def _handle_spawn_subtodo(
        self,
        parent_todo_id: str,
        parent_entity_id: str,
        args: Any,
        child_todos: list[str],
    ) -> str:
        """处理创建子 TODO"""
        if args is None:
            args = {}

        if isinstance(args, str):
            try:
                args = json.loads(args)
            except json.JSONDecodeError:
                return "Error: Invalid arguments"

        if not isinstance(args, dict):
            return "Error: Invalid arguments, object expected"

        if not self._todo_creator:
            return "Error: TODO creator not configured"

        title = str(args.get("title", "")).strip()
        if not title:
            return "Error: Missing required field 'title'"

        description = args.get("description")
        if description is not None:
            description = str(description).strip() or None

        depends_on_raw = args.get("depends_on", [])
        if depends_on_raw is None:
            depends_on: list[str] = []
        elif isinstance(depends_on_raw, list):
            depends_on = []
            for item in depends_on_raw:
                if item is None:
                    continue
                value = str(item).strip()
                if value:
                    depends_on.append(value)
        else:
            return "Error: 'depends_on' must be a list of todo IDs"

        priority = self._normalize_priority(args.get("priority"))
        entity_id = str(args.get("entity_id") or parent_entity_id or "default").strip() or "default"

        creator_kwargs = self._build_todo_creator_kwargs(
            {
                "title": title,
                "description": description,
                "entity_id": entity_id,
                "parent_todo_id": parent_todo_id,
                "depends_on": depends_on,
                "priority": priority,
            }
        )

        try:
            created = await self._todo_creator(**creator_kwargs)
            todo_id = self._extract_created_todo_id(created)
            if not todo_id:
                return "Error creating subtodo: callback returned empty todo id"

            if todo_id not in child_todos:
                child_todos.append(todo_id)

            return f"Created subtodo: {todo_id} - {title}"
        except Exception as e:  # noqa: BLE001 - subtodo creation callback must return error message for any failure
            logger.error(f"Failed to create subtodo for parent {parent_todo_id}: {e}")
            return f"Error creating subtodo: {e}"

    def _normalize_priority(self, raw_priority: Any) -> str:
        """规范化优先级，兼容中英文输入。"""
        if raw_priority is None:
            return "中"

        value = str(raw_priority).strip()
        if not value:
            return "中"

        if value in {"高", "中", "低"}:
            return value

        normalized = {
            "critical": "高",
            "urgent": "高",
            "high": "高",
            "medium": "中",
            "normal": "中",
            "low": "低",
            "background": "低",
        }.get(value.lower())

        return normalized or "中"

    def _build_todo_creator_kwargs(self, kwargs: dict[str, Any]) -> dict[str, Any]:
        """根据 callback 签名过滤参数，兼容不同 creator 实现。"""
        if not self._todo_creator:
            return kwargs

        try:
            signature = inspect.signature(self._todo_creator)
        except (TypeError, ValueError):
            return kwargs

        params = signature.parameters
        if any(param.kind == inspect.Parameter.VAR_KEYWORD for param in params.values()):
            return kwargs

        return {key: value for key, value in kwargs.items() if key in params}

    def _extract_created_todo_id(self, created: Any) -> str | None:
        """兼容多种 callback 返回类型并提取 todo_id。"""
        if created is None:
            return None

        if isinstance(created, str):
            value = created.strip()
            return value or None

        if isinstance(created, dict):
            todo_id = created.get("todo_id") or created.get("id")
            if todo_id is None:
                return None
            value = str(todo_id).strip()
            return value or None

        value_attr = getattr(created, "value", None)
        if value_attr is not None:
            value = str(value_attr).strip()
            return value or None

        value = str(created).strip()
        return value or None

    async def _execute_tool(
        self,
        tool_name: str,
        tool_args: Any,
        tool_call_id: str,
        timeout_seconds: float,
        user_id: str | None = None,
        user_roles: list[str] | None = None,
        invocation_mode: InvocationMode = InvocationMode.AGENT_AUTONOMOUS,
    ) -> str:
        """Helper to execute tool"""
        if isinstance(tool_args, str):
            arguments = tool_args
        else:
            arguments = json.dumps(tool_args)

        result = await asyncio.wait_for(
            self._tool_registry.execute_tool_call(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                arguments=arguments,
                user_id=user_id,
                user_roles=user_roles,
                invocation_mode=invocation_mode,
            ),
            timeout=max(self.MIN_TOOL_CALL_TIMEOUT_SECONDS, timeout_seconds),
        )

        status = getattr(getattr(result, "status", None), "value", "")
        if status == "success":
            return json.dumps(getattr(result, "result", None), ensure_ascii=False, default=str)

        return json.dumps(
            {
                "error": getattr(result, "error_message", "tool execution failed"),
                "status": status or "error",
                "code": getattr(result, "code", None),
                "message": getattr(result, "message", None)
                or getattr(result, "error_message", "tool execution failed"),
                "recoverable": bool(getattr(result, "recoverable", True)),
                "retryable": bool(getattr(result, "retryable", False)),
                "severity": getattr(result, "severity", "error"),
                "result": getattr(result, "result", None),
            },
            ensure_ascii=False,
        )

    def _inject_error_coaching(
        self,
        tool_name: str,
        tool_status: str,
        tool_result: str,
        available_tools: list[dict[str, Any]],
    ) -> str:
        """在工具执行失败的结果中注入纠偏引导提示，帮助模型正确重选工具。"""
        try:
            result_dict = json.loads(tool_result)
        except (json.JSONDecodeError, TypeError):
            result_dict = {"error": tool_result, "status": tool_status}

        code = result_dict.get("code", "")
        hint = self._build_error_coaching(tool_name, tool_status, code, available_tools)
        if hint:
            result_dict["_system_hint"] = hint

        return json.dumps(result_dict, ensure_ascii=False)

    def _build_error_coaching(
        self,
        tool_name: str,
        tool_status: str,
        error_code: str,
        available_tools: list[dict[str, Any]],
    ) -> str:
        """根据错误类型生成针对性的纠偏提示文本。"""
        tool_names = [
            t.get("function", {}).get("name", "") for t in available_tools if t.get("function", {}).get("name")
        ]
        tool_list_str = "、".join(tool_names[:15])

        if error_code == "TOOL_NOT_REGISTERED" or "未知的工具" in str(error_code):
            return (
                f"工具 '{tool_name}' 不存在。不要编造工具名称。"
                f"当前可用的工具有：{tool_list_str}。"
                "请根据用户意图从以上工具中重新选择，或直接用文字回答用户。"
            )

        if tool_status == "validation_error" or error_code == "TOOL_VALIDATION_ERROR":
            # 找到该工具的 required params
            required = []
            for t in available_tools:
                func = t.get("function", {})
                if func.get("name") == tool_name:
                    required = func.get("parameters", {}).get("required", [])
                    break
            if required:
                return f"参数格式错误。工具 '{tool_name}' 的必填参数为：{', '.join(required)}。请检查参数后重试。"
            return f"工具 '{tool_name}' 参数验证失败，请检查参数格式和类型后重试。"

        if tool_status == "not_found" or error_code == "TOOL_NOT_FOUND":
            return (
                "未找到该资源。请确认ID是否正确。"
                "如果不确定ID，请先使用搜索工具（如 search_flights_by_number、list_anomalies）检索。"
            )

        if tool_status == "permission_denied":
            return f"没有权限执行工具 '{tool_name}'。该操作可能需要更高级别的授权。请告知用户需要人工处理。"

        if tool_status == "timeout":
            return f"工具 '{tool_name}' 执行超时。请告知用户系统暂时繁忙，稍后再试。"

        return "工具执行失败。请尝试使用其他工具，或直接用文字告知用户当前无法完成该操作。"

    async def _create_ai_entity(self, config: dict[str, Any]) -> AIEntity:
        """从配置构建并初始化 AI 实体。"""
        entity_config = AIEntityConfig(
            api_key=config.get("api_key"),
            base_url=config.get("base_url", "https://api.openai.com/v1"),
            default_model=config.get("default_model", "gpt-3.5-turbo"),
            api_format=self._normalize_api_format(config.get("api_format")),
            temperature=config.get("temperature", 0.7),
            max_tokens=config.get("max_tokens", 1000),
            timeout=config.get("timeout", 30.0),
            max_retries=config.get("max_retries", 3),
            retry_delay=config.get("retry_delay", 0.5),
            cost_per_1k_input=config.get("cost_per_1k_input", 0.0),
            cost_per_1k_output=config.get("cost_per_1k_output", 0.0),
            context_window=config.get("context_window", 128000),
            allowed_tool_categories=config.get("allowed_tool_categories", []) or [],
            allowed_tools=config.get("allowed_tools"),
            denied_tools=config.get("denied_tools", []) or [],
            system_prompt=config.get("system_prompt"),
            task_template=config.get("task_template"),
        )
        ai_entity = AIEntity(config=entity_config)
        await ai_entity._ensure_initialized()
        return ai_entity

    def _build_task_description(
        self,
        todo: AgentTodoInfoDTO,
        entity_config: dict[str, Any] | None = None,
        has_downstream: bool = False,
    ) -> str:
        """构建任务描述，支持实体自定义模板。

        Args:
            has_downstream: 如果为 True，追加结论提炼提示，
                引导 Agent 在回答开头给出精炼结论以便共享给下游。
        """
        template = TASK_DESCRIPTION_TEMPLATE
        if entity_config and entity_config.get("task_template"):
            template = entity_config["task_template"]

        desc = template.format(
            title=todo.title,
            description=todo.description or "无",
        )
        if has_downstream:
            desc += DISTILLED_CONCLUSION_HINT
        return desc

    async def get_execution(self, run_id: str) -> AgentExecution | None:
        """获取执行详情并补全作业类型信息。"""
        execution = await self._execution_repo.get_execution(run_id)
        if not execution:
            return None

        task_types = await self._execution_repo.get_task_types(run_id)
        execution.task_types = task_types
        if task_types and execution.total_steps < len(task_types):
            execution.total_steps = len(task_types)
        return execution

    async def cancel_execution(self, run_id: str) -> bool:
        """取消尚未完成的执行。"""
        execution = await self._execution_repo.get_execution(run_id)
        if not execution:
            return False

        if execution.status in {
            AgentExecutionStatus.COMPLETED,
            AgentExecutionStatus.FAILED,
            AgentExecutionStatus.CANCELLED,
        }:
            return False

        execution.cancel()
        await self._execution_repo.update_execution(execution)

        cancel_event = await self._get_cancel_event(run_id)
        if cancel_event:
            cancel_event.set()

        if self._todo_updater:
            try:
                await self._todo_updater(execution.todo_id, "cancelled", execution.run_id)
            except Exception as exc:  # noqa: BLE001 - best-effort todo status update must not break cancellation
                logger.warning(f"Failed to update TODO status on cancel: {exc}")

        return True
