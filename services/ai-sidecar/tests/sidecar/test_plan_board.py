"""Tests for PlanBoardTools (Task C1).

Asserts:
1. update_plan creates/manages execution plans
2. complete_plan_step transitions step status correctly
3. list_plan_steps returns full plan state
4. Plans persist across tool calls within same run
5. Template integration works (anomaly_ops/dispatch_ops default to planning)
"""

from __future__ import annotations

import pytest

from src.infrastructure.ai.context_envelope import (
    ContextEnvelope,
    EnvelopeContext,
    EnvelopeLimits,
    EnvelopeOntology,
    EnvelopeRequester,
    EnvelopeTask,
)
from src.infrastructure.ai.tools.plan_tools import PlanBoardTools


def _make_test_envelope(task_type: str, run_id: str) -> ContextEnvelope:
    """Helper function to create test envelope."""
    return ContextEnvelope(
        job_id=f"job-{run_id}",
        run_id=run_id,
        entity_id="test_entity",
        requester=EnvelopeRequester(user_id="test_user", permissions=["read"]),
        ontology=EnvelopeOntology(),
        context=EnvelopeContext(limits=EnvelopeLimits(redaction="standard")),
        task=EnvelopeTask(task_type=task_type, user_message=f"Test message for {task_type}"),
    )


# ============================================================================
# Test Classes Start
# ============================================================================


class TestPlanCreation:
    """Test plan creation and management."""

    @pytest.mark.asyncio
    async def test_update_plan_creates_new_plan(self):
        """Creating a plan when none exists."""
        board = PlanBoardTools()
        envelope = _make_test_envelope("query_ops", "test-run-1")

        result = await board.update_plan(
            envelope=envelope,
            plan_description="Investigate flight delays",
            steps=[
                {"id": "step_1", "description": "Check weather conditions"},
                {"id": "step_2", "description": "Review crew availability"},
            ],
        )

        assert result["status"] == "succeeded"
        assert result["plan_id"] == "plan-test-run-1"
        assert result["step_count"] == 2
        assert result["incomplete_steps"] == 2

    @pytest.mark.asyncio
    async def test_update_plan_adds_to_existing(self):
        """Adding steps to existing plan."""
        board = PlanBoardTools()
        envelope = _make_test_envelope("anomaly_ops", "test-run-2")

        # First call
        result1 = await board.update_plan(
            envelope=envelope,
            plan_description="Initial investigation",
            steps=[{"id": "step_1", "description": "Gather facts"}],
        )

        # Second call adds more steps
        result2 = await board.update_plan(
            envelope=envelope,
            plan_description="Expanded investigation",
            steps=[
                {"id": "step_2", "description": "Analyze data"},
                {"id": "step_3", "description": "Formulate hypothesis"},
            ],
        )

        assert result2["step_count"] == 3
        assert result2["incomplete_steps"] == 3

    @pytest.mark.asyncio
    async def test_update_plan_updates_existing_step(self):
        """Updating description of existing step."""
        board = PlanBoardTools()
        envelope = _make_test_envelope("general", "test-run-3")

        await board.update_plan(
            envelope=envelope,
            plan_description="Test plan",
            steps=[{"id": "step_1", "description": "Initial description"}],
        )

        result = await board.update_plan(
            envelope=envelope,
            plan_description="Updated plan",
            steps=[{"id": "step_1", "description": "Revised description"}],
        )

        assert result["step_count"] == 1
        # Step count remains same, description updated


class TestStepCompletion:
    """Test marking steps as completed."""

    @pytest.mark.asyncio
    async def test_complete_plan_step_marks_completed(self):
        """Completing a single step."""
        board = PlanBoardTools()
        envelope = _make_test_envelope("anomaly_ops", "test-run-4")

        # Create plan with 3 steps
        await board.update_plan(
            envelope=envelope,
            plan_description="Investigation",
            steps=[
                {"id": "s1", "description": "Step 1"},
                {"id": "s2", "description": "Step 2"},
                {"id": "s3", "description": "Step 3"},
            ],
        )

        # Complete first step
        result = await board.complete_plan_step(
            envelope=envelope,
            step_id="s1",
            result_summary="Completed successfully",
        )

        assert result["status"] == "succeeded"
        assert result["all_completed"] is False
        assert result["remaining_steps"] == 2

    @pytest.mark.asyncio
    async def test_complete_nonexistent_step_fails(self):
        """Completing step that doesn't exist."""
        board = PlanBoardTools()
        envelope = _make_test_envelope("general", "test-run-5")

        await board.update_plan(
            envelope=envelope,
            plan_description="Test",
            steps=[{"id": "s1", "description": "Only one step"}],
        )

        result = await board.complete_plan_step(
            envelope=envelope,
            step_id="nonexistent",
        )

        assert result["status"] == "failed"
        assert result["error_code"] == "STEP_NOT_FOUND"

    @pytest.mark.asyncio
    async def test_all_steps_completed_flag(self):
        """Flag set when all steps are done."""
        board = PlanBoardTools()
        envelope = _make_test_envelope("dispatch_ops", "test-run-6")

        # Create plan with 2 steps
        await board.update_plan(
            envelope=envelope,
            plan_description="Small task",
            steps=[
                {"id": "a", "description": "First"},
                {"id": "b", "description": "Second"},
            ],
        )

        # Complete both
        await board.complete_plan_step(envelope=envelope, step_id="a")
        result = await board.complete_plan_step(envelope=envelope, step_id="b")

        assert result["all_completed"] is True


class TestListSteps:
    """Test listing plan steps."""

    @pytest.mark.asyncio
    async def test_list_steps_returns_full_state(self):
        """Full plan breakdown returned."""
        board = PlanBoardTools()
        envelope = _make_test_envelope("query_ops", "test-run-7")

        await board.update_plan(
            envelope=envelope,
            plan_description="Query ops investigation",
            steps=[
                {"id": "q1", "description": "Check flight status", "assigned_to": "llm"},
                {"id": "q2", "description": "Verify gate assignment", "assigned_to": "tool"},
            ],
        )

        result = await board.list_plan_steps(envelope=envelope)

        assert result["total_steps"] == 2
        assert len(result["steps"]) == 2
        assert result["steps"][0]["id"] == "q1"
        assert result["steps"][0]["assigned_to"] == "llm"


class TestPlanPersistence:
    """Test plan persistence across tool calls."""

    @pytest.mark.asyncio
    async def test_plan_survives_multiple_calls(self):
        """Plan persists through alternating tool calls."""
        board = PlanBoardTools()
        envelope = _make_test_envelope("anomaly_ops", "test-run-8")

        # Create plan
        await board.update_plan(
            envelope=envelope,
            plan_description="Long investigation",
            steps=[
                {"id": "gather", "description": "Gather evidence"},
                {"id": "analyze", "description": "Analyze patterns"},
                {"id": "report", "description": "Generate report"},
            ],
        )

        # Simulate multiple iterations
        await board.complete_plan_step(envelope=envelope, step_id="gather", result_summary="Done")

        list_result = await board.list_plan_steps(envelope=envelope)
        assert list_result["incomplete_steps"] == 2

        await board.complete_plan_step(envelope=envelope, step_id="analyze")

        final_result = await board.list_plan_steps(envelope=envelope)
        assert final_result["incomplete_steps"] == 1


class TestTemplateIntegration:
    """Test integration with task templates."""

    @pytest.mark.asyncio
    async def test_anomaly_ops_recommended_planning_pattern(self):
        """Anomalyops workflow shows expected pattern."""
        board = PlanBoardTools()
        envelope = _make_test_envelope("anomaly_ops", "anomaly-test-1")

        # Step 1: Establish plan upfront
        initial = await board.update_plan(
            envelope=envelope,
            plan_description="Investigate anomalies using triage workflow",
            steps=[
                {
                    "id": "fact_gathering",
                    "description": "Collect factual information about anomalies",
                    "assigned_to": "llm",
                },
                {
                    "id": "hypothesis",
                    "description": "Form labeled hypotheses about root causes",
                    "assigned_to": "llm",
                },
                {
                    "id": "recommendation",
                    "description": "Provide recommendations subject to rule engine approval",
                    "assigned_to": "human",
                },
            ],
        )

        assert initial["step_count"] == 3

        # Step 2: LLM executes fact gathering
        fact_result = await board.complete_plan_step(
            envelope=envelope,
            step_id="fact_gathering",
            result_summary="Identified 2 gate conflict anomalies during peak hours",
        )

        assert fact_result["all_completed"] is False

        # Step 3: LLM moves to hypothesis formation
        hypothesis_result = await board.complete_plan_step(
            envelope=envelope,
            step_id="hypothesis",
            result_summary="Root cause: morning peak hour congestion at gates 50-60",
        )

        assert hypothesis_result["all_completed"] is False

    @pytest.mark.asyncio
    async def test_dispatch_ops_approval_workflow(self):
        """Dispatch ops follows approval-required pattern."""
        board = PlanBoardTools()
        envelope = _make_test_envelope("dispatch_ops", "dispatch-test-1")

        # Dispatch ops plan includes approval step
        await board.update_plan(
            envelope=envelope,
            plan_description="Optimize gate assignments with approvals",
            steps=[
                {
                    "id": "read_status",
                    "description": "Read current gate assignments",
                    "assigned_to": "tool",
                },
                {
                    "id": "compute_optimization",
                    "description": "Run OR-Tools optimization model",
                    "assigned_to": "tool",
                },
                {
                    "id": "prepare_proposal",
                    "description": "Create proposal for gate changes",
                    "assigned_to": "llm",
                },
                {
                    "id": "human_approval",
                    "description": "Await human approval before executing changes",
                    "assigned_to": "human",
                },
            ],
        )

        result = await board.list_plan_steps(envelope=envelope)

        # Verify human approval step exists
        approval_step = next((s for s in result["steps"] if s["id"] == "human_approval"), None)
        assert approval_step is not None
        assert approval_step["assigned_to"] == "human"


class TestToolSchemas:
    """Test tool schema definitions."""

    def test_update_plan_schema_structure(self):
        """update_plan has correct schema."""
        from src.infrastructure.ai.tools.plan_tools import PlanBoardTools

        schema = PlanBoardTools.UPDATE_PLAN_TOOL
        func = schema["function"]

        assert func["name"] == "update_plan"
        assert "execution plan" in func["description"].lower()

        params = func["parameters"]
        assert params["type"] == "object"
        assert "plan_description" in params["properties"]
        assert "steps" in params["properties"]

    def test_complete_step_schema_structure(self):
        """complete_plan_step has correct schema."""
        from src.infrastructure.ai.tools.plan_tools import PlanBoardTools

        schema = PlanBoardTools.COMPLETE_STEP_TOOL
        func = schema["function"]

        assert func["name"] == "complete_plan_step"
        params = func["parameters"]
        assert "step_id" in params.get("required", [])

    def test_list_steps_no_params(self):
        """list_plan_steps takes no parameters."""
        from src.infrastructure.ai.tools.plan_tools import PlanBoardTools

        schema = PlanBoardTools.LIST_STEPS_TOOL
        func = schema["function"]

        assert func["name"] == "list_plan_steps"
        assert not func["parameters"]["properties"]


# ============================================================================
# C1 integration: WorkingMemory plan state, template injection, plan-first hook
# ============================================================================

from unittest.mock import MagicMock

from src.infrastructure.ai.hooks.pipeline import HookContext, HookPipeline, PlanFirstHook
from src.infrastructure.ai.llm_stream_runner import LLMStreamRunner, StreamEvent
from src.infrastructure.ai.tools.plan_tools import (
    PLAN_TOOL_NAMES,
    execute_plan_tool,
    is_plan_tool,
    plan_schemas_for_task_type,
)
from src.infrastructure.ai.working_memory import WorkingMemory


class TestWorkingMemoryPlanIntegration:
    """Plan state lives in the run's WorkingMemory (single source of truth)."""

    @pytest.mark.asyncio
    async def test_update_plan_writes_working_memory_plan_md(self):
        """update_plan with a workspace stores plan_state and renders plan.md."""
        board = PlanBoardTools()
        envelope = _make_test_envelope("anomaly_ops", "wm-run-1")
        memory = WorkingMemory(run_id="wm-run-1")

        result = await board.update_plan(
            envelope=envelope,
            plan_description="Triage gate anomalies",
            steps=[
                {"id": "facts", "description": "Collect anomaly facts"},
                {"id": "hypothesis", "description": "Label root-cause hypotheses"},
            ],
            working_memory=memory,
        )

        assert result["status"] == "succeeded"
        plan_md = memory.read_plan()
        assert "Triage gate anomalies" in plan_md
        assert "facts" in plan_md and "hypothesis" in plan_md
        state = memory.read_plan_state()
        assert state is not None
        assert len(state["steps"]) == 2
        # The in-memory fallback was NOT used — no second source of truth.
        assert board._plans == {}

    @pytest.mark.asyncio
    async def test_plan_state_survives_checkpoint_roundtrip(self):
        """plan_state rides to_dict/from_dict and re-renders plan.md."""
        board = PlanBoardTools()
        envelope = _make_test_envelope("dispatch_ops", "wm-run-2")
        memory = WorkingMemory(run_id="wm-run-2")

        await board.update_plan(
            envelope=envelope,
            plan_description="Advise on gate changes",
            steps=[{"id": "read", "description": "Read current assignments"}],
            working_memory=memory,
        )
        await board.complete_plan_step(envelope=envelope, step_id="read", working_memory=memory)

        snapshot = memory.to_dict()
        assert snapshot["plan_state"] is not None

        restored = WorkingMemory.from_dict(snapshot)
        state = restored.read_plan_state()
        assert state["steps"][0]["status"] == "completed"
        assert "[x] read" in restored.read_plan()

    @pytest.mark.asyncio
    async def test_complete_step_without_plan_fails(self):
        """complete_plan_step on an empty workspace returns PLAN_NOT_FOUND."""
        board = PlanBoardTools()
        envelope = _make_test_envelope("anomaly_ops", "wm-run-3")
        memory = WorkingMemory(run_id="wm-run-3")

        result = await board.complete_plan_step(envelope=envelope, step_id="x", working_memory=memory)
        assert result["status"] == "failed"
        assert result["error_code"] == "PLAN_NOT_FOUND"


class TestPlanToolSchemasForTaskType:
    """Plan tools are injected only for plan-first (high-risk) templates."""

    def test_anomaly_ops_gets_plan_tools(self):
        names = [s["function"]["name"] for s in plan_schemas_for_task_type("anomaly_ops")]
        assert names == ["update_plan", "complete_plan_step", "list_plan_steps"]

    def test_dispatch_ops_gets_plan_tools(self):
        names = [s["function"]["name"] for s in plan_schemas_for_task_type("dispatch_ops")]
        assert names == ["update_plan", "complete_plan_step", "list_plan_steps"]

    def test_query_ops_gets_no_plan_tools(self):
        assert plan_schemas_for_task_type("query_ops") == []

    def test_unknown_task_type_gets_no_plan_tools(self):
        assert plan_schemas_for_task_type(None) == []
        assert plan_schemas_for_task_type("general") == []

    def test_templates_carry_requires_plan_first_flag(self):
        from src.infrastructure.ai.templates import (
            ANOMALY_OPS_TEMPLATE,
            DISPATCH_OPS_TEMPLATE,
            QUERY_OPS_TEMPLATE,
        )

        assert ANOMALY_OPS_TEMPLATE.requires_plan_first is True
        assert DISPATCH_OPS_TEMPLATE.requires_plan_first is True
        assert QUERY_OPS_TEMPLATE.requires_plan_first is False

    def test_is_plan_tool(self):
        for name in PLAN_TOOL_NAMES:
            assert is_plan_tool(name) is True
        assert is_plan_tool("assign_gate") is False


class TestPlanFirstHook:
    """PreToolUse plan-first enforcement for high-risk templates."""

    @pytest.mark.asyncio
    async def test_blocks_proposal_tool_without_plan(self):
        """Write/proposal tool denied when the workspace has no plan."""
        hook = PlanFirstHook()
        envelope = _make_test_envelope("anomaly_ops", "pf-run-1")
        memory = WorkingMemory(run_id="pf-run-1")

        ctx = HookContext(
            phase="PreToolUse",
            run_id="pf-run-1",
            tool_name="assign_gate",
            tool_args={"flight_id": "F1234"},
            envelope=envelope,
            working_memory=memory,
        )
        result = await hook.execute(ctx)

        assert result is False
        assert any("PLAN_REQUIRED" in e for e in ctx.errors)

    @pytest.mark.asyncio
    async def test_allows_proposal_tool_after_update_plan(self):
        """Once update_plan wrote plan.md, proposal tools are allowed."""
        hook = PlanFirstHook()
        envelope = _make_test_envelope("anomaly_ops", "pf-run-2")
        memory = WorkingMemory(run_id="pf-run-2")

        board = PlanBoardTools()
        await board.update_plan(
            envelope=envelope,
            plan_description="Triage",
            steps=[{"id": "s1", "description": "Collect facts"}],
            working_memory=memory,
        )

        ctx = HookContext(
            phase="PreToolUse",
            run_id="pf-run-2",
            tool_name="assign_gate",
            tool_args={"flight_id": "F1234"},
            envelope=envelope,
            working_memory=memory,
        )
        assert await hook.execute(ctx) is True

    @pytest.mark.asyncio
    async def test_ignores_query_ops_and_read_only_tools(self):
        """query_ops is not plan-first; read-only tools are never gated."""
        hook = PlanFirstHook()

        query_ctx = HookContext(
            phase="PreToolUse",
            run_id="pf-run-3",
            tool_name="assign_gate",
            tool_args={},
            envelope=_make_test_envelope("query_ops", "pf-run-3"),
            working_memory=WorkingMemory(run_id="pf-run-3"),
        )
        assert await hook.execute(query_ctx) is True

        readonly_ctx = HookContext(
            phase="PreToolUse",
            run_id="pf-run-4",
            tool_name="list_flights",
            tool_args={},
            envelope=_make_test_envelope("anomaly_ops", "pf-run-4"),
            working_memory=WorkingMemory(run_id="pf-run-4"),
        )
        assert await hook.execute(readonly_ctx) is True

    @pytest.mark.asyncio
    async def test_plan_tools_themselves_never_blocked(self):
        hook = PlanFirstHook()
        ctx = HookContext(
            phase="PreToolUse",
            run_id="pf-run-5",
            tool_name="update_plan",
            tool_args={"plan_description": "x"},
            envelope=_make_test_envelope("dispatch_ops", "pf-run-5"),
            working_memory=WorkingMemory(run_id="pf-run-5"),
        )
        assert await hook.execute(ctx) is True


class _FakeExecutor:
    """Minimal executor stand-in (no MQ gate)."""

    mq_gate = None

    def __init__(self):
        self.calls: list[list[dict]] = []

    async def execute_batch(self, parsed_calls, **kwargs):
        from src.infrastructure.ai.tools.tool_executor import ToolExecutionResult

        self.calls.append(list(parsed_calls))
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


class TestPlanFirstRunnerIntegration:
    """End-to-end: plan tools execute in-process; plan-first gating in the loop."""

    @pytest.mark.asyncio
    async def test_update_plan_executes_against_working_memory(self):
        """A model-issued update_plan call writes plan.md without the executor."""
        runner = LLMStreamRunner(client=MagicMock())
        runner._stream_chat_impl = _scripted_impl(
            [
                {
                    "tool_calls": [
                        {
                            "id": "t1",
                            "function": {
                                "name": "update_plan",
                                "arguments": '{"plan_description": "Triage plan", "steps": [{"id": "s1", "description": "Gather facts"}]}',
                            },
                        }
                    ],
                    "text": "",
                },
                {"tool_calls": [], "text": "计划已建立"},
            ]
        )
        executor = _FakeExecutor()
        runner._tool_executor = executor
        memory = WorkingMemory(run_id="plan-run-1")
        envelope = _make_test_envelope("anomaly_ops", "plan-run-1")

        events = []
        async for event in runner.stream_chat_with_tools(
            messages=[],
            model="test-model",
            run_id="plan-run-1",
            envelope=envelope,
            max_tool_rounds=5,
            working_memory=memory,
            hook_pipeline=HookPipeline(),
        ):
            events.append(event)

        # Plan tool never reached the executor; plan.md holds the plan.
        assert executor.calls == []
        assert "Triage plan" in memory.read_plan()
        tool_results = [e for e in events if e.type == "tool_result"]
        assert tool_results and tool_results[0].tool_call.get("result", {}).get("status") == "succeeded"

    @pytest.mark.asyncio
    async def test_proposal_blocked_until_plan_exists_then_allowed(self):
        """assign_gate blocked without a plan; allowed after update_plan ran."""
        pipeline = HookPipeline()
        pipeline.register_hook(PlanFirstHook())

        runner = LLMStreamRunner(client=MagicMock())
        runner._stream_chat_impl = _scripted_impl(
            [
                # Round 1: model jumps straight to a proposal-class write.
                {
                    "tool_calls": [
                        {
                            "id": "t1",
                            "function": {"name": "assign_gate", "arguments": '{"flight_id": "F1234", "gate": "A12"}'},
                        }
                    ],
                    "text": "",
                },
                # Round 2: model complies and establishes the plan.
                {
                    "tool_calls": [
                        {
                            "id": "t2",
                            "function": {
                                "name": "update_plan",
                                "arguments": '{"plan_description": "Gate change plan", "steps": [{"id": "p1", "description": "Verify constraints"}]}',
                            },
                        }
                    ],
                    "text": "",
                },
                # Round 3: proposal tool is now allowed through the hook.
                {
                    "tool_calls": [
                        {
                            "id": "t3",
                            "function": {"name": "assign_gate", "arguments": '{"flight_id": "F1234", "gate": "A12"}'},
                        }
                    ],
                    "text": "",
                },
                {"tool_calls": [], "text": "已提交提案待审批"},
            ]
        )
        executor = _FakeExecutor()
        runner._tool_executor = executor
        memory = WorkingMemory(run_id="plan-run-2")
        envelope = _make_test_envelope("anomaly_ops", "plan-run-2")

        events = []
        async for event in runner.stream_chat_with_tools(
            messages=[],
            model="test-model",
            run_id="plan-run-2",
            envelope=envelope,
            max_tool_rounds=6,
            working_memory=memory,
            hook_pipeline=pipeline,
        ):
            events.append(event)

        tool_results = [e for e in events if e.type == "tool_result"]
        # Round 1 assign_gate: blocked by PlanFirstHook, never executed.
        assert "PLAN_REQUIRED" in tool_results[0].tool_call.get("error", "")
        # Round 2 update_plan executed in-process.
        assert tool_results[1].tool_call.get("result", {}).get("status") == "succeeded"
        # Round 3 assign_gate passed the hook and reached the executor.
        assert tool_results[2].tool_call.get("result") == {"ok": True}
        assert len(executor.calls) == 1
        assert executor.calls[0][0]["tool_name"] == "assign_gate"


class TestExecutePlanTool:
    """Module-level dispatcher for plan tool calls."""

    @pytest.mark.asyncio
    async def test_execute_plan_tool_roundtrip(self):
        envelope = _make_test_envelope("anomaly_ops", "exec-run-1")
        memory = WorkingMemory(run_id="exec-run-1")

        result = await execute_plan_tool(
            "update_plan",
            {"plan_description": "Investigate", "steps": [{"id": "a", "description": "First"}]},
            envelope=envelope,
            working_memory=memory,
        )
        assert result["status"] == "succeeded"

        listed = await execute_plan_tool("list_plan_steps", {}, envelope=envelope, working_memory=memory)
        assert listed["total_steps"] == 1

        done = await execute_plan_tool("complete_plan_step", {"step_id": "a"}, envelope=envelope, working_memory=memory)
        assert done["all_completed"] is True

    @pytest.mark.asyncio
    async def test_execute_plan_tool_unknown_name(self):
        envelope = _make_test_envelope("general", "exec-run-2")
        result = await execute_plan_tool("nope", {}, envelope=envelope, working_memory=None)
        assert result["status"] == "failed"
        assert result["error_code"] == "UNKNOWN_PLAN_TOOL"
