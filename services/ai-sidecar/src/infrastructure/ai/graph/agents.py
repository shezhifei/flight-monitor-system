"""
场景化 Agent 模板构建器 (Graph Templates)。

优先使用真实 LangGraph；在依赖尚未安装时，提供最小内置编排器，
用于 Todo Agent 试点和单元测试中的 suspend/resume 验证。
"""

from __future__ import annotations

import logging
from copy import deepcopy
from typing import Any
from uuid import uuid4

from src.infrastructure.ai.graph.builder import build_workflow_graph, route_after_act, route_after_tools
from src.infrastructure.ai.graph.checkpointer import AgentExecutionCheckpointer
from src.infrastructure.ai.graph.nodes import (
    act_node,
    approval_node,
    graceful_abort_node,
    observe_node,
    summarize_node,
    tool_exec_node,
)
from src.infrastructure.ai.graph.state import AgentCoreState

try:
    from langgraph.graph import END, StateGraph
except ImportError:  # pragma: no cover - fallback exercised in tests
    END = "__end__"
    StateGraph = Any  # type: ignore[assignment]  # fallback when langgraph is absent

logger = logging.getLogger(__name__)


def _merge_state(current_state: dict[str, Any], updates: dict[str, Any]) -> dict[str, Any]:
    merged = dict(current_state)
    for key, value in (updates or {}).items():
        if key in {"messages", "blackboard_facts"}:
            existing = list(merged.get(key) or [])
            existing.extend(list(value or []))
            merged[key] = existing
            continue
        merged[key] = value
    return merged


class _FallbackCompiledWorkflow:
    def __init__(self, checkpointer: AgentExecutionCheckpointer | None = None):
        self._checkpointer = checkpointer

    async def ainvoke(self, input_state: Any, config: dict[str, Any] | None = None) -> dict[str, Any]:
        base_config = dict(config or {})
        config = {
            **base_config,
            "callbacks": list(base_config.get("callbacks") or []),
        }
        configurable = dict(base_config.get("configurable") or {})
        config["configurable"] = configurable

        resume_payload = configurable.get("resume")
        if hasattr(input_state, "resume"):
            resume_payload = getattr(input_state, "resume", None)
            input_state = None

        checkpoint_state: dict[str, Any] | None = None
        next_node = "observe"
        if input_state is None and self._checkpointer is not None:
            checkpoint_tuple = await self._checkpointer.aget_tuple(config)
            if checkpoint_tuple is not None:
                checkpoint_state = deepcopy((checkpoint_tuple.checkpoint or {}).get("state") or {})
                next_node = str((checkpoint_tuple.metadata or {}).get("next_node") or "observe")

        state = deepcopy(checkpoint_state or (input_state or {}))
        configurable["resume"] = resume_payload

        while True:
            if next_node == "observe":
                state = _merge_state(state, await observe_node(state, config))
                next_node = "act"
                continue

            if next_node == "act":
                state = _merge_state(state, await act_node(state, config))
                next_node = route_after_act(state)
                continue

            if next_node == "tools":
                state = _merge_state(state, await tool_exec_node(state, config))
                next_node = route_after_tools(state)
                continue

            if next_node == "wait_for_approval":
                if configurable.get("resume") is None:
                    await self._save_checkpoint(config, state, next_node="wait_for_approval")
                    return state
                state = _merge_state(state, await approval_node(state, config))
                configurable.pop("resume", None)
                next_node = "act"
                continue

            if next_node == "summarize":
                state = _merge_state(state, await summarize_node(state, config))
                await self._save_checkpoint(config, state, next_node="summarize")
                return state

            if next_node == "graceful_abort":
                state = _merge_state(state, await graceful_abort_node(state, config))
                await self._save_checkpoint(config, state, next_node=str(END))
                return state

            if next_node == END:
                await self._save_checkpoint(config, state, next_node=str(END))
                return state

            raise RuntimeError(f"Unsupported fallback graph node: {next_node}")

    async def _save_checkpoint(
        self,
        config: dict[str, Any],
        state: dict[str, Any],
        *,
        next_node: str,
    ) -> None:
        if self._checkpointer is None:
            return
        checkpoint = {
            "id": f"ckpt_{uuid4().hex}",
            "state": deepcopy(state),
        }
        metadata = {"next_node": next_node}
        await self._checkpointer.aput(config, checkpoint, metadata, {})


class _FallbackWorkflowGraph:
    def compile(
        self,
        *,
        checkpointer: AgentExecutionCheckpointer | None = None,
        **kwargs: Any,
    ) -> _FallbackCompiledWorkflow:
        return _FallbackCompiledWorkflow(checkpointer=checkpointer)


def create_query_agent() -> StateGraph:
    if StateGraph is Any:  # pragma: no cover - query agent is not used in current tests
        return _FallbackWorkflowGraph()

    workflow = StateGraph(AgentCoreState)

    workflow.add_node("act", act_node)
    workflow.add_node("tools", tool_exec_node)

    workflow.set_entry_point("act")

    workflow.add_conditional_edges(
        "act",
        route_after_act,
        {
            "tools": "tools",
            "summarize": END,
        },
    )

    def _route_after_query_tools(state: AgentCoreState) -> str:
        if state.get("requires_approval", False):
            return "end"
        return "act"

    workflow.add_conditional_edges(
        "tools",
        _route_after_query_tools,
        {
            "act": "act",
            "end": END,
        },
    )

    return workflow


def create_workflow_agent() -> StateGraph:
    try:
        return build_workflow_graph()
    except Exception as exc:  # noqa: BLE001 - workflow build fallback must catch all
        logger.warning("building workflow graph failed; using fallback orchestrator", exc_info=exc)
        return _FallbackWorkflowGraph()
