"""Tests for DI container type annotations — Task 14.

Verifies that sensitive service fields are NOT annotated with `Any | None`,
which disables type checking.  Instead they must use concrete types or
Protocols imported under ``TYPE_CHECKING``.

This is a static-analysis test: Python does not evaluate local-variable
annotations at runtime, so we parse the source to assert the annotations.
"""

from __future__ import annotations

import re
from pathlib import Path

import pytest


def _container_source() -> str:
    path = Path(__file__).resolve().parents[2] / "src" / "di" / "container.py"
    return path.read_text(encoding="utf-8")


# Fields that have known implementation classes / Protocols.
# Each entry: (field_name, expected_type_name)
_TYPED_FIELDS: list[tuple[str, str]] = [
    # AI-related (most sensitive per plan P8)
    ("ai_config_store", "AIConfigStoreInterface"),
    ("ai_context_manager", "ContextManager"),
    ("ai_conversation_manager", "ConversationManager"),
    ("ai_pending_action_store", "PostgresPendingActionStore"),
    ("todo_graph_pilot_snapshot_service", "TodoGraphPilotSnapshotService"),
    ("todo_graph_pilot_ops_service", "TodoGraphPilotOpsService"),
    # Anomaly ports (Protocols already exist)
    ("anomaly_flight_read_port", "AnomalyFlightReadPort"),
    ("anomaly_todo_write_port", "AnomalyTodoWritePort"),
    ("anomaly_notify_port", "AnomalyNotifyPort"),
    # Services with concrete classes
    ("dispatch_conflict_service", "DispatchConflictService"),
    ("anomaly_query_service", "AnomalyQueryService"),
    # tool_registry had no type at all
    ("tool_registry", "ToolRegistry"),
]


class TestNoAnyNoneOnSensitiveFields:
    """Sensitive DI fields must not use ``Any | None``."""

    @pytest.mark.parametrize("field_name,expected_type", _TYPED_FIELDS)
    def test_field_not_annotated_any_none(self, field_name: str, expected_type: str):
        source = _container_source()
        # Match: self.<field_name>: <annotation> = None
        pattern = rf"self\.{re.escape(field_name)}\s*:\s*([^\n=]+?)\s*="
        match = re.search(pattern, source)
        assert match is not None, (
            f"Field '{field_name}' not found in container.py — "
            "was it removed or renamed?"
        )
        annotation = match.group(1).strip()
        assert "Any" not in annotation, (
            f"Field '{field_name}' is annotated with '{annotation}' which contains 'Any'. "
            f"Expected '{expected_type} | None'."
        )

    @pytest.mark.parametrize("field_name,expected_type", _TYPED_FIELDS)
    def test_field_uses_expected_type(self, field_name: str, expected_type: str):
        source = _container_source()
        pattern = rf"self\.{re.escape(field_name)}\s*:\s*([^\n=]+?)\s*="
        match = re.search(pattern, source)
        assert match is not None, f"Field '{field_name}' not found in container.py"
        annotation = match.group(1).strip()
        assert expected_type in annotation, (
            f"Field '{field_name}' is annotated with '{annotation}' but expected "
            f"type '{expected_type}' is missing."
        )


class TestTypeCheckingImports:
    """TYPE_CHECKING block must import the types used by sensitive fields."""

    @pytest.mark.parametrize("field_name,expected_type", _TYPED_FIELDS)
    def test_type_imported_under_type_checking(self, field_name: str, expected_type: str):
        source = _container_source()
        # Extract the TYPE_CHECKING block — includes indented lines and blank lines
        # between import groups, stops at the next unindented statement.
        tc_match = re.search(
            r"if TYPE_CHECKING:\s*\n((?:(?:[ \t]+[^\n]*)?\n)+?)(?=\S)",
            source,
        )
        assert tc_match is not None, "No TYPE_CHECKING block found in container.py"
        tc_block = tc_match.group(1)
        assert expected_type in tc_block, (
            f"Type '{expected_type}' (used by field '{field_name}') is not imported "
            "in the TYPE_CHECKING block."
        )


class TestNoUntypedFields:
    """Fields that previously had no type annotation must now be typed."""

    def test_sse_hub_has_type_annotation(self):
        source = _container_source()
        # Before: self.sse_hub = None  (no annotation)
        # After:  self.sse_hub: SomeType | None = None
        pattern = r"self\.sse_hub\s*:"
        assert re.search(pattern, source), (
            "Field 'sse_hub' has no type annotation. "
            "It should be typed (e.g. 'SSEHub | None')."
        )

    def test_sse_hub_not_typed_any(self):
        source = _container_source()
        pattern = r"self\.sse_hub\s*:\s*([^\n=]+?)\s*="
        match = re.search(pattern, source)
        if match:
            annotation = match.group(1).strip()
            assert "Any" not in annotation, (
                f"Field 'sse_hub' is annotated with '{annotation}' which contains 'Any'."
            )
