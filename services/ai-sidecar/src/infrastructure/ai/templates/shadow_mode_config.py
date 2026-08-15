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
- ``TOOL_FRESHNESS_LIMITS``: mirrors ``FreshnessValidator.MAX_AGE_THRESHOLDS``
  for the shadow-relevant query tools (P1-1-B).
- ``HIGH_RISK_KEYWORDS``: questions containing these phrases are excluded
  from shadow evaluation (Go/No-Go safety rule).
"""

from __future__ import annotations

READ_ONLY_MODE: bool = True
MAX_TOOL_ROUNDS: int = 20
FANOUT_DEPTH: int = 0
REQUIRE_EVIDENCE_COVERAGE: bool = True

TOOL_FRESHNESS_LIMITS: dict[str, int] = {
    "flights.lookup": 30,
    "stands.current": 10,
    "dispatch_orders.by_flight": 60,
}

HIGH_RISK_KEYWORDS: tuple[str, ...] = (
    "取消航班",
    "紧急调度",
    "重大调整",
    "cancel flight",
    "emergency dispatch",
    "major adjustment",
)

__all__ = [
    "READ_ONLY_MODE",
    "MAX_TOOL_ROUNDS",
    "FANOUT_DEPTH",
    "REQUIRE_EVIDENCE_COVERAGE",
    "TOOL_FRESHNESS_LIMITS",
    "HIGH_RISK_KEYWORDS",
]