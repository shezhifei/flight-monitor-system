"""
待办事项工具执行器

处理AI工具调用请求，路由到TodoApplicationService执行实际操作。
"""

from dataclasses import dataclass
from datetime import datetime
from typing import Any

from src.domain.models.todo import TodoId
from src.domain.models.todo_query import TodoQueryOptions
from src.domain.ports.service_interfaces import TodoServiceInterface
from src.infrastructure.logging.core import get_logger

from .base import (
    BaseToolExecutor,
    ToolCategory,
    ToolExecutionError,
    ToolExecutionResult,
    ToolExecutionStatus,
)
from .todo_tools import TodoToolName

logger = get_logger(__name__)


@dataclass
class CreateTodoCommand:
    title: str
    description: str | None = None
    priority: str = "中"
    category: str | None = None
    due_date: datetime | None = None
    estimated_duration: int | None = None
    tags: list[str] | None = None
    created_by: str = ""


@dataclass
class UpdateTodoCommand:
    todo_id: TodoId
    title: str | None = None
    description: str | None = None
    priority: str | None = None
    due_date: datetime | None = None
    estimated_duration: int | None = None
    tags: list[str] | None = None
    updated_by: str = ""


@dataclass
class AssignTodoCommand:
    todo_id: TodoId
    assignee: str
    assigned_by: str = ""


@dataclass
class CompleteTodoCommand:
    todo_id: TodoId
    actual_duration: int | None = None
    completed_by: str = ""


@dataclass
class CancelTodoCommand:
    todo_id: TodoId
    reason: str | None = None
    cancelled_by: str = ""


@dataclass
class UpdateProgressCommand:
    todo_id: TodoId
    progress: int
    updated_by: str = ""


class TodoToolExecutor(BaseToolExecutor):
    """
    待办事项工具执行器

    将AI的工具调用请求转换为实际的TodoApplicationService操作。
    """

    def __init__(self, todo_service: TodoServiceInterface = None, default_user: str = "AI_Assistant"):
        super().__init__(default_user)
        self._service = todo_service

    def _register_handlers(self) -> None:
        """注册工具处理器"""
        self._handlers = {
            TodoToolName.CREATE_TODO.value: self._handle_create_todo,
            TodoToolName.GET_TODO.value: self._handle_get_todo,
            TodoToolName.LIST_TODOS.value: self._handle_list_todos,
            TodoToolName.UPDATE_TODO.value: self._handle_update_todo,
            TodoToolName.COMPLETE_TODO.value: self._handle_complete_todo,
            TodoToolName.CANCEL_TODO.value: self._handle_cancel_todo,
            TodoToolName.ASSIGN_TODO.value: self._handle_assign_todo,
            TodoToolName.UPDATE_PROGRESS.value: self._handle_update_progress,
            TodoToolName.SEARCH_TODOS.value: self._handle_search_todos,
            TodoToolName.GET_TODO_STATS.value: self._handle_get_todo_stats,
        }

    def get_category(self) -> ToolCategory:
        """返回此执行器处理的工具类别"""
        return ToolCategory.TODO

    def set_todo_service(self, todo_service: TodoServiceInterface) -> None:
        self._service = todo_service

    async def _handle_create_todo(self, args: dict[str, Any]) -> dict[str, Any]:
        """处理创建待办事项"""
        self._ensure_service()
        title = self._require_arg(args, "title")
        due_date = self._parse_iso_datetime(
            args.get("due_date"),
            "截止日期",
            include_hint=True,
        )

        cmd = CreateTodoCommand(
            title=title,
            description=args.get("description"),
            priority=args.get("priority", "中"),
            category=args.get("category"),
            due_date=due_date,
            estimated_duration=args.get("estimated_duration"),
            tags=args.get("tags"),
            created_by=self.default_user,
        )

        todo_id = await self._service.create_todo(cmd)

        if args.get("assignee"):
            assign_cmd = AssignTodoCommand(todo_id=todo_id, assignee=args["assignee"], assigned_by=self.default_user)
            await self._service.assign_todo(assign_cmd)

        return self._success_response(
            todo_id=todo_id.value,
            message=f"成功创建待办事项: {title}",
        )

    async def _handle_get_todo(self, args: dict[str, Any]) -> dict[str, Any]:
        """处理获取待办事项"""
        self._ensure_service()
        todo_id_value = self._require_arg(args, "todo_id", "待办事项ID不能为空")
        todo_id = TodoId(todo_id_value)
        aggregate = await self._service.get_todo(todo_id)

        if not aggregate:
            raise ToolExecutionError(f"待办事项不存在: {todo_id_value}", ToolExecutionStatus.NOT_FOUND)
        todos = await self._serialize_todo_aggregates([aggregate])
        return todos[0]

    async def _handle_list_todos(self, args: dict[str, Any]) -> dict[str, Any]:
        self._ensure_service()

        agent_status = str(args.get("agent_status") or "").strip() or None
        agent_entity_id = str(args.get("agent_entity_id") or "").strip() or None
        agent_run_id = str(args.get("agent_run_id") or "").strip() or None
        if agent_status or agent_entity_id or agent_run_id:
            dedicated_query = getattr(self._service, "list_todos_by_agent_context", None)
            if callable(dedicated_query):
                aggregates = await dedicated_query(
                    agent_status=agent_status,
                    agent_entity_id=agent_entity_id,
                    agent_run_id=agent_run_id,
                    limit=args.get("limit", 20),
                    offset=0,
                )
                todos = await self._serialize_todo_aggregates(aggregates)
                return {"total": len(todos), "todos": todos}

        # 构建统一查询选项，将筛选逻辑下推到服务层
        options = TodoQueryOptions(
            limit=args.get("limit", 20),
            offset=0,
            status_filter=args.get("status"),
            priority_filter=args.get("priority"),
            category_filter=args.get("category"),
            assignee_filter=args.get("assignee") or self.default_user,
        )

        # 根据特殊条件选择查询方法
        if args.get("overdue_only"):
            aggregates = await self._service.get_overdue_todos(options)
        elif args.get("due_today"):
            aggregates = await self._service.get_due_today_todos(options)
        elif args.get("high_priority_only"):
            aggregates = await self._service.get_high_priority_todos(options)
        else:
            aggregates = await self._service.list_todos(options)

        todos = await self._serialize_todo_aggregates(aggregates)
        return {"total": len(todos), "todos": todos}

    async def _handle_update_todo(self, args: dict[str, Any]) -> dict[str, Any]:
        """处理更新待办事项"""
        self._ensure_service()
        due_date = self._parse_iso_datetime(args.get("due_date"), "截止日期")

        todo_id_value = self._require_arg(args, "todo_id", "待办事项ID不能为空")
        todo_id = TodoId(todo_id_value)

        cmd = UpdateTodoCommand(
            todo_id=todo_id,
            title=args.get("title"),
            description=args.get("description"),
            priority=args.get("priority"),
            due_date=due_date,
            estimated_duration=args.get("estimated_duration"),
            tags=args.get("tags"),
            updated_by=self.default_user,
        )

        success = await self._service.update_todo(cmd)

        return self._status_response(
            success=success,
            success_message="待办事项已更新",
            failure_message="更新失败",
            todo_id=todo_id_value,
        )

    async def _handle_complete_todo(self, args: dict[str, Any]) -> dict[str, Any]:
        """处理完成待办事项"""
        self._ensure_service()
        todo_id_value = self._require_arg(args, "todo_id", "待办事项ID不能为空")
        todo_id = TodoId(todo_id_value)

        cmd = CompleteTodoCommand(
            todo_id=todo_id, actual_duration=args.get("actual_duration"), completed_by=self.default_user
        )

        success = await self._service.complete_todo(cmd)

        return self._status_response(
            success=success,
            success_message="待办事项已完成",
            failure_message="完成操作失败",
            todo_id=todo_id_value,
        )

    async def _handle_cancel_todo(self, args: dict[str, Any]) -> dict[str, Any]:
        """处理取消待办事项"""
        self._ensure_service()
        todo_id_value = self._require_arg(args, "todo_id", "待办事项ID不能为空")
        todo_id = TodoId(todo_id_value)

        cmd = CancelTodoCommand(todo_id=todo_id, reason=args.get("reason"), cancelled_by=self.default_user)

        success = await self._service.cancel_todo(cmd)

        return self._status_response(
            success=success,
            success_message="待办事项已取消",
            failure_message="取消操作失败",
            todo_id=todo_id_value,
        )

    async def _handle_assign_todo(self, args: dict[str, Any]) -> dict[str, Any]:
        """处理分配待办事项"""
        self._ensure_service()
        todo_id_value = self._require_arg(args, "todo_id", "待办事项ID不能为空")
        assignee = self._require_arg(args, "assignee")
        todo_id = TodoId(todo_id_value)

        cmd = AssignTodoCommand(todo_id=todo_id, assignee=assignee, assigned_by=self.default_user)

        success = await self._service.assign_todo(cmd)

        return self._status_response(
            success=success,
            success_message=f"待办事项已分配给 {assignee}",
            failure_message="分配失败",
            todo_id=todo_id_value,
            assignee=assignee,
        )

    async def _handle_update_progress(self, args: dict[str, Any]) -> dict[str, Any]:
        """处理更新进度"""
        self._ensure_service()
        todo_id_value = self._require_arg(args, "todo_id", "待办事项ID不能为空")
        todo_id = TodoId(todo_id_value)
        progress = args.get("progress")
        if progress is None:
            raise ToolExecutionError(
                "缺少必需参数: progress",
                ToolExecutionStatus.VALIDATION_ERROR,
            )
        if not isinstance(progress, int):
            raise ToolExecutionError(
                "进度必须是0-100之间的整数",
                ToolExecutionStatus.VALIDATION_ERROR,
            )

        if not 0 <= progress <= 100:
            raise ToolExecutionError("进度必须在0-100之间", ToolExecutionStatus.VALIDATION_ERROR)

        cmd = UpdateProgressCommand(todo_id=todo_id, progress=progress, updated_by=self.default_user)

        success = await self._service.update_progress(cmd)

        return self._status_response(
            success=success,
            success_message=f"进度已更新为 {progress}%",
            failure_message="更新进度失败",
            todo_id=todo_id_value,
            progress=progress,
        )

    async def _handle_search_todos(self, args: dict[str, Any]) -> dict[str, Any]:
        """处理搜索待办事项"""
        self._ensure_service()
        query = self._require_arg(args, "query")
        options = TodoQueryOptions(limit=args.get("limit", 20), offset=0)

        aggregates = await self._service.search_todos(query, options)

        todos = await self._serialize_todo_aggregates(aggregates)

        return {"query": query, "total": len(todos), "todos": todos}

    async def _handle_get_todo_stats(self, args: dict[str, Any]) -> dict[str, Any]:
        """处理获取统计信息"""
        self._ensure_service()
        criteria = {}
        if args.get("assignee"):
            criteria["assignee"] = args["assignee"]

        stats = await self._service.get_todo_stats(criteria)

        return {
            "total": getattr(stats, "total", 0),
            "pending": getattr(stats, "pending", 0),
            "in_progress": getattr(stats, "in_progress", 0),
            "completed": getattr(stats, "completed", 0),
            "cancelled": getattr(stats, "cancelled", 0),
            "overdue": getattr(stats, "overdue", 0),
            "due_today": getattr(stats, "due_today", 0),
        }

    async def _serialize_todo_aggregates(self, aggregates: list[Any]) -> list[dict[str, Any]]:
        if not aggregates:
            return []

        context_map = await self._batch_load_agent_context(aggregates)
        todos: list[dict[str, Any]] = []
        for aggregate in aggregates:
            todo = aggregate.get_todo()
            todo_id = self._extract_value(todo.todo_id)
            context = context_map.get(todo_id)
            todos.append(self._todo_to_dict(todo, context=context))
        return todos

    async def _batch_load_agent_context(self, aggregates: list[Any]) -> dict[str, Any]:
        batch_get = getattr(self._service, "batch_get_agent_context", None)
        if not callable(batch_get):
            return {}

        todo_ids = [str(self._extract_value(aggregate.get_todo().todo_id) or "").strip() for aggregate in aggregates]
        todo_ids = [todo_id for todo_id in todo_ids if todo_id]
        if not todo_ids:
            return {}

        try:
            result = await batch_get(todo_ids)
        except Exception as exc:  # noqa: BLE001 - service call fallback must catch all errors
            logger.warning(f"Failed to batch load todo agent context: {exc}")
            return {}

        if not isinstance(result, dict):
            return {}

        return {str(key): value for key, value in result.items()}

    @staticmethod
    def _context_value(context: Any, key: str, default: Any = None) -> Any:
        if context is None:
            return default
        if isinstance(context, dict):
            return context.get(key, default)
        return getattr(context, key, default)

    def _todo_to_dict(self, todo, *, context: Any = None) -> dict[str, Any]:
        agent_entity_id = str(self._context_value(context, "agent_entity_id", "default") or "").strip() or "default"
        agent_run_id = self._context_value(context, "agent_run_id", None)
        agent_status = str(self._context_value(context, "agent_status", "pending") or "").strip() or "pending"
        return {
            "id": self._extract_value(todo.todo_id),
            "title": self._extract_value(todo.title),
            "description": self._extract_value(todo.description),
            "priority": self._extract_nested_value(todo.priority),
            "status": self._extract_nested_value(todo.status),
            "category": self._extract_nested_value(todo.category),
            "assigned_to": todo.assigned_to,
            "due_date": self._to_iso(todo.due_date),
            "progress": todo.progress,
            "tags": list(todo.tags) if todo.tags else [],
            "estimated_duration": todo.estimated_duration,
            "actual_duration": todo.actual_duration,
            "is_recurring": todo.is_recurring,
            "recurring_pattern": todo.recurring_pattern,
            "created_at": self._to_iso(getattr(todo, "created_at", None)),
            "updated_at": self._to_iso(getattr(todo, "updated_at", None)),
            "agent_entity_id": agent_entity_id,
            "agent_run_id": agent_run_id,
            "agent_status": agent_status,
            "parent_todo_id": todo.parent_todo_id,
            "depends_on": list(todo.depends_on) if todo.depends_on else [],
            "execution_order": todo.execution_order,
            "source_type": todo.source_type,
            "source_id": todo.source_id,
            "is_deleted": todo.is_deleted,
            "deleted_at": self._to_iso(todo.deleted_at),
        }


__all__ = [
    "TodoToolExecutor",
    "ToolExecutionError",
    "ToolExecutionResult",
    "ToolExecutionStatus",
]
