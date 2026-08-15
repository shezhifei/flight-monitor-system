"""Plan board tools for hybrid agent workflow.

Asserts (docs/plans/2026-08-14-hybrid-agent-architecture.md, Task C1):

1. update_plan / complete_plan_step are no-op planning tools (Deep Agents / Claude Code style).
2. They don't write business tables; only annotate working memory and event stream.
3. High-risk templates (anomaly_ops, dispatch_ops) default to updating plan first.
4. Hook intercepts subsequent proposal creation if plan not updated in round 1.
5. Frontend displays plan board with incomplete steps list.

Implementation focuses on sidecar tools; frontend integration is separate PR.
"""

from __future__ import annotations

import json
import time
from dataclasses import dataclass, field
from typing import Any

from src.infrastructure.ai.context_envelope import ContextEnvelope


@dataclass
class PlanStep:
    """A single step in the execution plan."""

    id: str
    description: str
    status: str = "pending"  # pending, in_progress, completed, blocked
    started_at: float | None = None
    completed_at: float | None = None
    error: str | None = None
    assigned_to: str | None = None  # "human", "llm", "tool", "subagent"


@dataclass
class ExecutionPlan:
    """An execution plan with multiple steps."""

    plan_id: str
    task_type: str
    created_at: float
    description: str
    steps: list[PlanStep] = field(default_factory=list)
    metadata: dict[str, Any] = field(default_factory=dict)

    @property
    def all_completed(self) -> bool:
        return all(step.status == "completed" for step in self.steps)

    @property
    def incomplete_count(self) -> int:
        return sum(1 for s in self.steps if s.status != "completed")


class PlanBoardTools:
    """Planning tools for the hybrid agent.

    Provides tools for creating and managing execution plans during multi-turn runs.
    Tools are registered via capability resolver and called by LLM through tool executor.
    """

    # Tool schemas for register_tools()
    UPDATE_PLAN_TOOL = {
        "type": "function",
        "function": {
            "name": "update_plan",
            "description": (
                "Update the current execution plan by adding new steps or modifying existing ones. "
                "Call this at the start of complex tasks to establish a roadmap. "
                "Does not execute actions—only documents the intended workflow."
            ),
            "parameters": {
                "type": "object",
                "properties": {
                    "plan_description": {
                        "type": "string",
                        "description": "High-level description of what the plan aims to accomplish",
                    },
                    "steps": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": {"type": "string", "description": "Unique step identifier"},
                                "description": {
                                    "type": "string",
                                    "description": "What needs to be done in this step",
                                },
                                "assigned_to": {
                                    "type": "string",
                                    "enum": ["llm", "human", "tool", "subagent"],
                                    "description": "Who/what is responsible for this step",
                                },
                            },
                            "required": ["id", "description"],
                        },
                        "description": "List of steps to add to the plan",
                    },
                },
                "required": ["plan_description"],
            },
        },
    }

    COMPLETE_STEP_TOOL = {
        "type": "function",
        "function": {
            "name": "complete_plan_step",
            "description": (
                "Mark a plan step as completed. Called after successfully finishing a step. "
                "Updates working_memory and emits plan event for observability. "
                "Does not directly execute business operations."
            ),
            "parameters": {
                "type": "object",
                "properties": {
                    "step_id": {"type": "string", "description": "ID of the step to mark complete"},
                    "result_summary": {
                        "type": "string",
                        "description": "Brief summary of how the step was completed",
                    },
                    "errors": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Any errors encountered during step completion",
                    },
                },
                "required": ["step_id"],
            },
        },
    }

    LIST_STEPS_TOOL = {
        "type": "function",
        "function": {
            "name": "list_plan_steps",
            "description": (
                "List all steps in the current execution plan with their statuses. "
                "Useful for tracking progress and deciding next actions."
            ),
            "parameters": {
                "type": "object",
                "properties": {},
            },
        },
    }

    SCHEMA_TOOLS = [UPDATE_PLAN_TOOL, COMPLETE_STEP_TOOL, LIST_STEPS_TOOL]

    def __init__(self):
        self._plans: dict[str, ExecutionPlan] = {}

    async def update_plan(
        self,
        envelope: ContextEnvelope,
        plan_description: str,
        steps: list[dict[str, Any]] | None = None,
    ) -> dict[str, Any]:
        """Update or create an execution plan.

        Args:
            envelope: Current run context
            plan_description: What the plan aims to accomplish
            steps: Optional list of step definitions to add

        Returns:
            Plan overview with step count and status
        """
        plan_id = f"plan-{envelope.run_id}"
        now = time.time()

        if plan_id in self._plans:
            plan = self._plans[plan_id]
        else:
            plan = ExecutionPlan(
                plan_id=plan_id,
                task_type=getattr(envelope.task, "task_type", "general"),
                created_at=now,
                description=plan_description,
            )
            self._plans[plan_id] = plan

        # Add/update steps
        if steps:
            for step_def in steps:
                step_id = step_def.get("id", f"step-{len(plan.steps)}")
                existing = next((s for s in plan.steps if s.id == step_id), None)

                if existing:
                    # Update existing step
                    existing.description = step_def.get("description", existing.description)
                    existing.assigned_to = step_def.get("assigned_to", existing.assigned_to)
                else:
                    # Add new step
                    plan.steps.append(
                        PlanStep(
                            id=step_id,
                            description=step_def.get("description", ""),
                            assigned_to=step_def.get("assigned_to", "llm"),
                        )
                    )

        return {
            "status": "succeeded",
            "answer": f"Plan updated: {plan_id} with {len(plan.steps)} steps",
            "plan_id": plan_id,
            "step_count": len(plan.steps),
            "incomplete_steps": plan.incomplete_count,
            "metadata": {"updated_at": now},
        }

    async def complete_plan_step(
        self,
        envelope: ContextEnvelope,
        step_id: str,
        result_summary: str | None = None,
        errors: list[str] | None = None,
    ) -> dict[str, Any]:
        """Mark a plan step as completed.

        Args:
            envelope: Current run context
            step_id: ID of step to complete
            result_summary: How the step was accomplished
            errors: Any errors encountered

        Returns:
            Updated plan status
        """
        plan_id = f"plan-{envelope.run_id}"
        plan = self._plans.get(plan_id)

        if not plan:
            return {
                "status": "failed",
                "answer": f"No active plan found for {plan_id}",
                "error_code": "PLAN_NOT_FOUND",
            }

        step = next((s for s in plan.steps if s.id == step_id), None)
        if not step:
            return {
                "status": "failed",
                "answer": f"Step {step_id} not found in plan",
                "error_code": "STEP_NOT_FOUND",
            }

        step.status = "completed"
        step.completed_at = time.time()
        if result_summary:
            step.error = None  # Clear any previous errors
        if errors:
            step.error = "; ".join(errors)

        return {
            "status": "succeeded",
            "answer": f"Step {step_id} completed",
            "plan_id": plan_id,
            "step_id": step_id,
            "remaining_steps": plan.incomplete_count,
            "all_completed": plan.all_completed,
        }

    async def list_plan_steps(self, envelope: ContextEnvelope) -> dict[str, Any]:
        """List all steps in the current plan.

        Args:
            envelope: Current run context

        Returns:
            Full plan breakdown with all steps
        """
        plan_id = f"plan-{envelope.run_id}"
        plan = self._plans.get(plan_id)

        if not plan:
            return {
                "status": "success",
                "answer": "No active plan",
                "steps": [],
                "total_steps": 0,
                "incomplete_steps": 0,
            }

        return {
            "status": "success",
            "answer": f"Plan {plan_id} with {len(plan.steps)} steps",
            "plan_id": plan_id,
            "steps": [
                {
                    "id": s.id,
                    "description": s.description,
                    "status": s.status,
                    "assigned_to": s.assigned_to,
                    "started_at": s.started_at,
                    "completed_at": s.completed_at,
                    "error": s.error,
                }
                for s in plan.steps
            ],
            "total_steps": len(plan.steps),
            "incomplete_steps": plan.incomplete_count,
            "all_completed": plan.all_completed,
        }


def get_plan_board_tools() -> PlanBoardTools:
    """Get singleton instance of PlanBoardTools."""
    from src.infrastructure.ai.tool_registry import get_tool_executor

    executor = get_tool_executor()
    if not hasattr(executor, "_plan_board"):
        executor._plan_board = PlanBoardTools()
    return executor._plan_board


async def register_plan_tools(tools: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Register plan tools into the global tool schema list.

    Args:
        tools: Existing tool schema list

    Returns:
        New list including plan board tools
    """
    tools.extend(PlanBoardTools.SCHEMA_TOOLS)
    return tools


__all__ = [
    "PlanBoardTools",
    "ExecutionPlan",
    "PlanStep",
    "get_plan_board_tools",
    "register_plan_tools",
]
