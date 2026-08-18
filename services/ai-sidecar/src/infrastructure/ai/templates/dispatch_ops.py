"""Dispatch-ops task template (hybrid agent Task A6).

Policy-only module for ``task_type=dispatch_ops``: read-only situational
awareness first, candidate options from the existing solver path
(OR-Tools / micro-model results surfaced as tool data), LLM ranking and
explanation, then high-risk changes as proposals awaiting approval. Applying
a schedule is forbidden for the model — solver output can only become an
OutputProposal executed by the Rust control plane
(docs/plans/2026-08-14-hybrid-agent-architecture.md, Task A6).
"""

from __future__ import annotations

from .base import TaskTemplate

# Reserved names for direct schedule-application actions. No such builtin
# tool exists today; the template denies them anyway so a future catalog
# addition can never surface a local "apply schedule" path on this task type.
_APPLY_SCHEDULE_TOOL_NAMES: frozenset[str] = frozenset(
    {
        "apply_schedule",
        "apply_dispatch_schedule",
    }
)

DISPATCH_OPS_TEMPLATE = TaskTemplate(
    task_type="dispatch_ops",
    display_name="派工建议",
    system_prompt_addendum=(
        "# Task Template: dispatch_ops (dispatch advisory)\n"
        "- Workflow order is enforced by the SolverFirst hook: (1) first call update_plan to lay "
        "out the advisory steps; (2) then ground the decision with `dispatch.list_solver_candidates` "
        "(deterministic solver candidates for the dispatch window) or ontology.explain_constraints "
        "(constraint check for the intended change); (3) only then rank the candidates, explain "
        "trade-offs, and submit high-risk changes as proposals via ontology.propose_action "
        "(awaiting approval, waiting_for_approval). Proposals without a solver or constraint result "
        "are blocked. Mark steps with complete_plan_step as you go.\n"
        "- You never apply a schedule. Solver output can only become a proposal executed by "
        "the Rust control plane after approval — never claim a schedule was applied or a crew "
        "was reassigned.\n"
        "- Cite evidence (tool name + object id) for every fact; label trade-off reasoning and "
        "assumptions explicitly.\n"
        "- Use ontology.lookup(entity_id) to fetch aircraft-gate compatibility constraints before proposing changes.\n"
    ),
    # Read-only situational surface: dispatch read models, flight state,
    # aggregate queries, anomaly conflicts, and ontology lookups (Task F5).
    allowed_tool_categories=frozenset({"dispatch_query", "flight", "query", "anomaly", "ontology"}),
    # Direct schedule application is forbidden (plan Task A6). Proposal-path
    # write actions (e.g. assign_gate) stay visible — the executor converts
    # them into OutputProposals for approval.
    denied_tools=_APPLY_SCHEDULE_TOOL_NAMES,
    default_max_tool_rounds=16,
    hard_max_tool_rounds=20,
    requires_plan_first=True,
)


__all__ = ["DISPATCH_OPS_TEMPLATE"]
