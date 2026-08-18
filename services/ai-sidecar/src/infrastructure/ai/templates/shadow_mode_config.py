"""Shadow-mode safety constraints for the ``query_ops`` template.

These knobs implement the strict read-only posture required for Phase 1
shadow deployment (docs/plans/SHADOW_MODE_DEPLOYMENT_STRATEGY.md, Day 1-3):

- ``READ_ONLY_MODE``: hard guarantee — the template denies every write
  action tool; the comparison/feedback loop must never mutate production
  state.
- ``MAX_TOOL_ROUNDS``: ceiling for shadow runs (also enforced by
  ``production_default_hard_cap`` in ``base.py``).
- ``FANOUT_DEPTH``: no branching tool chains — a shadow answer must stay
  auditable step by step.
- ``REQUIRE_EVIDENCE_COVERAGE``: every factual claim must cite evidence
  metadata (source/object_id/as_of) per P1-1-A.
- ``TOOL_FRESHNESS_LIMITS``: per-tool max age (seconds), keyed by the real
  production tool names. Shared by the ``FreshnessCheckHook`` PostToolUse
  hook and shadow evaluation (Task H1) — exactly one definition.
- ``HIGH_RISK_KEYWORDS``: questions containing these phrases are excluded
  from shadow evaluation (Go/No-Go safety rule).
"""

from __future__ import annotations

from typing import Any

READ_ONLY_MODE: bool = True
MAX_TOOL_ROUNDS: int = 20
FANOUT_DEPTH: int = 0
REQUIRE_EVIDENCE_COVERAGE: bool = True

#: Max evidence age (seconds) per read-only query tool. Keys are real
#: production tool names; ``ontology.lookup`` dispatches on the entity
#: namespace of its ``entity_id`` argument (``<prefix>:<object_id>``).
TOOL_FRESHNESS_LIMITS: dict[str, int] = {
    "ontology.lookup.flight": 30,   # flight status changes rapidly
    "ontology.lookup.stand": 10,    # stand assignments are the most dynamic
    "ontology.lookup.dispatch": 60,  # dispatch read-only
    "ontology.lookup.anomaly": 60,
    "ontology.lookup": 30,           # fallback for unrecognized namespaces
    "flight_status_lookup": 30,
    "get_delayed_flights": 30,
    "dispatch.get_status": 60,
    "dispatch.list_solver_candidates": 60,
    "get_dispatch_order": 60,
    "get_dispatch_by_flight": 60,
    "get_dispatch_by_team": 60,
}


def resolve_freshness_limit(
    tool_name: str, tool_args: dict[str, Any] | None = None
) -> int | None:
    """Resolve the max evidence age for a tool call.

    Returns ``None`` for tools outside the governed set (plan / skill /
    propose tools are never freshness-gated). ``ontology.lookup`` picks the
    per-entity-namespace entry from the call's ``entity_id`` prefix.
    """
    if tool_name == "ontology.lookup":
        entity_id = str((tool_args or {}).get("entity_id") or "")
        namespace = entity_id.partition(":")[0]
        namespaced = f"ontology.lookup.{namespace}"
        if namespaced in TOOL_FRESHNESS_LIMITS:
            return TOOL_FRESHNESS_LIMITS[namespaced]
        return TOOL_FRESHNESS_LIMITS["ontology.lookup"]
    return TOOL_FRESHNESS_LIMITS.get(tool_name)

HIGH_RISK_KEYWORDS: tuple[str, ...] = (
    "取消航班",
    "紧急调度",
    "重大调整",
    "cancel flight",
    "emergency dispatch",
    "major adjustment",
)

__all__ = [
    "FANOUT_DEPTH",
    "HIGH_RISK_KEYWORDS",
    "MAX_TOOL_ROUNDS",
    "READ_ONLY_MODE",
    "REQUIRE_EVIDENCE_COVERAGE",
    "TOOL_FRESHNESS_LIMITS",
    "resolve_freshness_limit",
]
