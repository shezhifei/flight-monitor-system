"""TODO Agent executor — thin facade composing all mixins."""

from __future__ import annotations

from src.domain.ports.agent_runtime_port import AgentRuntimePort
from src.infrastructure.ai.todo_agent_executor._agent_loop import _AgentLoopMixin
from src.infrastructure.ai.todo_agent_executor._executor_core import _ExecutorCoreMixin
from src.infrastructure.ai.todo_agent_executor._graph_loop import _GraphLoopMixin
from src.infrastructure.ai.todo_agent_executor._policy import _PolicyMixin
from src.infrastructure.ai.todo_agent_executor._single_todo import _SingleTodoMixin


class TodoAgentExecutor(
    _PolicyMixin,
    _GraphLoopMixin,
    _SingleTodoMixin,
    _AgentLoopMixin,
    _ExecutorCoreMixin,
    AgentRuntimePort,
):
    """TODO Agent executor — delegates to mixins."""

    MAX_EXECUTION_ITERATIONS = 10
    MAX_EXECUTION_TOKENS = 10000
    MAX_EXECUTION_TOOL_CALLS = 20
    MAX_EXECUTION_SECONDS = 300.0
    MIN_AI_CALL_TIMEOUT_SECONDS = 5.0
    MIN_TOOL_CALL_TIMEOUT_SECONDS = 1.0
    TOOL_EVENT_ARGUMENT_MAX_CHARS = 4000
    TOOL_EVENT_RESULT_MAX_CHARS = 6000
