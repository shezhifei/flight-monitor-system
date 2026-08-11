"""TODO Agent executor — main class."""

from __future__ import annotations

import asyncio
import logging
import time
from typing import Any

from src.domain.ai.agent_execution import (
    AgentExecution,
    AgentExecutionStatus,
    AgentStep,
    AgentStepType,
    ExecutionGraph,
    TodoExecutionNode,
)
from src.domain.ports.agent_runtime_port import (
    AgentExecutionResultDTO,
    AgentTodoInfoDTO,
)
from src.domain.utils.time_utils import utc_now
from src.infrastructure.ai.monitoring.metrics import (
    record_error,
)
from src.infrastructure.ai.shared_context_pool import (
    ContextEntry,
    MemorySharedContextPool,
    SharedContextPool,
)
from src.infrastructure.ai.tools.base import InvocationMode

logger = logging.getLogger(__name__)


class _SingleTodoMixin:
    """Mixin for TodoAgentExecutor."""

    async def execute_todo(
        self,
        todo: AgentTodoInfoDTO,
        max_iterations: int = 10,
        system_prompt_override: str | None = None,
        user_id: str | None = None,
        user_roles: list[str] | None = None,
        invocation_mode: InvocationMode = InvocationMode.AGENT_AUTONOMOUS,
        shared_pool: SharedContextPool | None = None,
        root_todo_id: str | None = None,
    ) -> AgentExecutionResultDTO:
        """执行单个 TODO"""
        start_time = time.time()
        child_todos: list[str] = []

        # 加载 AI 实体配置
        entity_config = await self._config_store.get_entity_config(todo.entity_id)
        if not entity_config:
            return AgentExecutionResultDTO(
                run_id="",
                todo_id=todo.todo_id,
                status=AgentExecutionStatus.FAILED,
                response=None,
                total_steps=0,
                total_tokens=0,
                total_tool_calls=0,
                duration_ms=0,
                error_message=f"AI entity '{todo.entity_id}' not found",
            )

        # 创建执行记录
        execution_metadata: dict[str, Any] = {}
        if user_id:
            execution_metadata["user_id"] = user_id
        if user_roles:
            execution_metadata["user_roles"] = list(user_roles)
        execution_metadata["invocation_mode"] = invocation_mode.value

        execution = AgentExecution.create(
            todo_id=todo.todo_id,
            entity_id=todo.entity_id,
            entity_config=entity_config,
            metadata=execution_metadata or None,
        )
        self._execution_started_at_iso[execution.run_id] = utc_now().isoformat()
        await self._execution_repo.save_execution(execution)
        await self._register_cancel_event(execution.run_id)

        # 更新 TODO 状态
        if self._todo_updater:
            await self._todo_updater(todo.todo_id, "running", execution.run_id)

        execution.start()
        await self._execution_repo.update_execution(execution)

        try:
            ai_timeout_seconds = max(
                self.MIN_AI_CALL_TIMEOUT_SECONDS,
                float(entity_config.get("timeout", 30.0) or 30.0),
            )
        except (TypeError, ValueError):
            ai_timeout_seconds = 30.0

        tool_timeout_seconds = ai_timeout_seconds
        tools_config = entity_config.get("tools")
        if isinstance(tools_config, dict):
            try:
                tool_timeout_seconds = max(
                    self.MIN_TOOL_CALL_TIMEOUT_SECONDS,
                    float(tools_config.get("timeout", ai_timeout_seconds) or ai_timeout_seconds),
                )
            except (TypeError, ValueError):
                tool_timeout_seconds = ai_timeout_seconds

        estimated_tokens = 1000
        rate_limit_acquired = False
        tokens_recorded = False

        try:
            # 获取速率限制配额
            async with self._semaphore:
                await self._rate_limiter.acquire(estimated_tokens=estimated_tokens)
                rate_limit_acquired = True

                if await self._is_execution_cancelled(execution.run_id):
                    raise asyncio.CancelledError(f"Execution {execution.run_id} cancelled before agent loop")

                # 创建 AI 实体
                ai_entity = await self._create_ai_entity(entity_config)

                # 读取上游 Agent 结论（Blackboard 注入）
                upstream_context = ""
                active_pool = shared_pool or self._shared_pool
                if active_pool and root_todo_id and todo.depends_on:
                    try:
                        upstream_entries = await active_pool.read_for_dependencies(
                            root_todo_id, todo.depends_on, max_tokens=2000
                        )
                        if upstream_entries:
                            upstream_context = self._format_upstream_context(upstream_entries)
                            logger.info(
                                f"Injecting {len(upstream_entries)} upstream entries "
                                f"({sum(e.token_count for e in upstream_entries)} tokens) "
                                f"into TODO {todo.todo_id}"
                            )
                    except Exception as pool_exc:  # noqa: BLE001 - best-effort shared pool read must not break execution
                        logger.warning(f"Failed to read shared pool: {pool_exc}")

                # 构建任务描述
                has_downstream = bool(active_pool and root_todo_id)
                task_description = self._build_task_description(
                    todo,
                    entity_config,
                    has_downstream=has_downstream,
                )
                if upstream_context:
                    task_description = upstream_context + task_description

                # 记录初始作业类型
                initial_step = AgentStep.create(
                    run_id=execution.run_id,
                    sequence=1,
                    step_type=AgentStepType.USER_INPUT,
                    role="user",
                    content=task_description,
                )
                execution.add_step(initial_step)
                await self._execution_repo.save_task_type(initial_step)

                # 获取允许的工具
                tools = ai_entity.get_allowed_tools(
                    user_id=user_id,
                    user_roles=user_roles,
                    invocation_mode=invocation_mode.value,
                )

                # 添加创建子任务工具
                spawn_tool = self._get_spawn_subtodo_tool()
                if spawn_tool:
                    tools.append(spawn_tool)

                # 意图预路由：根据任务描述裁剪工具集，减少 LLM 歧义
                try:
                    from src.infrastructure.ai.intent_router import route_tools

                    intent, tools = route_tools(task_description, tools)
                    logger.info(f"Intent routing for TODO {todo.todo_id}: intent='{intent}', tools={len(tools)}")
                except Exception as route_exc:  # noqa: BLE001 - best-effort intent routing must fall back to full tool set
                    logger.warning(f"Intent routing failed, using full tool set: {route_exc}")

                response_text = ""
                graph_runtime_completed = False
                graph_runtime_enabled = self._is_graph_runtime_enabled_for_entity(todo.entity_id, entity_config)
                requested_runtime_path = "graph" if graph_runtime_enabled else "legacy"
                if graph_runtime_enabled:
                    try:
                        from src.infrastructure.ai.graph.agents import create_workflow_agent
                        from src.infrastructure.ai.graph.callbacks import SSEStreamingCallbackHandler
                        from src.infrastructure.ai.graph.checkpointer import AgentExecutionCheckpointer
                        from src.infrastructure.ai.graph.nodes import create_human_message

                        checkpointer = AgentExecutionCheckpointer(self._execution_repo)
                        app = create_workflow_agent().compile(checkpointer=checkpointer)

                        callbacks = []
                        if self._notification_port:
                            callbacks.append(
                                SSEStreamingCallbackHandler(
                                    self._notification_port,
                                    execution.run_id,
                                    todo.todo_id,
                                )
                            )

                        tool_names = self._collect_graph_tool_names(tools)
                        config = await self._prepare_graph_runtime(
                            execution=execution,
                            entity_config=entity_config,
                            tool_names=tool_names,
                            user_id=user_id,
                            user_roles=user_roles,
                            invocation_mode=invocation_mode,
                            callbacks=callbacks,
                            task_description=task_description,
                            child_todos=child_todos,
                        )
                        await self._mark_execution_runtime_path(
                            execution,
                            "graph",
                            requested_path=requested_runtime_path,
                        )

                        initial_state = self._build_initial_graph_state(
                            todo_id=todo.todo_id,
                            entity_id=todo.entity_id,
                            current_plan="",
                            messages=[create_human_message(task_description)],
                            requires_approval=False,
                            pending_action_id=None,
                            pending_tool_call=None,
                            metrics={
                                "total_steps": execution.total_steps,
                                "total_tokens": execution.total_tokens,
                                "total_tool_calls": execution.total_tool_calls,
                            },
                            blackboard_facts=[],
                            error_message=None,
                        )

                        result_state = await app.ainvoke(initial_state, config=config)
                        await self._update_graph_guardrails_from_state(execution, result_state)
                        response_text = self._extract_graph_response_text(result_state)
                        graph_runtime_completed = True

                        if result_state.get("requires_approval", False):
                            execution.status = AgentExecutionStatus.PENDING
                            await self._set_execution_runtime_metadata(
                                execution,
                                runtime_path="graph",
                                runtime_status=self._resolve_pending_approval_status().value,
                                runtime_path_requested=requested_runtime_path,
                            )
                            duration_ms = int((time.time() - start_time) * 1000)
                            await self._emit_graph_approval_required(
                                config=config,
                                run_id=execution.run_id,
                                todo_id=execution.todo_id,
                                result_state=result_state,
                                message="等待人工审批中...",
                            )
                            return AgentExecutionResultDTO(
                                run_id=execution.run_id,
                                todo_id=todo.todo_id,
                                status=self._resolve_pending_approval_status(),
                                response="等待人工审批中...",
                                total_steps=execution.total_steps,
                                total_tokens=execution.total_tokens,
                                total_tool_calls=execution.total_tool_calls,
                                duration_ms=duration_ms,
                                child_todos=child_todos,
                            )
                    except Exception as graph_exc:  # noqa: BLE001 - graph runtime fallback must degrade to legacy loop for any failure
                        await self._mark_execution_runtime_path(
                            execution,
                            "legacy",
                            requested_path=requested_runtime_path,
                            fallback_reason=self._format_runtime_fallback_reason(graph_exc),
                        )
                        logger.warning(
                            "Graph runtime failed for TODO %s, falling back to legacy loop: %s",
                            todo.todo_id,
                            graph_exc,
                        )
                else:
                    await self._mark_execution_runtime_path(
                        execution,
                        "legacy",
                        requested_path=requested_runtime_path,
                    )
                    logger.info(
                        "Graph runtime disabled by feature flag for TODO %s, using legacy loop",
                        todo.todo_id,
                    )

                if not graph_runtime_completed:
                    current_response, _ = await self._execute_agent_loop(
                        ai_entity=ai_entity,
                        execution=execution,
                        task_description=task_description,
                        tools=tools,
                        max_iterations=max_iterations,
                        system_prompt=system_prompt_override or getattr(ai_entity.config, "system_prompt", None),
                        child_todos=child_todos,
                        ai_timeout_seconds=ai_timeout_seconds,
                        tool_timeout_seconds=tool_timeout_seconds,
                        user_id=user_id,
                        user_roles=user_roles,
                        invocation_mode=invocation_mode,
                    )
                    response_text = self._extract_response_content(current_response) or ""

                tokens_recorded = True

                if active_pool and root_todo_id:
                    try:
                        distilled = self._distill_conclusion(response_text)
                        if distilled:
                            await active_pool.write_or_update(
                                root_todo_id,
                                ContextEntry(
                                    source_todo_id=todo.todo_id,
                                    source_todo_title=todo.title,
                                    agent_entity_id=todo.entity_id,
                                    content_type="distilled_conclusion",
                                    content=distilled,
                                ),
                            )
                            logger.info(f"Wrote conclusion to shared pool for TODO {todo.todo_id}")
                    except Exception as pool_write_exc:  # noqa: BLE001 - best-effort shared pool write must not break execution
                        logger.warning(f"Failed to write to shared pool: {pool_write_exc}")

                # 完成执行
                execution.complete()
                await self._set_execution_runtime_metadata(
                    execution,
                    runtime_path="graph" if graph_runtime_completed else "legacy",
                    runtime_status=AgentExecutionStatus.COMPLETED.value,
                    runtime_path_requested=requested_runtime_path,
                )

                if self._todo_updater:
                    await self._todo_updater(todo.todo_id, "completed", execution.run_id)

                duration_ms = int((time.time() - start_time) * 1000)
                await self._emit_ai_event(
                    "execution_end",
                    execution.run_id,
                    execution.todo_id,
                    {
                        "event": "execution_end",
                        "phase": "report",
                        "status": "success",
                        "message": "agent execution completed",
                        "progress_pct": 100,
                        "meta": {"duration_ms": duration_ms},
                    },
                )

                return AgentExecutionResultDTO(
                    run_id=execution.run_id,
                    todo_id=todo.todo_id,
                    status=AgentExecutionStatus.COMPLETED,
                    response=response_text,
                    total_steps=execution.total_steps,
                    total_tokens=execution.total_tokens,
                    total_tool_calls=execution.total_tool_calls,
                    duration_ms=duration_ms,
                    child_todos=child_todos,
                )
        except asyncio.CancelledError as e:
            logger.info(f"Execution cancelled for TODO {todo.todo_id}: {e}")
            record_error("cancelled", execution.entity_id)
            execution.cancel()
            await self._execution_repo.update_execution(execution)

            if self._todo_updater:
                await self._todo_updater(todo.todo_id, "cancelled", execution.run_id)

            duration_ms = int((time.time() - start_time) * 1000)
            await self._emit_ai_event(
                "execution_end",
                execution.run_id,
                execution.todo_id,
                {
                    "event": "execution_end",
                    "phase": "report",
                    "status": "error",
                    "code": "EXECUTION_CANCELLED",
                    "message": str(e),
                    "progress_pct": 100,
                    "meta": {
                        "duration_ms": duration_ms,
                    },
                },
            )
            return AgentExecutionResultDTO(
                run_id=execution.run_id,
                todo_id=todo.todo_id,
                status=AgentExecutionStatus.CANCELLED,
                response=None,
                total_steps=execution.total_steps,
                total_tokens=execution.total_tokens,
                total_tool_calls=execution.total_tool_calls,
                duration_ms=duration_ms,
                error_message=str(e),
                child_todos=child_todos,
            )

        except Exception as e:  # noqa: BLE001 - top-level execution handler must record failure and return failed result for any error
            logger.error(f"Execution failed for TODO {todo.todo_id}: {e}")
            record_error(type(e).__name__, execution.entity_id)
            execution.fail(str(e))
            await self._execution_repo.update_execution(execution)

            if self._todo_updater:
                await self._todo_updater(todo.todo_id, "failed", execution.run_id)

            duration_ms = int((time.time() - start_time) * 1000)
            await self._emit_ai_event(
                "execution_end",
                execution.run_id,
                execution.todo_id,
                {
                    "event": "execution_end",
                    "phase": "report",
                    "status": "error",
                    "code": "EXECUTION_FAILED",
                    "message": str(e),
                    "progress_pct": 100,
                    "meta": {
                        "duration_ms": duration_ms,
                    },
                },
            )

            return AgentExecutionResultDTO(
                run_id=execution.run_id,
                todo_id=todo.todo_id,
                status=AgentExecutionStatus.FAILED,
                response=None,
                total_steps=execution.total_steps,
                total_tokens=execution.total_tokens,
                total_tool_calls=execution.total_tool_calls,
                duration_ms=duration_ms,
                error_message=str(e),
            )
        finally:
            if rate_limit_acquired and not tokens_recorded:
                try:
                    await self._rate_limiter.record_tokens(
                        0,
                        estimated_tokens=estimated_tokens,
                    )
                except Exception as release_error:  # noqa: BLE001 - best-effort token estimate release must not break cleanup
                    logger.warning(
                        f"Failed to release pending token estimate for execution {execution.run_id}: {release_error}"
                    )

            await self._remove_cancel_event(execution.run_id)
            self._execution_started_at_iso.pop(execution.run_id, None)

    async def execute_todo_tree(
        self,
        root_todo_id: str,
        max_iterations_per_todo: int = 10,
        fail_fast: bool = True,
        user_id: str | None = None,
        user_roles: list[str] | None = None,
        invocation_mode: InvocationMode = InvocationMode.AGENT_AUTONOMOUS,
        shared_pool: SharedContextPool | None = None,
    ) -> dict[str, AgentExecutionResultDTO]:
        """执行 TODO 树"""
        if not self._todo_loader:
            raise RuntimeError("TODO loader not configured")

        # 加载 TODO 树
        todos = await self._todo_loader(root_todo_id)
        if not todos:
            logger.warning(f"No TODOs found for root {root_todo_id}")
            return {}

        # 创建共享上下文池（如果未提供，则使用内存实现）
        pool = shared_pool or self._shared_pool or MemorySharedContextPool()
        logger.info(f"execute_todo_tree: using {type(pool).__name__} for root={root_todo_id}")

        # 构建执行图
        graph = ExecutionGraph()
        todo_map = {t.todo_id: t for t in todos}

        for todo in todos:
            graph.add_todo(todo_id=todo.todo_id, depends_on=todo.depends_on or [], entity_id=todo.entity_id)

        results: dict[str, AgentExecutionResultDTO] = {}

        pending_todo_ids: list[str] = [todo.todo_id for todo in todos]
        succeeded_todo_ids: set[str] = set()
        failed_todo_ids: set[str] = set()

        # 按依赖顺序执行
        while pending_todo_ids:
            ready_nodes: list[TodoExecutionNode] = []
            blocked_nodes: list[tuple[str, list[str]]] = []

            for todo_id in list(pending_todo_ids):
                node = graph.todos[todo_id]
                failed_dependencies = sorted([dep for dep in node.depends_on if dep in failed_todo_ids])
                if failed_dependencies:
                    blocked_nodes.append((todo_id, failed_dependencies))
                    continue

                if node.depends_on.issubset(succeeded_todo_ids):
                    ready_nodes.append(node)

            for blocked_todo_id, failed_dependencies in blocked_nodes:
                results[blocked_todo_id] = AgentExecutionResultDTO(
                    run_id="",
                    todo_id=blocked_todo_id,
                    status=AgentExecutionStatus.FAILED,
                    response=None,
                    total_steps=0,
                    total_tokens=0,
                    total_tool_calls=0,
                    duration_ms=0,
                    error_message=("Skipped: dependency failed (" + ", ".join(failed_dependencies) + ")"),
                )
                pending_todo_ids.remove(blocked_todo_id)
                failed_todo_ids.add(blocked_todo_id)

            if not ready_nodes:
                logger.error(f"Deadlock detected, pending: {pending_todo_ids}")
                for pending_todo_id in list(pending_todo_ids):
                    results[pending_todo_id] = AgentExecutionResultDTO(
                        run_id="",
                        todo_id=pending_todo_id,
                        status=AgentExecutionStatus.FAILED,
                        response=None,
                        total_steps=0,
                        total_tokens=0,
                        total_tool_calls=0,
                        duration_ms=0,
                        error_message="Skipped: unresolved dependencies or deadlock",
                    )
                    pending_todo_ids.remove(pending_todo_id)
                    failed_todo_ids.add(pending_todo_id)
                break

            # 并行执行所有就绪的 TODO（传入共享上下文池）
            tasks = [
                self.execute_todo(
                    todo=todo_map[node.todo_id],
                    max_iterations=max_iterations_per_todo,
                    user_id=user_id,
                    user_roles=user_roles,
                    invocation_mode=invocation_mode,
                    shared_pool=pool,
                    root_todo_id=root_todo_id,
                )
                for node in ready_nodes
            ]

            batch_results = await asyncio.gather(*tasks, return_exceptions=True)
            batch_has_failure = False

            for node, result in zip(ready_nodes, batch_results, strict=False):
                pending_todo_ids.remove(node.todo_id)

                if isinstance(result, Exception):
                    results[node.todo_id] = AgentExecutionResultDTO(
                        run_id="",
                        todo_id=node.todo_id,
                        status=AgentExecutionStatus.FAILED,
                        response=None,
                        total_steps=0,
                        total_tokens=0,
                        total_tool_calls=0,
                        duration_ms=0,
                        error_message=str(result),
                    )
                    failed_todo_ids.add(node.todo_id)
                    batch_has_failure = True
                else:
                    results[node.todo_id] = result

                    if result.status == AgentExecutionStatus.COMPLETED:
                        succeeded_todo_ids.add(node.todo_id)
                        graph.mark_completed(node.todo_id)
                    else:
                        failed_todo_ids.add(node.todo_id)
                        batch_has_failure = True

            if fail_fast and batch_has_failure and pending_todo_ids:
                logger.warning(f"Fail-fast enabled. Skipping remaining TODOs after batch failure: {pending_todo_ids}")
                for pending_todo_id in list(pending_todo_ids):
                    results[pending_todo_id] = AgentExecutionResultDTO(
                        run_id="",
                        todo_id=pending_todo_id,
                        status=AgentExecutionStatus.FAILED,
                        response=None,
                        total_steps=0,
                        total_tokens=0,
                        total_tool_calls=0,
                        duration_ms=0,
                        error_message="Skipped: fail-fast triggered after previous failure",
                    )
                    pending_todo_ids.remove(pending_todo_id)
                    failed_todo_ids.add(pending_todo_id)
                break

        return results
