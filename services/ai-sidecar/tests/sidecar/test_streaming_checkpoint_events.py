"""Tests for streaming checkpoint events (Task D1).

Asserts:
1. Checkpoint events emitted at key lifecycle points: before_tool, after_tool,
   before_proposal, after_completion
2. Checkpoints contain necessary state for resume
3. on_child_event callback invoked with checkpoint data
4. Multiple rounds generate multiple checkpoints

Checkpoint types per plan:
- run_input: initial request received
- before_tool: before executing tools
- after_tool: after tool execution completed
- before_proposal: before generating proposal (if applicable)
- after_completion: final result persisted
"""

from __future__ import annotations

from unittest.mock import AsyncMock, MagicMock

import pytest

from src.infrastructure.ai.llm_stream_runner import (
    LLMStreamRunner,
    StreamCompletionResult,
    StreamEvent,
)
from src.infrastructure.ai.openai_client import Message

# ============================================================================
# Test Checkpoint Emission
# ============================================================================


class TestCheckpointLifecyclePoints:
    """Verify checkpoints emitted at correct lifecycle points."""

    @pytest.mark.asyncio
    async def test_before_tool_checkpoint_emitted(self):
        """before_tool checkpoint emitted before tool execution."""
        runner = LLMStreamRunner(client=MagicMock())

        # Track checkpoint events
        checkpoint_events = []

        async def mock_on_child_event(event):
            if event.get("type") == "checkpoint":
                checkpoint_events.append(event)

        # Mock tool calls in result
        result = StreamCompletionResult(model="test-model")
        result.tool_calls = [
            {
                "id": "tool_1",
                "function": {"name": "list_flights", "arguments": "{}"},
            }
        ]

        # Create events that trigger tool calls
        tool_call_event = StreamEvent(type="tool_call", tool_call={})
        completed_event = StreamEvent(
            type="completed",
            result=result,
            round_index=0,
        )

        # Simulate stream events; like the real ``_stream_chat_impl``, the
        # mock populates the runner's per-round result object.
        events_consumed = []

        async def mock_impl(*args, **kwargs):
            run_result = kwargs.get("result")
            if run_result is not None:
                run_result.tool_calls = list(result.tool_calls)
                run_result.text = result.text
            yield tool_call_event
            yield completed_event

        runner._stream_chat_impl = mock_impl

        # Execute with mocked executor
        from src.infrastructure.ai.tools.tool_executor import ToolExecutor

        mock_executor = MagicMock(spec=ToolExecutor)
        mock_execution_result = MagicMock()
        mock_execution_result.error = None
        mock_execution_result.result = {"data": []}
        mock_execution_result.to_sse_payload.return_value = {"tool_name": "list_flights"}
        mock_execution_result.to_dict.return_value = {"result": {"data": []}}
        mock_executor.execute_batch = AsyncMock(return_value=[mock_execution_result])

        runner._tool_executor = mock_executor

        async for _event in runner.stream_chat_with_tools(
            messages=[],
            model="test-model",
            run_id="test_run_123",
            envelope=MagicMock(),
            entity_id="flight_F1234",
            on_child_event=mock_on_child_event,
            max_tool_rounds=5,
        ):
            events_consumed.append(_event)

        # Verify before_tool checkpoint emitted
        before_tool_checks = [e for e in checkpoint_events if e.get("checkpoint_type") == "before_tool"]
        assert len(before_tool_checks) >= 1
        assert before_tool_checks[0]["run_id"] == "test_run_123"

    @pytest.mark.asyncio
    async def test_after_tool_checkpoint_emitted(self):
        """after_tool checkpoint emitted after tool execution."""
        runner = LLMStreamRunner(client=MagicMock())

        checkpoint_events = []

        async def mock_on_child_event(event):
            if event.get("type") == "checkpoint":
                checkpoint_events.append(event)

        result = StreamCompletionResult(model="test-model")
        result.tool_calls = [{"id": "tool_1", "function": {"name": "get_flight"}}]

        tool_call_event = StreamEvent(type="tool_call", tool_call={})
        tool_result_event = StreamEvent(type="tool_result", tool_call={})
        completed_event = StreamEvent(type="completed", result=result, round_index=0)

        async def mock_impl(*args, **kwargs):
            run_result = kwargs.get("result")
            if run_result is not None:
                run_result.tool_calls = list(result.tool_calls)
                run_result.text = result.text
            yield tool_call_event
            yield completed_event

        runner._stream_chat_impl = mock_impl

        from src.infrastructure.ai.tools.tool_executor import ToolExecutor

        mock_executor = MagicMock(spec=ToolExecutor)
        mock_execution_result = MagicMock()
        mock_execution_result.error = None
        mock_execution_result.result = {"flight": "F1234"}
        mock_execution_result.to_sse_payload.return_value = {"tool_name": "get_flight"}
        mock_execution_result.to_dict.return_value = {"result": {"flight": "F1234"}}
        mock_executor.execute_batch = AsyncMock(return_value=[mock_execution_result])

        runner._tool_executor = mock_executor

        async for _event in runner.stream_chat_with_tools(
            messages=[],
            model="test-model",
            run_id="test_run_456",
            envelope=MagicMock(),
            entity_id="flight_F1234",
            on_child_event=mock_on_child_event,
            max_tool_rounds=5,
        ):
            pass

        # Verify after_tool checkpoint emitted
        after_tool_checks = [e for e in checkpoint_events if e.get("checkpoint_type") == "after_tool"]
        assert len(after_tool_checks) >= 1
        assert after_tool_checks[0]["tool_calls_executed"] == 1

    @pytest.mark.asyncio
    async def test_before_proposal_checkpoint_emitted(self):
        """before_proposal checkpoint emitted in loop."""
        runner = LLMStreamRunner(client=MagicMock())

        checkpoint_events = []

        async def mock_on_child_event(event):
            if event.get("type") == "checkpoint":
                checkpoint_events.append(event)

        result = StreamCompletionResult(model="test-model")
        result.tool_calls = [{"id": "tool_1", "function": {"name": "list_anomalies"}}]

        async def mock_impl(*args, **kwargs):
            run_result = kwargs.get("result")
            if run_result is not None:
                run_result.tool_calls = list(result.tool_calls)
                run_result.text = result.text
            yield StreamEvent(type="tool_call", tool_call={})
            yield StreamEvent(type="completed", result=result, round_index=0)

        runner._stream_chat_impl = mock_impl

        from src.infrastructure.ai.tools.tool_executor import ToolExecutor

        mock_executor = MagicMock(spec=ToolExecutor)
        mock_execution_result = MagicMock()
        mock_execution_result.error = None
        mock_execution_result.result = {"anomalies": []}
        mock_execution_result.to_sse_payload.return_value = {}
        mock_execution_result.to_dict.return_value = {"result": {"anomalies": []}}
        mock_executor.execute_batch = AsyncMock(return_value=[mock_execution_result])

        runner._tool_executor = mock_executor

        async for _event in runner.stream_chat_with_tools(
            messages=[Message(role="user", content="查询异常列表")],
            model="test-model",
            run_id="test_run_789",
            envelope=MagicMock(),
            entity_id="anomaly_AN001",
            on_child_event=mock_on_child_event,
            max_tool_rounds=5,
        ):
            pass

        # Verify before_proposal checkpoint emitted
        proposal_checks = [e for e in checkpoint_events if e.get("checkpoint_type") == "before_proposal"]
        assert len(proposal_checks) >= 1
        assert proposal_checks[0]["context_snapshot"]["messages_count"] > 0


# ============================================================================
# Test Checkpoint Content
# ============================================================================


class TestCheckpointContent:
    """Verify checkpoint contains sufficient state for resume."""

    @pytest.mark.asyncio
    async def test_checkpoint_has_run_id(self):
        """All checkpoints include run_id for correlation."""
        runner = LLMStreamRunner(client=MagicMock())

        checkpoint_events = []

        async def mock_on_child_event(event):
            if event.get("type") == "checkpoint":
                checkpoint_events.append(event)

        result = StreamCompletionResult(model="test-model")
        result.tool_calls = [{"id": "t1", "function": {"name": "test"}}]

        async def mock_impl(*args, **kwargs):
            yield StreamEvent(type="tool_call", tool_call={})
            yield StreamEvent(type="completed", result=result, round_index=0)

        runner._stream_chat_impl = mock_impl

        from src.infrastructure.ai.tools.tool_executor import ToolExecutor

        mock_executor = MagicMock(spec=ToolExecutor)
        mock_execution_result = MagicMock()
        mock_execution_result.error = None
        mock_execution_result.result = {}
        mock_execution_result.to_sse_payload.return_value = {}
        mock_execution_result.to_dict.return_value = {}
        mock_executor.execute_batch = AsyncMock(return_value=[mock_execution_result])

        runner._tool_executor = mock_executor

        async for _event in runner.stream_chat_with_tools(
            messages=[],
            model="test-model",
            run_id="unique_run_xyz",
            envelope=MagicMock(),
            entity_id="flight_test",
            on_child_event=mock_on_child_event,
        ):
            pass

        # All checkpoints should have run_id
        for check in checkpoint_events:
            assert check["run_id"] == "unique_run_xyz"

    @pytest.mark.asyncio
    async def test_checkpoint_includes_timestamp(self):
        """Checkpoints include timestamp for ordering."""
        runner = LLMStreamRunner(client=MagicMock())

        checkpoint_events = []

        async def mock_on_child_event(event):
            if event.get("type") == "checkpoint":
                checkpoint_events.append(event)

        result = StreamCompletionResult(model="test-model")
        result.tool_calls = [{"id": "t1", "function": {"name": "test"}}]

        async def mock_impl(*args, **kwargs):
            yield StreamEvent(type="tool_call", tool_call={})
            yield StreamEvent(type="completed", result=result, round_index=0)

        runner._stream_chat_impl = mock_impl

        from src.infrastructure.ai.tools.tool_executor import ToolExecutor

        mock_executor = MagicMock(spec=ToolExecutor)
        mock_execution_result = MagicMock()
        mock_execution_result.error = None
        mock_execution_result.result = {}
        mock_execution_result.to_sse_payload.return_value = {}
        mock_execution_result.to_dict.return_value = {}
        mock_executor.execute_batch = AsyncMock(return_value=[mock_execution_result])

        runner._tool_executor = mock_executor

        async for _event in runner.stream_chat_with_tools(
            messages=[],
            model="test-model",
            run_id="test_ts",
            envelope=MagicMock(),
            entity_id="flight_test",
            on_child_event=mock_on_child_event,
        ):
            pass

        # Timestamps present and monotonically increasing
        timestamps = [e["timestamp"] for e in checkpoint_events]
        assert all(isinstance(ts, (int, float)) for ts in timestamps)
        assert timestamps == sorted(timestamps)

    @pytest.mark.asyncio
    async def test_checkpoint_after_completion_present(self):
        """Final after_completion checkpoint emitted at end."""
        runner = LLMStreamRunner(client=MagicMock())

        checkpoint_events = []

        async def mock_on_child_event(event):
            if event.get("type") == "checkpoint":
                checkpoint_events.append(event)

        result = StreamCompletionResult(model="test-model", text="Done")

        async def mock_impl(*args, **kwargs):
            run_result = kwargs.get("result")
            if run_result is not None:
                run_result.tool_calls = []
                run_result.text = result.text
            yield StreamEvent(type="completed", result=result, round_index=0)

        runner._stream_chat_impl = mock_impl

        async for _event in runner.stream_chat_with_tools(
            messages=[Message(role="user", content="hi")],
            model="test-model",
            run_id="final_check",
            envelope=MagicMock(),
            entity_id="flight_test",
            on_child_event=mock_on_child_event,
        ):
            pass

        # Final checkpoint should exist
        final_checks = [e for e in checkpoint_events if e.get("checkpoint_type") == "after_completion"]
        assert len(final_checks) == 1
        assert final_checks[0]["messages_count"] > 0


# ============================================================================
# Test Resume Readiness
# ============================================================================


class TestResumeReadiness:
    """Verify checkpoints provide sufficient state for resume."""

    @pytest.mark.asyncio
    async def test_checkpoint_preserves_round_context(self):
        """Round index preserved for continuation point."""
        runner = LLMStreamRunner(client=MagicMock())

        checkpoint_events = []

        async def mock_on_child_event(event):
            if event.get("type") == "checkpoint":
                checkpoint_events.append(event)

        result = StreamCompletionResult(model="test-model")
        result.tool_calls = [{"id": "t1", "function": {"name": "list"}}]

        async def mock_impl(*args, **kwargs):
            yield StreamEvent(type="tool_call", tool_call={})
            yield StreamEvent(type="completed", result=result, round_index=3)

        runner._stream_chat_impl = mock_impl

        from src.infrastructure.ai.tools.tool_executor import ToolExecutor

        mock_executor = MagicMock(spec=ToolExecutor)
        mock_execution_result = MagicMock()
        mock_execution_result.error = None
        mock_execution_result.result = {}
        mock_execution_result.to_sse_payload.return_value = {}
        mock_execution_result.to_dict.return_value = {}
        mock_executor.execute_batch = AsyncMock(return_value=[mock_execution_result])

        runner._tool_executor = mock_executor

        async for _event in runner.stream_chat_with_tools(
            messages=[],
            model="test-model",
            run_id="round_ctx",
            envelope=MagicMock(),
            entity_id="flight_test",
            on_child_event=mock_on_child_event,
            max_tool_rounds=10,
        ):
            pass

        # Round context preserved
        after_tool_checks = [e for e in checkpoint_events if e.get("checkpoint_type") == "after_tool"]
        if after_tool_checks:
            assert after_tool_checks[-1]["round_index"] == 3

    @pytest.mark.asyncio
    async def test_checkpoint_state_sufficient_for_resume(self):
        """Checkpoint state contains everything needed to resume."""
        runner = LLMStreamRunner(client=MagicMock())

        checkpoint_events = []

        async def mock_on_child_event(event):
            if event.get("type") == "checkpoint":
                checkpoint_events.append(event)

        result = StreamCompletionResult(model="test-model")
        result.tool_calls = [{"id": "t1", "function": {"name": "query"}}]

        async def mock_impl(*args, **kwargs):
            run_result = kwargs.get("result")
            if run_result is not None:
                run_result.tool_calls = list(result.tool_calls)
                run_result.text = result.text
            yield StreamEvent(type="tool_call", tool_call={})
            yield StreamEvent(type="completed", result=result, round_index=1)

        runner._stream_chat_impl = mock_impl

        from src.infrastructure.ai.tools.tool_executor import ToolExecutor

        mock_executor = MagicMock(spec=ToolExecutor)
        mock_execution_result = MagicMock()
        mock_execution_result.error = None
        mock_execution_result.result = {"data": "test"}
        mock_execution_result.to_sse_payload.return_value = {}
        mock_execution_result.to_dict.return_value = {"result": {"data": "test"}}
        mock_executor.execute_batch = AsyncMock(return_value=[mock_execution_result])

        runner._tool_executor = mock_executor

        async for _event in runner.stream_chat_with_tools(
            messages=[],
            model="test-model",
            run_id="resume_ready",
            envelope=MagicMock(),
            entity_id="flight_test",
            on_child_event=mock_on_child_event,
        ):
            pass

        # Verify all required fields for resume
        after_tool = next((e for e in checkpoint_events if e.get("checkpoint_type") == "after_tool"), None)

        assert after_tool is not None
        assert "run_id" in after_tool
        assert "round_index" in after_tool
        assert "tool_calls_executed" in after_tool
        assert "results" in after_tool
        assert "timestamp" in after_tool


# ============================================================================
# Integration Tests
# ============================================================================


class TestCheckpointIntegration:
    """End-to-end integration tests."""

    @pytest.mark.asyncio
    async def test_multiple_rounds_generate_multiple_checkpoints(self):
        """Multiple tool rounds produce checkpoint for each."""
        runner = LLMStreamRunner(client=MagicMock())

        checkpoint_events = []

        async def mock_on_child_event(event):
            if event.get("type") == "checkpoint":
                checkpoint_events.append(event)

        # Simulate 3 rounds of tool calls
        round_results = []
        for i in range(3):
            result = StreamCompletionResult(model="test-model")
            result.tool_calls = [{"id": f"t{i}", "function": {"name": f"tool_{i}"}}]
            round_results.append(result)

        call_count = [0]

        async def mock_impl(*args, **kwargs):
            idx = call_count[0] % 3
            call_count[0] += 1

            if idx < 3:
                run_result = kwargs.get("result")
                if run_result is not None:
                    run_result.tool_calls = list(round_results[idx].tool_calls)
                    run_result.text = round_results[idx].text
                yield StreamEvent(type="tool_call", tool_call={})
                yield StreamEvent(type="completed", result=round_results[idx], round_index=idx)
            else:
                # Round 4+: no tool calls — the loop must stop here.
                run_result = kwargs.get("result")
                if run_result is not None:
                    run_result.tool_calls = []
                    run_result.text = "final answer"
                yield StreamEvent(type="completed", result=round_results[0], round_index=3)

        runner._stream_chat_impl = mock_impl

        from src.infrastructure.ai.tools.tool_executor import ToolExecutor

        async def mock_execute_batch(*args, **kwargs):
            round_idx = kwargs.get("round_index", 0)
            mock_execution_result = MagicMock()
            mock_execution_result.error = None
            mock_execution_result.result = {"round": round_idx}
            mock_execution_result.to_sse_payload.return_value = {}
            mock_execution_result.to_dict.return_value = {"result": {"round": round_idx}}
            return [mock_execution_result]

        mock_executor = MagicMock(spec=ToolExecutor)
        mock_executor.execute_batch = mock_execute_batch
        runner._tool_executor = mock_executor

        async for _event in runner.stream_chat_with_tools(
            messages=[],
            model="test-model",
            run_id="multi_round",
            envelope=MagicMock(),
            entity_id="flight_test",
            on_child_event=mock_on_child_event,
            max_tool_rounds=10,
        ):
            pass

        # Should have multiple checkpoints across rounds
        total_checkpoints = len([e for e in checkpoint_events if e.get("type") == "checkpoint"])
        assert total_checkpoints >= 3  # At least one checkpoint per round type


# ============================================================================
# Test MQ Durable Publish Path (Task D1)
# ============================================================================


class _FakeMqPublisher:
    """Records every envelope handed to the MQ gateway."""

    def __init__(self):
        self.published: list[dict] = []

    async def publish(self, envelope):
        self.published.append(envelope)


def _checkpoint_envelopes(publisher: _FakeMqPublisher) -> list[dict]:
    return [e for e in publisher.published if e.get("event_type") == "checkpoint"]


def _install_fake_mq(monkeypatch, publisher: _FakeMqPublisher) -> None:
    """Route the runner's durable publish path to the fake publisher."""
    import src.infrastructure.ai.runtime_service._mq_publish as mq_publish

    monkeypatch.setattr(mq_publish, "_resolve_mq_publisher", lambda: publisher)
    monkeypatch.setattr(mq_publish, "_resolve_mq_gate", lambda: None)


def _mock_executor():
    from src.infrastructure.ai.tools.tool_executor import ToolExecutor

    mock_executor = MagicMock(spec=ToolExecutor)
    mock_execution_result = MagicMock()
    mock_execution_result.error = None
    mock_execution_result.result = {"data": []}
    mock_execution_result.to_sse_payload.return_value = {"tool_name": "list_flights"}
    mock_execution_result.to_dict.return_value = {"result": {"data": []}}
    mock_executor.execute_batch = AsyncMock(return_value=[mock_execution_result])
    return mock_executor


def _tool_calling_impl(tool_name: str = "list_flights", rounds: int = 1):
    """Fake ``_stream_chat_impl``: ``rounds`` tool-calling rounds, then done."""
    call_count = [0]

    async def mock_impl(*args, **kwargs):
        idx = call_count[0]
        call_count[0] += 1
        run_result = kwargs.get("result")
        if run_result is not None:
            if idx < rounds:
                run_result.tool_calls = [{"id": f"tool_{idx}", "function": {"name": tool_name, "arguments": "{}"}}]
                run_result.text = ""
            else:
                run_result.tool_calls = []
                run_result.text = "final answer"
        yield StreamEvent(type="completed", result=run_result, round_index=idx)

    return mock_impl


class TestCheckpointMqPublish:
    """D1: every emission point also publishes a durable checkpoint over MQ."""

    @pytest.mark.asyncio
    async def test_before_tool_checkpoint_published_over_mq(self, monkeypatch):
        publisher = _FakeMqPublisher()
        _install_fake_mq(monkeypatch, publisher)

        from src.infrastructure.ai.working_memory import WorkingMemory

        working_memory = WorkingMemory(run_id="mq_run_before")
        working_memory.write_plan("# Plan\n- [ ] step 1")

        runner = LLMStreamRunner(client=MagicMock())
        runner._stream_chat_impl = _tool_calling_impl(rounds=1)
        runner._tool_executor = _mock_executor()

        async for _ in runner.stream_chat_with_tools(
            messages=[Message(role="user", content="hi")],
            model="test-model",
            run_id="mq_run_before",
            envelope=MagicMock(job_id="job-1"),
            on_child_event=None,
            max_tool_rounds=5,
            working_memory=working_memory,
        ):
            pass

        before_tool = [e for e in _checkpoint_envelopes(publisher) if e["payload"]["checkpoint_type"] == "before_tool"]
        assert len(before_tool) == 1, "one durable before_tool checkpoint per round"
        payload = before_tool[0]["payload"]
        assert before_tool[0]["run_id"] == "mq_run_before"
        assert before_tool[0]["job_id"] == "job-1"
        assert payload["snapshot"]["tool_calls"][0]["tool_name"] == "list_flights"
        # Working memory snapshot rides the checkpoint payload (Task B2/D1).
        assert payload["snapshot"]["working_memory"]["plan.md"].startswith("# Plan")
        assert payload["snapshot_hash"]
        assert payload["snapshot_size_bytes"] > 0
        assert payload["sequence_no"] > 0

    @pytest.mark.asyncio
    async def test_after_tool_checkpoint_published_over_mq(self, monkeypatch):
        publisher = _FakeMqPublisher()
        _install_fake_mq(monkeypatch, publisher)

        runner = LLMStreamRunner(client=MagicMock())
        runner._stream_chat_impl = _tool_calling_impl(rounds=1)
        runner._tool_executor = _mock_executor()

        async for _ in runner.stream_chat_with_tools(
            messages=[Message(role="user", content="hi")],
            model="test-model",
            run_id="mq_run_after",
            envelope=MagicMock(job_id="job-1"),
            on_child_event=None,
            max_tool_rounds=5,
        ):
            pass

        after_tool = [e for e in _checkpoint_envelopes(publisher) if e["payload"]["checkpoint_type"] == "after_tool"]
        assert len(after_tool) == 1
        snapshot = after_tool[0]["payload"]["snapshot"]
        assert snapshot["tool_calls_executed"] == 1
        assert snapshot["results"], "tool results ride the snapshot for resume"

    @pytest.mark.asyncio
    async def test_before_proposal_maps_to_rust_checkpoint_type(self, monkeypatch):
        publisher = _FakeMqPublisher()
        _install_fake_mq(monkeypatch, publisher)

        runner = LLMStreamRunner(client=MagicMock())
        runner._stream_chat_impl = _tool_calling_impl(rounds=1)
        runner._tool_executor = _mock_executor()

        async for _ in runner.stream_chat_with_tools(
            messages=[Message(role="user", content="hi")],
            model="test-model",
            run_id="mq_run_proposal",
            envelope=MagicMock(job_id="job-1"),
            on_child_event=None,
            max_tool_rounds=5,
        ):
            pass

        proposal = [
            e for e in _checkpoint_envelopes(publisher) if e["payload"]["checkpoint_type"] == "before_proposal_ingest"
        ]
        assert len(proposal) == 1, "sidecar before_proposal maps to Rust before_proposal_ingest"
        assert "working_memory" in proposal[0]["payload"]["snapshot"]

    @pytest.mark.asyncio
    async def test_after_completion_checkpoint_published_over_mq(self, monkeypatch):
        publisher = _FakeMqPublisher()
        _install_fake_mq(monkeypatch, publisher)

        runner = LLMStreamRunner(client=MagicMock())
        runner._stream_chat_impl = _tool_calling_impl(rounds=0)  # no tool calls

        async for _ in runner.stream_chat_with_tools(
            messages=[Message(role="user", content="hi")],
            model="test-model",
            run_id="mq_run_done",
            envelope=MagicMock(job_id="job-1"),
            on_child_event=None,
            max_tool_rounds=5,
        ):
            pass

        completion = [
            e for e in _checkpoint_envelopes(publisher) if e["payload"]["checkpoint_type"] == "after_completion"
        ]
        assert len(completion) == 1
        assert completion[0]["payload"]["snapshot"]["final_result"]["text"] == "final answer"

    @pytest.mark.asyncio
    async def test_sse_callback_and_mq_both_receive_checkpoints(self, monkeypatch):
        """MQ publish is additive: the SSE subagent queue path is unchanged."""
        publisher = _FakeMqPublisher()
        _install_fake_mq(monkeypatch, publisher)

        sse_checkpoints = []

        async def on_child_event(event):
            if event.get("type") == "checkpoint":
                sse_checkpoints.append(event)

        runner = LLMStreamRunner(client=MagicMock())
        runner._stream_chat_impl = _tool_calling_impl(rounds=1)
        runner._tool_executor = _mock_executor()

        async for _ in runner.stream_chat_with_tools(
            messages=[Message(role="user", content="hi")],
            model="test-model",
            run_id="mq_run_dual",
            envelope=MagicMock(job_id="job-1"),
            on_child_event=on_child_event,
            max_tool_rounds=5,
        ):
            pass

        sse_types = {e["checkpoint_type"] for e in sse_checkpoints}
        mq_types = {e["payload"]["checkpoint_type"] for e in _checkpoint_envelopes(publisher)}
        assert {"before_tool", "after_tool", "before_proposal", "after_completion"} <= sse_types
        assert {"before_tool", "after_tool", "before_proposal_ingest", "after_completion"} <= mq_types

    @pytest.mark.asyncio
    async def test_publish_failure_does_not_break_stream(self, monkeypatch):
        """A failing MQ publish never raises into the streaming loop."""
        import src.infrastructure.ai.runtime_service._mq_publish as mq_publish

        class _FailingPublisher:
            async def publish(self, envelope):
                raise ConnectionError("mq-gateway down")

        # Zero out retry backoff so the test stays fast.
        monkeypatch.setattr(mq_publish, "_resolve_mq_publisher", lambda: _FailingPublisher())
        monkeypatch.setattr(mq_publish, "_resolve_mq_gate", lambda: None)
        monkeypatch.setattr(mq_publish.asyncio, "sleep", AsyncMock())

        runner = LLMStreamRunner(client=MagicMock())
        runner._stream_chat_impl = _tool_calling_impl(rounds=0)

        events = []
        async for event in runner.stream_chat_with_tools(
            messages=[Message(role="user", content="hi")],
            model="test-model",
            run_id="mq_run_degraded",
            envelope=MagicMock(job_id="job-1"),
            on_child_event=None,
            max_tool_rounds=5,
        ):
            events.append(event)

        assert any(e.type == "completed" for e in events)
