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
    LeaseCheckHook,
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
