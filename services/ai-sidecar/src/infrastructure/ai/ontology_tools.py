"""Ontology tools: thin adapter over the Rust-owned ontology actions (Task F3).

This module NO LONGER executes any domain logic itself. All reads and
advisory computations happen in Rust behind the internal endpoints; this
adapter only:

* maps ``entity_id`` / ``proposed_change`` onto registered action names
  and arguments,
* forwards them through the fail-closed :class:`OntologyActionClient`,
* passes Rust results through unchanged (with an evidence block),
* refuses anything not registered (fail-closed, no invented rules).

Controlled writes (e.g. ``StandOccupation.allocate``) are never executed by
this adapter: they are simulated first (constraint check + before-state
snapshot through registered read actions) and then return
``execution_mode="proposal_only"`` with a ``simulate`` block, or
``execution_mode="rejected"`` when a hard constraint is violated — in
which case no proposal is created. Stand/gate overlaps are soft warnings,
never hard; carousel occupancy has no constraints at all.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import UTC, datetime
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
        # notification.suggest_broadcast removed - Notification object deleted in PR #本体两层改造
    }
)

#: Controlled write actions. These NEVER execute here: proposal path only.
#: PR #本体两层改造（PR3 占用收口）：名单换成占用三对象。机位/口重叠是 soft
#: （告警不硬拦，见 `_simulate_controlled_write`）；转盘显式零约束。
#: 废止的 `Flight.change_stand` / `Stand.reserve` 不在此名单 → propose_action
#: 直接 `UnregisteredActionError`（fail-closed，不留兼容分支）。
CONTROLLED_WRITE_ACTIONS: frozenset[str] = frozenset({
    "StandOccupation.allocate",
    "StandOccupation.adjust",
    "StandOccupation.release",
    "GateAssignment.allocate",
    "GateAssignment.release",
    "CarouselAssignment.allocate",
    "CarouselAssignment.release",
})

#: 仅机位占用走可用性模拟（`stand.check_availability`，重叠 soft）。
#: 口/转盘占用与全部 release 不模拟冲突（转盘显式零约束）。
_OCCUPATION_STAND_SIM_ACTIONS: frozenset[str] = frozenset({
    "StandOccupation.allocate",
    "StandOccupation.adjust",
})

#: entity_id prefix → registered read action + object id argument name.
_ENTITY_PREFIX_MAP: dict[str, tuple[str, str]] = {
    "flight": ("flight.get_context", "flight_id"),
    "flight_leg": ("flight_leg.get_context", "leg_id"),  # FlightLeg 不再使用但仍保留查询接口
    "stand": ("stand.get_context", "stand_id"),
    "stand_occupation": ("stand_occupation.get_context", "occupation_id"),
    "gate": ("gate.get_context", "gate_id"),
    "gate_assignment": ("gate_assignment.get_context", "assignment_id"),
    "baggage_carousel": ("baggage_carousel.get_context", "carousel_id"),
    "carousel_assignment": ("carousel_assignment.get_context", "assignment_id"),
    "terminal": ("terminal.get_context", "terminal_id"),
    "dispatch": ("dispatch.get_status", "dispatch_order_id"),
    "dispatch_order": ("dispatch_order.get_status", "order_id"),
    "anomaly": ("anomaly.list_open", "anomaly_id"),
    "business_case": ("business_case.get_context", "case_id"),
    "team": ("team.get_context", "team_id"),
    "department": ("department.get_context", "department_id"),
    "personnel": ("personnel.get_context", "user_id"),
    "equipment": ("equipment.get_context", "equipment_id"),
    "equipment_type": ("equipment_type.get_context", "equipment_type_id"),
    "task_type": ("task_type.get_context", "task_type_id"),
    "aircraft": ("aircraft.get_context", "registration"),
    "turnaround_link": ("turnaround_link.get_context", "link_id"),
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


def _occupation_resource_spec(
    action_name: str, parameters: dict[str, Any]
) -> tuple[str | None, str | None]:
    """Return ``(display_column, code)`` for an occupation action's target resource."""
    if action_name.startswith("StandOccupation"):
        return "stand", str(parameters.get("stand_code") or "").strip() or None
    if action_name.startswith("GateAssignment"):
        return "gate", str(parameters.get("gate_code") or "").strip() or None
    if action_name.startswith("CarouselAssignment"):
        return "carousel", str(parameters.get("carousel_code") or "").strip() or None
    return None, None


def _occupation_time_window(parameters: dict[str, Any]) -> dict[str, Any] | None:
    """Derive a check window from occupation args (``starts_at``/``ends_at`` or ``time_window``)."""
    start = parameters.get("starts_at")
    end = parameters.get("ends_at")
    if start and end:
        return {"start": str(start), "end": str(end)}
    tw = parameters.get("time_window")
    if isinstance(tw, dict) and tw.get("start") and tw.get("end"):
        return {"start": str(tw["start"]), "end": str(tw["end"])}
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
        result = attach_evidence(raw, source="ontology.lookup", object_id=_arguments_object_id(arguments))
        # Task H1: freshness is a runtime invariant — every read stamps its
        # own as_of unless the Rust evidence already carries one.
        evidence = result["evidence"]
        evidence.setdefault("as_of", datetime.now(UTC).isoformat())
        return result

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

        * stand occupation / gate assignment → ``stand.check_availability``
          (requires a time window; overlaps are SOFT warnings, never hard)
        * flight context → ``flight.get_context`` (pass-through evidence)
        """
        change = dict(proposed_change or {})
        action = str(change.get("action", "")).strip()

        if (
            change.get("new_stand_id")
            or change.get("stand_code")
            or change.get("target_gate")
            or change.get("gate_code")
            or action
            in {
                "change_stand",
                "reassign_gate",
                "StandOccupation.allocate",
                "StandOccupation.adjust",
                "GateAssignment.allocate",
            }
        ):
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
        new_stand_id = str(
            change.get("new_stand_id")
            or change.get("stand_code")
            or change.get("target_gate")
            or change.get("gate_code")
            or ""
        ).strip()
        time_window = _occupation_time_window(change)
        if not new_stand_id or time_window is None:
            return {
                "violations": [
                    {
                        "severity": ConstraintSeverity.HARD.value,
                        "rule_id": "missing_constraint_inputs",
                        "reason": "stand/gate occupation requires a resource code and a time window to evaluate constraints",
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
                # 机位/口重叠是告警（soft），不硬拦（与占用台一致）。
                "severity": ConstraintSeverity.SOFT.value,
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
            # Controlled writes are never executed here. Simulate first
            # (constraints + before-state); the proposal/approval path
            # stays the single write surface.
            return await self._simulate_controlled_write(
                run_id=run_id,
                action_name=action_name,
                parameters=parameters or {},
            )

        raise UnregisteredActionError(action_name)

    async def _simulate_controlled_write(
        self,
        *,
        run_id: str,
        action_name: str,
        parameters: dict[str, Any],
    ) -> dict[str, Any]:
        """Simulate a controlled occupation write before any proposal is created.

        Per flight-ops.v1:
        * stand occupations (allocate/adjust) → ``stand.check_availability``;
          overlap is a SOFT warning, never a hard rejection.
        * gate / carousel occupations and all ``release`` actions → no
          constraint simulation (carousel is explicitly zero-constraint).

        The ``after`` block snapshots the occupation's target resource — it is
        NOT a flight "planned field" write (those columns are read-only here).
        Client errors propagate (fail-closed; nothing is fabricated).
        """
        violations: list[dict[str, Any]] = []
        availability: dict[str, Any] = {}
        resource_field, resource_code = _occupation_resource_spec(action_name, parameters)

        if action_name in _OCCUPATION_STAND_SIM_ACTIONS and resource_code:
            time_window = _occupation_time_window(parameters)
            if time_window is not None:
                explained = await self._explain_stand_change(
                    run_id=run_id,
                    entity_type="StandOccupation",
                    change={
                        "action": action_name,
                        "stand_code": resource_code,
                        "time_window": time_window,
                    },
                )
                violations = [v for v in explained.get("violations", []) if isinstance(v, dict)]
                availability = {
                    k: v for k, v in explained.items() if k not in {"violations", "evidence"}
                }

        # 机位重叠现在是 soft → 本名单没有会触发 hard 拒绝的动作。
        hard_violations = [v for v in violations if v.get("severity") == ConstraintSeverity.HARD.value]

        flight_id = str(parameters.get("flight_id") or "").strip()
        registration = str(parameters.get("registration") or "").strip()

        after: dict[str, Any] = {}
        if resource_code:
            after[resource_field] = resource_code
        if flight_id:
            after["flight_id"] = flight_id
        if registration:
            after["registration"] = registration

        if hard_violations:
            logger.warning(
                "ontology_propose_simulate_rejected run=%s action=%s rules=%s",
                run_id,
                action_name,
                [v.get("rule_id") for v in hard_violations],
            )
            return {
                "execution_mode": "rejected",
                "action_name": action_name,
                "parameters": parameters,
                "hard_constraint_violations": hard_violations,
                "simulate": {
                    "action_name": action_name,
                    "before": None,
                    "after": after,
                    "violations": violations,
                    "availability": availability,
                },
            }

        before: dict[str, Any] = {"action": action_name}
        if flight_id:
            raw = await self._client.read(
                run_id=run_id,
                action_name="flight.get_context",
                arguments={"flight_id": flight_id},
            )
            flight = raw.get("flight") if isinstance(raw.get("flight"), dict) else {}
            before = {
                "flight_id": flight_id,
                "stand": flight.get("stand"),
                "gate": flight.get("gate"),
                "baggage_carousel": flight.get("baggage_carousel"),
            }

        return {
            "execution_mode": "proposal_only",
            "action_name": action_name,
            "parameters": parameters,
            "simulate": {
                "action_name": action_name,
                "before": before,
                "after": after,
                "violations": violations,
                "availability": availability,
            },
        }


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
