"""异步待办事项应用服务"""

import time
from dataclasses import dataclass
from datetime import datetime
from typing import Any

from src.application.interfaces.service_contracts import TodoCompletionPort
from src.application.services.todo_priority import normalize_todo_priority
from src.domain.aggregates.todo_aggregate import TodoAggregate
from src.domain.exceptions.base import DomainException, ValidationException
from src.domain.models.todo import TodoId, TodoPriority
from src.domain.models.todo_query import TodoQueryOptions, TodoStats
from src.domain.ports.service_interfaces import TodoServiceInterface
from src.infrastructure.logging.core import get_logger
from src.infrastructure.repositories.todo_agent_context_repository import (
    TodoAgentContext,
    TodoAgentContextRepository,
)

logger = get_logger(__name__)


@dataclass
class CreateTodoCommand:
    title: str
    description: str | None = None
    priority: str = "中"
    category: str | None = None
    due_date: datetime | None = None
    estimated_duration: int | None = None
    tags: list[str] = None
    agent_entity_id: str | None = None
    parent_todo_id: str | None = None
    depends_on: list[str] = None
    source_type: str = "manual"
    source_id: str | None = None
    created_by: str = "System"


@dataclass
class UpdateTodoCommand:
    todo_id: TodoId
    title: str | None = None
    description: str | None = None
    priority: str | None = None
    due_date: datetime | None = None
    estimated_duration: int | None = None
    tags: list[str] | None = None
    updated_by: str = "System"


@dataclass
class AssignTodoCommand:
    todo_id: TodoId
    assignee: str
    assigned_by: str = "System"


@dataclass
class CompleteTodoCommand:
    todo_id: TodoId
    actual_duration: int | None = None
    completed_by: str = "System"


@dataclass
class CancelTodoCommand:
    todo_id: TodoId
    reason: str | None = None
    cancelled_by: str = "System"


@dataclass
class UpdateProgressCommand:
    todo_id: TodoId
    progress: int
    updated_by: str = "System"


class AsyncTodoApplicationService(TodoServiceInterface):
    def __init__(
        self,
        todo_repository,
        cache_service,
        todo_agent_context_repository: TodoAgentContextRepository | None = None,
        todo_completion_port: TodoCompletionPort | None = None,
    ):
        self.repo = todo_repository
        self.cache = cache_service
        self.todo_agent_context_repo = todo_agent_context_repository
        self._todo_completion_port = todo_completion_port
        self._agent_context_query_metrics: dict[str, float] = {
            "dedicated_query_calls": 0,
            "dedicated_query_context_repo_path_calls": 0,
            "dedicated_query_compat_fallback_calls": 0,
            "dedicated_query_duration_ms_total": 0.0,
            "dedicated_query_empty_results": 0,
        }

    def set_todo_completion_port(self, completion_port: TodoCompletionPort | None) -> None:
        self._todo_completion_port = completion_port

    async def create_todo(self, cmd: CreateTodoCommand) -> TodoId:
        self._validate_create(cmd)
        normalized_priority = normalize_todo_priority(cmd.priority)
        agg = TodoAggregate.create(
            title=cmd.title,
            description=cmd.description,
            priority=normalized_priority,
            category=cmd.category,
            due_date=cmd.due_date,
            estimated_duration=cmd.estimated_duration,
            created_by=cmd.created_by,
            parent_todo_id=cmd.parent_todo_id,
            depends_on=cmd.depends_on or [],
            source_type=cmd.source_type or "manual",
            source_id=cmd.source_id,
        )
        if cmd.tags:
            for tag in cmd.tags:
                agg.add_tag(tag, cmd.created_by)

        await self.repo.save(agg)
        if cmd.agent_entity_id:
            await self.set_agent_context(
                todo_id=agg.get_todo().todo_id.value,
                agent_entity_id=cmd.agent_entity_id,
                updated_by=cmd.created_by,
            )
        self._clear_cache(cmd.created_by)
        return agg.get_todo().todo_id

    async def update_todo(self, cmd: UpdateTodoCommand) -> bool:
        self._validate_update(cmd)
        agg = await self._get_agg(cmd.todo_id)
        agg.update_details(
            title=cmd.title,
            description=cmd.description,
            priority=cmd.priority,
            due_date=cmd.due_date,
            estimated_duration=cmd.estimated_duration,
            updated_by=cmd.updated_by,
        )
        await self.repo.save(agg)
        self._clear_cache(agg.get_todo().assigned_to)
        return True

    async def complete_todo(self, cmd: CompleteTodoCommand) -> bool:
        agg = await self._get_agg(cmd.todo_id)
        agg.complete(actual_duration=cmd.actual_duration, completed_by=cmd.completed_by)
        await self.repo.save(agg)
        self._clear_cache(agg.get_todo().assigned_to)

        if self._todo_completion_port is not None:
            try:
                await self._todo_completion_port.on_todo_completed(cmd.todo_id.value)
            except Exception as exc:  # noqa: BLE001 - completion hook must not break todo flow
                logger.warning(f"Failed to process todo chain completion hook for {cmd.todo_id.value}: {exc}")

        return True

    async def cancel_todo(self, cmd: CancelTodoCommand) -> bool:
        agg = await self._get_agg(cmd.todo_id)
        agg.cancel(reason=cmd.reason, cancelled_by=cmd.cancelled_by)
        await self.repo.save(agg)
        self._clear_cache(agg.get_todo().assigned_to)
        return True

    async def assign_todo(self, cmd: AssignTodoCommand) -> bool:
        agg = await self._get_agg(cmd.todo_id)
        agg.assign(assignee=cmd.assignee, assigned_by=cmd.assigned_by)
        await self.repo.save(agg)
        self._clear_cache(cmd.assignee)
        return True

    async def update_progress(self, cmd: UpdateProgressCommand) -> bool:
        agg = await self._get_agg(cmd.todo_id)
        agg.update_progress(progress=cmd.progress, updated_by=cmd.updated_by)
        await self.repo.save(agg)
        return True

    async def get_todo(self, todo_id: TodoId) -> TodoAggregate | None:
        return await self.repo.find_by_id(todo_id)

    async def list_todos(self, options: TodoQueryOptions) -> list[TodoAggregate]:
        return await self.repo.find_by_criteria(options)

    async def list_todos_by_source(
        self,
        *,
        source_type: str,
        source_id: str | None = None,
        assignee: str | None = None,
        limit: int = 50,
    ) -> list[TodoAggregate]:
        options = TodoQueryOptions(
            page=1,
            limit=max(1, int(limit or 50)),
            source_type_filter=str(source_type or "").strip() or None,
            source_id_filter=str(source_id or "").strip() or None,
            assignee_filter=str(assignee or "").strip() or None,
        )
        return await self.list_todos(options)

    async def list_todos_by_agent_context(
        self,
        *,
        agent_status: str | None = None,
        agent_entity_id: str | None = None,
        agent_run_id: str | None = None,
        limit: int = 20,
        offset: int = 0,
    ) -> list[TodoAggregate]:
        """Dedicated query path for agent context filters (outside Todo generic filters)."""
        started = time.perf_counter()
        self._inc_query_metric("dedicated_query_calls")
        normalized_status = str(agent_status or "").strip() or None
        normalized_entity = str(agent_entity_id or "").strip() or None
        normalized_run_id = str(agent_run_id or "").strip() or None
        normalized_limit = max(1, int(limit or 20))
        normalized_offset = max(0, int(offset or 0))

        if self.todo_agent_context_repo and hasattr(self.todo_agent_context_repo, "find_todo_ids"):
            self._inc_query_metric("dedicated_query_context_repo_path_calls")
            todo_ids = await self.todo_agent_context_repo.find_todo_ids(
                agent_status=normalized_status,
                agent_entity_id=normalized_entity,
                agent_run_id=normalized_run_id,
                limit=normalized_limit,
                offset=normalized_offset,
            )
            if not todo_ids:
                self._inc_query_metric("dedicated_query_empty_results")
                self._inc_query_metric("dedicated_query_duration_ms_total", self._duration_ms(started))
                return []

            result = await self._load_todos_with_context(todo_ids)
            self._inc_query_metric("dedicated_query_duration_ms_total", self._duration_ms(started))
            return result

        # Compatibility fallback when context repo is disabled.
        self._inc_query_metric("dedicated_query_compat_fallback_calls")
        fallback_opts = TodoQueryOptions(page=1, limit=max(200, normalized_limit + normalized_offset))
        aggregates = await self.list_todos(fallback_opts)
        todo_ids = [aggregate.get_todo().todo_id.value for aggregate in aggregates]
        context_map: dict[str, TodoAgentContext] = await self.batch_get_agent_context(todo_ids)
        filtered = [
            aggregate
            for aggregate in aggregates
            if self._matches_agent_filters(
                context=context_map.get(aggregate.get_todo().todo_id.value),
                agent_status=normalized_status,
                agent_entity_id=normalized_entity,
                agent_run_id=normalized_run_id,
            )
        ]
        result = filtered[normalized_offset : normalized_offset + normalized_limit]
        if not result:
            self._inc_query_metric("dedicated_query_empty_results")
        self._inc_query_metric("dedicated_query_duration_ms_total", self._duration_ms(started))
        return result

    async def set_agent_context(
        self,
        todo_id: Any,
        agent_entity_id: str | None = None,
        agent_run_id: str | None = None,
        agent_status: str | None = None,
        updated_by: str = "system",
    ) -> TodoAgentContext | None:
        if self.todo_agent_context_repo is None:
            logger.warning("TodoAgentContextRepository is not configured; skip set_agent_context")
            return None

        todo_key = self._normalize_todo_id(todo_id)
        context = await self.todo_agent_context_repo.upsert_partial(
            todo_id=todo_key,
            agent_entity_id=agent_entity_id,
            agent_run_id=agent_run_id,
            agent_status=agent_status,
            updated_by=updated_by,
        )
        return context

    async def get_agent_context(self, todo_id: Any) -> TodoAgentContext | None:
        if self.todo_agent_context_repo is None:
            return None
        return await self.todo_agent_context_repo.get(self._normalize_todo_id(todo_id))

    async def batch_get_agent_context(self, todo_ids: list[Any]) -> dict[str, TodoAgentContext]:
        normalized_ids = [self._normalize_todo_id(todo_id) for todo_id in (todo_ids or [])]
        normalized_ids = [todo_id for todo_id in normalized_ids if todo_id]
        if not normalized_ids:
            return {}

        if self.todo_agent_context_repo is None:
            return {}
        return await self.todo_agent_context_repo.batch_get(normalized_ids)

    async def get_stats(self) -> TodoStats:
        return await self.repo.get_statistics()

    async def count_by_time_buckets(
        self,
        *,
        start_time: datetime,
        end_time: datetime,
        granularity: str = "hour",
        filters: dict[str, Any] | None = None,
    ) -> list[dict[str, Any]]:
        counter = getattr(self.repo, "count_by_time_buckets", None)
        if not callable(counter):
            return []

        criteria = dict(filters or {})
        status_filter = str(criteria.get("status") or "").strip() or None
        priority_filter = str(criteria.get("priority") or "").strip() or None
        category_filter = str(criteria.get("category") or "").strip() or None
        assignee_filter = str(criteria.get("assignee") or criteria.get("owner") or "").strip() or None

        return await counter(
            start_time=start_time,
            end_time=end_time,
            granularity=granularity,
            status_filter=status_filter,
            priority_filter=priority_filter,
            category_filter=category_filter,
            assignee_filter=assignee_filter,
            include_deleted=bool(criteria.get("include_deleted", False)),
            overdue_only=bool(criteria.get("overdue_only", False)),
            due_today=bool(criteria.get("due_today", False)),
            high_priority_only=bool(criteria.get("high_priority_only", False)),
        )

    def get_agent_context_query_metrics(self) -> dict[str, float]:
        """Expose migration observability counters for dedicated agent context queries."""
        snapshot = dict(self._agent_context_query_metrics)
        calls = snapshot.get("dedicated_query_calls", 0.0)
        snapshot["dedicated_query_avg_duration_ms"] = (
            snapshot.get("dedicated_query_duration_ms_total", 0.0) / calls if calls else 0.0
        )
        snapshot["dedicated_query_context_repo_path_ratio"] = (
            snapshot.get("dedicated_query_context_repo_path_calls", 0.0) / calls if calls else 0.0
        )
        snapshot["dedicated_query_compat_fallback_ratio"] = (
            snapshot.get("dedicated_query_compat_fallback_calls", 0.0) / calls if calls else 0.0
        )

        repo_metrics_getter = getattr(self.todo_agent_context_repo, "get_metrics_snapshot", None)
        if callable(repo_metrics_getter):
            try:
                repo_metrics = repo_metrics_getter()
            except Exception as exc:  # noqa: BLE001 - metrics snapshot must not break service
                logger.debug(f"get_metrics_snapshot from todo agent context repo failed: {exc}")
                repo_metrics = {}

            for key, value in (repo_metrics or {}).items():
                snapshot[f"repo_{key}"] = float(value) if isinstance(value, (int, float)) else value

        return snapshot

    # ==== TodoServiceInterface 抽象方法实现 ====

    async def search_todos(self, query: str, options=None) -> list[TodoAggregate]:
        """搜索待办事项"""
        search_options = options or TodoQueryOptions()
        search_options.search_query = query
        return await self.list_todos(search_options)

    async def get_todo_stats(self, criteria: dict) -> TodoStats:
        """获取待办事项统计"""
        return await self.repo.get_statistics()

    async def get_overdue_todos(self, options=None) -> list[TodoAggregate]:
        """获取逾期待办事项"""
        query_options = options or TodoQueryOptions()
        query_options.overdue_only = True
        return await self.list_todos(query_options)

    async def get_due_today_todos(self, options=None) -> list[TodoAggregate]:
        """获取今日到期的待办事项"""
        from datetime import date

        query_options = options or TodoQueryOptions()
        query_options.due_date = date.today()
        return await self.list_todos(query_options)

    async def get_high_priority_todos(self, options=None) -> list[TodoAggregate]:
        """获取高优先级待办事项"""
        query_options = options or TodoQueryOptions()
        query_options.priority = "高"
        return await self.list_todos(query_options)

    async def _get_agg(self, todo_id: TodoId) -> TodoAggregate:
        agg = await self.repo.find_by_id(todo_id)
        if not agg:
            raise DomainException(f"Todo not found: {todo_id}")
        return agg

    def _clear_cache(self, user: str | None):
        if user:
            self.cache.invalidate(f"todo:assignee:{user}")
        self.cache.invalidate("todo:stats:*")

    def _validate_create(self, cmd: CreateTodoCommand):
        if not cmd.title or not cmd.title.strip():
            raise ValidationException("Title required", "title")
        if cmd.priority:
            try:
                TodoPriority(normalize_todo_priority(cmd.priority))
            except ValueError as exc:
                raise ValidationException(f"Invalid priority: {cmd.priority}", "priority") from exc

    def _validate_update(self, cmd: UpdateTodoCommand):
        if cmd.title and not cmd.title.strip():
            raise ValidationException("Title cannot be empty", "title")

    async def _load_todos_with_context(self, todo_ids: list[str]) -> list[TodoAggregate]:
        if not todo_ids:
            return []

        aggregates_by_id: dict[str, TodoAggregate] = {}
        finder = getattr(self.repo, "find_by_ids", None)
        if callable(finder):
            try:
                result = await finder([TodoId(todo_id) for todo_id in todo_ids])
            except Exception as exc:  # noqa: BLE001 - batch fetch fallback must not break context load
                logger.warning(f"todo find_by_ids failed in context query, fallback to single get: {exc}")
                result = {}

            if isinstance(result, dict):
                aggregates_by_id = {str(key): value for key, value in result.items() if value is not None}
            elif isinstance(result, list):
                for aggregate in result:
                    if aggregate is None:
                        continue
                    key = aggregate.get_todo().todo_id.value
                    aggregates_by_id[key] = aggregate

        if not aggregates_by_id:
            for todo_id in todo_ids:
                aggregate = await self.repo.find_by_id(TodoId(todo_id))
                if aggregate is not None:
                    aggregates_by_id[todo_id] = aggregate

        ordered_aggregates = [aggregates_by_id[todo_id] for todo_id in todo_ids if todo_id in aggregates_by_id]
        return ordered_aggregates

    @staticmethod
    def _normalize_todo_id(todo_id: Any) -> str:
        if isinstance(todo_id, TodoId):
            return todo_id.value
        if hasattr(todo_id, "value"):
            return str(todo_id.value or "").strip()
        return str(todo_id or "").strip()

    @staticmethod
    def _matches_agent_filters(
        *,
        context: TodoAgentContext | None,
        agent_status: str | None,
        agent_entity_id: str | None,
        agent_run_id: str | None,
    ) -> bool:
        current_status = str(getattr(context, "agent_status", "pending") or "pending").strip()
        current_entity = str(getattr(context, "agent_entity_id", "default") or "default").strip()
        current_run_id = str(getattr(context, "agent_run_id", "") or "").strip()

        if agent_status and current_status != agent_status:
            return False
        if agent_entity_id and current_entity != agent_entity_id:
            return False
        return not (agent_run_id and current_run_id != agent_run_id)

    def _inc_query_metric(self, key: str, value: float = 1.0) -> None:
        self._agent_context_query_metrics[key] = float(self._agent_context_query_metrics.get(key, 0.0)) + float(value)

    @staticmethod
    def _duration_ms(started: float) -> float:
        return (time.perf_counter() - started) * 1000.0
