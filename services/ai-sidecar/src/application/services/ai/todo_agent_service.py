"""
TODO Agent 应用服务

协调 TODO 与 AI Agent 执行的应用层服务。
"""

import json
from collections import defaultdict, deque
from dataclasses import dataclass
from datetime import datetime
from typing import Any

from src.application.services.async_todo_service import CreateTodoCommand
from src.domain.ai.agent_execution import (
    AgentExecutionStatus,
)
from src.domain.models.todo import TodoId
from src.domain.models.todo_query import TodoQueryOptions
from src.domain.ports.agent_runtime_port import (
    AgentRuntimePort,
    AgentTodoInfoDTO,
)
from src.infrastructure.ai.agent_execution_repository import AgentExecutionRepository
from src.infrastructure.ai.config_store import AIConfigStoreInterface
from src.infrastructure.ai.rate_limiter import RateLimiter
from src.infrastructure.ai.tools.base import InvocationMode
from src.infrastructure.ai.tools.business_case_tools import BusinessCaseToolName
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


@dataclass
class TodoExecutionRequest:
    """TODO 执行请求"""

    todo_id: str
    entity_id: str | None = None
    max_iterations: int = 10
    system_prompt_override: str | None = None
    user_id: str | None = None
    user_roles: list[str] | None = None
    invocation_mode: InvocationMode = InvocationMode.AGENT_AUTONOMOUS


@dataclass
class TodoExecutionResponse:
    """TODO 执行响应"""

    run_id: str
    todo_id: str
    status: str
    response: str | None
    total_steps: int
    total_tokens: int
    total_tool_calls: int
    duration_ms: int
    error_message: str | None = None
    child_todos: list[str] = None

    def to_dict(self) -> dict[str, Any]:
        return {
            "run_id": self.run_id,
            "todo_id": self.todo_id,
            "status": self.status,
            "response": self.response,
            "total_steps": self.total_steps,
            "total_tokens": self.total_tokens,
            "total_tool_calls": self.total_tool_calls,
            "duration_ms": self.duration_ms,
            "error_message": self.error_message,
            "child_todos": self.child_todos or [],
        }


class TodoAgentService:
    """
    TODO Agent 应用服务

    提供 TODO 与 AI Agent 执行的高层 API。
    """

    def __init__(
        self,
        executor: AgentRuntimePort,
        execution_repo: AgentExecutionRepository,
        config_store: AIConfigStoreInterface,
        rate_limiter: RateLimiter,
        todo_service: Any,  # TodoApplicationService
        business_case_service: Any = None,
    ):
        self._executor = executor
        self._execution_repo = execution_repo
        self._config_store = config_store
        self._rate_limiter = rate_limiter
        self._todo_service = todo_service
        self._business_case_service = business_case_service

        # 设置 TODO 操作回调
        self._executor.set_todo_callbacks(
            creator=self._create_todo, loader=self._load_todo_tree, updater=self._update_todo_status
        )

        logger.info("TodoAgentService initialized")

    async def execute_todo(self, request: TodoExecutionRequest) -> TodoExecutionResponse:
        """
        执行单个 TODO

        Args:
            request: 执行请求

        Returns:
            执行响应
        """
        # 获取 TODO 信息
        todo = await self._get_todo_info(request.todo_id)
        if not todo:
            return TodoExecutionResponse(
                run_id="",
                todo_id=request.todo_id,
                status="failed",
                response=None,
                total_steps=0,
                total_tokens=0,
                total_tool_calls=0,
                duration_ms=0,
                error_message=f"TODO {request.todo_id} not found",
            )

        # 覆盖实体 ID
        if request.entity_id:
            todo.entity_id = request.entity_id

        # 执行
        result = await self._executor.execute_todo(
            todo=todo,
            max_iterations=request.max_iterations,
            system_prompt_override=request.system_prompt_override,
            user_id=request.user_id,
            user_roles=request.user_roles,
            invocation_mode=request.invocation_mode,
        )
        await self._append_execution_trace_to_business_cases(result.run_id)

        return TodoExecutionResponse(
            run_id=result.run_id,
            todo_id=result.todo_id,
            status=result.status.value,
            response=result.response,
            total_steps=result.total_steps,
            total_tokens=result.total_tokens,
            total_tool_calls=result.total_tool_calls,
            duration_ms=result.duration_ms,
            error_message=result.error_message,
            child_todos=result.child_todos,
        )

    async def execute_todo_tree(
        self,
        root_todo_id: str,
        max_iterations_per_todo: int = 10,
        fail_fast: bool = True,
        user_id: str | None = None,
        user_roles: list[str] | None = None,
        invocation_mode: InvocationMode = InvocationMode.AGENT_AUTONOMOUS,
    ) -> dict[str, TodoExecutionResponse]:
        """
        执行 TODO 树

        Args:
            root_todo_id: 根 TODO ID
            max_iterations_per_todo: 每个 TODO 的最大迭代次数

        Returns:
            {todo_id: TodoExecutionResponse} 字典
        """
        results = await self._executor.execute_todo_tree(
            root_todo_id=root_todo_id,
            max_iterations_per_todo=max_iterations_per_todo,
            fail_fast=fail_fast,
            user_id=user_id,
            user_roles=user_roles,
            invocation_mode=invocation_mode,
        )
        for result in results.values():
            await self._append_execution_trace_to_business_cases(result.run_id)

        return {
            todo_id: TodoExecutionResponse(
                run_id=result.run_id,
                todo_id=result.todo_id,
                status=result.status.value,
                response=result.response,
                total_steps=result.total_steps,
                total_tokens=result.total_tokens,
                total_tool_calls=result.total_tool_calls,
                duration_ms=result.duration_ms,
                error_message=result.error_message,
                child_todos=result.child_todos,
            )
            for todo_id, result in results.items()
        }

    async def get_execution(self, run_id: str) -> dict[str, Any] | None:
        """获取执行详情"""
        execution = await self._executor.get_execution(run_id)
        if not execution:
            return None
        return execution.to_dict()

    async def get_execution_steps(self, run_id: str) -> list[dict[str, Any]]:
        """获取执行步骤详情。"""
        steps = await self._execution_repo.get_task_types(run_id)
        return [step.to_dict() for step in steps]

    async def get_execution_steps_batch(self, run_ids: list[str]) -> dict[str, list[dict[str, Any]]]:
        """批量获取执行步骤详情。"""
        steps_by_run_id = await self._execution_repo.get_task_types_batch(run_ids)
        return {run_id: [step.to_dict() for step in steps] for run_id, steps in steps_by_run_id.items()}

    async def get_todo_executions(self, todo_id: str, limit: int = 10) -> list[dict[str, Any]]:
        """获取 TODO 的执行历史"""
        executions = await self._execution_repo.list_executions(todo_id=todo_id, limit=limit)
        return [e.to_dict() for e in executions]

    async def get_entity_executions(
        self,
        entity_id: str | None,
        status: str | None = None,
        limit: int = 50,
        started_after: datetime | None = None,
    ) -> list[dict[str, Any]]:
        """获取实体的执行历史"""
        status_enum = AgentExecutionStatus(status) if status else None
        executions = await self._execution_repo.list_executions(
            entity_id=entity_id,
            status=status_enum,
            started_after=started_after,
            limit=limit,
        )
        return [e.to_dict() for e in executions]

    async def cancel_execution(self, run_id: str) -> bool:
        """取消执行"""
        return await self._executor.cancel_execution(run_id)

    async def get_rate_limit_status(self) -> dict[str, Any]:
        """获取速率限制状态"""
        status = self._rate_limiter.get_status()
        return status.to_dict()

    async def list_available_entities(self) -> list[dict[str, Any]]:
        """列出可用的 AI 实体"""
        # Interface uses get_all()
        entities = await self._config_store.get_all()
        return [
            {
                "entity_id": eid,
                "model": config.get("default_model", "unknown"),
                "has_tools": bool(config.get("allowed_tool_categories")),
            }
            for eid, config in entities.items()
        ]

    async def _append_execution_trace_to_business_cases(self, run_id: str) -> None:
        append_trace = getattr(self._business_case_service, "append_agent_execution_trace", None)
        if not run_id or not callable(append_trace):
            return

        try:
            execution = await self._executor.get_execution(run_id)
        except Exception as exc:  # noqa: BLE001 - top-level handler must catch all service failures
            logger.warning(f"Failed to load execution {run_id} for business case append sync: {exc}")
            return

        if execution is None:
            return

        case_ids = self._extract_business_case_ids_from_execution(execution)
        if not case_ids:
            return

        operator_name = self._resolve_business_case_operator_name(execution)
        for case_id in sorted(case_ids):
            try:
                await append_trace(
                    case_id=case_id,
                    execution=execution,
                    operator_name=operator_name,
                )
            except Exception as exc:  # noqa: BLE001 - top-level handler must catch all service failures
                logger.warning(
                    "Failed to append AI execution trace to business case %s for run %s: %s",
                    case_id,
                    run_id,
                    exc,
                )

    def _extract_business_case_ids_from_execution(self, execution: Any) -> set[str]:
        case_ids: set[str] = set()
        for step in getattr(execution, "task_types", []) or []:
            for tool_call in getattr(step, "tool_calls", []) or []:
                tool_name = str(getattr(tool_call, "tool_name", "") or "").strip()
                if tool_name not in {
                    BusinessCaseToolName.CREATE.value,
                    BusinessCaseToolName.GET.value,
                    BusinessCaseToolName.UPDATE.value,
                }:
                    continue
                case_ids.update(self._extract_case_ids_from_tool_call(tool_call))
        return {case_id for case_id in case_ids if case_id}

    def _extract_case_ids_from_tool_call(self, tool_call: Any) -> set[str]:
        case_ids: set[str] = set()
        arguments_payload = self._parse_mapping_payload(getattr(tool_call, "arguments", None))
        result_payload = self._parse_mapping_payload(getattr(tool_call, "result", None))

        direct_case_id = str(arguments_payload.get("case_id") or "").strip()
        if direct_case_id:
            case_ids.add(direct_case_id)

        for candidate in (
            result_payload.get("case_id"),
            (result_payload.get("data") or {}).get("case_id") if isinstance(result_payload.get("data"), dict) else None,
        ):
            normalized = str(candidate or "").strip()
            if normalized:
                case_ids.add(normalized)

        return case_ids

    @staticmethod
    def _parse_mapping_payload(value: Any) -> dict[str, Any]:
        if isinstance(value, dict):
            return value
        if isinstance(value, str):
            normalized = value.strip()
            if not normalized:
                return {}
            try:
                parsed = json.loads(normalized)
            except json.JSONDecodeError:
                return {}
            return parsed if isinstance(parsed, dict) else {}
        return {}

    @staticmethod
    def _resolve_business_case_operator_name(execution: Any) -> str | None:
        metadata = getattr(execution, "metadata", None)
        if isinstance(metadata, dict):
            operator_name = str(metadata.get("operator_name") or metadata.get("agent_name") or "").strip()
            if operator_name:
                return operator_name
        entity_id = str(getattr(execution, "entity_id", "") or "").strip()
        return entity_id or None

    async def _get_todo_info(self, todo_id: str) -> AgentTodoInfoDTO | None:
        """获取 TODO 信息"""
        try:
            todo_data = await self._fetch_todo(todo_id)
            if not todo_data:
                return None

            todo_info = self._to_todo_info(todo_data, fallback_todo_id=todo_id)
            await self._apply_entity_contexts([todo_info])
            return todo_info
        except Exception as e:  # noqa: BLE001 - top-level handler must catch all service failures
            logger.error(f"Failed to get TODO {todo_id}: {e}")
            return None

    async def _load_todo_tree(self, root_todo_id: str) -> list[AgentTodoInfoDTO]:
        """加载 TODO 树"""
        try:
            if hasattr(self._todo_service, "get_todo_tree"):
                todos = await self._todo_service.get_todo_tree(root_todo_id)
                todo_infos = [self._to_todo_info(todo_data) for todo_data in todos if todo_data]
                await self._apply_entity_contexts(todo_infos)
                return todo_infos

            root_info = await self._get_todo_info(root_todo_id)
            if not root_info:
                return []

            page_size = 200
            max_items = 2000
            all_todos: list[Any] = []
            page = 1
            while len(all_todos) < max_items:
                options = TodoQueryOptions(page=page, limit=page_size, offset=(page - 1) * page_size)
                chunk = await self._todo_service.list_todos(options)
                if not chunk:
                    break

                remaining = max_items - len(all_todos)
                all_todos.extend(chunk[:remaining])
                if len(chunk) < page_size:
                    break
                page += 1

            todo_infos = [self._to_todo_info(todo_data) for todo_data in all_todos if todo_data]
            await self._apply_entity_contexts(todo_infos)

            children_by_parent: dict[str, list[AgentTodoInfoDTO]] = defaultdict(list)
            for todo_info in todo_infos:
                if todo_info.parent_todo_id:
                    children_by_parent[todo_info.parent_todo_id].append(todo_info)

            result: list[AgentTodoInfoDTO] = []
            queue = deque([root_info])
            visited: set[str] = set()

            while queue:
                current = queue.popleft()
                if current.todo_id in visited:
                    continue

                visited.add(current.todo_id)
                result.append(current)

                children = sorted(
                    children_by_parent.get(current.todo_id, []),
                    key=lambda item: item.execution_order,
                )
                queue.extend(children)

            return result
        except Exception as e:  # noqa: BLE001 - top-level handler must catch all service failures
            logger.error(f"Failed to load TODO tree {root_todo_id}: {e}")
            return []

    async def _create_todo(
        self,
        title: str,
        description: str | None,
        entity_id: str,
        parent_todo_id: str,
        depends_on: list[str],
        priority: str = "中",
    ) -> str:
        """创建子 TODO"""
        command = CreateTodoCommand(
            title=title,
            description=description,
            priority=priority,
            created_by="AI Agent",
            parent_todo_id=parent_todo_id,
            depends_on=depends_on,
            source_type="ai",
            source_id=parent_todo_id,
        )
        created = await self._todo_service.create_todo(command)
        todo_id = self._extract_todo_id(created)
        if not todo_id:
            raise RuntimeError("create_todo returned empty todo id")

        await self._set_agent_context(todo_id, entity_id=entity_id)
        return todo_id

    async def _update_todo_status(self, todo_id: str, agent_status: str, run_id: str) -> None:
        """更新 TODO 的 Agent 状态"""
        if await self._set_agent_context(
            todo_id=todo_id,
            agent_status=agent_status,
            run_id=run_id,
        ):
            return

        logger.warning(f"Cannot persist agent status for TODO {todo_id}: set_agent_context unavailable")

    async def _fetch_todo(self, todo_id: str) -> Any | None:
        """兼容不同服务签名获取 TODO。"""
        try:
            result = await self._todo_service.get_todo(TodoId(todo_id))
            if result is not None:
                return result
        except (TypeError, AttributeError):
            pass

        return await self._todo_service.get_todo(todo_id)

    def _to_todo_info(self, todo_data: Any, fallback_todo_id: str | None = None) -> AgentTodoInfoDTO:
        """将不同 TODO 表示转换为执行器所需结构。"""
        if isinstance(todo_data, dict):
            todo_id = str(todo_data.get("todo_id") or todo_data.get("id") or fallback_todo_id or "")
            return AgentTodoInfoDTO(
                todo_id=todo_id,
                title=str(todo_data.get("title", "")),
                description=todo_data.get("description"),
                entity_id=str(todo_data.get("agent_entity_id") or "default"),
                parent_todo_id=todo_data.get("parent_todo_id"),
                depends_on=list(todo_data.get("depends_on") or []),
                execution_order=int(todo_data.get("execution_order", 0) or 0),
            )

        if hasattr(todo_data, "get_todo"):
            todo = todo_data.get_todo()
            description_vo = getattr(todo, "description", None)
            return AgentTodoInfoDTO(
                todo_id=getattr(todo.todo_id, "value", fallback_todo_id or ""),
                title=getattr(todo.title, "value", ""),
                description=getattr(description_vo, "value", None) if description_vo else None,
                entity_id="default",
                parent_todo_id=getattr(todo, "parent_todo_id", None),
                depends_on=list(getattr(todo, "depends_on", []) or []),
                execution_order=int(getattr(todo, "execution_order", 0) or 0),
            )

        raise TypeError(f"Unsupported TODO payload type: {type(todo_data)}")

    async def _set_agent_context(
        self,
        todo_id: str,
        *,
        entity_id: str | None = None,
        agent_status: str | None = None,
        run_id: str | None = None,
    ) -> bool:
        setter = getattr(self._todo_service, "set_agent_context", None)
        if not callable(setter):
            return False

        try:
            await setter(
                todo_id=todo_id,
                agent_entity_id=entity_id,
                agent_status=agent_status,
                agent_run_id=run_id,
                updated_by="AI Agent",
            )
            return True
        except Exception as exc:  # noqa: BLE001 - top-level handler must catch all service failures
            logger.warning(f"Failed to set agent context for TODO {todo_id}: {exc}")
            return False

    async def _apply_entity_contexts(self, todo_infos: list[AgentTodoInfoDTO]) -> None:
        if not todo_infos:
            return

        batch_get = getattr(self._todo_service, "batch_get_agent_context", None)
        if not callable(batch_get):
            for info in todo_infos:
                info.entity_id = str(info.entity_id or "default").strip() or "default"
            return

        todo_ids = [info.todo_id for info in todo_infos if info.todo_id]
        if not todo_ids:
            return

        try:
            context_map = await batch_get(todo_ids)
        except Exception as exc:  # noqa: BLE001 - top-level handler must catch all service failures
            logger.warning(f"Failed to batch get todo agent context: {exc}")
            context_map = {}

        for info in todo_infos:
            context = context_map.get(info.todo_id) if isinstance(context_map, dict) else None
            if context is not None:
                entity_id = str(getattr(context, "agent_entity_id", "") or "").strip()
                info.entity_id = entity_id or "default"
                continue

            info.entity_id = str(info.entity_id or "default").strip() or "default"

    def _extract_todo_id(self, value: Any) -> str | None:
        """从服务返回值中提取 todo_id。"""
        if value is None:
            return None

        if isinstance(value, str):
            return value.strip() or None

        if isinstance(value, dict):
            raw = value.get("todo_id") or value.get("id")
            return str(raw).strip() if raw is not None else None

        raw_value = getattr(value, "value", None)
        if raw_value is not None:
            return str(raw_value).strip() or None

        return None

    def _clear_todo_cache(self) -> None:
        """尽力清理 TODO 缓存，避免状态读写不一致。"""
        clear_cache = getattr(self._todo_service, "_clear_cache", None)
        if not callable(clear_cache):
            return

        try:
            clear_cache(None)
        except Exception as exc:  # noqa: BLE001 - best-effort cache clear must catch all failures
            logger.debug(f"Todo cache clear skipped: {exc}")


__all__ = [
    "TodoAgentService",
    "TodoExecutionRequest",
    "TodoExecutionResponse",
]
