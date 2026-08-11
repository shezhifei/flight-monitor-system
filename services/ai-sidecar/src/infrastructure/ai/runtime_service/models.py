"""Dataclasses for the AI runtime service."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from src.infrastructure.ai.structured_output import (
    OutputEvidence,
    OutputProposal,
    ReasoningStep,
)


@dataclass
class _CapabilityPreparation:
    """Result of the shared capability preamble (resolve + security gate + cache).

    Shared by all three run entrypoints. ``resolved_config is None`` means no
    resolver was wired (legacy env-only behavior). A non-None ``rejection_event``
    / ``rejection_answer`` means the run must stop (fail-closed): streaming callers
    yield ``rejection_event`` then return; the non-streaming caller builds a failed
    output from ``rejection_answer``.
    """

    resolved_config: Any = None
    cached_prior_messages: list[Any] = field(default_factory=list)
    progress_events: list[dict[str, Any]] = field(default_factory=list)
    rejection_event: dict[str, Any] | None = None
    rejection_answer: str | None = None


@dataclass
class _RunContext:
    reasoning_steps: list[ReasoningStep] = field(default_factory=list)
    evidence: list[OutputEvidence] = field(default_factory=list)
    proposals: list[OutputProposal] = field(default_factory=list)
    limitations: list[str] = field(default_factory=list)
