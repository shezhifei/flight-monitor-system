"""Task A4 — sanitize allowlist + feature-gate tests preserved from the legacy file.

``test_tool_execution.py`` was deleted (mock-era tools are gone); the still-valid
pure-function tests were moved here so SSE sanitization and the tool-streaming
feature gate keep their regression coverage.
"""

from __future__ import annotations

import json

# ---------------------------------------------------------------------------
# P2.5: SSE Allowlist Regression Tests
# ---------------------------------------------------------------------------

# Allowlisted keys for tool.call SSE events (metadata-only)
TOOL_CALL_ALLOWLIST = {"run_id", "tool_call_id", "tool_name", "tool_type"}

# Allowlisted keys for tool.result SSE events (metadata-only)
TOOL_RESULT_ALLOWLIST = {
    "run_id",
    "tool_call_id",
    "tool_name",
    "tool_type",
    "result_status",
    "proposal_count",
    "rejected_count",
}


def test_sanitize_tool_call_event_allowlist():
    """P2.5: tool.call SSE payload must only contain allowlisted keys."""
    from src.infrastructure.ai.runtime_service import _sanitize_tool_call_event

    raw_tool_call = {
        "tool_call_id": "tc_allow_001",
        "tool_name": "flight_status_lookup",
        "arguments": {"flight_id": "CA1234", "api_key": "sk-secret-key"},
        "result": {"flight_id": "CA1234", "status": "on_time"},
        "error": "some internal error",
        "secret": "Bearer token-12345",
    }
    sanitized = _sanitize_tool_call_event(raw_tool_call)
    extra_keys = set(sanitized.keys()) - TOOL_CALL_ALLOWLIST
    assert not extra_keys, f"tool.call SSE payload has disallowed keys: {extra_keys}"
    assert "arguments" not in sanitized
    assert "result" not in sanitized
    assert "error" not in sanitized
    assert "secret" not in sanitized


def test_sanitize_tool_result_event_allowlist():
    """P2.5: tool.result SSE payload must only contain allowlisted keys."""
    from src.infrastructure.ai.runtime_service import _sanitize_tool_result_event

    raw_payload = {
        "tool_call_id": "tc_allow_002",
        "tool_name": "add_flight_note",
        "arguments": {"flight_id": "CA1234", "note_content": "secret stuff"},
        "result": {"status": "proposal_created", "db_conn": "postgres://user:pass@host/db"},
        "error": "internal stack trace here",
        "api_key": "sk-12345678",
    }
    sanitized = _sanitize_tool_result_event(raw_payload)
    extra_keys = set(sanitized.keys()) - TOOL_RESULT_ALLOWLIST
    assert not extra_keys, f"tool.result SSE payload has disallowed keys: {extra_keys}"
    assert "arguments" not in sanitized
    assert "error" not in sanitized
    assert "api_key" not in sanitized


def test_sanitize_tool_call_no_nested_secrets():
    """P2.5: No nested secrets in serialized tool.call SSE text."""
    from src.infrastructure.ai.runtime_service import _sanitize_tool_call_event

    raw_tool_call = {
        "tool_call_id": "tc_nested_001",
        "tool_name": "flight_status_lookup",
        "arguments": {
            "nested": {"api_key": "sk-secret-key", "password": "hunter2"},
        },
        "result": {"authorization": "Bearer super-secret"},
    }
    sanitized = _sanitize_tool_call_event(raw_tool_call)
    serialized = json.dumps(sanitized)
    assert "sk-" not in serialized
    assert "hunter2" not in serialized
    assert "super-secret" not in serialized
    assert "Bearer" not in serialized


def test_sanitize_tool_result_no_nested_secrets():
    """P2.5: No nested secrets in serialized tool.result SSE text."""
    from src.infrastructure.ai.runtime_service import _sanitize_tool_result_event

    raw_payload = {
        "tool_call_id": "tc_nested_002",
        "tool_name": "add_flight_note",
        "result": {
            "data": {"jwt": "eyJ...", "password": "s3cret"},
            "status": "proposal_created",
        },
        "arguments": {"flight_id": "CA1234", "token": "abc123"},
    }
    sanitized = _sanitize_tool_result_event(raw_payload)
    serialized = json.dumps(sanitized)
    assert "eyJ" not in serialized
    assert "s3cret" not in serialized
    assert "abc123" not in serialized


def test_sanitize_tool_call_bounded_payload():
    """P2.5: tool.call SSE payload is bounded even with huge arguments."""
    from src.infrastructure.ai.runtime_service import _sanitize_tool_call_event

    raw_tool_call = {
        "tool_call_id": "tc_bounded_001",
        "tool_name": "flight_status_lookup",
        "arguments": {"huge_field": "x" * 100000},
    }
    sanitized = _sanitize_tool_call_event(raw_tool_call)
    serialized = json.dumps(sanitized)
    # The sanitized payload should be small (metadata only)
    assert len(serialized) < 500, f"tool.call SSE payload too large: {len(serialized)}"


def test_sanitize_tool_result_bounded_payload():
    """P2.5: tool.result SSE payload is bounded even with huge result."""
    from src.infrastructure.ai.runtime_service import _sanitize_tool_result_event

    raw_payload = {
        "tool_call_id": "tc_bounded_002",
        "tool_name": "add_flight_note",
        "result": {"data": "x" * 100000, "status": "proposal_created"},
    }
    sanitized = _sanitize_tool_result_event(raw_payload)
    serialized = json.dumps(sanitized)
    assert len(serialized) < 500, f"tool.result SSE payload too large: {len(serialized)}"


def test_allowed_metadata_preserved_in_tool_result():
    """P2.5: Allowed metadata keys are preserved in tool.result SSE payload."""
    from src.infrastructure.ai.runtime_service import _sanitize_tool_result_event

    raw_payload = {
        "tool_call_id": "tc_meta_001",
        "tool_name": "add_flight_note",
        "result": {"status": "proposal_created"},
    }
    sanitized = _sanitize_tool_result_event(raw_payload)
    assert sanitized["tool_call_id"] == "tc_meta_001"
    assert sanitized["tool_name"] == "add_flight_note"
    assert "result_status" in sanitized
    assert "proposal_count" in sanitized
    assert "rejected_count" in sanitized
    assert sanitized["tool_type"] in ("read_only", "write_action")


# ---------------------------------------------------------------------------
# Tool-streaming feature gate (P2.4-alpha)
# ---------------------------------------------------------------------------


def test_feature_gate_default_disabled(monkeypatch):
    """Test that AI_RUNTIME_ENABLE_TOOL_STREAMING defaults to disabled."""
    from src.infrastructure.ai.api_routes import _is_tool_streaming_enabled

    monkeypatch.delenv("AI_RUNTIME_ENABLE_TOOL_STREAMING", raising=False)
    assert _is_tool_streaming_enabled() is False


def test_feature_gate_enabled_with_1(monkeypatch):
    """Test that AI_RUNTIME_ENABLE_TOOL_STREAMING=1 enables."""
    from src.infrastructure.ai.api_routes import _is_tool_streaming_enabled

    monkeypatch.setenv("AI_RUNTIME_ENABLE_TOOL_STREAMING", "1")
    assert _is_tool_streaming_enabled() is True


def test_feature_gate_enabled_with_true(monkeypatch):
    """Test that AI_RUNTIME_ENABLE_TOOL_STREAMING=true enables."""
    from src.infrastructure.ai.api_routes import _is_tool_streaming_enabled

    monkeypatch.setenv("AI_RUNTIME_ENABLE_TOOL_STREAMING", "true")
    assert _is_tool_streaming_enabled() is True


def test_feature_gate_disabled_with_0(monkeypatch):
    """Test that AI_RUNTIME_ENABLE_TOOL_STREAMING=0 disables."""
    from src.infrastructure.ai.api_routes import _is_tool_streaming_enabled

    monkeypatch.setenv("AI_RUNTIME_ENABLE_TOOL_STREAMING", "0")
    assert _is_tool_streaming_enabled() is False


def test_feature_gate_disabled_with_false(monkeypatch):
    """Test that AI_RUNTIME_ENABLE_TOOL_STREAMING=false disables."""
    from src.infrastructure.ai.api_routes import _is_tool_streaming_enabled

    monkeypatch.setenv("AI_RUNTIME_ENABLE_TOOL_STREAMING", "false")
    assert _is_tool_streaming_enabled() is False


def test_feature_gate_disabled_with_random(monkeypatch):
    """Test that AI_RUNTIME_ENABLE_TOOL_STREAMING=random disables."""
    from src.infrastructure.ai.api_routes import _is_tool_streaming_enabled

    monkeypatch.setenv("AI_RUNTIME_ENABLE_TOOL_STREAMING", "random_value")
    assert _is_tool_streaming_enabled() is False
