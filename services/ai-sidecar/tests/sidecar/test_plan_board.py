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
