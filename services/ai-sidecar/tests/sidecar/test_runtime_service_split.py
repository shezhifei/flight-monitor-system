"""Regression tests for runtime_service split (TD-28)."""
from __future__ import annotations

import inspect
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))


def _runtime_service_dir() -> Path:
    return Path(__file__).resolve().parents[2] / "src" / "infrastructure" / "ai" / "runtime_service"


def test_all_unit_files_under_800_lines():
    """All split unit .py files under runtime_service must be < 800 lines."""
    base = _runtime_service_dir()
    targets = [
        "service.py",
        "_constants.py",
        "_streaming_tools.py",
        "_streaming.py",
        "_resolve.py",
        "_context_cache.py",
    ]
    for name in targets:
        f = base / name
        assert f.is_file(), f"missing {name}"
        lines = f.read_text(encoding="utf-8").count("\n") + 1
        assert lines < 800, f"{name} has {lines} lines (must be < 800)"


def test_package_reexports():
    """Public imports from runtime_service package must remain available."""
    from src.infrastructure.ai.runtime_service import (
        CONTRACT_VERSION,
        STATUS_FAILED,
        STATUS_SUCCEEDED,
        RuntimeService,
        get_runtime_service,
        structured_output_to_response_dict,
    )

    assert CONTRACT_VERSION == "ai-structured-output.v1"
    assert STATUS_SUCCEEDED == "succeeded"
    assert STATUS_FAILED == "failed"
    assert RuntimeService is not None
    assert callable(get_runtime_service)
    assert callable(structured_output_to_response_dict)


def test_runtime_service_mixin_inheritance():
    """RuntimeService must compose streaming/resolve/context mixins."""
    from src.infrastructure.ai.runtime_service import RuntimeService
    from src.infrastructure.ai.runtime_service._context_cache import _ContextCacheMixin
    from src.infrastructure.ai.runtime_service._resolve import _ResolveMixin
    from src.infrastructure.ai.runtime_service._streaming import _StreamingMixin
    from src.infrastructure.ai.runtime_service._streaming_tools import _StreamingToolsMixin

    assert issubclass(RuntimeService, _StreamingToolsMixin)
    assert issubclass(RuntimeService, _StreamingMixin)
    assert issubclass(RuntimeService, _ResolveMixin)
    assert issubclass(RuntimeService, _ContextCacheMixin)


def test_helpers_still_on_service_module_for_monkeypatch():
    """Tests patch _iter_envelope_attachments on runtime_service.service; symbol must exist."""
    import src.infrastructure.ai.runtime_service.service as service_mod

    assert hasattr(service_mod, "_iter_envelope_attachments")


def test_public_methods_on_runtime_service():
    """Key entrypoints must exist on RuntimeService after split."""
    from src.infrastructure.ai.runtime_service import RuntimeService

    for name in (
        "execute_run",
        "stream_run",
        "stream_run_with_tools",
        "_validate_attachments",
        "_prepare_capabilities",
    ):
        assert hasattr(RuntimeService, name), f"missing {name}"
        assert callable(getattr(RuntimeService, name))


def test_runtime_service_source_under_package():
    from src.infrastructure.ai.runtime_service import RuntimeService

    source_file = inspect.getsourcefile(RuntimeService)
    assert source_file is not None
    assert "runtime_service" in source_file.replace("\\", "/")