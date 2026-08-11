"""Shared helpers for normalizing todo priority input at application boundaries."""

from __future__ import annotations

from typing import Any

from src.domain.models.todo import TodoPriority

_PRIORITY_ALIASES = {
    TodoPriority.CRITICAL.value: TodoPriority.CRITICAL.value,
    TodoPriority.HIGH.value: TodoPriority.HIGH.value,
    TodoPriority.MEDIUM.value: TodoPriority.MEDIUM.value,
    TodoPriority.LOW.value: TodoPriority.LOW.value,
    TodoPriority.BACKGROUND.value: TodoPriority.BACKGROUND.value,
    "CRITICAL": TodoPriority.CRITICAL.value,
    "HIGH": TodoPriority.HIGH.value,
    "MEDIUM": TodoPriority.MEDIUM.value,
    "LOW": TodoPriority.LOW.value,
    "BACKGROUND": TodoPriority.BACKGROUND.value,
    "critical": TodoPriority.CRITICAL.value,
    "high": TodoPriority.HIGH.value,
    "medium": TodoPriority.MEDIUM.value,
    "low": TodoPriority.LOW.value,
    "background": TodoPriority.BACKGROUND.value,
}


def normalize_todo_priority(priority: Any, *, default: str = TodoPriority.MEDIUM.value) -> str:
    """Normalize external priority input into the domain canonical value."""
    if isinstance(priority, TodoPriority):
        return priority.value
    if priority is None:
        return default
    if isinstance(priority, str):
        normalized = _PRIORITY_ALIASES.get(priority.strip())
        if normalized is not None:
            return normalized
        return priority.strip()
    return str(priority)
