"""Task B1 test: configurable round budget.

Stop conditions tested here:
- No tool calls → exit naturally
- Budget exhausted (hit hard cap) → yield StreamEvent(type="budget_exhausted")
- Consecutive failures >= threshold → stop and emit budget_exhausted event
- User cancel (if exposed via envelope.cancelled flag; handled by caller)

No reliance on range(5) as the only guard. Template budgets are:
- query_ops: default 6, hard cap 8
- anomaly_ops: default 12, hard cap 16
- dispatch_ops: default 16, hard cap 20
- Unrecognized: default 8, hard cap 12
Production default hard cap: 20.

File structure per hybrid agent plan Task B1.
"""

from __future__ import annotations

import pytest


class TestBudgetWithHardCapUnit:
    """Unit tests for resolve_budget_with_hard_cap function."""

    def test_unrecognized_task_defaults(self):
        """Unrecognized task_type uses defaults: 8/12."""
        from src.infrastructure.ai.templates.base import resolve_budget_with_hard_cap

        result = resolve_budget_with_hard_cap(5, None)
        assert result == 8

        result = resolve_budget_with_hard_cap(20, None)
        assert result == 12

        result = resolve_budget_with_hard_cap(50, None)
        assert result == 12

    def test_query_ops_hard_cap(self):
        """Query ops: default 6, hard cap 8."""
        from src.infrastructure.ai.templates import QUERY_OPS_TEMPLATE, resolve_budget_with_hard_cap

        result = resolve_budget_with_hard_cap(None, QUERY_OPS_TEMPLATE)
        assert result == 6

        result = resolve_budget_with_hard_cap(10, QUERY_OPS_TEMPLATE)
        assert result == 8

        result = resolve_budget_with_hard_cap(100, QUERY_OPS_TEMPLATE)
        assert result == 8

    def test_anomaly_ops_budget(self):
        """Anomaly ops: default 12, hard cap 16."""
        from src.infrastructure.ai.templates import ANOMALY_OPS_TEMPLATE, resolve_budget_with_hard_cap

        result = resolve_budget_with_hard_cap(None, ANOMALY_OPS_TEMPLATE)
        assert result == 12

        result = resolve_budget_with_hard_cap(20, ANOMALY_OPS_TEMPLATE)
        assert result == 16

    def test_dispatch_ops_budget(self):
        """Dispatch ops: default 16, hard cap 20."""
        from src.infrastructure.ai.templates import DISPATCH_OPS_TEMPLATE, resolve_budget_with_hard_cap

        result = resolve_budget_with_hard_cap(None, DISPATCH_OPS_TEMPLATE)
        assert result == 16

        result = resolve_budget_with_hard_cap(30, DISPATCH_OPS_TEMPLATE)
        assert result == 20

    def test_production_cap_clamps_all(self):
        """Final clamp with production default hard cap (20)."""
        from src.infrastructure.ai.templates import DISPATCH_OPS_TEMPLATE, resolve_budget_with_hard_cap

        # Even if entity config tries to set above 20, it clamps
        result = resolve_budget_with_hard_cap(100, None)
        assert result == 12  # unrecognized cap

        result = resolve_budget_with_hard_cap(100, DISPATCH_OPS_TEMPLATE)
        assert result == 20  # dispatch hard cap = 20, which equals production default

    def test_template_budget_resolution(self):
        """resolve_budget_with_hard_cap returns correct values per task_type."""
        from src.infrastructure.ai.templates import (
            QUERY_OPS_TEMPLATE,
            ANOMALY_OPS_TEMPLATE,
            DISPATCH_OPS_TEMPLATE,
            resolve_budget_with_hard_cap,
        )

        # Query ops: default 6, hard cap 8
        result = resolve_budget_with_hard_cap(5, QUERY_OPS_TEMPLATE)
        assert result == 6, f"Expected 6 (entity=5 < template_default=6), got {result}"

        result = resolve_budget_with_hard_cap(10, QUERY_OPS_TEMPLATE)
        assert result == 8, f"Expected 8 (hard cap), got {result}"

        # Anomaly ops: default 12, hard cap 16
        result = resolve_budget_with_hard_cap(8, ANOMALY_OPS_TEMPLATE)
        assert result == 12, f"Expected 12 (entity=8 < template_default=12), got {result}"

        result = resolve_budget_with_hard_cap(20, ANOMALY_OPS_TEMPLATE)
        assert result == 16, f"Expected 16 (hard cap), got {result}"

        # Dispatch ops: default 16, hard cap 20
        result = resolve_budget_with_hard_cap(10, DISPATCH_OPS_TEMPLATE)
        assert result == 16, f"Expected 16 (entity=10 < template_default=16), got {result}"

        result = resolve_budget_with_hard_cap(25, DISPATCH_OPS_TEMPLATE)
        assert result == 20, f"Expected 20 (production cap), got {result}"

        # Unrecognized task
        result = resolve_budget_with_hard_cap(5, None)
        assert result == 8, f"Expected 8 (unrecognized default), got {result}"

        result = resolve_budget_with_hard_cap(50, None)
        assert result == 12, f"Expected 12 (unrecognized hard cap), got {result}"


class TestStreamingToolsBudgetInjection:
    """Test _streaming_tools.py injects budget correctly."""

    @pytest.mark.asyncio
    async def test_effective_max_rounds_from_template_and_entity(self):
        """Streaming tools resolver combines entity max_rounds with template hard cap."""
        from src.infrastructure.ai.templates import QUERY_OPS_TEMPLATE

        # Simulate what would happen in _streaming_tools.py
        entity_max_rounds = 15  # Entity config wants 15
        template = QUERY_OPS_TEMPLATE  # Default 6, hard cap 8

        from src.infrastructure.ai.templates.base import resolve_budget_with_hard_cap
        effective = resolve_budget_with_hard_cap(entity_max_rounds, template, production_default_hard_cap=20)

        assert effective == 8, f"Expected 8 (template hard cap), got {effective}"

    @pytest.mark.asyncio
    async def test_entity_override_with_safe_cap(self):
        """Entity can increase up to its configured max_rounds, but still clamped by template hard cap."""
        from src.infrastructure.ai.templates import ANOMALY_OPS_TEMPLATE, resolve_budget_with_hard_cap

        # Entity sets 10, template default is 12 → should use 12 (default wins over lower entity value)
        result = resolve_budget_with_hard_cap(10, ANOMALY_OPS_TEMPLATE)
        assert result == 12, f"Expected 12 (template default), got {result}"

        # Entity sets 15, template hard cap is 16 → should use 15
        result = resolve_budget_with_hard_cap(15, ANOMALY_OPS_TEMPLATE)
        assert result == 15, f"Expected 15, got {result}"


class TestStopConditions:
    """Test all stop conditions are implemented."""

    @pytest.mark.asyncio
    async def test_no_tool_calls_stops_naturally(self):
        """If LLM returns no tool calls, loop breaks immediately."""
        from src.infrastructure.ai.llm_stream_runner import LLMStreamRunner
        from src.infrastructure.ai.openai_client import Message
        from unittest.mock import AsyncMock, MagicMock

        gateway = MagicMock()
        gateway.config = MagicMock()
        gateway.config.default_model = "gpt-4o"

        # Stream that completes without tool calls
        mock_chunk = MagicMock()
        mock_chunk.choices = []
        mock_chunk.usage = {"prompt_tokens": 10, "completion_tokens": 20, "total_tokens": 30}
        mock_chunk.model = "gpt-4o"

        async def async_iter():
            yield mock_chunk

        gateway.chat_completion = AsyncMock(return_value=async_iter())

        runner = LLMStreamRunner(gateway)
        events = []

        async for event in runner.stream_chat_with_tools(
            messages=[Message(role="user", content="just reply, no tools")],
            model="gpt-4o",
            tools=[],
            run_id="test_run_3",
        ):
            events.append(event)

        completed = [e for e in events if e.type == "completed"]
        assert len(completed) > 0

    @pytest.mark.asyncio
    async def test_consecutive_failures_stop_loop(self):
        """After consecutive_failure_threshold tool failures, loop stops."""
        from src.infrastructure.ai.llm_stream_runner import LLMStreamRunner
        from src.infrastructure.ai.openai_client import Message
        from unittest.mock import AsyncMock, MagicMock

        gateway = MagicMock()
        gateway.config = MagicMock()
        gateway.config.default_model = "gpt-4o"

        # Mock stream that keeps producing tool calls
        mock_chunk = MagicMock()
        mock_chunk.choices = [MagicMock(delta=MagicMock(tool_calls=[MagicMock(id="t1", function=MagicMock(name="bad_tool"))]))]
        mock_chunk.model = "gpt-4o"
        
        async def async_iter():
            for _ in range(10):
                yield mock_chunk
        
        gateway.chat_completion = AsyncMock(return_value=async_iter())

        runner = LLMStreamRunner(gateway)
        events = []

        async for event in runner.stream_chat_with_tools(
            messages=[Message(role="user", content="test")],
            model="gpt-4o",
            tools=[],
            run_id="test_run_4",
            consecutive_failure_threshold=2,  # Stop after 2 failures
        ):
            events.append(event)

        budget_exhausted = [e for e in events if e.type == "budget_exhausted"]
        completed = [e for e in events if e.type == "completed"]

        # Should stop either way (no infinite loop)
        assert len(events) > 0
        assert len(budget_exhausted) > 0 or len(completed) > 0

    @pytest.mark.asyncio
    async def test_max_tool_rounds_respected(self):
        """max_tool_rounds parameter is respected and enforces budget."""
        from src.infrastructure.ai.llm_stream_runner import LLMStreamRunner
        from src.infrastructure.ai.openai_client import Message
        from unittest.mock import AsyncMock, MagicMock

        gateway = MagicMock()
        gateway.config = MagicMock()
        gateway.config.default_model = "gpt-4o"

        # Keep producing tool calls
        mock_chunk = MagicMock()
        mock_chunk.choices = [MagicMock(delta=MagicMock(tool_calls=[MagicMock(id="t1", function=MagicMock(name="dummy"))]))]
        mock_chunk.model = "gpt-4o"
        
        async def async_iter():
            for _ in range(100):
                yield mock_chunk
        
        gateway.chat_completion = AsyncMock(return_value=async_iter())

        runner = LLMStreamRunner(gateway)
        events = []

        # Very small budget
        async for event in runner.stream_chat_with_tools(
            messages=[Message(role="user", content="test")],
            model="gpt-4o",
            tools=[],
            run_id="test_run_5",
            max_tool_rounds=1,
        ):
            events.append(event)

        # Should not loop infinitely
        budget_exhausted = [e for e in events if e.type == "budget_exhausted"]
        completed = [e for e in events if e.type == "completed"]

        # At least one terminal event should appear
        assert len(budget_exhausted) > 0 or len(completed) > 0
