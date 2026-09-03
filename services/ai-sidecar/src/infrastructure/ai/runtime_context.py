"""Read-only ontology context enhancement for RuntimeService.

This module provides a **read-only** enrichment layer that:
- Uses the Rust-owned ontology schema mirror and intent classification.
- Does NOT call write operations or directly access business databases.
- Degrades gracefully into limitations when dependencies are missing.

Design constraints (enforced by code, not just policy):
- No tool execution.
- No LangGraph full workflow.
- No direct DB writes.
"""

from __future__ import annotations

import logging
from collections.abc import Mapping
from dataclasses import dataclass, field
from typing import Any

from src.infrastructure.ai.context_envelope import ContextEnvelope
from src.infrastructure.ai.intent_router import classify_intent
from src.infrastructure.ai.structured_output import OutputEvidence, ReasoningStep

logger = logging.getLogger(__name__)


@dataclass
class EnrichmentResult:
    """Result of read-only context enrichment."""

    reasoning_steps: list[ReasoningStep] = field(default_factory=list)
    evidence: list[OutputEvidence] = field(default_factory=list)
    limitations: list[str] = field(default_factory=list)


def _get_schema_mirror() -> Any | None:
    try:
        from src.infrastructure.ai.ontology.schema_mirror import schema_mirror

        return schema_mirror
    except Exception as exc:  # noqa: BLE001 - schema mirror import guard
        logger.debug("runtime_context_schema_mirror_unavailable", exc_info=exc)
        return None


def _build_intent_reasoning(envelope: ContextEnvelope) -> ReasoningStep:
    intent = classify_intent(envelope.task.user_message, task_type=getattr(envelope.task, "task_type", None))
    return ReasoningStep(
        step="intent_classify",
        summary=f"Intent classified as '{intent}' from user message",
    )


def _enrich_from_schema_mirror(
    envelope: ContextEnvelope,
) -> tuple[list[ReasoningStep], list[OutputEvidence], list[str]]:
    steps: list[ReasoningStep] = []
    evidence: list[OutputEvidence] = []
    limitations: list[str] = []

    try:
        mirror = _get_schema_mirror()
    except Exception as exc:  # noqa: BLE001 - schema mirror access may fail in various ways
        logger.debug("runtime_context_schema_mirror_access_failed", exc_info=exc)
        limitations.append("Schema mirror unavailable")
        return steps, evidence, limitations

    if mirror is None:
        limitations.append("Schema mirror unavailable")
        return steps, evidence, limitations

    try:
        cache = mirror.get_cached_schema_snapshot()
    except Exception as exc:  # noqa: BLE001 - mirror implementations may fail independently
        logger.debug("runtime_context_schema_mirror_snapshot_failed", exc_info=exc)
        limitations.append("Schema mirror unavailable")
        return steps, evidence, limitations

    if not cache:
        limitations.append("Schema mirror cache empty (read-only, no fetch attempted)")
        return steps, evidence, limitations
    if not isinstance(cache, Mapping):
        limitations.append("Schema mirror cache invalid")
        return steps, evidence, limitations

    objects = cache.get("objects", {})
    if not isinstance(objects, Mapping):
        objects = {}
    for obj in envelope.context.objects:
        if obj.object_type in objects:
            evidence.append(
                OutputEvidence(
                    object_type=obj.object_type,
                    object_id=obj.object_id,
                    source="schema_mirror.snapshot",
                    field=None,
                )
            )

    actions = cache.get("actions", {})
    if not isinstance(actions, Mapping):
        actions = {}
    action_count = sum(action_key in actions for action_key in envelope.ontology.allowed_actions[:20])
    if action_count:
        steps.append(
            ReasoningStep(
                step="action_inventory",
                summary=f"Resolved {action_count} allowed action(s) from ontology schema mirror",
            )
        )
    elif envelope.ontology.allowed_actions:
        limitations.append("No allowed actions resolved from ontology schema mirror")

    return steps, evidence, limitations


def enhance_context(envelope: ContextEnvelope) -> EnrichmentResult:
    """Read-only enrichment of a ContextEnvelope using the Rust ontology schema mirror.

    Returns additional reasoning_steps and evidence.
    If dependencies are missing or schemas unavailable, returns limitations instead of raising.
    """
    result = EnrichmentResult()

    result.reasoning_steps.append(_build_intent_reasoning(envelope))

    mirror_steps, mirror_evidence, mirror_limitations = _enrich_from_schema_mirror(envelope)
    result.reasoning_steps.extend(mirror_steps)
    result.evidence.extend(mirror_evidence)
    result.limitations.extend(mirror_limitations)

    return result
