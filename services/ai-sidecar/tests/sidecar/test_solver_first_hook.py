"""SolverFirst gate for dispatch_ops (Task I2).

Asserts:

1. ``SolverFirstHook`` (PreToolUse) blocks ``ontology.propose_action`` and
   every ``WRITE_ACTION_TOOLS`` member while the run has not yet produced a
   successful ``dispatch.list_solver_candidates`` (or
   ``ontology.explain_constraints``) result. Error surfaces as
   ``blocked_by=hook``, ``rule=SolverFirstHook`` via the pipeline.
2. The gate is scoped to ``task_type=dispatch_ops``; ``query_ops`` /
   ``anomaly_ops`` / unset task types are never gated.
3. ``SolverGateEvidenceHook`` (PostToolUse) records the satisfying result
   in the run's working-memory evidence chain — but never a failed or
   stale (``EVIDENCE_STALE``) one.
4. The dispatch_ops prompt orders the workflow: plan → solver/constraints
   → propose_action.
"""

from __future__ import annotations

from types import SimpleNamespace
from typing import Any

import pytest

from src.infrastructure.ai.hooks.pipeline import (
    HookContext,
    HookPipeline,
    SolverFirstHook,
    SolverGateEvidenceHook,
    build_default_pipeline,
    get_builtin_hooks,
)
from src.infrastructure.ai.tools.solver_tools import SOLVER_TOOL_NAME
from src.infrastructure.ai.working_memory import WorkingMemory

EXPLAIN_TOOL = "ontology.explain_constraints"
PROPOSE_TOOL = "ontology.propose_action"


def _envelope(task_type: str | None) -> Any:
    if task_type is None:
        return SimpleNamespace(task=None)
    return SimpleNamespace(task=SimpleNamespace(task_type=task_type))


def _pre_ctx(
    *,
    tool_name: str,
    task_type: str | None = "dispatch_ops",
    memory: WorkingMemory | None = None,
) -> HookContext:
    return HookContext(
        phase="PreToolUse",
        run_id="run_1",
        tool_name=tool_name,
        tool_args={},
        envelope=_envelope(task_type),
        working_memory=memory if memory is not None else WorkingMemory(run_id="run_1"),
    )


def _record_evidence(memory: WorkingMemory, source: str) -> None:
    memory.add_evidence(
        source=source,
        object_id="",
        summary=f"{source} ok",
        content="{}",
    )


# ---------------------------------------------------------------------------
# PreToolUse gating
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_blocks_propose_action_without_solver_evidence() -> None:
    ctx = _pre_ctx(tool_name=PROPOSE_TOOL)
    assert await SolverFirstHook().execute(ctx) is False
    assert any("SOLVER_FIRST" in e for e in ctx.errors)


@pytest.mark.asyncio
async def test_blocks_write_action_tools_without_solver_evidence() -> None:
    ctx = _pre_ctx(tool_name="assign_gate")
    assert await SolverFirstHook().execute(ctx) is False


@pytest.mark.asyncio
async def test_pipeline_reports_rule_solver_first_hook() -> None:
    pipeline = HookPipeline()
    pipeline.register_hook(SolverFirstHook())
    ctx = _pre_ctx(tool_name=PROPOSE_TOOL)
    assert await pipeline.execute_phase("PreToolUse", ctx) is False
    assert ctx.blocked_rule == "SolverFirstHook"


@pytest.mark.asyncio
async def test_allows_propose_action_after_solver_candidates() -> None:
    memory = WorkingMemory(run_id="run_1")
    _record_evidence(memory, SOLVER_TOOL_NAME)
    ctx = _pre_ctx(tool_name=PROPOSE_TOOL, memory=memory)
    assert await SolverFirstHook().execute(ctx) is True


@pytest.mark.asyncio
async def test_allows_write_tool_after_explain_constraints() -> None:
    memory = WorkingMemory(run_id="run_1")
    _record_evidence(memory, EXPLAIN_TOOL)
    ctx = _pre_ctx(tool_name="update_flight_status", memory=memory)
    assert await SolverFirstHook().execute(ctx) is True


@pytest.mark.asyncio
async def test_read_only_tools_are_never_gated() -> None:
    ctx = _pre_ctx(tool_name="get_delayed_flights")
    assert await SolverFirstHook().execute(ctx) is True


@pytest.mark.asyncio
async def test_plan_tools_are_never_gated() -> None:
    ctx = _pre_ctx(tool_name="update_plan")
    assert await SolverFirstHook().execute(ctx) is True


@pytest.mark.parametrize("task_type", ["query_ops", "anomaly_ops", None])
@pytest.mark.asyncio
async def test_gate_is_scoped_to_dispatch_ops(task_type: str | None) -> None:
    ctx = _pre_ctx(tool_name=PROPOSE_TOOL, task_type=task_type)
    assert await SolverFirstHook().execute(ctx) is True


# ---------------------------------------------------------------------------
# PostToolUse evidence recording
# ---------------------------------------------------------------------------


def _post_ctx(tool_name: str, tool_result: dict[str, Any]) -> HookContext:
    return HookContext(
        phase="PostToolUse",
        run_id="run_1",
        tool_name=tool_name,
        tool_args={},
        tool_result=tool_result,
        envelope=_envelope("dispatch_ops"),
        working_memory=WorkingMemory(run_id="run_1"),
    )


@pytest.mark.asyncio
async def test_recorder_records_successful_solver_result() -> None:
    ctx = _post_ctx(
        SOLVER_TOOL_NAME,
        {"source": SOLVER_TOOL_NAME, "as_of": "2026-08-18T00:00:00Z", "candidates": []},
    )
    assert await SolverGateEvidenceHook().execute(ctx) is True
    sources = [rec["source"] for rec in ctx.working_memory.read_evidence()]
    assert SOLVER_TOOL_NAME in sources


@pytest.mark.asyncio
async def test_recorder_records_successful_explain_constraints() -> None:
    ctx = _post_ctx(EXPLAIN_TOOL, {"constraints": [], "violations": []})
    assert await SolverGateEvidenceHook().execute(ctx) is True
    sources = [rec["source"] for rec in ctx.working_memory.read_evidence()]
    assert EXPLAIN_TOOL in sources


@pytest.mark.asyncio
async def test_recorder_ignores_failed_solver_result() -> None:
    ctx = _post_ctx(SOLVER_TOOL_NAME, {"content": "Error: SOLVER_SNAPSHOT_FAILED"})
    assert await SolverGateEvidenceHook().execute(ctx) is True
    assert ctx.working_memory.read_evidence() == []


@pytest.mark.asyncio
async def test_recorder_ignores_stale_result() -> None:
    ctx = _post_ctx(
        SOLVER_TOOL_NAME,
        {"ok": False, "error_code": "EVIDENCE_STALE", "detail": "missing as_of"},
    )
    assert await SolverGateEvidenceHook().execute(ctx) is True
    assert ctx.working_memory.read_evidence() == []


@pytest.mark.asyncio
async def test_recorder_ignores_unrelated_tools() -> None:
    ctx = _post_ctx("get_delayed_flights", {"flights": []})
    assert await SolverGateEvidenceHook().execute(ctx) is True
    assert ctx.working_memory.read_evidence() == []


@pytest.mark.asyncio
async def test_recorder_tolerates_missing_working_memory() -> None:
    ctx = _post_ctx(SOLVER_TOOL_NAME, {"source": SOLVER_TOOL_NAME, "candidates": []})
    ctx.working_memory = None
    assert await SolverGateEvidenceHook().execute(ctx) is True


# ---------------------------------------------------------------------------
# End-to-end: gate opens only after a satisfying result
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_gate_opens_after_successful_solver_round() -> None:
    pipeline = HookPipeline()
    pipeline.register_hook(SolverFirstHook())
    pipeline.register_hook(SolverGateEvidenceHook())
    memory = WorkingMemory(run_id="run_1")

    # Round 1: proposal without solver evidence → blocked.
    blocked_ctx = HookContext(
        phase="PreToolUse",
        run_id="run_1",
        tool_name=PROPOSE_TOOL,
        tool_args={},
        envelope=_envelope("dispatch_ops"),
        working_memory=memory,
    )
    assert await pipeline.execute_phase("PreToolUse", blocked_ctx) is False
    assert blocked_ctx.blocked_rule == "SolverFirstHook"

    # Round 2: solver candidates succeed → evidence recorded.
    post_ctx = HookContext(
        phase="PostToolUse",
        run_id="run_1",
        tool_name=SOLVER_TOOL_NAME,
        tool_args={},
        tool_result={"source": SOLVER_TOOL_NAME, "candidates": [{"object_id": "o-1"}]},
        envelope=_envelope("dispatch_ops"),
        working_memory=memory,
    )
    await pipeline.execute_phase("PostToolUse", post_ctx)

    # Round 3: proposal now passes the gate.
    open_ctx = HookContext(
        phase="PreToolUse",
        run_id="run_1",
        tool_name=PROPOSE_TOOL,
        tool_args={},
        envelope=_envelope("dispatch_ops"),
        working_memory=memory,
    )
    assert await pipeline.execute_phase("PreToolUse", open_ctx) is True


@pytest.mark.asyncio
async def test_stale_solver_result_does_not_open_gate() -> None:
    pipeline = HookPipeline()
    pipeline.register_hook(SolverFirstHook())
    pipeline.register_hook(SolverGateEvidenceHook())
    memory = WorkingMemory(run_id="run_1")

    post_ctx = HookContext(
        phase="PostToolUse",
        run_id="run_1",
        tool_name=SOLVER_TOOL_NAME,
        tool_args={},
        tool_result={"ok": False, "error_code": "EVIDENCE_STALE"},
        envelope=_envelope("dispatch_ops"),
        working_memory=memory,
    )
    await pipeline.execute_phase("PostToolUse", post_ctx)

    blocked_ctx = HookContext(
        phase="PreToolUse",
        run_id="run_1",
        tool_name=PROPOSE_TOOL,
        tool_args={},
        envelope=_envelope("dispatch_ops"),
        working_memory=memory,
    )
    assert await pipeline.execute_phase("PreToolUse", blocked_ctx) is False


# ---------------------------------------------------------------------------
# Default pipeline + template prompt ordering
# ---------------------------------------------------------------------------


def test_default_pipeline_contains_solver_first_hooks() -> None:
    hook_types = {type(hook) for hook in get_builtin_hooks()}
    assert SolverFirstHook in hook_types
    assert SolverGateEvidenceHook in hook_types
    assert isinstance(build_default_pipeline(), HookPipeline)


def test_dispatch_ops_prompt_orders_solver_before_propose() -> None:
    from src.infrastructure.ai.templates.dispatch_ops import DISPATCH_OPS_TEMPLATE

    prompt = DISPATCH_OPS_TEMPLATE.system_prompt_addendum
    assert DISPATCH_OPS_TEMPLATE.requires_plan_first is True
    solver_pos = prompt.find(SOLVER_TOOL_NAME)
    explain_pos = prompt.find("ontology.explain_constraints")
    propose_pos = prompt.find("ontology.propose_action")
    assert solver_pos != -1 and propose_pos != -1
    # plan → solver/constraints → propose_action
    assert prompt.find("update_plan") < min(solver_pos, explain_pos) < propose_pos
