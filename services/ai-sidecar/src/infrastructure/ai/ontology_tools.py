"""Ontology tools: thin adapter over the Rust-owned ontology actions (Task F3).

This module NO LONGER executes any domain logic itself. All reads and
advisory computations happen in Rust behind the internal endpoints; this
adapter only:

* maps ``entity_id`` / ``proposed_change`` onto registered action names
  and arguments,
* forwards them through the fail-closed :class:`OntologyActionClient`,
* passes Rust results through unchanged (with an evidence block),
* refuses anything not registered (fail-closed, no invented rules).

Controlled writes (e.g. ``Flight.change_stand``) are never executed by
this adapter: they return ``execution_mode="proposal_only"`` so the
proposal/approval path stays the single write surface.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Any

from src.infrastructure.ai.ontology.action_client import OntologyActionClient
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class ConstraintSeverity(Enum):
    """约束严重程度分类。"""

    HARD = "hard"  # 不可违反的硬约束
    SOFT = "soft"  # 可权衡的软约束/启发式规则


@dataclass
class ConstraintViolation:
    """约束违规描述（保留的公开类型；新代码直接消费 Rust 返回的 dict）。"""

    severity: ConstraintSeverity
    rule_id: str
    reason: str
    entity_type: str | None = None
    recommended_fix: str | None = None

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary representation."""
        return {
            "severity": self.severity.value,
            "rule_id": self.rule_id,
            "reason": self.reason,
            "entity_type": self.entity_type,
            "recommended_fix": self.recommended_fix,
        }


@dataclass
class ProposalCandidate:
    """提议的动作候选（保留的公开类型）。"""

    action_name: str
    parameters: dict[str, Any]
    confidence: float = 0.0
    rationale: str | None = None
    constraint_warnings: list[ConstraintViolation] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary representation."""
        return {
            "action_name": self.action_name,
            "parameters": self.parameters,
            "confidence": self.confidence,
            "rationale": self.rationale,
            "constraint_warnings": [v.to_dict() for v in self.constraint_warnings],
        }


@dataclass
class EntityLookupResult:
    """实体查询结果（保留的公开类型）。"""

    entity: dict[str, Any]
    relationships: list[dict[str, Any]] = field(default_factory=list)
    constraints: list[ConstraintViolation] = field(default_factory=list)
    metadata: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary representation."""
        return {
            "entity": self.entity,
            "relationships": self.relationships,
            "constraints": [c.to_dict() for c in self.constraints],
            "metadata": self.metadata,
        }


class UnregisteredActionError(ValueError):
    """Raised when an action name is not registered for the run."""

    def __init__(self, action_name: str) -> None:
        super().__init__(f"action '{action_name}' is not registered")
        self.action_name = action_name


#: Registered advisory actions (mirror of Rust `advisory_action_permission`).
ADVISORY_ACTIONS: frozenset[str] = frozenset(
    {
        "flight.suggest_stand_adjustment",
        "flight.suggest_delay_action",
        "dispatch.suggest_replan",
        "anomaly.suggest_escalation",
        "notification.suggest_broadcast",
    }
)

#: Controlled write actions. These NEVER execute here: proposal path only.
CONTROLLED_WRITE_ACTIONS: frozenset[str] = frozenset({"Flight.change_stand"})

#: entity_id prefix → registered read action + object id argument name.
_ENTITY_PREFIX_MAP: dict[str, tuple[str, str]] = {
    "flight": ("flight.get_context", "flight_id"),
    "stand": ("stand.check_availability", "stand_id"),
    "dispatch": ("dispatch.get_status", "dispatch_order_id"),
    "anomaly": ("anomaly.list_open", "flight_id"),
}


def parse_entity_id(entity_id: str) -> tuple[str, dict[str, Any]]:
    """Map ``<prefix>:<object_id>`` onto a registered read action + arguments.

    Raises:
        ValueError: when the prefix is not a registered object namespace.
    """
    raw = str(entity_id or "").strip()
    prefix, sep, object_id = raw.partition(":")
    if not sep or not object_id or prefix not in _ENTITY_PREFIX_MAP:
        raise ValueError(f"unrecognized entity_id: {entity_id!r}")
    action_name, arg_name = _ENTITY_PREFIX_MAP[prefix]
    return action_name, {arg_name: object_id}


def _arguments_object_id(arguments: dict[str, Any]) -> str | None:
    for key in ("flight_id", "stand_id", "dispatch_order_id"):
        value = arguments.get(key)
        if isinstance(value, str) and value:
            return value
    return None


def attach_evidence(
    raw: dict[str, Any],
    *,
    source: str,
    object_id: str | None = None,
) -> dict[str, Any]:
    """Return ``raw`` with a merged ``evidence`` block (Rust evidence wins)."""
    result = dict(raw)
    evidence: dict[str, Any] = {"source": source}
    if object_id is not None:
        evidence["object_id"] = object_id
    existing = raw.get("evidence")
    if isinstance(existing, dict):
        evidence.update(existing)
    result["evidence"] = evidence
    return result


class OntologyTools:
    """Flight domain ontology query interface (adapter over Rust actions)."""

    def __init__(self, client: OntologyActionClient) -> None:
        self._client = client

    async def lookup(self, *, run_id: str, entity_id: str, include_relations: bool = True) -> dict[str, Any]:
        """Look up an entity through the registered read action for its type.

        Fail-closed: any client error propagates; nothing is fabricated.
        """
        action_name, arguments = parse_entity_id(entity_id)
        if not include_relations and action_name == "flight.get_context":
            arguments = {**arguments, "include_relations": []}
        logger.info("ontology_lookup run=%s action=%s entity=%s", run_id, action_name, entity_id)
        raw = await self._client.read(run_id=run_id, action_name=action_name, arguments=arguments)
        return attach_evidence(raw, source="ontology.lookup", object_id=_arguments_object_id(arguments))

    async def explain_constraints(
        self,
        *,
        run_id: str,
        entity_type: str,
        proposed_change: dict[str, Any],
    ) -> dict[str, Any]:
        """Explain constraints for a proposed change using Rust read actions.

        Mappings (only these are understood; anything else is a hard
        violation — rules are never invented on the client side):

        * stand change   → ``stand.check_availability`` (requires
          ``time_window``; conflicts are hard violations)
        * flight context → ``flight.get_context`` (pass-through evidence)
        """
        change = dict(proposed_change or {})
        action = str(change.get("action", "")).strip()

        if change.get("new_stand_id") or action in {"change_stand", "reassign_gate"}:
            return await self._explain_stand_change(run_id=run_id, entity_type=entity_type, change=change)

        if change.get("flight_id"):
            raw = await self._client.read(
                run_id=run_id,
                action_name="flight.get_context",
                arguments={"flight_id": str(change["flight_id"])},
            )
            return attach_evidence(
                {"violations": [], **raw},
                source="ontology.explain_constraints",
                object_id=str(change["flight_id"]),
            )

        return {
            "violations": [
                {
                    "severity": ConstraintSeverity.HARD.value,
                    "rule_id": "unsupported_constraint_mapping",
                    "reason": (
                        f"proposed change for {entity_type!r} has no registered constraint "
                        "mapping; refusing to invent rules"
                    ),
                    "entity_type": entity_type,
                    "recommended_fix": None,
                }
            ],
            "evidence": {"source": "ontology.explain_constraints"},
        }

    async def _explain_stand_change(
        self,
        *,
        run_id: str,
        entity_type: str,
        change: dict[str, Any],
    ) -> dict[str, Any]:
        new_stand_id = str(change.get("new_stand_id") or change.get("target_gate") or "").strip()
        time_window = change.get("time_window")
        if not new_stand_id or not isinstance(time_window, dict):
            return {
                "violations": [
                    {
                        "severity": ConstraintSeverity.HARD.value,
                        "rule_id": "missing_constraint_inputs",
                        "reason": "stand change requires `new_stand_id` and `time_window` to evaluate constraints",
                        "entity_type": entity_type,
                        "recommended_fix": None,
                    }
                ],
                "evidence": {"source": "ontology.explain_constraints"},
            }

        raw = await self._client.read(
            run_id=run_id,
            action_name="stand.check_availability",
            arguments={"stand_id": new_stand_id, "time_window": time_window},
        )
        conflicts = raw.get("conflicts") if isinstance(raw.get("conflicts"), list) else []
        violations = [
            {
                "severity": ConstraintSeverity.HARD.value,
                "rule_id": "stand_occupation_conflict",
                "reason": str(conflict.get("reason", "stand occupied in requested window")),
                "entity_type": entity_type,
                "recommended_fix": None,
            }
            for conflict in conflicts
            if isinstance(conflict, dict)
        ]
        return attach_evidence(
            {"violations": violations, **{k: v for k, v in raw.items() if k != "evidence"}},
            source="ontology.explain_constraints",
            object_id=new_stand_id,
        )

    async def propose_action(
        self,
        *,
        run_id: str,
        action_name: str,
        parameters: dict[str, Any],
        allowed_actions: list[str],
    ) -> dict[str, Any]:
        """Propose an action strictly within the run's registered allowlist.

        ``allowed_actions`` must come from ``envelope.ontology.allowed_actions``
        (the snapshot fixed at run start), never from client self-reporting.
        """
        if action_name not in set(allowed_actions or []):
            logger.warning("ontology_propose_rejected_unregistered run=%s action=%s", run_id, action_name)
            raise UnregisteredActionError(action_name)

        if action_name in ADVISORY_ACTIONS:
            return await self._client.advisory(run_id=run_id, action_name=action_name, arguments=parameters or {})

        if action_name in CONTROLLED_WRITE_ACTIONS:
            # Controlled writes are never executed here. The proposal /
            # approval path is the single write surface.
            return {
                "execution_mode": "proposal_only",
                "action_name": action_name,
                "parameters": parameters or {},
            }

        raise UnregisteredActionError(action_name)


__all__ = [
    "ADVISORY_ACTIONS",
    "CONTROLLED_WRITE_ACTIONS",
    "ConstraintSeverity",
    "ConstraintViolation",
    "EntityLookupResult",
    "OntologyTools",
    "ProposalCandidate",
    "UnregisteredActionError",
    "attach_evidence",
    "parse_entity_id",
]
