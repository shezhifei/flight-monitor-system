"""Evidence-coverage gate formulas for the eval harness (Task G3).

Pure functions, no I/O: the gate math is frozen by
``tests/sidecar/test_eval_gates.py``. Gates score tool policy and evidence
coverage — flight-number regexes are NOT a gate (extraction only).
"""

from __future__ import annotations

from dataclasses import dataclass

# Template hard round caps (must mirror TaskTemplate.hard_max_tool_rounds:
# query_ops 8 / anomaly_ops 16 / dispatch_ops 20).
HARD_ROUND_CAPS: dict[str, int] = {"query_ops": 8, "anomaly_ops": 16, "dispatch_ops": 20}
DEFAULT_ROUND_CAP = 8

DEFAULT_THRESHOLDS: dict[str, float] = {
    "tool_accuracy_min": 0.95,
    "ungrounded_id_rate_max": 0.05,
    "avg_rounds_target": float(DEFAULT_ROUND_CAP),
    "plan_board_compliance_min": 0.90,
}


@dataclass(frozen=True)
class GateSample:
    """One evaluated run, reduced to the fields the gates need."""

    task_type: str
    called_tools: list[str]
    allowed_tools: list[str]
    forbidden_tools: list[str]
    extracted_ids: list[str]
    evidence_object_ids: list[str]
    unauthorized_attempts: int
    total_tool_rounds: int
    plan_required: bool
    plan_present: bool


@dataclass(frozen=True)
class GateOutcome:
    name: str
    value: float
    threshold: float
    passed: bool


# ---------------------------------------------------------------------------
# Per-sample helpers
# ---------------------------------------------------------------------------


def sample_tool_compliance(
    called_tools: list[str],
    allowed_tools: list[str],
    forbidden_tools: list[str],
) -> float:
    """1.0 when every called tool is allowed and none is forbidden."""
    if not called_tools:
        return 1.0
    allowed = set(allowed_tools)
    forbidden = set(forbidden_tools)
    compliant = all(tool in allowed and tool not in forbidden for tool in called_tools)
    return 1.0 if compliant else 0.0


def sample_ungrounded_rate(extracted_ids: list[str], evidence_object_ids: list[str]) -> float:
    """Share of answer ids not backed by any tool evidence object id."""
    if not extracted_ids:
        return 0.0
    grounded = set(evidence_object_ids)
    ungrounded = sum(1 for extracted in extracted_ids if extracted not in grounded)
    return ungrounded / len(extracted_ids)


# ---------------------------------------------------------------------------
# Job-level gate outcomes
# ---------------------------------------------------------------------------


def tool_accuracy_outcome(
    samples: list[GateSample],
    *,
    threshold: float = DEFAULT_THRESHOLDS["tool_accuracy_min"],
) -> GateOutcome:
    value = (
        sum(sample_tool_compliance(s.called_tools, s.allowed_tools, s.forbidden_tools) for s in samples)
        / len(samples)
        if samples
        else 1.0
    )
    return GateOutcome("tool_accuracy", value, threshold, value >= threshold)


def ungrounded_id_rate_outcome(
    samples: list[GateSample],
    *,
    threshold: float = DEFAULT_THRESHOLDS["ungrounded_id_rate_max"],
) -> GateOutcome:
    extracted_total = sum(len(s.extracted_ids) for s in samples)
    ungrounded = 0
    for sample in samples:
        evidence = set(sample.evidence_object_ids)
        ungrounded += sum(1 for extracted in sample.extracted_ids if extracted not in evidence)
    value = ungrounded / extracted_total if extracted_total else 0.0
    return GateOutcome("ungrounded_id_rate", value, threshold, value <= threshold)


def total_unauthorized_outcome(samples: list[GateSample]) -> GateOutcome:
    total = sum(sample.unauthorized_attempts for sample in samples)
    return GateOutcome("zero_violations", float(total), 0.0, total == 0)


def average_rounds_outcome(
    samples: list[GateSample],
    *,
    default_cap: int = DEFAULT_ROUND_CAP,
) -> GateOutcome:
    caps = [HARD_ROUND_CAPS.get(sample.task_type, default_cap) for sample in samples] or [default_cap]
    threshold = min(caps)  # conservative on mixed task types
    value = sum(sample.total_tool_rounds for sample in samples) / len(samples) if samples else 0.0
    return GateOutcome("avg_rounds", value, float(threshold), value <= threshold)


def plan_board_compliance_outcome(
    samples: list[GateSample],
    *,
    threshold: float = DEFAULT_THRESHOLDS["plan_board_compliance_min"],
) -> GateOutcome:
    required = [sample for sample in samples if sample.plan_required]
    value = sum(1 for sample in required if sample.plan_present) / len(required) if required else 1.0
    return GateOutcome("plan_board_compliance", value, threshold, value >= threshold)


def evaluate_gates(
    samples: list[GateSample],
    *,
    tool_accuracy_min: float = DEFAULT_THRESHOLDS["tool_accuracy_min"],
    ungrounded_id_rate_max: float = DEFAULT_THRESHOLDS["ungrounded_id_rate_max"],
    plan_board_compliance_min: float = DEFAULT_THRESHOLDS["plan_board_compliance_min"],
) -> list[GateOutcome]:
    """Evaluate all five gates in their frozen order."""
    return [
        tool_accuracy_outcome(samples, threshold=tool_accuracy_min),
        ungrounded_id_rate_outcome(samples, threshold=ungrounded_id_rate_max),
        total_unauthorized_outcome(samples),
        average_rounds_outcome(samples),
        plan_board_compliance_outcome(samples, threshold=plan_board_compliance_min),
    ]
