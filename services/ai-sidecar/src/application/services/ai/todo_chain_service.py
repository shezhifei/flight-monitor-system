"""AI TODO chain orchestration service."""

from __future__ import annotations

import asyncio
from collections import deque
from collections.abc import Awaitable, Callable
from datetime import datetime, timedelta
from typing import Any, ClassVar

from src.application.services.ai.todo_agent_service import TodoExecutionRequest
from src.application.services.async_todo_service import CreateTodoCommand
from src.domain.models.todo import TodoId, TodoStatus
from src.domain.models.todo_query import TodoQueryOptions
from src.domain.utils.time_utils import utc_now
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class TodoChainService:
    """Create and orchestrate chained TODO executions."""

    _PRIORITY_ALIASES: ClassVar[dict[str, str]] = {
        "CRITICAL": "关键",
        "HIGH": "高",
        "MEDIUM": "中",
        "LOW": "低",
        "BACKGROUND": "后台",
        "关键": "关键",
        "高": "高",
        "中": "中",
        "低": "低",
        "后台": "后台",
    }

    def __init__(self, todo_service: Any, agent_service: Any, chain_repo: Any, sse_hub: Any):
        self._todo_service = todo_service
        self._agent_service = agent_service
        self._chain_repo = chain_repo
        self._sse_hub = sse_hub
        self._create_concurrency = 12
        self._save_concurrency = 20
        self._execute_concurrency = 8

    async def create_chain_from_template(
        self,
        template_id: str,
        context: dict[str, Any] | None,
        created_by: str,
    ) -> list[str]:
        """Create TODO chain from template and set dependencies/execution order."""
        template = await self._chain_repo.get_template(template_id)
        if not template:
            raise ValueError(f"TODO chain template not found: {template_id}")

        task_types = self._validate_template_steps(template.get("task_types"))

        sorted_steps = self._sort_steps(task_types)
        render_context = context or {}

        create_inputs: list[tuple[int, dict[str, Any], CreateTodoCommand]] = []
        for index, step in enumerate(sorted_steps):
            step_key = self._step_key(step, index)
            title = self._render_text(step.get("title") or step.get("name") or step_key, render_context)
            description = self._render_text(step.get("description"), render_context)
            priority = self._normalize_priority(step.get("priority"))
            due_date = self._resolve_due_date(step)
            category = self._render_text(step.get("category"), render_context)

            command = CreateTodoCommand(
                title=title,
                description=description,
                priority=priority,
                category=category,
                due_date=due_date,
                created_by=created_by,
            )
            create_inputs.append((index, step, command))

        created_meta = await self._create_todos_concurrently(create_inputs)
        created_meta.sort(key=lambda item: int(item["index"]))
        created_ids = [item["todo_id"] for item in created_meta]
        step_id_to_todo_id = {
            self._step_key(item["step"], int(item["index"])): item["todo_id"] for item in created_meta
        }
        agent_context_updates: list[tuple[str, str]] = []
        for item in created_meta:
            agent_entity_id = str(item["step"].get("agent_entity_id") or "").strip()
            if agent_entity_id:
                agent_context_updates.append((item["todo_id"], agent_entity_id))

        repo = getattr(self._todo_service, "repo", None)
        if repo is not None and created_meta:
            aggregate_map: dict[str, Any] = {}
            todo_ids = [TodoId(item["todo_id"]) for item in created_meta]
            batch_finder = getattr(repo, "find_by_ids", None)
            if callable(batch_finder):
                try:
                    batch_result = await batch_finder(todo_ids)
                except Exception as exc:  # noqa: BLE001 - best-effort batch find; repo backend may vary
                    logger.warning(f"batch find_by_ids failed in todo chain create flow: {exc}")
                    batch_result = {}

                if isinstance(batch_result, dict):
                    aggregate_map = {
                        str(key): value for key, value in (batch_result or {}).items() if value is not None
                    }
                else:
                    for aggregate in batch_result or []:
                        if aggregate is None:
                            continue
                        aggregate_map[self._todo_id(aggregate)] = aggregate
            else:
                lookups = [repo.find_by_id(todo_id) for todo_id in todo_ids]
                results = await asyncio.gather(*lookups, return_exceptions=True)
                for result in results:
                    if isinstance(result, Exception) or result is None:
                        continue
                    aggregate_map[self._todo_id(result)] = result

            pending_updates: list[Any] = []
            for item in created_meta:
                step = item["step"]
                todo_id = item["todo_id"]
                index = item["index"]

                aggregate = aggregate_map.get(todo_id)
                if aggregate is None:
                    continue

                todo = aggregate.get_todo()
                depends_on_keys = [str(x) for x in (step.get("depends_on") or []) if str(x).strip()]
                depends_on_ids = [
                    step_id_to_todo_id[key]
                    for key in depends_on_keys
                    if key in step_id_to_todo_id and step_id_to_todo_id[key] != todo_id
                ]

                aggregate.update_execution_order(int(step.get("execution_order") or index), updated_by=created_by)
                aggregate.update_source_info(
                    source_type="ai_chain",
                    source_id=template_id,
                    updated_by=created_by,
                )

                parent_step_key = str(step.get("parent_step_id") or "").strip()
                parent_todo_id = step_id_to_todo_id.get(parent_step_key)
                if parent_todo_id:
                    todo.parent_todo_id = parent_todo_id

                if depends_on_ids:
                    aggregate.update_dependencies(depends_on_ids, updated_by=created_by)
                    if todo.status.value != TodoStatus.BLOCKED.value:
                        try:
                            aggregate.update_status(TodoStatus.BLOCKED.value, updated_by=created_by)
                        except Exception as exc:  # noqa: BLE001 - domain status update; may raise validation errors
                            logger.warning(f"Failed to set BLOCKED status on chained TODO: {exc}")

                pending_updates.append(aggregate)

            await self._save_aggregates(repo, pending_updates)
        await self._apply_agent_context_updates(agent_context_updates, updated_by=created_by)

        await self._publish_chain_event(
            event_type="todo_chain_created",
            payload={
                "template_id": template_id,
                "todo_ids": created_ids,
                "created_by": created_by,
            },
        )
        return created_ids

    async def on_todo_completed(self, todo_id: str) -> list[str]:
        """Handle TODO completion and trigger downstream TODO execution when unblocked."""
        aggregates = await self._list_all_todos()
        if not aggregates:
            return []

        todo_map = {self._todo_id(item): item for item in aggregates}
        downstream = [item for item in aggregates if todo_id in (item.get_todo().depends_on or [])]

        triggered_ids: list[str] = []
        repo = getattr(self._todo_service, "repo", None)
        pending_unblocks: list[Any] = []

        for aggregate in downstream:
            current_todo = aggregate.get_todo()
            current_id = current_todo.todo_id.value

            if not self._dependencies_completed(current_todo.depends_on or [], todo_map):
                continue

            if current_todo.status.value == TodoStatus.BLOCKED.value and repo is not None:
                try:
                    aggregate.update_status(TodoStatus.PENDING.value, updated_by="TodoChainService")
                    pending_unblocks.append(aggregate)
                except Exception as exc:  # noqa: BLE001 - domain status update; may raise validation errors
                    logger.warning(f"Failed to unblock chained TODO {current_id}: {exc}")

            triggered_ids.append(current_id)

        if pending_unblocks and repo is not None:
            await self._save_aggregates(repo, pending_unblocks)

        if triggered_ids and self._agent_service is not None:
            await self._execute_todos(triggered_ids)

        if triggered_ids:
            await self._publish_chain_event(
                event_type="todo_chain_progressed",
                payload={
                    "completed_todo_id": todo_id,
                    "triggered_todo_ids": triggered_ids,
                },
            )

        return triggered_ids

    async def get_chain_status(self, root_todo_id: str) -> dict[str, Any]:
        """Build DAG view payload (nodes + edges + summary) for a chain root."""
        aggregates = await self._list_all_todos()
        if not aggregates:
            return {"root_todo_id": root_todo_id, "nodes": [], "edges": [], "summary": {"total": 0}}

        by_id = {self._todo_id(item): item for item in aggregates}
        if root_todo_id not in by_id:
            raise ValueError(f"Root TODO not found: {root_todo_id}")

        collect_ids = self._collect_related_ids(root_todo_id, aggregates)
        nodes: list[dict[str, Any]] = []
        edges: list[dict[str, Any]] = []
        status_stats: dict[str, int] = {}

        for todo_id in sorted(collect_ids):
            aggregate = by_id.get(todo_id)
            if aggregate is None:
                continue
            todo = aggregate.get_todo()
            status = todo.status.value
            status_stats[status] = status_stats.get(status, 0) + 1

            nodes.append(
                {
                    "id": todo_id,
                    "title": todo.title.value,
                    "status": status,
                    "priority": todo.priority.value,
                    "parent_todo_id": todo.parent_todo_id,
                    "execution_order": todo.execution_order,
                    "depends_on": list(todo.depends_on or []),
                    "due_date": todo.due_date.isoformat() if todo.due_date else None,
                }
            )

            if todo.parent_todo_id and todo.parent_todo_id in collect_ids:
                edges.append(
                    {
                        "source": todo.parent_todo_id,
                        "target": todo_id,
                        "type": "parent",
                    }
                )

            for dep in todo.depends_on or []:
                if dep in collect_ids:
                    edges.append(
                        {
                            "source": dep,
                            "target": todo_id,
                            "type": "depends_on",
                        }
                    )

        return {
            "root_todo_id": root_todo_id,
            "nodes": nodes,
            "edges": edges,
            "summary": {
                "total": len(nodes),
                "status": status_stats,
                "completed": status_stats.get(TodoStatus.COMPLETED.value, 0),
            },
        }

    async def list_templates(self) -> list[dict[str, Any]]:
        """List TODO chain templates."""
        return await self._chain_repo.list_templates()

    async def _list_all_todos(self) -> list[Any]:
        options = TodoQueryOptions(page=1, limit=2000, include_deleted=False)
        return await self._todo_service.list_todos(options)

    def _collect_related_ids(self, root_todo_id: str, aggregates: list[Any]) -> set[str]:
        by_id = {self._todo_id(item): item.get_todo() for item in aggregates}
        children: dict[str, set[str]] = {}
        dependents: dict[str, set[str]] = {}

        for item in aggregates:
            todo = item.get_todo()
            todo_id = todo.todo_id.value

            parent_id = todo.parent_todo_id
            if parent_id:
                children.setdefault(parent_id, set()).add(todo_id)

            for dep_id in todo.depends_on or []:
                dependents.setdefault(dep_id, set()).add(todo_id)

        related: set[str] = set()
        queue = deque([root_todo_id])
        while queue:
            current = queue.popleft()
            if current in related:
                continue

            related.add(current)
            current_todo = by_id.get(current)
            if current_todo:
                for dep in current_todo.depends_on or []:
                    if dep in by_id and dep not in related:
                        queue.append(dep)

            for nxt in children.get(current, set()):
                if nxt not in related:
                    queue.append(nxt)
            for nxt in dependents.get(current, set()):
                if nxt not in related:
                    queue.append(nxt)

        return related

    @staticmethod
    def _todo_id(aggregate: Any) -> str:
        return aggregate.get_todo().todo_id.value

    @staticmethod
    def _extract_todo_id(value: Any) -> str | None:
        if value is None:
            return None
        if isinstance(value, str):
            return value.strip() or None
        raw_value = getattr(value, "value", None)
        if raw_value is not None:
            return str(raw_value).strip() or None
        if isinstance(value, dict):
            raw = value.get("id") or value.get("todo_id")
            return str(raw).strip() if raw is not None else None
        return None

    @classmethod
    def _normalize_priority(cls, value: Any) -> str:
        normalized = str(value or "中").strip()
        return cls._PRIORITY_ALIASES.get(normalized, "中")

    @staticmethod
    def _validate_template_steps(raw_steps: Any) -> list[dict[str, Any]]:
        if not isinstance(raw_steps, list) or not raw_steps:
            raise ValueError("TODO chain template has no valid task_types")

        normalized_steps: list[dict[str, Any]] = []
        for index, step in enumerate(raw_steps):
            if not isinstance(step, dict):
                raise ValueError(f"TODO chain template step[{index}] must be an object")

            depends_on = step.get("depends_on")
            if depends_on is not None and not isinstance(depends_on, list):
                raise ValueError(f"TODO chain template step[{index}].depends_on must be a list")

            normalized_steps.append(step)

        return normalized_steps

    @staticmethod
    def _sort_steps(task_types: list[dict[str, Any]]) -> list[dict[str, Any]]:
        def key_fn(item: dict[str, Any]) -> tuple:
            order = item.get("execution_order")
            if isinstance(order, int):
                return (0, order)
            if isinstance(order, str) and order.isdigit():
                return (0, int(order))
            return (1, 0)

        return sorted(task_types, key=key_fn)

    @staticmethod
    def _step_key(step: dict[str, Any], index: int) -> str:
        for field in ("step_id", "id", "code", "name"):
            value = str(step.get(field) or "").strip()
            if value:
                return value
        return f"step_{index + 1}"

    @staticmethod
    def _render_text(raw: Any, context: dict[str, Any]) -> str | None:
        if raw is None:
            return None
        text = str(raw)
        if not text:
            return None
        try:
            return text.format(**context)
        except Exception as exc:  # noqa: BLE001 - template rendering; multiple failure modes
            logger.warning(f"Template rendering failed for text '{text[:80]}': {exc}")
            return text

    @staticmethod
    def _resolve_due_date(step: dict[str, Any]) -> datetime | None:
        due_in_minutes = step.get("due_in_minutes")
        if due_in_minutes is not None:
            try:
                return utc_now() + timedelta(minutes=int(due_in_minutes))
            except Exception as exc:  # noqa: BLE001 - best-effort due date parse; ignore invalid values
                logger.warning(f"Failed to parse due_in_minutes={due_in_minutes!r}: {exc}")

        raw_due_date = step.get("due_date")
        if isinstance(raw_due_date, datetime):
            return raw_due_date
        if isinstance(raw_due_date, str):
            text = raw_due_date.strip()
            if not text:
                return None
            if text.endswith("Z"):
                text = text[:-1] + "+00:00"
            try:
                return datetime.fromisoformat(text)
            except ValueError:
                return None

        return None

    @staticmethod
    def _dependencies_completed(depends_on: list[str], todo_map: dict[str, Any]) -> bool:
        if not depends_on:
            return True
        for dep_id in depends_on:
            aggregate = todo_map.get(dep_id)
            if aggregate is None:
                return False
            if aggregate.get_todo().status.value != TodoStatus.COMPLETED.value:
                return False
        return True

    async def _create_todos_concurrently(
        self,
        create_inputs: list[tuple[int, dict[str, Any], CreateTodoCommand]],
    ) -> list[dict[str, Any]]:
        if not create_inputs:
            return []

        semaphore = asyncio.Semaphore(max(1, int(self._create_concurrency)))
        results: list[dict[str, Any] | None] = [None] * len(create_inputs)

        async def _run(item_index: int, step: dict[str, Any], command: CreateTodoCommand) -> None:
            async with semaphore:
                created = await self._todo_service.create_todo(command)
                todo_id = self._extract_todo_id(created)
                if not todo_id:
                    raise RuntimeError("failed to create todo for chain template")
                results[item_index] = {
                    "step": step,
                    "todo_id": todo_id,
                    "index": item_index,
                }

        tasks = [_run(item_index=index, step=step, command=command) for index, step, command in create_inputs]
        await asyncio.gather(*tasks, return_exceptions=False)
        return [item for item in results if item is not None]

    async def _save_aggregates(self, repo: Any, aggregates: list[Any]) -> None:
        if not aggregates:
            return

        save_batch = getattr(repo, "save_batch", None)
        if callable(save_batch):
            try:
                await save_batch(aggregates)
                return
            except Exception as exc:  # noqa: BLE001 - best-effort save_batch; repo backend may vary
                logger.warning(f"todo chain save_batch failed, fallback to single save: {exc}")

        await self._run_bounded_jobs(
            [lambda agg=aggregate: repo.save(agg) for aggregate in aggregates],
            concurrency=self._save_concurrency,
            error_prefix="todo chain save failed",
        )

    async def _apply_agent_context_updates(
        self,
        updates: list[tuple[str, str]],
        *,
        updated_by: str,
    ) -> None:
        if not updates:
            return

        setter = getattr(self._todo_service, "set_agent_context", None)
        if not callable(setter):
            logger.debug("todo service has no set_agent_context; skip chain agent context updates")
            return

        deduped: dict[str, str] = {}
        for todo_id, agent_entity_id in updates:
            if todo_id and agent_entity_id:
                deduped[todo_id] = agent_entity_id

        await self._run_bounded_jobs(
            [
                lambda tid=todo_id, eid=entity_id: setter(
                    todo_id=tid,
                    agent_entity_id=eid,
                    updated_by=updated_by,
                )
                for todo_id, entity_id in deduped.items()
            ],
            concurrency=self._save_concurrency,
            error_prefix="todo chain set agent context failed",
        )

    async def _execute_todos(self, todo_ids: list[str]) -> None:
        if not todo_ids or self._agent_service is None:
            return

        await self._run_bounded_jobs(
            [
                lambda todo_id=todo_id: self._agent_service.execute_todo(TodoExecutionRequest(todo_id=todo_id))
                for todo_id in todo_ids
            ],
            concurrency=self._execute_concurrency,
            error_prefix="failed to trigger AI execution for chained TODO",
        )

    async def _run_bounded_jobs(
        self,
        jobs: list[Callable[[], Awaitable[Any]]],
        *,
        concurrency: int,
        error_prefix: str,
    ) -> None:
        if not jobs:
            return

        semaphore = asyncio.Semaphore(max(1, int(concurrency)))

        async def _run(job: Callable[[], Awaitable[Any]]) -> None:
            async with semaphore:
                try:
                    await job()
                except Exception as exc:  # noqa: BLE001 - bounded job runner; job may raise arbitrary errors
                    logger.warning(f"{error_prefix}: {exc}")

        await asyncio.gather(*[_run(job) for job in jobs], return_exceptions=False)

    async def _publish_chain_event(self, event_type: str, payload: dict[str, Any]) -> None:
        if self._sse_hub is None:
            return

        event_payload = {
            "type": event_type,
            "timestamp": utc_now().isoformat(),
            **payload,
        }
        try:
            await self._sse_hub.broadcast_to_topic("ai_execution", event_payload)
        except Exception as exc:  # noqa: BLE001 - best-effort SSE publish; transport may vary
            logger.warning(f"Failed to publish todo chain event: {exc}")
