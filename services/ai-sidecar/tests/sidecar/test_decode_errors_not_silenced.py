"""Tests for Task 11: Repository decode errors not silenced to zero (Python side).

Covers:
- parse_tool_arguments raises ValueError on invalid JSON (not silent {})
- ToolExecutor returns failed ToolExecutionResult for invalid JSON arguments
"""
import json
import pytest

from src.infrastructure.ai.tools.tool_executor import (
    ToolExecutor,
    ToolExecutionResult,
    parse_tool_arguments,
)


class TestParseToolArguments:
    def test_returns_dict_for_valid_json(self):
        result = parse_tool_arguments('{"key": "value", "num": 42}')
        assert result == {"key": "value", "num": 42}

    def test_returns_empty_dict_for_empty_string(self):
        assert parse_tool_arguments("") == {}

    def test_raises_value_error_for_invalid_json(self):
        with pytest.raises(ValueError, match="Invalid JSON in tool arguments"):
            parse_tool_arguments("{not valid json")

    def test_raises_value_error_for_trailing_garbage(self):
        with pytest.raises(ValueError):
            parse_tool_arguments('{"valid": true} garbage')

    def test_invalid_json_never_returns_empty_dict_silently(self):
        """Regression guard: malformed JSON must raise, not return {}."""
        malformed_inputs = [
            "{",
            "}{",
            "not json",
            '{"key": value}',
            "{key: 'value'}",
        ]
        for bad in malformed_inputs:
            with pytest.raises((ValueError, json.JSONDecodeError)):
                parse_tool_arguments(bad)


class TestToolExecutorInvalidArguments:
    def _make_executor(self) -> ToolExecutor:
        return ToolExecutor()

    @pytest.mark.asyncio
    async def test_invalid_json_arguments_return_failed_result(self):
        executor = self._make_executor()
        result = await executor.execute(
            {
                "tool_call_id": "call_1",
                "tool_name": "any_tool",
                "arguments": "{broken json!!!",
            },
            run_id="run_1",
        )
        assert isinstance(result, ToolExecutionResult)
        assert result.success is False
        assert result.tool_call_id == "call_1"
        assert "INVALID_TOOL_ARGUMENTS" in (result.error or "")

    @pytest.mark.asyncio
    async def test_valid_json_arguments_proceed_to_tool_lookup(self):
        executor = self._make_executor()
        result = await executor.execute(
            {
                "tool_call_id": "call_2",
                "tool_name": "unknown_tool",
                "arguments": '{"param": "value"}',
            },
            run_id="run_2",
        )
        assert isinstance(result, ToolExecutionResult)
        assert result.success is False
        assert "INVALID_TOOL_ARGUMENTS" not in (result.error or "")
        assert "Unknown tool" in (result.error or "")

    @pytest.mark.asyncio
    async def test_empty_string_arguments_treated_as_empty_dict(self):
        executor = self._make_executor()
        result = await executor.execute(
            {
                "tool_call_id": "call_3",
                "tool_name": "unknown_tool",
                "arguments": "",
            },
            run_id="run_3",
        )
        assert isinstance(result, ToolExecutionResult)
        assert result.success is False
        assert "INVALID_TOOL_ARGUMENTS" not in (result.error or "")
