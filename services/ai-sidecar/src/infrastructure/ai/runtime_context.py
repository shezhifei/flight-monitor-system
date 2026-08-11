"""Read-only ontology/AIP context enhancement for RuntimeService.

This module provides a **read-only** enrichment layer that:
- Uses existing ontology/schema_mirror/intent_router capabilities.
- Does NOT call write operations or directly access business databases.
- Degrades gracefully into limitations when dependencies are missing.

Design constraints (enforced by code, not just policy):
- No tool execution.
- No LangGraph full workflow.
- No direct DB writes.
"""

from __future__ import annotations

import logging
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


def _get_ontology_registry() -> Any | None:
    try:
        from src.infrastructure.ai.ontology import get_ontology_registry

        return get_ontology_registry()
    except Exception as exc:  # noqa: BLE001 - ontology registry import guard
        logger.debug("runtime_context_ontology_registry_unavailable", exc_info=exc)
        return None


def _get_schema_mirror() -> Any | None:
    try:
        from src.infrastructure.ai.ontology.schema_mirror import schema_mirror

        return schema_mirror
    except Exception as exc:  # noqa: BLE001 - schema mirror import guard
        logger.debug("runtime_context_schema_mirror_unavailable", exc_info=exc)
        return None


def _build_intent_reasoning(envelope: ContextEnvelope) -> ReasoningStep:
    intent = classify_intent(envelope.task.user_message)
    return ReasoningStep(
        step="intent_classify",
        summary=f"Intent classified as '{intent}' from user message",
    )


def _enrich_from_ontology_objects(
    envelope: ContextEnvelope,
) -> tuple[list[OutputEvidence], list[str]]:
    evidence: list[OutputEvidence] = []
    limitations: list[str] = []

    try:
        registry = _get_ontology_registry()
    except Exception as exc:  # noqa: BLE001 - ontology registry access may fail in various ways
        logger.debug("runtime_context_ontology_registry_access_failed", exc_info=exc)
        limitations.append("Ontology registry unavailable")
        return evidence, limitations

    if registry is None:
        limitations.append("Ontology registry unavailable")
        return evidence, limitations

    for obj in envelope.context.objects:
        ont_obj = registry.get_object(obj.object_type)
        if ont_obj is None:
            limitations.append(f"Ontology object '{obj.object_type}' not found in registry")
            continue

        evidence.append(
            OutputEvidence(
                object_type=obj.object_type,
                object_id=obj.object_id,
                source=f"ontology.{obj.object_type.lower()}",
                field=None,
            )
        )

        for prop_name in list(obj.data.keys())[:10]:
            prop_def = None
            if hasattr(ont_obj, "get_property"):
                prop_def = ont_obj.get_property(prop_name)
            if prop_def is not None:
                evidence.append(
                    OutputEvidence(
                        object_type=obj.object_type,
                        object_id=obj.object_id,
                        source=f"ontology.property.{prop_name}",
                        field=prop_name,
                    )
                )

    return evidence, limitations


def _enrich_from_allowed_actions(
    envelope: ContextEnvelope,
) -> tuple[list[ReasoningStep], list[str]]:
    steps: list[ReasoningStep] = []
    limitations: list[str] = []

    try:
        registry = _get_ontology_registry()
    except Exception as exc:  # noqa: BLE001 - ontology registry access may fail in various ways
        logger.debug("runtime_context_ontology_registry_for_actions_failed", exc_info=exc)
        return steps, limitations

    if registry is None:
        return steps, limitations

    action_count = 0
    for action_key in envelope.ontology.allowed_actions[:20]:
        parts = action_key.split(".", 1)
        if len(parts) != 2:
            continue
        _obj_type, action_name = parts
        action_def = registry.get_action(action_name)
        if action_def is not None:
            action_count += 1

    if action_count:
        steps.append(
            ReasoningStep(
                step="action_inventory",
                summary=f"Resolved {action_count} allowed action(s) from ontology",
            )
        )
    else:
        limitations.append("No allowed actions resolved from ontology")

    return steps, limitations


def _enrich_from_schema_mirror(
    envelope: ContextEnvelope,
) -> tuple[list[OutputEvidence], list[str]]:
    evidence: list[OutputEvidence] = []
    limitations: list[str] = []

    try:
        mirror = _get_schema_mirror()
    except Exception as exc:  # noqa: BLE001 - schema mirror access may fail in various ways
        logger.debug("runtime_context_schema_mirror_access_failed", exc_info=exc)
        limitations.append("Schema mirror unavailable")
        return evidence, limitations

    if mirror is None:
        limitations.append("Schema mirror unavailable")
        return evidence, limitations

    cache = getattr(mirror, "_schema_cache", None)
    if not cache:
        limitations.append("Schema mirror cache empty (read-only, no fetch attempted)")
        return evidence, limitations

    objects = cache.get("objects", {})
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

    return evidence, limitations


def enhance_context(envelope: ContextEnvelope) -> EnrichmentResult:
    """Read-only enrichment of a ContextEnvelope using ontology/AIP capabilities.

    Returns additional reasoning_steps and evidence.
    If dependencies are missing or schemas unavailable, returns limitations instead of raising.
    """
    result = EnrichmentResult()

    result.reasoning_steps.append(_build_intent_reasoning(envelope))

    obj_evidence, obj_limitations = _enrich_from_ontology_objects(envelope)
    result.evidence.extend(obj_evidence)
    result.limitations.extend(obj_limitations)

    action_steps, action_limitations = _enrich_from_allowed_actions(envelope)
    result.reasoning_steps.extend(action_steps)
    result.limitations.extend(action_limitations)

    mirror_evidence, mirror_limitations = _enrich_from_schema_mirror(envelope)
    result.evidence.extend(mirror_evidence)
    result.limitations.extend(mirror_limitations)

    return result
