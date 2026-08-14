"""Task template registry (hybrid agent Tasks A4–A6).

Maps envelope ``task.task_type`` values to their policy template. Unknown or
missing task types resolve to ``None`` — the run then keeps the resolved
entity tool face and the base system prompt unchanged.
"""

from __future__ import annotations

from .base import TaskTemplate, template_allows_tool
from .query_ops import QUERY_OPS_TEMPLATE

_TASK_TEMPLATES: dict[str, TaskTemplate] = {
    QUERY_OPS_TEMPLATE.task_type: QUERY_OPS_TEMPLATE,
}


def get_task_template(task_type: str | None) -> TaskTemplate | None:
    """Return the template registered for ``task_type``, or ``None``."""
    key = (task_type or "").strip()
    return _TASK_TEMPLATES.get(key) if key else None


__all__ = ["QUERY_OPS_TEMPLATE", "TaskTemplate", "get_task_template", "template_allows_tool"]
