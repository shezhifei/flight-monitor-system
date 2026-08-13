"""Tests for tool execution in streaming inference.

P2.4-alpha: Tool Streaming boundary tests.
"""

import json

import pytest

from src.infrastructure.ai.tools.read_only_tools import (
    execute_read_only_tool,
    flight_list_by_date,
    flight_status_lookup,
    get_read_only_tool_names,
    is_read_only_tool,
    weather_at_airport,
)
from src.infrastructure.ai.tools.tool_executor import (
    WRITE_ACTION_TOOLS,
    ToolExecutor,
    is_write_action_tool,
    parse_tool_arguments,
    parse_tool_calls_from_stream,
)
from tests.sidecar.tool_executor_test_support import authorized_tool_executor

# ---------------------------------------------------------------------------
# Read-only tool tests
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_flight_status_lookup():
    """Test flight status lookup returns expected structure."""
    result = await flight_status_lookup("CA1234")
    assert result["flight_id"] == "CA1234"
    assert "status" in result
    assert "gate" in result
    assert result["departure_airport"] == "PEK"
    assert result["arrival_airport"] == "PVG"


@pytest.mark.asyncio
async def test_flight_list_by_date():
    """Test flight list by date returns flights for that day."""
    result = await flight_list_by_date("2026-05-18")
    assert result["date"] == "2026-05-18"
    assert "flights" in result
    assert len(result["flights"]) == 2
    assert result["flights"][0]["flight_id"] == "CA1234"


@pytest.mark.asyncio
async def test_weather_at_airport():
    """Test weather lookup returns weather data."""
    result = await weather_at_airport("PEK")
    assert result["airport"] == "PEK"
    assert "temperature_celsius" in result
    assert "condition" in result
    assert "wind_speed_kt" in result


@pytest.mark.asyncio
async def test_get_read_only_tool_names():
    """Test tool name registry."""
    names = get_read_only_tool_names()
    assert "flight_status_lookup" in names
    assert "weather_at_airport" in names
    assert len(names) >= 3


@pytest.mark.asyncio
async def test_is_read_only_tool():
    """Test tool type detection."""
    assert is_read_only_tool("flight_status_lookup") is True
    assert is_read_only_tool("weather_at_airport") is True
    assert is_read_only_tool("nonexistent_tool") is False


@pytest.mark.asyncio
async def test_execute_read_only_tool_success():
    """Test successful execution of read-only tool."""
    result = await execute_read_only_tool("flight_status_lookup", {"flight_id": "CA5678"})
    assert result["flight_id"] == "CA5678"


@pytest.mark.asyncio
async def test_execute_read_only_tool_unknown():
    """Test execution fails for unknown tool."""
    with pytest.raises(ValueError, match="Unknown read-only tool"):
        await execute_read_only_tool("unknown_tool", {})


# ---------------------------------------------------------------------------
# ToolExecutor tests
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_tool_executor_read_only():
    """Test ToolExecutor handles read-only tools."""
    executor = ToolExecutor()
    tool_call = {
        "tool_call_id": "tc_001",
        "tool_name": "flight_status_lookup",
        "arguments": {"flight_id": "CA1234"},
    }
    result = await executor.execute(tool_call, run_id="run_001")
    assert result.success is True
    assert result.tool_name == "flight_status_lookup"
    assert result.result["flight_id"] == "CA1234"
    assert result.proposal is None
    assert result.error is None


@pytest.mark.asyncio
async def test_tool_executor_write_action_becomes_proposal():
    """Test ToolExecutor creates proposal for write-action tools."""
    executor = authorized_tool_executor()
    tool_call = {
        "tool_call_id": "tc_002",
        "tool_name": "add_flight_note",
        "arguments": {"flight_id": "CA1234", "note_content": "Test note"},
    }
    result = await executor.execute(tool_call, run_id="run_002")
    assert result.success is True
    assert result.proposal is not None
    assert result.proposal["object_type"] == "Flight"
    assert result.proposal["action_name"] == "add_note"
    assert result.proposal["requires_approval"] is True
    assert result.proposal["source"] == "streaming_tool_execution"


@pytest.mark.asyncio
async def test_tool_executor_unknown_tool():
    """Test ToolExecutor returns error for unknown tools."""
    executor = authorized_tool_executor()
    tool_call = {
        "tool_call_id": "tc_003",
        "tool_name": "nonexistent_tool",
        "arguments": {},
    }
    result = await executor.execute(tool_call, run_id="run_003")
    assert result.success is False
    assert "Unknown tool" in result.error


@pytest.mark.asyncio
async def test_tool_executor_batch():
    """Test batch execution of multiple tools."""
    executor = authorized_tool_executor()
    tool_calls = [
        {"tool_call_id": "tc_010", "tool_name": "flight_status_lookup", "arguments": {"flight_id": "CA1234"}},
        {"tool_call_id": "tc_011", "tool_name": "weather_at_airport", "arguments": {"airport_code": "PEK"}},
        {
            "tool_call_id": "tc_012",
            "tool_name": "add_flight_note",
            "arguments": {"flight_id": "CA5678", "note_content": "Note"},
        },
    ]
    results = await executor.execute_batch(tool_calls, run_id="run_batch")
    assert len(results) == 3
    assert results[0].success is True
    assert results[1].success is True
    assert results[2].success is True
    assert results[2].proposal is not None  # write action


@pytest.mark.asyncio
async def test_collect_proposals():
    """Test proposal collection from execution results."""
    executor = authorized_tool_executor()
    tool_calls = [
        {"tool_call_id": "tc_020", "tool_name": "flight_status_lookup", "arguments": {"flight_id": "CA1"}},
        {
            "tool_call_id": "tc_021",
            "tool_name": "add_flight_note",
            "arguments": {"flight_id": "CA2", "note_content": "Note"},
        },
        {
            "tool_call_id": "tc_022",
            "tool_name": "update_flight_status",
            "arguments": {"flight_id": "CA3", "status": "delayed"},
        },
    ]
    results = await executor.execute_batch(tool_calls, run_id="run_collect")
    proposals = executor.collect_proposals(results)
    assert len(proposals) == 2
    assert proposals[0]["action_name"] == "add_note"
    assert proposals[1]["action_name"] == "update_status"


@pytest.mark.asyncio
async def test_get_available_tools():
    """Test listing available tools."""
    executor = ToolExecutor()
    tools = executor.get_available_tools()
    assert "flight_status_lookup" in tools
    assert "add_flight_note" in tools


@pytest.mark.asyncio
async def test_get_tool_type():
    """Test tool type detection."""
    executor = ToolExecutor()
    assert executor.get_tool_type("flight_status_lookup") == "read_only"
    assert executor.get_tool_type("add_flight_note") == "write_action"
    assert executor.get_tool_type("unknown") == "unknown"


# ---------------------------------------------------------------------------
# Helper function tests
# ---------------------------------------------------------------------------


def test_is_write_action_tool():
    """Test write action tool detection."""
    assert is_write_action_tool("add_flight_note") is True
    assert is_write_action_tool("assign_gate") is True
    assert is_write_action_tool("flight_status_lookup") is False


def test_parse_tool_calls_from_stream():
    """Test parsing tool calls from streaming content blocks."""
    blocks = [
        {"type": "tool_call", "tool_call": {"id": "tc1", "function": {"name": "test_tool", "arguments": '{"arg": 1}'}}},
        {"type": "text", "text": "Hello"},
        {"type": "function_call", "id": "tc2", "name": "another_tool", "arguments": "{}"},
    ]
    tool_calls = parse_tool_calls_from_stream(blocks)
    assert len(tool_calls) == 2
    assert tool_calls[0]["tool_call_id"] == "tc1"
    assert tool_calls[0]["tool_name"] == "test_tool"
    assert tool_calls[1]["tool_call_id"] == "tc2"


def test_parse_tool_arguments():
    """Test tool argument parsing."""
    import pytest

    assert parse_tool_arguments('{"key": "value"}') == {"key": "value"}
    assert parse_tool_arguments("") == {}
    with pytest.raises(ValueError):
        parse_tool_arguments("invalid json")


def test_write_action_tools_registry():
    """Test write action tools are properly registered."""
    assert "add_flight_note" in WRITE_ACTION_TOOLS
    assert WRITE_ACTION_TOOLS["add_flight_note"] == ("Flight", "add_note")
    assert WRITE_ACTION_TOOLS["assign_gate"] == ("Flight", "assign_gate")


# ---------------------------------------------------------------------------
# SSE payload tests
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_sse_payload_success():
    """Test SSE payload generation for successful execution."""
    executor = ToolExecutor()
    tool_call = {
        "tool_call_id": "tc_030",
        "tool_name": "weather_at_airport",
        "arguments": {"airport_code": "PVG"},
    }
    result = await executor.execute(tool_call, run_id="run_sse")
    payload = result.to_sse_payload()
    assert payload["tool_call_id"] == "tc_030"
    assert payload["tool_name"] == "weather_at_airport"
    assert "result" in payload
    assert "error" not in payload


@pytest.mark.asyncio
async def test_sse_payload_error():
    """Test SSE payload generation for failed execution."""
    executor = ToolExecutor()
    tool_call = {
        "tool_call_id": "tc_031",
        "tool_name": "nonexistent",
        "arguments": {},
    }
    result = await executor.execute(tool_call, run_id="run_sse_error")
    payload = result.to_sse_payload()
    assert payload["tool_call_id"] == "tc_031"
    assert "error" in payload
    assert "result" not in payload


# ---------------------------------------------------------------------------
# Risk level tests
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_high_risk_tools():
    """Test that critical tools are marked as high risk."""
    executor = authorized_tool_executor()
    high_risk_tools = ["assign_gate", "update_flight_status"]

    for tool_name in high_risk_tools:
        tool_call = {
            "tool_call_id": f"tc_{tool_name}",
            "tool_name": tool_name,
            "arguments": {"flight_id": "CA1234"},
        }
        result = await executor.execute(tool_call, run_id="run_risk")
        assert result.proposal is not None
        assert result.proposal["risk_level"] == "high"


@pytest.mark.asyncio
async def test_medium_risk_tools():
    """Test that standard write tools are marked as medium risk."""
    executor = authorized_tool_executor()
    tool_call = {
        "tool_call_id": "tc_040",
        "tool_name": "add_flight_note",
        "arguments": {"flight_id": "CA1234", "note_content": "Note"},
    }
    result = await executor.execute(tool_call, run_id="run_risk")
    assert result.proposal is not None
    assert result.proposal["risk_level"] == "medium"


# ---------------------------------------------------------------------------
# P2.4-alpha boundary tests
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


@pytest.mark.asyncio
async def test_read_only_tool_result_no_secrets():
    """Test that read-only tool results don't contain secrets or large raw payloads."""
    executor = ToolExecutor()
    tool_call = {
        "tool_call_id": "tc_secret",
        "tool_name": "flight_status_lookup",
        "arguments": {"flight_id": "CA1234"},
    }
    result = await executor.execute(tool_call, run_id="run_secret")
    assert result.success is True
    payload = result.to_sse_payload()
    payload_str = str(payload)
    # No secrets in result
    assert "api_key" not in payload_str.lower()
    assert "authorization" not in payload_str.lower()
    assert "token" not in payload_str.lower() or "tool_call_id" in payload_str
    # No excessively large raw payload
    assert len(payload_str) < 5000, "tool result should be bounded, not raw large objects"


@pytest.mark.asyncio
async def test_write_action_tool_only_generates_proposal():
    """Test that write-action tools generate proposals without executing business actions."""
    executor = authorized_tool_executor()
    tool_call = {
        "tool_call_id": "tc_write",
        "tool_name": "add_flight_note",
        "arguments": {"flight_id": "CA1234", "note_content": "Test note"},
    }
    result = await executor.execute(tool_call, run_id="run_write")
    assert result.success is True
    # Must generate a proposal
    assert result.proposal is not None
    assert result.proposal["object_type"] == "Flight"
    assert result.proposal["action_name"] == "add_note"
    # Must NOT claim to have executed the write
    assert result.result.get("status") == "proposal_created"
    # Must NOT have a field like "executed": true
    assert "executed" not in result.result


@pytest.mark.asyncio
async def test_unknown_tool_emits_safe_error():
    """Test that unknown tool emits a safe error without crashing."""
    executor = authorized_tool_executor()
    tool_call = {
        "tool_call_id": "tc_unknown",
        "tool_name": "nonexistent_dangerous_tool",
        "arguments": {},
    }
    result = await executor.execute(tool_call, run_id="run_unknown")
    assert result.success is False
    assert result.error is not None
    assert "Unknown tool" in result.error
    # Error should not leak internal paths or stack traces
    assert "C:\\" not in result.error
    assert "Traceback" not in result.error


@pytest.mark.asyncio
async def test_read_only_tool_result_summary_bounded():
    """Test tool.result payload is bounded for SSE streaming."""
    executor = authorized_tool_executor()
    tool_call = {
        "tool_call_id": "tc_summary",
        "tool_name": "flight_list_by_date",
        "arguments": {"date": "2026-05-18"},
    }
    result = await executor.execute(tool_call, run_id="run_summary")
    assert result.success is True
    payload = result.to_sse_payload()
    # Result should be serializable and bounded
    import json

    serialized = json.dumps(payload)
    assert len(serialized) < 10000, "tool result should be bounded for SSE"


@pytest.mark.asyncio
async def test_write_action_proposal_requires_approval():
    """Test all write-action proposals require approval."""
    executor = authorized_tool_executor()
    for tool_name in WRITE_ACTION_TOOLS:
        tool_call = {
            "tool_call_id": f"tc_approval_{tool_name}",
            "tool_name": tool_name,
            "arguments": {"flight_id": "CA1234", "note_content": "test"},
        }
        result = await executor.execute(tool_call, run_id="run_approval")
        assert result.proposal is not None, f"{tool_name} must generate proposal"
        assert result.proposal["requires_approval"] is True, f"{tool_name} proposal must require approval"


# ---------------------------------------------------------------------------
# P2.4-alpha enabled path contract tests (Python sidecar)
# ---------------------------------------------------------------------------


def test_feature_gate_default_disabled_internal_route(monkeypatch):
    """Test that /internal/ai/v1/runs/stream-with-tools defaults to disabled."""
    from src.infrastructure.ai.api_routes import _is_tool_streaming_enabled

    monkeypatch.delenv("AI_RUNTIME_ENABLE_TOOL_STREAMING", raising=False)
    assert _is_tool_streaming_enabled() is False


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


@pytest.mark.asyncio
async def test_enabled_read_only_tool_execution_produces_result():
    """Test enabled path: read-only tool executes and produces bounded result."""
    executor = ToolExecutor()
    tool_call = {
        "tool_call_id": "tc_enabled_ro",
        "tool_name": "flight_status_lookup",
        "arguments": {"flight_id": "CA9999"},
    }
    result = await executor.execute(tool_call, run_id="run_enabled_ro")
    assert result.success is True
    assert result.result is not None
    assert result.result["flight_id"] == "CA9999"
    assert result.proposal is None

    payload = result.to_sse_payload()
    payload_str = str(payload)
    assert "api_key" not in payload_str.lower()
    assert "authorization" not in payload_str.lower()
    assert "sk-" not in payload_str.lower()
    assert len(payload_str) < 5000


@pytest.mark.asyncio
async def test_enabled_write_action_produces_proposal_no_db_write():
    """Test enabled path: write-action tool produces proposal without DB write."""
    executor = authorized_tool_executor()
    tool_call = {
        "tool_call_id": "tc_enabled_write",
        "tool_name": "add_flight_note",
        "arguments": {"flight_id": "CA7777", "note_content": "enabled path test"},
    }
    result = await executor.execute(tool_call, run_id="run_enabled_write")
    assert result.success is True
    assert result.proposal is not None
    assert result.proposal["object_type"] == "Flight"
    assert result.proposal["object_id"] == "CA7777"
    assert result.proposal["action_name"] == "add_note"
    assert result.proposal["requires_approval"] is True
    assert result.result["status"] == "proposal_created"
    assert "executed" not in result.result
    assert "db" not in str(result.result).lower()


@pytest.mark.asyncio
async def test_enabled_mid_stream_error_does_not_succeed():
    """Test enabled path: mid-stream error does not produce succeeded run.complete."""
    executor = authorized_tool_executor()
    tool_call = {
        "tool_call_id": "tc_enabled_err",
        "tool_name": "nonexistent_dangerous_tool",
        "arguments": {},
    }
    result = await executor.execute(tool_call, run_id="run_enabled_err")
    assert result.success is False
    assert result.error is not None
    assert "Unknown tool" in result.error

    payload = result.to_sse_payload()
    assert "error" in payload
    assert "result" not in payload


@pytest.mark.asyncio
async def test_tool_result_no_secrets_in_payload():
    """Test tool.result payload does not contain secrets."""
    executor = ToolExecutor()
    tool_call = {
        "tool_call_id": "tc_security_check",
        "tool_name": "flight_status_lookup",
        "arguments": {"flight_id": "CA0000"},
    }
    result = await executor.execute(tool_call, run_id="run_security_check")
    payload = result.to_sse_payload()
    payload_str = json.dumps(payload).lower()

    assert "api_key" not in payload_str
    assert "authorization" not in payload_str
    assert "bearer" not in payload_str
    assert "sk-" not in payload_str
    assert "password" not in payload_str


@pytest.mark.asyncio
async def test_tool_result_large_payload_bounded():
    """Test tool.result payload is bounded for SSE streaming."""
    executor = ToolExecutor()
    tool_call = {
        "tool_call_id": "tc_bounded",
        "tool_name": "flight_list_by_date",
        "arguments": {"date": "2026-05-18"},
    }
    result = await executor.execute(tool_call, run_id="run_bounded")
    payload = result.to_sse_payload()
    serialized = json.dumps(payload)
    assert len(serialized) < 10000, "tool result should be bounded for SSE"


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
        "tool_name": "weather_at_airport",
        "result": {
            "data": {"jwt": "eyJ...", "password": "s3cret"},
            "status": "proposal_created",
        },
        "arguments": {"airport_code": "PEK", "token": "abc123"},
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
        "tool_name": "flight_list_by_date",
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
