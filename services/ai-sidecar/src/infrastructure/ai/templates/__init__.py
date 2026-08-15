"""Task template registry (hybrid agent Tasks A4–A6).

Maps envelope ``task.task_type`` values to their policy template. Unknown or
missing task types resolve to ``None`` — the run then keeps the resolved
entity tool face and the base system prompt unchanged.

Exported:
    get_task_template(task_type: str) -> TaskTemplate | None:
        Retrieve a template by task type.
    resolve_budget_with_hard_cap(entity_max_rounds, template) -> int:
        Resolve effective max rounds with per-template hard cap (Task B1).

Non-negotiable constraint: templates are POLICY ONLY — they never add or replace
the production execution loop (docs/architecture/AGENT_RUNTIME_LOOP.md).
"""

from __future__ import annotations

from .anomaly_ops import ANOMALY_OPS_TEMPLATE
from .base import TaskTemplate, resolve_budget_with_hard_cap, template_allows_tool
from .dispatch_ops import DISPATCH_OPS_TEMPLATE
from .query_ops import QUERY_OPS_TEMPLATE

_TASK_TEMPLATES: dict[str, TaskTemplate] = {
    QUERY_OPS_TEMPLATE.task_type: QUERY_OPS_TEMPLATE,
    ANOMALY_OPS_TEMPLATE.task_type: ANOMALY_OPS_TEMPLATE,
    DISPATCH_OPS_TEMPLATE.task_type: DISPATCH_OPS_TEMPLATE,
}


def get_task_template(task_type: str | None) -> TaskTemplate | None:
    """Return the template registered for ``task_type``, or ``None``."""
    key = (task_type or "").strip()
    return _TASK_TEMPLATES.get(key) if key else None


__all__ = [
    "ANOMALY_OPS_TEMPLATE",
    "DISPATCH_OPS_TEMPLATE",
    "QUERY_OPS_TEMPLATE",
    "TaskTemplate",
    "get_task_template",
    "resolve_budget_with_hard_cap",
    "template_allows_tool",
]
