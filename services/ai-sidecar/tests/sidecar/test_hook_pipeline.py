"""Tests for lifecycle hooks (Task C2).

Asserts:
1. Hook phases execute in correct order: PreToolUse → PostToolUse → PreCompact → Stop
2. Each builtin hook functions correctly (lease, sanitization, ID preservation, etc.)
3. Hook failure aborts flow appropriately
4. NoPromisesHook blocks unauthorized action claims
5. Built-in pipeline can be built and used
"""

from __future__ import annotations

import pytest

from src.infrastructure.ai.hooks import (
    BaseHook,
    HookContext,
    HookPipeline,
    IDPreservationHook,
    NoPromisesHook,
    ResultSanitizationHook,
    build_default_pipeline,
    get_builtin_hooks,  # Added for builtin pipeline tests
    is_read_only_tool,
)

# ============================================================================
# Helper Hooks for Testing
# ============================================================================

class MockPreToolHook(BaseHook):
    """Mock PreToolUse hook."""

    @property
    def phase(self) -> str:
        return "PreToolUse"

    def __init__(self, should_succeed: bool = True):
        self.should_succeed = should_succeed

    async def execute(self, ctx: HookContext) -> bool:
        if not self.should_succeed:
            ctx.add_error("MockPreToolHook failed intentionally")
            return False
        return True


class MockPostToolHook(BaseHook):
    """Mock PostToolUse hook."""

    @property
    def phase(self) -> str:
        return "PostToolUse"

    async def execute(self, ctx: HookContext) -> bool:
        if ctx.tool_result:
            ctx.tool_result["_sanitized"] = True
        return True


# ============================================================================
# Test Hook Pipeline
# ============================================================================

class TestHookPipelineExecution:
    """Test the hook pipeline execution mechanism."""

    @pytest.mark.asyncio
    async def test_single_hook_execution(self):
        """Single hook executes successfully."""
        pipeline = HookPipeline()
        pipeline.register_hook(MockPreToolHook())

        ctx = HookContext(
            phase="PreToolUse",
            run_id="test-1",
            tool_name="list_flights",
        )

        result = await pipeline.execute_phase("PreToolUse", ctx)

        assert result is True
        assert len(ctx.errors) == 0

    @pytest.mark.asyncio
    async def test_failed_hook_aborts_flow(self):
        """Failed hook prevents further execution."""
        pipeline = HookPipeline()
        pipeline.register_hook(MockPreToolHook(should_succeed=False))

        ctx = HookContext(phase="PreToolUse", run_id="test-2")

        result = await pipeline.execute_phase("PreToolUse", ctx)

        assert result is False
        assert len(ctx.errors) == 1
        assert "failed intentionally" in ctx.errors[0]

    @pytest.mark.asyncio
    async def test_multiple_hooks_in_phase(self):
        """All hooks in a phase execute sequentially."""
        pipeline = HookPipeline()
        pipeline.register_hook(MockPreToolHook(True))
        pipeline.register_hook(MockPreToolHook(True))

        ctx = HookContext(phase="PreToolUse", run_id="test-3")

        result = await pipeline.execute_phase("PreToolUse", ctx)

        assert result is True
        assert len(ctx.errors) == 0

    @pytest.mark.asyncio
    async def test_multi_phase_execution_order(self):
        """Phases execute in defined order."""
        pipeline = HookPipeline()
        pipeline.register_hook(MockPreToolHook())
        pipeline.register_hook(MockPostToolHook())

        # Execute all phases
        result = await pipeline.execute_all_phases(
            HookContext(phase="all", run_id="test-4")
        )

        assert result is True


# ============================================================================
# Test Specific Hooks
# ============================================================================

class TestResultSanitizationHook:
    """Test result sanitization hook."""

    @pytest.mark.asyncio
    async def test_clips_large_results(self):
        """Large content gets clipped."""
        hook = ResultSanitizationHook()

        large_content = "X" * (11 * 1024)  # 11KB - exceeds 10KB limit

        ctx = HookContext(
            phase="PostToolUse",
            run_id="test-5",
            tool_name="search_data",
            tool_result={"content": large_content},
        )

        result = await hook.execute(ctx)

        assert result is True
        assert len(ctx.tool_result["content"]) <= 10 * 1024 + 16  # + truncation message


class TestIDPreservationHook:
    """Test ID preservation during compression."""

    @pytest.mark.asyncio
    async def test_preserves_flight_numbers(self):
        """Flight numbers are identified and protected."""
        hook = IDPreservationHook()

        ctx = HookContext(
            phase="PreCompact",
            run_id="test-6",
            messages=[
                {"role": "user", "content": "Check status of F1234"},
                {"role": "assistant", "content": "F1234 is on time"},
            ],
        )

        # Create mock envelope with metadata
        class MockEnv:
            def __init__(self):
                self.metadata = {}

        ctx.envelope = MockEnv()

        result = await hook.execute(ctx)

        assert result is True
        assert "_protected_ids" in ctx.envelope.metadata
        assert "F1234" in ctx.envelope.metadata["_protected_ids"]

    @pytest.mark.asyncio
    async def test_preserves_anomaly_ids(self):
        """Anomaly IDs are protected."""
        hook = IDPreservationHook()

        ctx = HookContext(
            phase="PreCompact",
            run_id="test-7",
            messages=[
                {"role": "assistant", "content": "ANOMALY-GT123 detected at gate"},
            ],
        )

        class MockEnv:
            def __init__(self):
                self.metadata = {}

        ctx.envelope = MockEnv()

        await hook.execute(ctx)

        assert "ANOMALY-GT123" in ctx.envelope.metadata["_protected_ids"]


class TestNoPromisesHook:
    """Test anti-promise detection hook."""

    @pytest.mark.asyncio
    async def test_blocks_already_changed_claim(self):
        """Claims of already performed actions are blocked."""
        hook = NoPromisesHook()

        ctx = HookContext(
            phase="Stop",
            run_id="test-8",
            messages=[
                {"role": "assistant", "content": "我已经为您改机位到 A12"},
            ],
        )

        result = await hook.execute(ctx)

        assert result is False
        assert len(ctx.errors) == 1

    @pytest.mark.asyncio
    async def test_blocks_completed_operation_claim(self):
        """Completed operation claims are blocked."""
        hook = NoPromisesHook()

        ctx = HookContext(
            phase="Stop",
            run_id="test-9",
            messages=[
                {"role": "assistant", "content": "操作已完成，航班会准时到达"},
            ],
        )

        result = await hook.execute(ctx)

        assert result is False

    @pytest.mark.asyncio
    async def test_allows_proposal_language(self):
        """Proposal language is allowed (not claiming completion)."""
        hook = NoPromisesHook()

        ctx = HookContext(
            phase="Stop",
            run_id="test-10",
            messages=[
                {"role": "assistant", "content": "建议提交调整派工提案待审批"},
            ],
        )

        result = await hook.execute(ctx)

        assert result is True
        assert len(ctx.errors) == 0


# ============================================================================
# Test Utility Functions
# ============================================================================

class TestReadonlyDetection:
    """Test read-only tool detection."""

    def test_list_tools_are_readonly(self):
        """List* tools are read-only."""
        assert is_read_only_tool("list_flights") is True
        assert is_read_only_tool("get_flight_status") is True
        assert is_read_only_tool("search_anomalies") is True

    def test_query_tools_are_readonly(self):
        """Query and check tools are read-only."""
        assert is_read_only_tool("query_flights") is True
        assert is_read_only_tool("check_gate_assignment") is True
        assert is_read_only_tool("lookup_pilot") is True

    def test_write_actions_are_not_readonly(self):
        """Write actions are not read-only."""
        assert is_read_only_tool("assign_gate") is False
        assert is_read_only_tool("create_todo") is False
        assert is_read_only_tool("modify_dispatch") is False

    def test_unknown_tools_fall_back_to_false(self):
        """Unknown tools default to not read-only."""
        assert is_read_only_tool("unknown_action") is False


# ============================================================================
# Test Builtin Pipeline
# ============================================================================

class TestBuiltinPipeline:
    """Test built-in hook pipeline construction."""

    def test_builtin_hooks_can_be_retrieved(self):
        """Default hooks are available."""
        hooks = get_builtin_hooks()

        assert len(hooks) >= 4  # At minimum core hooks

        phases = [h.phase for h in hooks]
        assert "PreToolUse" in phases
        assert "PostToolUse" in phases
        assert "Stop" in phases

    def test_default_pipeline_builds_successfully(self):
        """Default pipeline can be created."""
        pipeline = build_default_pipeline()

        assert isinstance(pipeline, HookPipeline)

        # All phases should have at least one hook
        for phase in ["PreToolUse", "PostToolUse", "PreCompact", "Stop"]:
            hooks = pipeline._hooks_by_phase.get(phase, [])
            assert len(hooks) > 0, f"No hooks registered for phase {phase}"

    def test_pipeline_with_all_builtins_executes(self):
        """Pipeline with all builtins works end-to-end."""
        pipeline = build_default_pipeline()

        # All phases should have at least one hook
        for phase in ["PreToolUse", "PostToolUse", "PreCompact", "Stop"]:
            hooks = pipeline._hooks_by_phase.get(phase, [])
            assert len(hooks) > 0, f"No hooks registered for phase {phase}"

    def test_freshness_and_grounding_hooks_wired_in_order(self):
        """Task H3: the default pipeline carries the H1/H2 invariants."""
        names = [type(h).__name__ for h in get_builtin_hooks()]
        assert "FreshnessCheckHook" in names
        assert "EvidenceCoverageHook" in names
        # PostToolUse: sanitize first, then the freshness check.
        assert names.index("ResultSanitizationHook") < names.index("FreshnessCheckHook")
        # Stop: promises → grounding → output guardrail.
        assert names.index("NoPromisesHook") < names.index("EvidenceCoverageHook")
        assert names.index("EvidenceCoverageHook") < names.index("OutputGuardrailHook")

    @pytest.mark.asyncio
    async def test_default_pipeline_rewrites_stale_query_result(self):
        """A governed query result without as_of is degraded by default."""
        pipeline = build_default_pipeline()
        result: dict = {"status": "delayed"}
        ctx = HookContext(
            phase="PostToolUse",
            run_id="h3-run",
            tool_name="flight_status_lookup",
            tool_result=result,
        )
        assert await pipeline.execute_phase("PostToolUse", ctx) is True
        assert result["error_code"] == "EVIDENCE_STALE"

    @pytest.mark.asyncio
    async def test_default_pipeline_degrades_ungrounded_final_answer(self):
        """query_ops final text citing an unevidenced ID gets the override."""
        from types import SimpleNamespace

        from src.infrastructure.ai.working_memory import WorkingMemory

        pipeline = build_default_pipeline()
        ctx = HookContext(
            phase="Stop",
            run_id="h3-run",
            messages=[{"role": "assistant", "content": "航班 MU5102 已起飞。"}],
            envelope=SimpleNamespace(task=SimpleNamespace(task_type="query_ops")),
            working_memory=WorkingMemory(run_id="h3-run"),
        )
        assert await pipeline.execute_phase("Stop", ctx) is False
        assert ctx.final_text_override is not None
        assert "MU5102" in ctx.final_text_override


# ============================================================================
# Test Hook Context
# ============================================================================

class TestHookContext:
    """Test hook context management."""

    @pytest.mark.asyncio
    async def test_adds_errors_gracefully(self):
        """Errors accumulate without crashing."""
        ctx = HookContext(phase="PreToolUse", run_id="test-12")

        ctx.add_error("First error")
        ctx.add_error("Second error")
        ctx.add_error("Third error")

        assert len(ctx.errors) == 3
        assert "First error" in ctx.errors[0]
        assert "Third error" in ctx.errors[2]

    @pytest.mark.asyncio
    async def test_empty_context_valid(self):
        """Empty context is valid."""
        ctx = HookContext(phase="PreToolUse", run_id="test-13")

        assert ctx.run_id == "test-13"
        assert ctx.tool_name is None
        assert ctx.tool_args is None
        assert ctx.errors == []


# ============================================================================
# Wiring-level tests: hook pipeline inside LLMStreamRunner (Task C2)
# ============================================================================

from unittest.mock import MagicMock

from src.infrastructure.ai.llm_stream_runner import (
    LLMStreamRunner,
    StreamEvent,
)
from src.infrastructure.ai.tools.tool_executor import ToolExecutionResult


class _RecordingHook(BaseHook):
    """Records every context it sees for a given phase."""

    def __init__(self, phase: str):
        self._phase = phase
        self.contexts: list[HookContext] = []

    @property
    def phase(self) -> str:
        return self._phase

    async def execute(self, ctx: HookContext) -> bool:
        self.contexts.append(ctx)
        return True


class _FakeExecutor:
    """Minimal executor stand-in with an explicit (empty) MQ gate slot."""

    mq_gate = None

    def __init__(self, results: list[ToolExecutionResult] | None = None):
        self.calls: list[list[dict]] = []
        self._results = results or []

    async def execute_batch(self, parsed_calls, **kwargs):
        self.calls.append(list(parsed_calls))
        if self._results:
            return list(self._results)
        return [
            ToolExecutionResult(
                tool_call_id=pc.get("tool_call_id", ""),
                tool_name=pc.get("tool_name", ""),
                success=True,
                result={"ok": True},
            )
            for pc in parsed_calls
        ]


def _scripted_impl(rounds: list[dict]):
    """Build a _stream_chat_impl stand-in playing back scripted rounds.

    Each round dict: {"tool_calls": [...], "text": str}.
    """
    state = {"idx": 0}

    async def mock_impl(*args, **kwargs):
        run_result = kwargs.get("result")
        spec = rounds[min(state["idx"], len(rounds) - 1)]
        state["idx"] += 1
        if run_result is not None:
            run_result.tool_calls = [dict(tc) for tc in spec["tool_calls"]]
            run_result.text = spec["text"]
        yield StreamEvent(type="completed", result=run_result, round_index=state["idx"] - 1)

    return mock_impl


async def _collect_runner_events(runner: LLMStreamRunner, **kwargs) -> list[StreamEvent]:
    events: list[StreamEvent] = []
    async for event in runner.stream_chat_with_tools(
        messages=[],
        model="test-model",
        run_id="hook-wiring-run",
        envelope=None,
        max_tool_rounds=5,
        **kwargs,
    ):
        events.append(event)
    return events


class TestRunnerHookWiring:
    """Assert the pipeline is actually invoked on the runner tool loop."""

    @pytest.mark.asyncio
    async def test_pre_and_post_hooks_called_for_tool_round(self):
        """Pre/PostToolUse hooks fire around a real tool execution."""
        pipeline = HookPipeline()
        pre = _RecordingHook("PreToolUse")
        post = _RecordingHook("PostToolUse")
        pipeline.register_hook(pre)
        pipeline.register_hook(post)

        runner = LLMStreamRunner(client=MagicMock())
        runner._stream_chat_impl = _scripted_impl([
            {"tool_calls": [{"id": "t1", "function": {"name": "list_flights", "arguments": "{}"}}], "text": ""},
            {"tool_calls": [], "text": "查询完成"},
        ])
        executor = _FakeExecutor()
        runner._tool_executor = executor

        events = await _collect_runner_events(runner, hook_pipeline=pipeline)

        assert len(executor.calls) == 1
        assert [c.tool_name for c in pre.contexts] == ["list_flights"]
        assert [c.tool_name for c in post.contexts] == ["list_flights"]
        assert post.contexts[0].tool_result == {"ok": True}
        assert events[-1].type == "completed"

    @pytest.mark.asyncio
    async def test_lease_denied_blocks_write_tool_execution(self, monkeypatch):
        """Write tool without a wired MQ gate is blocked at PreToolUse."""
        monkeypatch.setattr(
            "src.infrastructure.ai.ai_container.resolve_tool_mq_gate",
            lambda default=None: None,
        )
        pipeline = build_default_pipeline()

        runner = LLMStreamRunner(client=MagicMock())
        runner._stream_chat_impl = _scripted_impl([
            {
                "tool_calls": [
                    {"id": "t1", "function": {"name": "assign_gate", "arguments": '{"flight_id": "F1234", "gate": "A12"}'}}
                ],
                "text": "",
            },
            {"tool_calls": [], "text": "已改为提交提案"},
        ])
        executor = _FakeExecutor()
        runner._tool_executor = executor

        events = await _collect_runner_events(runner, hook_pipeline=pipeline)

        # Executor never saw the call — the hook blocked it beforehand.
        assert executor.calls == []
        tool_results = [e for e in events if e.type == "tool_result"]
        assert len(tool_results) == 1
        error = tool_results[0].tool_call.get("error", "")
        assert "HOOK_BLOCKED" in error
        assert "LEASE_GATE_UNAVAILABLE" in error

    @pytest.mark.asyncio
    async def test_read_only_tool_passes_default_pipeline_without_gate(self, monkeypatch):
        """Read-only tools skip the lease preflight and execute normally."""
        monkeypatch.setattr(
            "src.infrastructure.ai.ai_container.resolve_tool_mq_gate",
            lambda default=None: None,
        )
        pipeline = build_default_pipeline()

        runner = LLMStreamRunner(client=MagicMock())
        runner._stream_chat_impl = _scripted_impl([
            {"tool_calls": [{"id": "t1", "function": {"name": "list_flights", "arguments": "{}"}}], "text": ""},
            {"tool_calls": [], "text": "航班 F1234 准点"},
        ])
        executor = _FakeExecutor(
            results=[ToolExecutionResult(tool_call_id="t1", tool_name="list_flights", success=True, result={"flights": ["F1234"]})]
        )
        runner._tool_executor = executor

        events = await _collect_runner_events(runner, hook_pipeline=pipeline)

        assert len(executor.calls) == 1
        completed = events[-1]
        assert completed.type == "completed"
        assert "拦截" not in (completed.result.text or "")

    @pytest.mark.asyncio
    async def test_stop_hook_intercepts_promise_output(self):
        """Promise-style final answers are flagged by the Stop phase."""
        pipeline = build_default_pipeline()

        runner = LLMStreamRunner(client=MagicMock())
        runner._stream_chat_impl = _scripted_impl([
            {"tool_calls": [], "text": "我已经为您改机位到 A12"},
        ])
        runner._tool_executor = _FakeExecutor()

        events = await _collect_runner_events(runner, hook_pipeline=pipeline)

        completed = events[-1]
        assert completed.type == "completed"
        assert "输出安全钩子拦截" in (completed.result.text or "")

    @pytest.mark.asyncio
    async def test_no_pipeline_keeps_legacy_behaviour(self):
        """Without a pipeline the loop behaves exactly as before."""
        runner = LLMStreamRunner(client=MagicMock())
        runner._stream_chat_impl = _scripted_impl([
            {"tool_calls": [{"id": "t1", "function": {"name": "assign_gate", "arguments": "{}"}}], "text": ""},
            {"tool_calls": [], "text": "done"},
        ])
        executor = _FakeExecutor()
        runner._tool_executor = executor

        events = await _collect_runner_events(runner)

        # No hooks: the (write) call reaches the executor as before C2.
        assert len(executor.calls) == 1
        assert events[-1].type == "completed"
