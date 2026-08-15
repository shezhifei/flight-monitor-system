"""Shadow-mode comparison engine (Phase 1, Day 6-7).

Compares AI answers against operator (human) answers for the double-run
shadow loop, classifies discrepancies, and assigns severity per
docs/plans/AGENT_HANDOFF_GUIDE_PHASE_1_COMPLETE.md.

Discrepancy types:
- ``missing_information``: the AI answer omits facts the human covered.
- ``conflicting_data``: extracted facts disagree on the same key.
- ``over_confidence``: high AI confidence despite missing/conflicting facts.

Severity levels: critical > major > minor > informational.

This module is intentionally dependency-free (stdlib only) so it can run
in scripts, tests, and the ai-sidecar service without import side effects.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from datetime import datetime
from typing import Iterable

SEVERITY_ORDER = {"informational": 0, "minor": 1, "major": 2, "critical": 3}
DISCREPANCY_TYPES = ("missing_information", "conflicting_data", "over_confidence")

# Keys whose values are compared verbatim after normalisation.
_FACT_VALUE_RE = re.compile(r"[^0-9a-zA-Z\u4e00-\u9fff]+")

OVERCONFIDENCE_THRESHOLD = 0.85


@dataclass(frozen=True)
class ExtractedFact:
    """A single normalised fact extracted from an answer."""

    key: str
    value: str
    source_span: str = ""


@dataclass
class Discrepancy:
    discrepancy_type: str
    severity: str
    detail: str
    facts: list[ExtractedFact] = field(default_factory=list)


def extract_facts(answer: str) -> list[ExtractedFact]:
    """Extract normalised key/value facts from a free-text answer.

    Heuristic extraction: recognises ``key: value`` / ``key=value`` pairs
    and inline ``key 为 value`` (Chinese) patterns. Values are normalised
    by stripping punctuation/whitespace and lowercasing ASCII so that
    ``CA1234`` and ``ca 1234`` compare equal.
    """
    if not answer:
        return []
    facts: list[ExtractedFact] = []
    pair_re = re.compile(
        r"([0-9A-Za-z\u4e00-\u9fff][0-9A-Za-z\u4e00-\u9fff _/.\-]{0,40})\s*[:：=是为]\s*([^\n,，;；。]{1,80})"
    )
    for match in pair_re.finditer(answer):
        key = match.group(1).strip().lower()
        raw_value = match.group(2).strip()
        value = _FACT_VALUE_RE.sub("", raw_value).lower()
        if not key or not value:
            continue
        facts.append(ExtractedFact(key=key, value=value, source_span=match.group(0)))
    return facts


def _facts_by_key(facts: Iterable[ExtractedFact]) -> dict[str, list[str]]:
    grouped: dict[str, list[str]] = {}
    for fact in facts:
        grouped.setdefault(fact.key, []).append(fact.value)
    return grouped


def find_missing_information(
    ai_facts: list[ExtractedFact], human_facts: list[ExtractedFact]
) -> list[ExtractedFact]:
    """Facts present in the human answer but absent from the AI answer."""
    ai_keys = _facts_by_key(ai_facts)
    missing: list[ExtractedFact] = []
    for fact in human_facts:
        if fact.key not in ai_keys or fact.value not in ai_keys[fact.key]:
            missing.append(fact)
    return missing


def find_conflicting_data(
    ai_facts: list[ExtractedFact], human_facts: list[ExtractedFact]
) -> list[tuple[ExtractedFact, ExtractedFact]]:
    """Fact keys where AI and human provide different values."""
    human_by_key = _facts_by_key(human_facts)
    conflicts: list[tuple[ExtractedFact, ExtractedFact]] = []
    seen_keys: set[str] = set()
    for ai_fact in ai_facts:
        if ai_fact.key in seen_keys:
            continue
        human_values = human_by_key.get(ai_fact.key)
        if human_values and ai_fact.value not in human_values:
            seen_keys.add(ai_fact.key)
            conflicts.append(
                (ai_fact, ExtractedFact(key=ai_fact.key, value="/".join(human_values)))
            )
    return conflicts


def determine_severity(
    discrepancy_type: str,
    ai_confidence: float | None = None,
) -> str:
    """Map a discrepancy type (+ confidence context) to a severity level."""
    if discrepancy_type == "conflicting_data":
        return "critical"
    if discrepancy_type == "missing_information":
        return "major"
    if discrepancy_type == "over_confidence":
        return "major"
    return "informational"


def find_highest_severity(discrepancies: Iterable[Discrepancy]) -> str:
    """Return the highest severity among discrepancies (informational if none)."""
    highest = "informational"
    for discrepancy in discrepancies:
        if SEVERITY_ORDER[discrepancy.severity] > SEVERITY_ORDER[highest]:
            highest = discrepancy.severity
    return highest


def compare_answers(
    question_text: str,
    ai_answer: str,
    human_answer: str,
    ai_confidence: float | None = None,
    generated_at: datetime | None = None,
) -> list[Discrepancy]:
    """Compare an AI answer against the human answer for one shadow session.

    Returns a list of :class:`Discrepancy`; an empty list means the answers
    agree (no discrepancies recorded).
    """
    del question_text, generated_at  # reserved for future scoring / audit
    ai_facts = extract_facts(ai_answer)
    human_facts = extract_facts(human_answer)

    discrepancies: list[Discrepancy] = []

    missing = find_missing_information(ai_facts, human_facts)
    if missing:
        discrepancies.append(
            Discrepancy(
                discrepancy_type="missing_information",
                severity=determine_severity("missing_information", ai_confidence),
                detail="; ".join(
                    f"{fact.key}={fact.value}" for fact in missing[:10]
                ),
                facts=missing,
            )
        )

    conflicts = find_conflicting_data(ai_facts, human_facts)
    if conflicts:
        discrepancies.append(
            Discrepancy(
                discrepancy_type="conflicting_data",
                severity=determine_severity("conflicting_data", ai_confidence),
                detail="; ".join(
                    f"{ai.key}: ai={ai.value} vs human={human.value}"
                    for ai, human in conflicts[:10]
                ),
                facts=[ai for ai, _ in conflicts],
            )
        )

    if (
        ai_confidence is not None
        and ai_confidence >= OVERCONFIDENCE_THRESHOLD
        and (missing or conflicts)
    ):
        discrepancies.append(
            Discrepancy(
                discrepancy_type="over_confidence",
                severity=determine_severity("over_confidence", ai_confidence),
                detail=(
                    f"ai_confidence={ai_confidence:.3f} despite "
                    f"{len(missing)} missing facts and {len(conflicts)} conflicts"
                ),
            )
        )

    return discrepancies


__all__ = [
    "DISCREPANCY_TYPES",
    "SEVERITY_ORDER",
    "Discrepancy",
    "ExtractedFact",
    "compare_answers",
    "determine_severity",
    "extract_facts",
    "find_conflicting_data",
    "find_highest_severity",
    "find_missing_information",
]