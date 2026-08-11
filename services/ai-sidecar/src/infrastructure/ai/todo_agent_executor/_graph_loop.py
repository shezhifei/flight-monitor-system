"""TODO Agent executor — main class."""

from __future__ import annotations

import asyncio
import logging
import time
from typing import Any

from src.domain.ai.agent_execution import (
    AgentExecution,
    AgentExecutionStatus,
)
from src.domain.ai.todo_graph_pilot import (
    is_default_todo_graph_pilot_entity,
)
from src.domain.ports.agent_runtime_port import (
    AgentExecutionResultDTO,
)
from src.infrastructure.ai.feature_flags import is_ai_feature_enabled
from src.infrastructure.ai.todo_agent_executor.models import AgentLoopContext
from src.infrastructure.ai.tools.base import InvocationMode

logger = logging.getLogger(__name__)


class _GraphLoopMixin:
    """Mixin for TodoAgentExecutor."""

    async def _is_execution_cancelled(self, run_id: str) -> bool:
        """Check in-flight or persisted cancellation flag."""
        cancel_event = await self._get_cancel_event(run_id)
        return bool(cancel_event and cancel_event.is_set())

    async def resume_execution(self, run_id: str, action: str, **kwargs) -> AgentExecutionResultDTO:
        """从挂起状态恢复执行 (LangGraph HITL)"""
        start_time = time.time()
        resume_lock = await self._get_graph_resume_lock(run_id)
        async with resume_lock:
            execution = await self._execution_repo.get_execution(run_id)
            if not execution:
                raise ValueError(f"Execution {run_id} not found")

            if execution.status == AgentExecutionStatus.COMPLETED:
                guardrails = self._normalize_graph_guardrail_state(
                    (execution.metadata or {}).get("graph_runtime_guardrails")
                )
                if guardrails["executed_tool_call_ids"]:
                    guardrails["duplicate_tool_execution_blocked_total"] += 1
                    self._append_graph_guardrail_event(
                        guardrails,
                        tool_call_id=guardrails["executed_tool_call_ids"][-1],
                        tool_name=None,
                        status="blocked",
                        reason="execution_already_completed",
                    )
                    await self._persist_graph_guardrails(execution, guardrails)
                    return self._build_resume_guardrail_response(
                        execution,
                        reason="execution_already_completed",
                    )

            pending_tool_call = self._extract_graph_pending_tool_call(execution)
            pending_tool_call_id = str((pending_tool_call or {}).get("id") or "").strip()
            pending_tool_name = str((pending_tool_call or {}).get("name") or "").strip() or None

            if pending_tool_call_id:
                reservation = await self._reserve_graph_resume_tool_call(
                    execution,
                    tool_call_id=pending_tool_call_id,
                    tool_name=pending_tool_name,
                )
                if reservation.get("blocked"):
                    latest_execution = await self._execution_repo.get_execution(run_id) or execution
                    return self._build_resume_guardrail_response(
                        latest_execution,
                        reason=str(reservation.get("reason") or "duplicate_resume_blocked"),
                    )

            execution.status = AgentExecutionStatus.RUNNING
            await self._execution_repo.update_execution(execution)

            from src.infrastructure.ai.graph.agents import create_workflow_agent
            from src.infrastructure.ai.graph.callbacks import SSEStreamingCallbackHandler
            from src.infrastructure.ai.graph.checkpointer import AgentExecutionCheckpointer

            try:
                from langgraph.types import Command

                command_cls = Command
            except ImportError:
                command_cls = None

            runtime_metadata = (execution.metadata or {}).get("graph_runtime") or {}
            invocation_mode = self._resolve_invocation_mode(runtime_metadata.get("invocation_mode"))
            user_id = runtime_metadata.get("user_id")
            user_roles = runtime_metadata.get("user_roles")
            tool_names = [
                str(tool_name).strip()
                for tool_name in runtime_metadata.get("tool_names") or []
                if str(tool_name).strip()
            ]

            checkpointer = AgentExecutionCheckpointer(self._execution_repo)
            app = create_workflow_agent().compile(checkpointer=checkpointer)

            callbacks = []
            if self._notification_port:
                callbacks.append(
                    SSEStreamingCallbackHandler(
                        self._notification_port,
                        execution.run_id,
                        execution.todo_id,
                    )
                )

            config = await self._prepare_graph_runtime(
                execution=execution,
                entity_config=execution.entity_config or {},
                tool_names=tool_names,
                user_id=user_id,
                user_roles=user_roles if isinstance(user_roles, list) else None,
                invocation_mode=invocation_mode,
                callbacks=callbacks,
            )

            resume_payload = {"status": action, **kwargs}
            try:
                if command_cls:
                    resume_cmd = command_cls(resume=resume_payload)
                    result_state = await app.ainvoke(resume_cmd, config=config)
                else:
                    config.setdefault("configurable", {})["resume"] = resume_payload
                    result_state = await app.ainvoke(None, config=config)
                await self._update_graph_guardrails_from_state(execution, result_state)
            except Exception as exc:
                logger.debug("graph_resume_guardrail_update_failed", exc_info=exc)
                if pending_tool_call_id:
                    await self._release_graph_resume_tool_call(
                        execution,
                        tool_call_id=pending_tool_call_id,
                        tool_name=pending_tool_name,
                        reason="resume_failed",
                    )
                raise

            if pending_tool_call_id:
                if str(action or "").strip().lower() == "approved":
                    await self._complete_graph_resume_tool_call(
                        execution,
                        tool_call_id=pending_tool_call_id,
                        tool_name=pending_tool_name,
                    )
                else:
                    await self._release_graph_resume_tool_call(
                        execution,
                        tool_call_id=pending_tool_call_id,
                        tool_name=pending_tool_name,
                        reason=f"resume_{str(action or 'unknown').strip().lower() or 'unknown'}",
                    )

            # 检查是否再次挂起
            if result_state.get("requires_approval", False):
                execution.status = AgentExecutionStatus.PENDING
                execution.metadata = {
                    **(execution.metadata or {}),
                    "runtime_status": self._resolve_pending_approval_status().value,
                }
                await self._persist_graph_guardrails(
                    execution,
                    (execution.metadata or {}).get("graph_runtime_guardrails") or {},
                )
                await self._emit_graph_approval_required(
                    config=config,
                    run_id=execution.run_id,
                    todo_id=execution.todo_id,
                    result_state=result_state,
                    message="再次等待人工审批中...",
                )
                return AgentExecutionResultDTO(
                    run_id=execution.run_id,
                    todo_id=execution.todo_id,
                    status=self._resolve_pending_approval_status(),
                    response="再次等待人工审批中...",
                    total_steps=execution.total_steps,
                    total_tokens=execution.total_tokens,
                    total_tool_calls=execution.total_tool_calls,
                    duration_ms=int((time.time() - start_time) * 1000),
                    child_todos=[],
                )

            execution.complete()
            response_text = self._extract_graph_response_text(result_state)
            execution.metadata = {
                **(execution.metadata or {}),
                "runtime_status": AgentExecutionStatus.COMPLETED.value,
            }
            await self._persist_graph_guardrails(
                execution,
                (execution.metadata or {}).get("graph_runtime_guardrails") or {},
                last_response=response_text,
            )

            if getattr(self, "_todo_updater", None):
                await self._todo_updater(execution.todo_id, "completed", execution.run_id)

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
                    "meta": {"duration_ms": duration_ms, "runtime": "graph"},
                },
            )

            return AgentExecutionResultDTO(
                run_id=execution.run_id,
                todo_id=execution.todo_id,
                status=AgentExecutionStatus.COMPLETED,
                response=response_text,
                total_steps=execution.total_steps,
                total_tokens=execution.total_tokens,
                total_tool_calls=execution.total_tool_calls,
                duration_ms=duration_ms,
                child_todos=[],
            )

    async def _register_cancel_event(self, run_id: str) -> None:
        async with self._cancel_events_lock:
            self._cancel_events[run_id] = asyncio.Event()

    async def _get_cancel_event(self, run_id: str) -> asyncio.Event | None:
        async with self._cancel_events_lock:
            return self._cancel_events.get(run_id)

    async def _remove_cancel_event(self, run_id: str) -> None:
        async with self._cancel_events_lock:
            self._cancel_events.pop(run_id, None)

    async def _get_graph_resume_lock(self, run_id: str) -> asyncio.Lock:
        async with self._graph_resume_locks_lock:
            lock = self._graph_resume_locks.get(run_id)
            if lock is None:
                lock = asyncio.Lock()
                self._graph_resume_locks[run_id] = lock
            return lock

    def _should_continue(self, context: AgentLoopContext) -> bool:
        return (
            context.iterations < self.MAX_EXECUTION_ITERATIONS
            and context.total_tokens < self.MAX_EXECUTION_TOKENS
            and context.total_tool_calls < self.MAX_EXECUTION_TOOL_CALLS
            and context.elapsed_time < self.MAX_EXECUTION_SECONDS
        )

    def _extract_graph_callbacks(self: dict[str, Any] | None) -> list[Any]:
        if not isinstance(self, dict):
            return []
        callbacks = self.get("callbacks")
        if not isinstance(callbacks, list):
            return []
        return callbacks

    async def _emit_graph_approval_required(
        self,
        *,
        config: dict[str, Any] | None,
        run_id: str,
        todo_id: str,
        result_state: dict[str, Any] | None,
        message: str,
    ) -> None:
        pending_action_id = None
        pending_tool_call = None
        if isinstance(result_state, dict):
            pending_action_id = result_state.get("pending_action_id")
            pending_tool_call = result_state.get("pending_tool_call")

        for callback in self._extract_graph_callbacks(config):
            emit_approval_required = getattr(callback, "emit_approval_required", None)
            if callable(emit_approval_required):
                await emit_approval_required(
                    pending_action_id=pending_action_id,
                    pending_tool_call=pending_tool_call if isinstance(pending_tool_call, dict) else None,
                    message=message,
                )
                return

        payload: dict[str, Any] = {
            "event": "approval_required",
            "phase": "approval",
            "status": "pending_approval",
            "message": message,
        }
        if isinstance(pending_tool_call, dict):
            tool_name = str(pending_tool_call.get("name") or "").strip()
            if tool_name:
                payload["tool_name"] = tool_name
                payload["tool"] = {"name": tool_name}
            payload["tool_call_id"] = str(pending_tool_call.get("id") or "")
            payload["tool_arguments"] = pending_tool_call.get("args")
            payload["tool_arguments_truncated"] = False
        if pending_action_id is not None:
            payload["meta"] = {"pending_action_id": pending_action_id}
        await self._emit_ai_event("approval_required", run_id, todo_id, payload)

    async def _persist_graph_guardrails(
        self,
        execution: AgentExecution,
        guardrails: dict[str, Any],
        *,
        last_response: str | None = None,
    ) -> None:
        metadata = dict(execution.metadata or {})
        metadata["graph_runtime_guardrails"] = self._normalize_graph_guardrail_state(guardrails)
        if last_response is not None:
            metadata["graph_runtime_last_response"] = self._normalize_graph_response_text(last_response)
        execution.metadata = metadata
        await self._execution_repo.update_execution(execution)

    async def _update_graph_guardrails_from_state(
        self, execution: AgentExecution, result_state: dict[str, Any]
    ) -> None:
        consecutive = result_state.get("consecutive_tool_failures", 0)
        max_streak = result_state.get("max_consecutive_tool_failures", 0)
        last_error = result_state.get("last_tool_error_code", "")

        from src.infrastructure.ai.graph.constants import MAX_GRAPH_TOOL_RETRIES

        is_aborted = 1 if consecutive >= MAX_GRAPH_TOOL_RETRIES else 0

        guardrails = self._normalize_graph_guardrail_state((execution.metadata or {}).get("graph_runtime_guardrails"))
        guardrails["graph_local_abort_total"] = is_aborted
        guardrails["graph_tool_failure_streak_max"] = max_streak
        guardrails["last_graph_abort_reason"] = last_error if is_aborted else ""

        await self._persist_graph_guardrails(execution, guardrails)

    async def _reserve_graph_resume_tool_call(
        self,
        execution: AgentExecution,
        *,
        tool_call_id: str,
        tool_name: str | None,
    ) -> dict[str, Any]:
        normalized_tool_call_id = str(tool_call_id or "").strip()
        if not normalized_tool_call_id:
            return {"blocked": False, "reason": None}

        metadata = dict(execution.metadata or {})
        guardrails = self._normalize_graph_guardrail_state(metadata.get("graph_runtime_guardrails"))
        executed = set(guardrails["executed_tool_call_ids"])
        inflight = set(guardrails["inflight_tool_call_ids"])

        if normalized_tool_call_id in executed or normalized_tool_call_id in inflight:
            guardrails["duplicate_tool_execution_blocked_total"] += 1
            reason = "already_executed" if normalized_tool_call_id in executed else "already_inflight"
            self._append_graph_guardrail_event(
                guardrails,
                tool_call_id=normalized_tool_call_id,
                tool_name=tool_name,
                status="blocked",
                reason=reason,
            )
            await self._persist_graph_guardrails(execution, guardrails)
            return {"blocked": True, "reason": reason}

        inflight.add(normalized_tool_call_id)
        guardrails["inflight_tool_call_ids"] = sorted(inflight)
        self._append_graph_guardrail_event(
            guardrails,
            tool_call_id=normalized_tool_call_id,
            tool_name=tool_name,
            status="reserved",
        )
        await self._persist_graph_guardrails(execution, guardrails)
        return {"blocked": False, "reason": None}

    async def _release_graph_resume_tool_call(
        self,
        execution: AgentExecution,
        *,
        tool_call_id: str,
        tool_name: str | None,
        reason: str,
    ) -> None:
        normalized_tool_call_id = str(tool_call_id or "").strip()
        if not normalized_tool_call_id:
            return

        metadata = dict(execution.metadata or {})
        guardrails = self._normalize_graph_guardrail_state(metadata.get("graph_runtime_guardrails"))
        inflight = set(guardrails["inflight_tool_call_ids"])
        if normalized_tool_call_id in inflight:
            inflight.remove(normalized_tool_call_id)
            guardrails["inflight_tool_call_ids"] = sorted(inflight)
        self._append_graph_guardrail_event(
            guardrails,
            tool_call_id=normalized_tool_call_id,
            tool_name=tool_name,
            status="released",
            reason=reason,
        )
        await self._persist_graph_guardrails(execution, guardrails)

    async def _complete_graph_resume_tool_call(
        self,
        execution: AgentExecution,
        *,
        tool_call_id: str,
        tool_name: str | None,
    ) -> None:
        normalized_tool_call_id = str(tool_call_id or "").strip()
        if not normalized_tool_call_id:
            return

        metadata = dict(execution.metadata or {})
        guardrails = self._normalize_graph_guardrail_state(metadata.get("graph_runtime_guardrails"))
        executed = set(guardrails["executed_tool_call_ids"])
        inflight = set(guardrails["inflight_tool_call_ids"])

        if normalized_tool_call_id in executed:
            guardrails["duplicate_tool_execution_total"] += 1
            self._append_graph_guardrail_event(
                guardrails,
                tool_call_id=normalized_tool_call_id,
                tool_name=tool_name,
                status="duplicate_executed",
                reason="completed_twice",
            )
        else:
            executed.add(normalized_tool_call_id)
            self._append_graph_guardrail_event(
                guardrails,
                tool_call_id=normalized_tool_call_id,
                tool_name=tool_name,
                status="completed",
            )

        if normalized_tool_call_id in inflight:
            inflight.remove(normalized_tool_call_id)

        guardrails["executed_tool_call_ids"] = sorted(executed)
        guardrails["inflight_tool_call_ids"] = sorted(inflight)
        await self._persist_graph_guardrails(execution, guardrails)

    def _is_graph_runtime_enabled_for_entity(self, entity_id: str, entity_config: dict[str, Any]) -> bool:
        entity_override = self._resolve_entity_graph_runtime_override(entity_config)

        if not self._feature_overrides:
            # Keep direct executor construction backward-compatible for tests and ad-hoc usage.
            if entity_override is not None:
                return entity_override
            return True

        if not is_default_todo_graph_pilot_entity(entity_id):
            return False

        if entity_override is not None:
            return entity_override

        return is_ai_feature_enabled(
            "AI_TODO_AGENT_GRAPH_V1",
            overrides=self._feature_overrides,
        )

    async def _set_execution_runtime_metadata(
        self,
        execution: AgentExecution,
        *,
        runtime_path: str | None = None,
        runtime_status: str | None = None,
        runtime_path_requested: str | None = None,
        runtime_fallback_reason: str | None = None,
    ) -> None:
        metadata = dict(execution.metadata or {})
        if runtime_path is not None:
            metadata["runtime_path"] = runtime_path
        if runtime_status is not None:
            metadata["runtime_status"] = runtime_status
        if runtime_path_requested is not None:
            metadata["runtime_path_requested"] = runtime_path_requested
        if runtime_fallback_reason is not None:
            metadata["runtime_fallback_reason"] = runtime_fallback_reason
        execution.metadata = metadata
        await self._execution_repo.update_execution(execution)

    async def _mark_execution_runtime_path(
        self,
        execution: AgentExecution,
        runtime_path: str,
        *,
        requested_path: str | None = None,
        fallback_reason: str | None = None,
    ) -> None:
        await self._set_execution_runtime_metadata(
            execution,
            runtime_path=runtime_path,
            runtime_path_requested=requested_path,
            runtime_fallback_reason=fallback_reason,
        )

    async def _build_graph_langchain_tools(
        self,
        *,
        tool_names: list[str],
        user_id: str | None,
        user_roles: list[str] | None,
        invocation_mode: InvocationMode,
        parent_todo_id: str | None = None,
        parent_entity_id: str | None = None,
        child_todos: list[str] | None = None,
    ) -> list[Any]:
        from src.infrastructure.ai.langchain_adapter.tools import ToolAdapterFactory

        adapted_tools: list[Any] = []
        for tool_name in tool_names:
            if tool_name == "spawn_subtodo":
                adapted_tools.append(
                    self._create_graph_spawn_subtodo_tool(
                        parent_todo_id=parent_todo_id,
                        parent_entity_id=parent_entity_id,
                        child_todos=child_todos,
                    )
                )
                continue
            adapted_tools.append(
                ToolAdapterFactory.create_adapted_tool(
                    tool_name,
                    self._tool_registry,
                    invocation_mode=invocation_mode,
                    user_id=user_id,
                    user_roles=user_roles,
                )
            )
        return adapted_tools

    async def _prepare_graph_runtime(
        self,
        *,
        execution: AgentExecution,
        entity_config: dict[str, Any],
        tool_names: list[str],
        user_id: str | None,
        user_roles: list[str] | None,
        invocation_mode: InvocationMode,
        callbacks: list[Any] | None = None,
        task_description: str | None = None,
        child_todos: list[str] | None = None,
    ) -> dict[str, Any]:
        from langchain_openai import ChatOpenAI

        from src.infrastructure.ai.security.url_guard import validate_external_http_url

        langchain_tools = await self._build_graph_langchain_tools(
            tool_names=tool_names,
            user_id=user_id,
            user_roles=user_roles,
            invocation_mode=invocation_mode,
            parent_todo_id=execution.todo_id,
            parent_entity_id=execution.entity_id,
            child_todos=child_todos,
        )
        base_url = validate_external_http_url(
            entity_config.get("base_url", "https://api.openai.com/v1"),
            purpose="OpenAI base_url",
        )

        llm = ChatOpenAI(
            model=entity_config.get("default_model", "gpt-4"),
            openai_api_key=entity_config.get("api_key", ""),
            openai_api_base=base_url,
            temperature=entity_config.get("temperature", 0.7),
            max_tokens=entity_config.get("max_tokens", 1000),
        ).bind_tools(langchain_tools)

        graph_runtime = {
            "tool_names": list(tool_names or []),
            "user_id": user_id,
            "user_roles": list(user_roles or []),
            "invocation_mode": invocation_mode.value,
        }
        if task_description is not None:
            graph_runtime["task_description"] = task_description

        execution_metadata = execution.metadata or {}
        execution_metadata["graph_runtime"] = graph_runtime
        execution.metadata = execution_metadata
        await self._execution_repo.update_execution(execution)

        return {
            "configurable": {
                "thread_id": execution.run_id,
                "llm": llm,
                "tools": langchain_tools,
            },
            "callbacks": list(callbacks or []),
        }

    def _create_graph_spawn_subtodo_tool(
        self,
        *,
        parent_todo_id: str | None,
        parent_entity_id: str | None,
        child_todos: list[str] | None,
    ) -> Any:
        executor = self
        normalized_parent_todo_id = str(parent_todo_id or "").strip()
        normalized_parent_entity_id = str(parent_entity_id or "default").strip() or "default"
        child_todo_buffer = child_todos if child_todos is not None else []

        class _GraphSpawnSubtodoTool:
            name = "spawn_subtodo"
            description = "Create a sub-task (TODO) when the current task is too complex."

            async def ainvoke(self, args: Any, config: dict[str, Any] | None = None) -> Any:
                return await executor._handle_spawn_subtodo(
                    parent_todo_id=normalized_parent_todo_id,
                    parent_entity_id=normalized_parent_entity_id,
                    args=args,
                    child_todos=child_todo_buffer,
                )

        return _GraphSpawnSubtodoTool()
