"""Working memory workspace for hybrid agent runs (Task B2).

Minimal object set (docs/plans/2026-08-14-hybrid-agent-architecture.md, Task B2):

    plan.md        unfinished steps
    notes.md       current flight / conflicts / conclusions
    evidence.json  tool excerpts with source / object_id

Tool results larger than ~2k tokens spill into the workspace: the model
receives only ``summary + pointer`` while the full payload is kept in
``evidence.json``. Everything is JSON-serializable (no pickle) so the snapshot
can ride the existing ``ai_run_checkpoints`` JSONB payload (checkpoint
``context_snapshot``) and be restored through
``resume.ResumeContext.working_memory``.
"""

from __future__ import annotations

import time
from dataclasses import dataclass, field
from typing import Any

PLAN_FILE = "plan.md"
NOTES_FILE = "notes.md"
EVIDENCE_FILE = "evidence.json"
PLAN_STATE_KEY = "plan_state"

# Roughly 2k tokens; results above this spill to the workspace.
DEFAULT_SPILL_TOKEN_THRESHOLD = 2000
_MAX_SUMMARY_CHARS = 500


def estimate_tokens(text: str) -> int:
    """Estimate token count (same heuristic as ContextBudgetPlanner)."""
    ascii_chars = sum(1 for c in text if ord(c) < 128)
    non_ascii_chars = len(text) - ascii_chars
    return (ascii_chars // 4) + non_ascii_chars


@dataclass
class EvidenceRecord:
    """One entry in evidence.json — a tool excerpt with provenance."""

    evidence_id: str
    source: str  # tool name that produced the result
    object_id: str  # flight_id / anomaly id / etc. ("" when unknown)
    summary: str  # short digest fed back to the model
    content: str  # full payload, kept in the workspace only
    size_tokens: int
    created_at: float = field(default_factory=time.time)

    @property
    def pointer(self) -> str:
        return f"{EVIDENCE_FILE}#{self.evidence_id}"

    def to_dict(self) -> dict[str, Any]:
        return {
            "evidence_id": self.evidence_id,
            "source": self.source,
            "object_id": self.object_id,
            "summary": self.summary,
            "content": self.content,
            "size_tokens": self.size_tokens,
            "created_at": self.created_at,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> EvidenceRecord:
        return cls(
            evidence_id=str(data.get("evidence_id", "")),
            source=str(data.get("source", "")),
            object_id=str(data.get("object_id", "")),
            summary=str(data.get("summary", "")),
            content=str(data.get("content", "")),
            size_tokens=int(data.get("size_tokens", 0) or 0),
            created_at=float(data.get("created_at", 0.0) or 0.0),
        )


class WorkingMemory:
    """Per-run working memory workspace.

    In-memory object set (plan.md / notes.md / evidence.json) with a
    JSON-serializable snapshot for checkpoint persistence. Never writes
    business tables; the snapshot travels inside the existing checkpoint
    ``context_snapshot`` JSONB payload.
    """

    def __init__(self, run_id: str, spill_token_threshold: int = DEFAULT_SPILL_TOKEN_THRESHOLD):
        self.run_id = run_id
        self.spill_token_threshold = max(1, int(spill_token_threshold))
        self._plan: str = ""
        self._notes: str = ""
        self._evidence: list[EvidenceRecord] = []
        # Task C1: structured plan state. This is the single source of truth
        # for the run's execution plan; ``plan.md`` is a derived rendering
        # regenerated on every mutation. JSON-serializable so it rides the
        # checkpoint snapshot like the rest of the workspace.
        self._plan_state: dict[str, Any] | None = None

    # ---- plan.md -----------------------------------------------------

    def read_plan(self) -> str:
        return self._plan

    def write_plan(self, content: str) -> None:
        self._plan = content or ""

    # ---- structured plan state (Task C1) ------------------------------

    def read_plan_state(self) -> dict[str, Any] | None:
        """Return the structured plan (``{"description", "steps", ...}``) or None."""
        return self._plan_state

    def write_plan_state(self, state: dict[str, Any]) -> None:
        """Store the structured plan and re-render ``plan.md`` from it."""
        self._plan_state = state
        self._plan = self._render_plan_markdown(state)

    @staticmethod
    def _render_plan_markdown(state: dict[str, Any]) -> str:
        """Render the structured plan as the human-readable ``plan.md``."""
        if not state:
            return ""
        lines = [f"# Plan: {state.get('description', '')}".rstrip()]
        for step in state.get("steps") or []:
            status = step.get("status", "pending")
            mark = "x" if status == "completed" else " "
            assigned = step.get("assigned_to")
            suffix = f" ({assigned})" if assigned else ""
            lines.append(f"- [{mark}] {step.get('id', '')}: {step.get('description', '')}{suffix}")
        return "\n".join(lines)

    # ---- notes.md ----------------------------------------------------

    def read_notes(self) -> str:
        return self._notes

    def append_notes(self, note: str) -> None:
        note = (note or "").strip()
        if not note:
            return
        self._notes = f"{self._notes}\n{note}" if self._notes else note

    # ---- evidence.json -----------------------------------------------

    def add_evidence(self, *, source: str, object_id: str, summary: str, content: str) -> EvidenceRecord:
        record = EvidenceRecord(
            evidence_id=f"ev-{len(self._evidence) + 1:04d}",
            source=source,
            object_id=object_id or "",
            summary=summary,
            content=content,
            size_tokens=estimate_tokens(content),
        )
        self._evidence.append(record)
        return record

    def read_evidence(self) -> list[dict[str, Any]]:
        return [record.to_dict() for record in self._evidence]

    def get_evidence_content(self, evidence_id: str) -> str | None:
        """Resolve a pointer (``evidence.json#ev-0001`` or bare id) to full content."""
        evidence_id = (evidence_id or "").split("#")[-1].strip()
        for record in self._evidence:
            if record.evidence_id == evidence_id:
                return record.content
        return None

    # ---- large tool result spill -------------------------------------

    def should_spill(self, content: str) -> bool:
        return estimate_tokens(content) > self.spill_token_threshold

    def spill_tool_result(self, *, tool_name: str, content: str, object_id: str = "") -> str:
        """Store the full result in evidence.json; return summary + pointer.

        The returned string is what the model receives in place of the raw
        tool payload (Task B2: 只回灌 summary + pointer).
        """
        size_tokens = estimate_tokens(content)
        summary = content[:_MAX_SUMMARY_CHARS]
        if len(content) > _MAX_SUMMARY_CHARS:
            summary += "..."
        record = self.add_evidence(
            source=tool_name,
            object_id=object_id,
            summary=summary,
            content=content,
        )
        return (
            f"[工具结果过大，已存入工作记忆] tool={tool_name} "
            f"~{size_tokens} tokens\n"
            f"summary: {summary}\n"
            f"pointer: {record.pointer} (完整内容在工作记忆 {EVIDENCE_FILE}，"
            "需要时请按 pointer 取回)"
        )

    # ---- checkpoint serialization (no pickle) -------------------------

    def to_dict(self) -> dict[str, Any]:
        """JSON-serializable snapshot for checkpoint context_snapshot."""
        return {
            "run_id": self.run_id,
            "spill_token_threshold": self.spill_token_threshold,
            PLAN_FILE: self._plan,
            NOTES_FILE: self._notes,
            EVIDENCE_FILE: [record.to_dict() for record in self._evidence],
            PLAN_STATE_KEY: self._plan_state,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any] | None) -> WorkingMemory:
        """Restore from a checkpoint snapshot.

        Accepts either a snapshot produced by :meth:`to_dict` or a checkpoint
        ``context_snapshot`` that nests it under the ``"working_memory"`` key
        (the shape ``resume.RunRestorer`` restores into
        ``ResumeContext.working_memory``).
        """
        data = data or {}
        if isinstance(data.get("working_memory"), dict):
            data = data["working_memory"]
        memory = cls(
            run_id=str(data.get("run_id", "")),
            spill_token_threshold=int(data.get("spill_token_threshold", DEFAULT_SPILL_TOKEN_THRESHOLD) or DEFAULT_SPILL_TOKEN_THRESHOLD),
        )
        memory._plan = str(data.get(PLAN_FILE, "") or "")
        memory._notes = str(data.get(NOTES_FILE, "") or "")
        memory._evidence = [
            EvidenceRecord.from_dict(item)
            for item in data.get(EVIDENCE_FILE, []) or []
            if isinstance(item, dict)
        ]
        plan_state = data.get(PLAN_STATE_KEY)
        if isinstance(plan_state, dict):
            memory._plan_state = plan_state
            if not memory._plan:
                # Re-render plan.md from the structured state on restore.
                memory._plan = memory._render_plan_markdown(plan_state)
        return memory


__all__ = [
    "DEFAULT_SPILL_TOKEN_THRESHOLD",
    "EVIDENCE_FILE",
    "NOTES_FILE",
    "PLAN_FILE",
    "PLAN_STATE_KEY",
    "EvidenceRecord",
    "WorkingMemory",
    "estimate_tokens",
]
