"""Shared Todo graph pilot rollout constants."""

from __future__ import annotations

from collections.abc import Mapping
from typing import Any

DEFAULT_TODO_GRAPH_PILOT_ENTITY_ID = "todo_graph_pilot"
DEFAULT_TODO_GRAPH_PILOT_WINDOW_HOURS = 168
DEFAULT_TODO_GRAPH_PILOT_PENDING_STALE_AFTER_MINUTES = 30
DEFAULT_TODO_GRAPH_PILOT_REVIEW_INTERVAL_SECONDS = 600
DEFAULT_TODO_GRAPH_PILOT_ALERT_DEDUPE_SECONDS = 3600
DEFAULT_TODO_GRAPH_PILOT_ROLLBACK_VERIFY_DELAY_MINUTES = 15
DEFAULT_TODO_GRAPH_PILOT_DAILY_REVIEW_HOUR = 10
DEFAULT_TODO_GRAPH_PILOT_DAILY_REVIEW_MINUTE = 0
DEFAULT_TODO_GRAPH_PILOT_TIMEZONE = "Asia/Shanghai"
TODO_GRAPH_PILOT_RUNBOOK_REF = "docs/AI_TODO_GRAPH_PILOT_RUNBOOK.md"
TODO_GRAPH_PILOT_ROLLOUT_FLAG_KEYS = (
    "todo_agent_graph_enabled",
    "todo_agent_graph_runtime_enabled",
    "graph_runtime_enabled",
)


def is_default_todo_graph_pilot_entity(entity_id: Any) -> bool:
    """Return True when the provided entity belongs to the default graph pilot cohort."""
    return str(entity_id or "").strip() == DEFAULT_TODO_GRAPH_PILOT_ENTITY_ID


def resolve_todo_graph_pilot_rollout_override(
    entity_config: Mapping[str, Any] | None,
) -> bool | None:
    """Resolve the first explicit entity-level rollout override."""
    if not isinstance(entity_config, Mapping):
        return None

    for key in TODO_GRAPH_PILOT_ROLLOUT_FLAG_KEYS:
        if key not in entity_config:
            continue
        value = entity_config.get(key)
        if isinstance(value, bool):
            return value
        if value is None:
            return None

        normalized = str(value).strip().lower()
        if normalized in {"1", "true", "yes", "y", "on", "enabled"}:
            return True
        if normalized in {"0", "false", "no", "n", "off", "disabled"}:
            return False
        return None

    return None


def is_todo_graph_pilot_rollout_enabled(entity_config: Mapping[str, Any] | None) -> bool:
    """Return True when any entity-level pilot rollout switch is enabled."""
    return resolve_todo_graph_pilot_rollout_override(entity_config) is True


__all__ = [
    "DEFAULT_TODO_GRAPH_PILOT_ALERT_DEDUPE_SECONDS",
    "DEFAULT_TODO_GRAPH_PILOT_DAILY_REVIEW_HOUR",
    "DEFAULT_TODO_GRAPH_PILOT_DAILY_REVIEW_MINUTE",
    "DEFAULT_TODO_GRAPH_PILOT_ENTITY_ID",
    "DEFAULT_TODO_GRAPH_PILOT_PENDING_STALE_AFTER_MINUTES",
    "DEFAULT_TODO_GRAPH_PILOT_REVIEW_INTERVAL_SECONDS",
    "DEFAULT_TODO_GRAPH_PILOT_ROLLBACK_VERIFY_DELAY_MINUTES",
    "DEFAULT_TODO_GRAPH_PILOT_TIMEZONE",
    "DEFAULT_TODO_GRAPH_PILOT_WINDOW_HOURS",
    "TODO_GRAPH_PILOT_ROLLOUT_FLAG_KEYS",
    "TODO_GRAPH_PILOT_RUNBOOK_REF",
    "is_default_todo_graph_pilot_entity",
    "is_todo_graph_pilot_rollout_enabled",
    "resolve_todo_graph_pilot_rollout_override",
]
