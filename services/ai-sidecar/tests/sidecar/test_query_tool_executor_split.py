"""Regression tests for query_tool_executor split."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent.parent))


def test_all_files_under_800_lines():
    """All .py files in query_tool_executor must be < 800 lines."""
    base = (
        Path(__file__).resolve().parent.parent.parent.parent
        / "services"
        / "ai-sidecar"
        / "src"
        / "infrastructure"
        / "ai"
        / "tools"
        / "query_tool_executor"
    )
    for f in base.glob("*.py"):
        if f.name.startswith("_") or f.name == "__init__.py":
            lines = f.read_text(encoding="utf-8").count("\n") + 1
            assert lines < 800, f"{f.name} has {lines} lines (must be < 800)"


def test_import_query_tool_executor():
    """QueryToolExecutor must be importable from the package."""
    from src.infrastructure.ai.tools.query_tool_executor import QueryToolExecutor

    assert QueryToolExecutor is not None


def test_mixin_classes_exist():
    """All mixin classes must be importable."""
    from src.infrastructure.ai.tools.query_tool_executor._builders import _BuildersMixin
    from src.infrastructure.ai.tools.query_tool_executor._core import _CoreMixin
    from src.infrastructure.ai.tools.query_tool_executor._filters import _FiltersMixin
    from src.infrastructure.ai.tools.query_tool_executor._handlers_flights import _HandlersFlightsMixin
    from src.infrastructure.ai.tools.query_tool_executor._handlers_insights import _HandlersInsightsMixin
    from src.infrastructure.ai.tools.query_tool_executor._handlers_timeseries import _HandlersTimeseriesMixin

    assert _CoreMixin is not None
    assert _FiltersMixin is not None
    assert _BuildersMixin is not None
    assert _HandlersFlightsMixin is not None
    assert _HandlersInsightsMixin is not None
    assert _HandlersTimeseriesMixin is not None


def test_executor_inherits_all_mixins():
    """QueryToolExecutor must inherit from all mixins."""
    from src.infrastructure.ai.tools.query_tool_executor import QueryToolExecutor
    from src.infrastructure.ai.tools.query_tool_executor._builders import _BuildersMixin
    from src.infrastructure.ai.tools.query_tool_executor._core import _CoreMixin
    from src.infrastructure.ai.tools.query_tool_executor._filters import _FiltersMixin
    from src.infrastructure.ai.tools.query_tool_executor._handlers_flights import _HandlersFlightsMixin
    from src.infrastructure.ai.tools.query_tool_executor._handlers_insights import _HandlersInsightsMixin
    from src.infrastructure.ai.tools.query_tool_executor._handlers_timeseries import _HandlersTimeseriesMixin

    assert issubclass(QueryToolExecutor, _CoreMixin)
    assert issubclass(QueryToolExecutor, _FiltersMixin)
    assert issubclass(QueryToolExecutor, _BuildersMixin)
    assert issubclass(QueryToolExecutor, _HandlersFlightsMixin)
    assert issubclass(QueryToolExecutor, _HandlersInsightsMixin)
    assert issubclass(QueryToolExecutor, _HandlersTimeseriesMixin)


def test_public_api_methods_exist():
    """All public API methods must exist on QueryToolExecutor."""
    from src.infrastructure.ai.tools.query_tool_executor import QueryToolExecutor

    public_methods = [
        "get_category",
        "_register_handlers",
    ]
    for method_name in public_methods:
        assert hasattr(QueryToolExecutor, method_name), f"Missing method: {method_name}"
