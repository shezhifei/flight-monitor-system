"""Verify tool_executor does not contain 'For now' hardcoded defaults."""
from pathlib import Path

TOOL_EXECUTOR_PATH = (
    Path(__file__).resolve().parents[2]
    / "src/infrastructure/ai/tools/tool_executor.py"
)


def test_no_for_now_hardcoded_defaults():
    source = TOOL_EXECUTOR_PATH.read_text(encoding="utf-8")
    assert "For now" not in source, "Remove 'For now' hardcoded defaults in tool_executor.py"
