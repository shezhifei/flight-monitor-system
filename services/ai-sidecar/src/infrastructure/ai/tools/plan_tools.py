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

import time
from dataclasses import dataclass, field
from typing import Any

from src.infrastructure.ai.context_envelope import ContextEnvelope

# Names of the no-op planning tools. They are executed in-process against the
# run's WorkingMemory (never through the MQ gate / executor pipeline).
PLAN_TOOL_NAMES: frozenset[str] = frozenset({"update_plan", "complete_plan_step", "list_plan_steps"})


def is_plan_tool(tool_name: str) -> bool:
    """Check if a tool name is one of the plan-board tools."""
    return tool_name in PLAN_TOOL_NAMES


def plan_schemas_for_task_type(task_type: str | None) -> list[dict[str, Any]]:
    """Return plan-tool schemas when the task template is plan-first.

    High-risk templates (``anomaly_ops`` / ``dispatch_ops``) expose
    ``update_plan`` / ``complete_plan_step`` / ``list_plan_steps``; other
    task types (e.g. ``query_ops``) get an empty list.
    """
    from src.infrastructure.ai.templates import get_task_template

    template = get_task_template(task_type)
    if template is None or not getattr(template, "requires_plan_first", False):
        return []
    return [dict(schema) for schema in PlanBoardTools.SCHEMA_TOOLS]


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
    UPDATE_PLAN_TOOL = {  # noqa: RUF012
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

    COMPLETE_STEP_TOOL = {  # noqa: RUF012
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

    LIST_STEPS_TOOL = {  # noqa: RUF012
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

    SCHEMA_TOOLS = [UPDATE_PLAN_TOOL, COMPLETE_STEP_TOOL, LIST_STEPS_TOOL]  # noqa: RUF012

    def __init__(self):
        # Legacy in-memory fallback, used only when no WorkingMemory workspace
        # is provided. In the production runner path the plan state lives in
        # WorkingMemory (plan_state + rendered plan.md) — a single source of
        # truth that also rides the checkpoint snapshot.
        self._plans: dict[str, dict[str, Any]] = {}

    # ---- state backend -------------------------------------------------

    @staticmethod
    def _plan_id(envelope: ContextEnvelope) -> str:
        return f"plan-{envelope.run_id}"

    def _load_state(self, envelope: ContextEnvelope, working_memory: Any | None) -> dict[str, Any] | None:
        if working_memory is not None:
            return working_memory.read_plan_state()
        return self._plans.get(self._plan_id(envelope))

    def _store_state(self, envelope: ContextEnvelope, working_memory: Any | None, state: dict[str, Any]) -> None:
        if working_memory is not None:
            working_memory.write_plan_state(state)
        else:
            self._plans[self._plan_id(envelope)] = state

    @staticmethod
    def _new_state(envelope: ContextEnvelope, plan_description: str) -> dict[str, Any]:
        return {
            "plan_id": PlanBoardTools._plan_id(envelope),
            "task_type": getattr(envelope.task, "task_type", "general"),
            "created_at": time.time(),
            "description": plan_description,
            "steps": [],
            "metadata": {},
        }

    @staticmethod
    def _incomplete_count(state: dict[str, Any]) -> int:
        return sum(1 for s in state["steps"] if s.get("status") != "completed")

    async def update_plan(
        self,
        envelope: ContextEnvelope,
        plan_description: str,
        steps: list[dict[str, Any]] | None = None,
        working_memory: Any | None = None,
    ) -> dict[str, Any]:
        """Update or create an execution plan.

        Args:
            envelope: Current run context
            plan_description: What the plan aims to accomplish
            steps: Optional list of step definitions to add
            working_memory: Optional WorkingMemory workspace; when given, the
                plan state is stored there (and rendered into ``plan.md``)
                instead of the in-memory fallback.

        Returns:
            Plan overview with step count and status
        """
        now = time.time()

        state = self._load_state(envelope, working_memory)
        if state is None:
            state = self._new_state(envelope, plan_description)
        state["description"] = plan_description

        # Add/update steps
        if steps:
            for step_def in steps:
                step_id = step_def.get("id", f"step-{len(state['steps'])}")
                existing = next((s for s in state["steps"] if s.get("id") == step_id), None)

                if existing:
                    # Update existing step
                    existing["description"] = step_def.get("description", existing.get("description", ""))
                    existing["assigned_to"] = step_def.get("assigned_to", existing.get("assigned_to"))
                else:
                    # Add new step
                    state["steps"].append(
                        {
                            "id": step_id,
                            "description": step_def.get("description", ""),
                            "status": "pending",
                            "assigned_to": step_def.get("assigned_to", "llm"),
                            "started_at": None,
                            "completed_at": None,
                            "error": None,
                        }
                    )

        self._store_state(envelope, working_memory, state)

        return {
            "status": "succeeded",
            "answer": f"Plan updated: {state['plan_id']} with {len(state['steps'])} steps",
            "plan_id": state["plan_id"],
            "step_count": len(state["steps"]),
            "incomplete_steps": self._incomplete_count(state),
            "metadata": {"updated_at": now},
        }

    async def complete_plan_step(
        self,
        envelope: ContextEnvelope,
        step_id: str,
        result_summary: str | None = None,
        errors: list[str] | None = None,
        working_memory: Any | None = None,
    ) -> dict[str, Any]:
        """Mark a plan step as completed.

        Args:
            envelope: Current run context
            step_id: ID of step to complete
            result_summary: How the step was accomplished
            errors: Any errors encountered
            working_memory: Optional WorkingMemory workspace (see update_plan)

        Returns:
            Updated plan status
        """
        state = self._load_state(envelope, working_memory)

        if not state:
            return {
                "status": "failed",
                "answer": f"No active plan found for {self._plan_id(envelope)}",
                "error_code": "PLAN_NOT_FOUND",
            }

        step = next((s for s in state["steps"] if s.get("id") == step_id), None)
        if not step:
            return {
                "status": "failed",
                "answer": f"Step {step_id} not found in plan",
                "error_code": "STEP_NOT_FOUND",
            }

        step["status"] = "completed"
        step["completed_at"] = time.time()
        if result_summary:
            step["error"] = None  # Clear any previous errors
        if errors:
            step["error"] = "; ".join(errors)

        self._store_state(envelope, working_memory, state)

        all_completed = all(s.get("status") == "completed" for s in state["steps"])
        return {
            "status": "succeeded",
            "answer": f"Step {step_id} completed",
            "plan_id": state["plan_id"],
            "step_id": step_id,
            "remaining_steps": self._incomplete_count(state),
            "all_completed": all_completed,
        }

    async def list_plan_steps(
        self,
        envelope: ContextEnvelope,
        working_memory: Any | None = None,
    ) -> dict[str, Any]:
        """List all steps in the current plan.

        Args:
            envelope: Current run context
            working_memory: Optional WorkingMemory workspace (see update_plan)

        Returns:
            Full plan breakdown with all steps
        """
        state = self._load_state(envelope, working_memory)

        if not state:
            return {
                "status": "success",
                "answer": "No active plan",
                "steps": [],
                "total_steps": 0,
                "incomplete_steps": 0,
            }

        all_completed = all(s.get("status") == "completed" for s in state["steps"])
        return {
            "status": "success",
            "answer": f"Plan {state['plan_id']} with {len(state['steps'])} steps",
            "plan_id": state["plan_id"],
            "steps": [
                {
                    "id": s.get("id"),
                    "description": s.get("description"),
                    "status": s.get("status"),
                    "assigned_to": s.get("assigned_to"),
                    "started_at": s.get("started_at"),
                    "completed_at": s.get("completed_at"),
                    "error": s.get("error"),
                }
                for s in state["steps"]
            ],
            "total_steps": len(state["steps"]),
            "incomplete_steps": self._incomplete_count(state),
            "all_completed": all_completed,
        }


def get_plan_board_tools() -> PlanBoardTools:
    """Get singleton instance of PlanBoardTools."""
    from src.infrastructure.ai.tool_registry import get_tool_executor

    executor = get_tool_executor()
    if not hasattr(executor, "_plan_board"):
        executor._plan_board = PlanBoardTools()
    return executor._plan_board


_FALLBACK_BOARD: PlanBoardTools | None = None


def _get_fallback_board() -> PlanBoardTools:
    """Module-level board used when no WorkingMemory workspace is available."""
    global _FALLBACK_BOARD
    if _FALLBACK_BOARD is None:
        _FALLBACK_BOARD = PlanBoardTools()
    return _FALLBACK_BOARD


async def execute_plan_tool(
    tool_name: str,
    arguments: dict[str, Any],
    *,
    envelope: ContextEnvelope,
    working_memory: Any | None = None,
) -> dict[str, Any]:
    """Execute a plan-board tool call in-process.

    Plan tools are no-op planning primitives: they never touch the MQ gate,
    the executor, or business tables. When ``working_memory`` is provided the
    plan state is stored in the run's workspace (rendered into ``plan.md``).
    """
    board = _get_fallback_board()
    if tool_name == "update_plan":
        return await board.update_plan(
            envelope=envelope,
            plan_description=str(arguments.get("plan_description") or ""),
            steps=arguments.get("steps"),
            working_memory=working_memory,
        )
    if tool_name == "complete_plan_step":
        return await board.complete_plan_step(
            envelope=envelope,
            step_id=str(arguments.get("step_id") or ""),
            result_summary=arguments.get("result_summary"),
            errors=arguments.get("errors"),
            working_memory=working_memory,
        )
    if tool_name == "list_plan_steps":
        return await board.list_plan_steps(envelope=envelope, working_memory=working_memory)
    return {
        "status": "failed",
        "answer": f"Unknown plan tool: {tool_name}",
        "error_code": "UNKNOWN_PLAN_TOOL",
    }


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
    "PLAN_TOOL_NAMES",
    "ExecutionPlan",
    "PlanBoardTools",
    "PlanStep",
    "execute_plan_tool",
    "get_plan_board_tools",
    "is_plan_tool",
    "plan_schemas_for_task_type",
    "register_plan_tools",
]
