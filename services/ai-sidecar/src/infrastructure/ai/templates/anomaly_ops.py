"""Anomaly-ops task template (hybrid agent Task A5).

Policy-only module for ``task_type=anomaly_ops``: list anomaly/KPI facts
first, then clearly-labeled root-cause hypotheses, then recommendations.
Write actions are proposal-only — never executed by the model. The anomaly
rule engine keeps owning thresholds and the flagged list; the model only
explains and advises (docs/plans/2026-08-14-hybrid-agent-architecture.md,
Task A5).
"""

from __future__ import annotations

from .base import TaskTemplate

ANOMALY_OPS_TEMPLATE = TaskTemplate(
    task_type="anomaly_ops",
    display_name="异常研判",
    system_prompt_addendum=(
        "# Task Template: anomaly_ops (anomaly triage)\n"
        "- Workflow: first list anomaly / KPI facts via read-only tools, then root-cause "
        "HYPOTHESES explicitly labeled as hypotheses, then recommendations.\n"
        "- The anomaly rule engine owns thresholds and the flagged list. You explain and "
        "advise — never claim to have changed a rule, threshold, or disposition, and never "
        "decide thresholds yourself.\n"
        "- Cite evidence (tool name + object id) for every fact; mark assumptions as assumptions.\n"
        "- Write actions are proposal-only: submit them as proposals for approval through the "
        "control plane and never claim they were executed."
    ),
    # Read-only triage surface: anomaly events, flight details, KPI queries,
    # dispatch read models, and advisor knowledge retrieval.
    allowed_tool_categories=frozenset({"anomaly", "flight", "query", "dispatch_query", "advisor"}),
    # Write actions are NOT denied here (unlike query_ops): they stay visible
    # so the executor can turn them into OutputProposals (write_action_policy
    # = proposal_only). The model proposes; the Rust control plane executes.
    denied_tools=frozenset(),
    default_max_tool_rounds=12,
)


__all__ = ["ANOMALY_OPS_TEMPLATE"]
