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
        hard_max_tool_rounds: Non-configurable upper bound for this task type;
            even if the entity config sets a higher value, the runner caps at
            this number (Task B1 production safety).
        default_llm_summary: Default LLM summary policy for conversations of
            this task type (Task B3). On by default; query_ops opts out. A
            per-conversation ``enable_llm_summary`` override always wins.
        requires_plan_first: When True (high-risk templates, Task C1), the run
            exposes the plan-board tools and the first round must establish a
            plan via ``update_plan`` before any proposal-class write tool call
            is allowed (enforced by the PreToolUse PlanFirstHook).
    """

    task_type: str
    display_name: str
    system_prompt_addendum: str
    allowed_tool_categories: frozenset[str]
    denied_tools: frozenset[str]
    default_max_tool_rounds: int
    hard_max_tool_rounds: int
    default_llm_summary: bool = True
    requires_plan_first: bool = False


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


def resolve_budget_with_hard_cap(
    entity_max_rounds: int | None,
    template: TaskTemplate | None,
    production_default_hard_cap: int = 20,
) -> int:
    """Resolve effective max rounds with per-template hard cap and production default.

    Resolution rules (Task B1):
    - Unrecognized task_type → default 8, hard cap 12
    - query_ops → default 6, hard cap 8
    - anomaly_ops → default 12, hard cap 16
    - dispatch_ops → default 16, hard cap 20
    - Production default hard cap: 20 (entity-configurable via tooling.max_rounds,
      but runner always clamps at min(entity_max, production_default_hard_cap)).

    Args:
        entity_max_rounds: The entity's tooling.max_rounds setting (default 5 if absent).
        template: The matched TaskTemplate for this task_type (may be None).
        production_default_hard_cap: Global production safety cap (default 20).

    Returns:
        Effective max rounds: min(entity_max, template.hard_cap) capped at production_default_hard_cap.
    """
    # Unrecognized task fallback
    if template is None:
        effective_default = 8
        effective_hard_cap = 12
    else:
        effective_default = template.default_max_tool_rounds
        effective_hard_cap = template.hard_max_tool_rounds

    # Entity config can raise up to its own hard cap, then we intersect with the template's hard cap
    # If entity value is lower than template default, use the template default (never go below)
    candidate = max(entity_max_rounds or effective_default, effective_default) if entity_max_rounds is not None else effective_default
    intersected = min(candidate, effective_hard_cap)

    # Final clamp with production hard cap
    return min(intersected, production_default_hard_cap)


__all__ = ["TaskTemplate", "template_allows_tool"]
