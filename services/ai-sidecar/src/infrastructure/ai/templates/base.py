"""Task template contract (hybrid agent Tasks A4–A6).

A task template is POLICY, not a loop: it shapes the system prompt, narrows
the visible tool face, and declares the round budget for one ``task_type``.
The production execution loop stays ``LLMStreamRunner.stream_chat_with_tools``
(docs/architecture/AGENT_RUNTIME_LOOP.md) — templates never add a second one.
"""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class TaskTemplate:
    """Immutable policy snapshot for one ``task_type``.

    Attributes:
        task_type: Envelope ``task.task_type`` value this template matches.
        display_name: Human-readable label (logs / UI).
        system_prompt_addendum: Policy block appended to the base system prompt.
        allowed_tool_categories: Tool categories visible to this task. Empty
            means "no category restriction". A template may only NARROW the
            resolved entity tool face, never widen it.
        denied_tools: Tool names always hidden from this task (write actions
            for read-only templates).
        default_max_tool_rounds: Default tool-loop budget (enforced by Task B1).
    """

    task_type: str
    display_name: str
    system_prompt_addendum: str
    allowed_tool_categories: frozenset[str]
    denied_tools: frozenset[str]
    default_max_tool_rounds: int


def template_allows_tool(
    template: TaskTemplate | None,
    *,
    tool_name: str,
    tool_category: str | None = None,
) -> bool:
    """Check a resolved tool against a task template.

    Fail-closed semantics: when a template restricts categories, a tool whose
    category is unknown/empty is hidden (never shown just because we cannot
    classify it). ``template=None`` (no template for this task_type) keeps the
    resolved face as-is.
    """
    if template is None:
        return True
    if tool_name in template.denied_tools:
        return False
    if template.allowed_tool_categories:
        category = (tool_category or "").strip()
        return category in template.allowed_tool_categories
    return True


__all__ = ["TaskTemplate", "template_allows_tool"]
