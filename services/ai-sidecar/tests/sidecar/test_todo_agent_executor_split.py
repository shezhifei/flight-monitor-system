"""Regression tests for todo_agent_executor split."""
from __future__ import annotations

import inspect
import importlib
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent.parent))


def test_all_files_under_800_lines():
    """All .py files in todo_agent_executor must be < 800 lines."""
    base = Path(__file__).resolve().parent.parent.parent.parent / "services" / "ai-sidecar" / "src" / "infrastructure" / "ai" / "todo_agent_executor"
    for f in base.glob("*.py"):
        if f.name.startswith("_") or f.name == "__init__.py":
            lines = f.read_text(encoding="utf-8").count("\n") + 1
            assert lines < 800, f"{f.name} has {lines} lines (must be < 800)"


def test_import_todo_agent_executor():
    """TodoAgentExecutor must be importable from the package."""
    from src.infrastructure.ai.todo_agent_executor import TodoAgentExecutor
    assert TodoAgentExecutor is not None


def test_source_file_under_package():
    """TodoAgentExecutor source must be under todo_agent_executor/ package."""
    from src.infrastructure.ai.todo_agent_executor import TodoAgentExecutor
    source_file = inspect.getsourcefile(TodoAgentExecutor)
    assert source_file is not None
    assert "todo_agent_executor" in source_file


def test_mixin_classes_exist():
    """All mixin classes must be importable."""
    from src.infrastructure.ai.todo_agent_executor._agent_loop import _AgentLoopMixin
    from src.infrastructure.ai.todo_agent_executor._executor_core import _ExecutorCoreMixin
    from src.infrastructure.ai.todo_agent_executor._graph_loop import _GraphLoopMixin
    from src.infrastructure.ai.todo_agent_executor._policy import _PolicyMixin
    from src.infrastructure.ai.todo_agent_executor._single_todo import _SingleTodoMixin
    assert _AgentLoopMixin is not None
    assert _ExecutorCoreMixin is not None
    assert _GraphLoopMixin is not None
    assert _PolicyMixin is not None
    assert _SingleTodoMixin is not None


def test_executor_inherits_all_mixins():
    """TodoAgentExecutor must inherit from all mixins."""
    from src.infrastructure.ai.todo_agent_executor import TodoAgentExecutor
    from src.infrastructure.ai.todo_agent_executor._agent_loop import _AgentLoopMixin
    from src.infrastructure.ai.todo_agent_executor._executor_core import _ExecutorCoreMixin
    from src.infrastructure.ai.todo_agent_executor._graph_loop import _GraphLoopMixin
    from src.infrastructure.ai.todo_agent_executor._policy import _PolicyMixin
    from src.infrastructure.ai.todo_agent_executor._single_todo import _SingleTodoMixin
    assert issubclass(TodoAgentExecutor, _AgentLoopMixin)
    assert issubclass(TodoAgentExecutor, _ExecutorCoreMixin)
    assert issubclass(TodoAgentExecutor, _GraphLoopMixin)
    assert issubclass(TodoAgentExecutor, _PolicyMixin)
    assert issubclass(TodoAgentExecutor, _SingleTodoMixin)


def test_public_api_methods_exist():
    """All public API methods must exist on TodoAgentExecutor."""
    from src.infrastructure.ai.todo_agent_executor import TodoAgentExecutor
    public_methods = [
        "execute_todo",
        "execute_todo_tree",
        "get_execution",
        "cancel_execution",
        "resume_execution",
        "set_todo_callbacks",
    ]
    for method_name in public_methods:
        assert hasattr(TodoAgentExecutor, method_name), f"Missing method: {method_name}"


def test_class_constants_preserved():
    """Class-level constants must be preserved."""
    from src.infrastructure.ai.todo_agent_executor import TodoAgentExecutor
    assert TodoAgentExecutor.MAX_EXECUTION_ITERATIONS == 10
    assert TodoAgentExecutor.MAX_EXECUTION_TOKENS == 10000
    assert TodoAgentExecutor.MAX_EXECUTION_TOOL_CALLS == 20
    assert TodoAgentExecutor.MAX_EXECUTION_SECONDS == 300.0
