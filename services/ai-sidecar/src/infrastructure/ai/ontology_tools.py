"""
E1: Flight Domain Ontology Tools (本体入环)

提供领域本体查询接口，支持：
- ontology.lookup: 实体对象图查询
- ontology.explain_constraints: 约束解释与冲突检测
- ontology.propose_action: 合法动作建议（受注册动作限制）

参考设计模式：
- Neurosymbolic Architecture (arXiv 2604.00555)
- Ontology-constrained neural reasoning
"""

import json
from dataclasses import dataclass, field
from enum import Enum
from typing import Any

from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class ConstraintSeverity(Enum):
    """约束严重程度分类。"""
    
    HARD = "hard"       # 不可违反的硬约束
    SOFT = "soft"       # 可权衡的软约束/启发式规则


@dataclass
class ConstraintViolation:
    """约束违规描述。"""
    
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
    """提议的动作候选。"""
    
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
    """实体查询结果。"""
    
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


class OntologyTools:
    """Flight domain ontology query interface."""
    
    def __init__(self):
        self._registry: set[str] = set()  # Registered action names as strings
        self._cached_lookups: dict[str, dict[str, Any]] = {}  # Redis cache placeholder
        
    async def lookup(
        self, 
        entity_id: str,           # e.g., "flight_F1234", "gate_A12"
        include_relations: bool = True,
        max_depth: int = 2,
    ) -> EntityLookupResult:
        """
        拉取实体及其关联对象图。
        
        Args:
            entity_id: Entity identifier ("flight_F1234", "gate_A12", etc.)
            include_relations: Whether to fetch related entities
            max_depth: Maximum relationship traversal depth
            
        Returns:
            EntityLookupResult with full context
            
        Example:
            >>> result = await tools.lookup("flight_CA1598")
            >>> {
            ...     "entity": {...},              # Flight details
            ...     "relationships": [...],       # Connected entities
            ...     "constraints": [...]          # Active constraints
            ... }
        """
        logger.info(f"[Ontology] Lookup entity={entity_id}, relations={include_relations}, depth={max_depth}")
        
        # Check cache first
        if entity_id in self._cached_lookups:
            cached = self._cached_lookups[entity_id]
            logger.debug(f"[Ontology] Cache hit for {entity_id}")
            return EntityLookupResult(**cached)
        
        # In production, would query Rust API or Postgres view
        # This is a stub implementation
        entity_data = await self._fetch_entity_from_api(entity_id)
        
        relationships = []
        if include_relations and max_depth > 0:
            relationships = await self._fetch_relationships(entity_id, max_depth)
        
        # Infer constraints from entity state
        constraints = await self._infer_constraints(entity_data, relationships)
        
        result = EntityLookupResult(
            entity=entity_data,
            relationships=relationships,
            constraints=constraints,
            metadata={"source": "ontology_lookup", "depth": max_depth},
        )
        
        # Update cache
        self._cached_lookups[entity_id] = result.to_dict()
        
        return result
    
    async def explain_constraints(
        self,
        entity_type: str,        # "Flight", "DispatchOrder", etc.
        proposed_change: dict,   # {"action": "reassign_gate", "to": "A15"}
    ) -> list[ConstraintViolation]:
        """
        解释并验证变更请求中的约束条件。
        
        Args:
            entity_type: Type of entity being modified
            proposed_change: Dictionary describing proposed modification
            
        Returns:
            List of ConstraintViolation objects (empty = compliant)
            
        Example:
            >>> violations = await tools.explain_constraints(
            ...     "Flight",
            ...     {"action": "change_gate", "target": "A15"}
            ... )
            >>> if violations:
            ...     for v in violations:
            ...         print(f"{v.severity.value}: {v.reason}")
        """
        logger.info(f"[Ontology] Explain constraints for {entity_type}: {proposed_change}")
        
        violations = []
        
        # Hard constraints examples
        aircraft_gate_rules = await self._load_aircraft_gate_matrix()
        current_aircraft = proposed_change.get("aircraft_type")
        target_gate = proposed_change.get("target_gate")
        
        if aircraft_gate_rules:
            valid_gates = aircraft_gate_rules.get(current_aircraft, [])
            if target_gate not in valid_gates:
                violations.append(ConstraintViolation(
                    severity=ConstraintSeverity.HARD,
                    rule_id="aircraft_gate_incompatibility_001",
                    reason=f"Gate {target_gate} is incompatible with aircraft type {current_aircraft}",
                    entity_type=entity_type,
                    recommended_fix=f"Use one of: {valid_gates}",
                ))
        
        # Soft constraints (heuristic rules)
        turnaround_minimum = await self._load_turnaround_minimum_rules()
        crew_start_time = proposed_change.get("crew_arrival", "00:00")
        departure_time = proposed_change.get("departure_scheduled", "01:00")
        
        if crew_start_time and departure_time and turnaround_minimum:
            hours_diff = self._calculate_hours_difference(crew_start_time, departure_time)
            if hours_diff < turnaround_minimum:
                violations.append(ConstraintViolation(
                    severity=ConstraintSeverity.SOFT,
                    rule_id="minimum_turnaround_time",
                    reason=f"Crew turnaround time ({hours_diff:.1f}h) below minimum threshold ({turnaround_minimum}h)",
                    entity_type=entity_type,
                    recommended_fix="Increase crew arrival time or adjust departure schedule",
                ))
        
        # Validate against registered action schema
        action_validations = await self._validate_action_schema(proposed_change)
        violations.extend(action_validations)
        
        if violations:
            logger.warning(
                f"[Ontology] Found {len(violations)} constraint violation(s) for {entity_type}"
            )
        else:
            logger.debug(f"[Ontology] No violations found for {entity_type}")
        
        return violations
    
    async def propose_action(
        self,
        problem_state: dict[str, Any],
        available_actions: list[dict[str, Any]],
    ) -> list[ProposalCandidate]:
        """
        提出合法的动作建议。
        
        **关键约束**: 只能产出已注册的动作，禁止自由发挥。
        
        Args:
            problem_state: Current domain state snapshot
            available_actions: List of action schemas (OpenAI tool format)
            
        Returns:
            Ranked list of ProposalCandidates sorted by confidence
            
        Example:
            >>> proposals = await tools.propose_action(
            ...     problem_state={"delayed_flights": ["CA1234"], "available_gates": ["A12"]},
            ...     available_actions=[{...}]
            ... )
            >>> for p in proposals:
            ...     print(f"{p.action_name} (conf={p.confidence}): {p.rationale}")
        """
        logger.info(f"[Ontology] Propose action for {problem_state.get('problem_type', 'unknown')}")
        
        candidates = []
        
        # Step 1: Filter to only registered actions (enforce action registry)
        filtered_actions = await self._filter_registered_actions(available_actions)
        
        if not filtered_actions:
            logger.warning("[Ontology] No registered actions available")
            return []
        
        # Step 2: Apply domain heuristics to score candidates
        for action in filtered_actions:
            action_name = action.get("function", {}).get("name", "")
            params = action.get("function", {}).get("parameters", {})
            
            # Evaluate problem-state compatibility
            scores = await self._score_action_compatibility(problem_state, action)
            
            # Generate rationale based on highest scoring parameter
            rationale = self._generate_rationale(problem_state, action, scores)
            
            # Check for constraint conflicts
            warnings = await self.explain_constraints(
                entity_type=problem_state.get("affected_entity_type", "General"),
                proposed_change=params,
            )
            
            candidate = ProposalCandidate(
                action_name=action_name,
                parameters=params,
                confidence=scores.get("overall", 0.0),
                rationale=rationale,
                constraint_warnings=warnings,
            )
            
            candidates.append(candidate)
        
        # Step 3: Rank by confidence
        candidates.sort(key=lambda c: c.confidence, reverse=True)
        
        logger.debug(f"[Ontology] Generated {len(candidates)} action proposals")
        
        # Limit to top-K to prevent LLM overload
        return candidates[:3]
    
    # ========================================================================
    # Private Implementation Methods
    # ========================================================================
    
    async def _fetch_entity_from_api(self, entity_id: str) -> dict[str, Any]:
        """Fetch entity from Rust API or database (stub)."""
        # Production implementation:
        # response = await api_client.get(f"/api/v2/ai/ontology/{entity_id}")
        # return response.json()
        
        # Stub for testing
        if entity_id.startswith("flight_"):
            return {
                "id": entity_id,
                "type": "Flight",
                "flight_number": entity_id.replace("flight_", ""),
                "status": "delayed",
                "scheduled_departure": "2026-08-15T10:30:00Z",
                "estimated_departure": "2026-08-15T11:45:00Z",
                "current_gate": "A10",
            }
        elif entity_id.startswith("gate_"):
            return {
                "id": entity_id,
                "type": "Stand",
                "stand_number": entity_id.replace("gate_", ""),
                "capacity": "B737",
                "current_occupant": None,
            }
        
        return {"id": entity_id, "type": "Unknown"}
    
    async def _fetch_relationships(
        self, 
        entity_id: str, 
        max_depth: int,
    ) -> list[dict[str, Any]]:
        """Fetch relationship graph from entity (stub)."""
        if not max_depth:
            return []
        
        # In production: query Postgres JSONB array or Neo4j
        return [
            {"type": "has_aircraft", "target": "B737-800"},
            {"type": "assigned_to_gate", "target": "gate_A10"},
            {"type": "operated_by_crew", "target": "team_CZ001"},
        ]
    
    async def _infer_constraints(
        self,
        entity_data: dict[str, Any],
        relationships: list[dict[str, Any]],
    ) -> list[ConstraintViolation]:
        """Infer active constraints from entity state (stub)."""
        constraints = []
        
        # Example: check for conflict constraints
        if entity_data.get("status") == "delayed":
            scheduled_time = entity_data.get("scheduled_departure")
            estimated_time = entity_data.get("estimated_departure")
            
            if scheduled_time and estimated_time:
                # Would calculate delay duration here
                constraints.append(ConstraintViolation(
                    severity=ConstraintSeverity.SOFT,
                    rule_id="delay_threshold_exceeded",
                    reason=f"Delay exceeds 30 minutes threshold",
                    entity_type="Flight",
                ))
        
        return constraints
    
    async def _load_aircraft_gate_matrix(self) -> dict[str, list[str]]:
        """Load aircraft-gate compatibility matrix (stub)."""
        return {
            "B737": ["A10", "A11", "A12", "B20", "B21"],
            "B777": ["C30", "C31", "C32"],
            "A320": ["A10", "A11", "A12"],
        }
    
    async def _load_turnaround_minimum_rules(self) -> float:
        """Load minimum turnaround time in hours (stub)."""
        return 1.5  # 1.5 hours minimum
    
    def _calculate_hours_difference(self, start_time: str, end_time: str) -> float:
        """Calculate hours between two time strings (HH:MM format)."""
        from datetime import datetime
        
        start = datetime.strptime(start_time, "%H:%M")
        end = datetime.strptime(end_time, "%H:%M")
        
        delta = end - start
        return delta.total_seconds() / 3600
    
    async def _validate_action_schema(
        self,
        proposed_change: dict,
    ) -> list[ConstraintViolation]:
        """Validate action against registered schema (stub)."""
        violations = []
        
        # Ensure required parameters are present
        required_params = proposed_change.get("_required_parameters", [])
        for param in required_params:
            if param not in proposed_change:
                violations.append(ConstraintViolation(
                    severity=ConstraintSeverity.HARD,
                    rule_id="missing_required_parameter",
                    reason=f"Required parameter '{param}' is missing",
                ))
        
        return violations
    
    async def _filter_registered_actions(
        self,
        available_actions: list[dict[str, Any]],
    ) -> list[dict[str, Any]]:
        """Filter actions to only those in the action registry."""
        if not self._registry:
            # If no registry, allow all actions (permissive mode)
            logger.debug("[Ontology] No action registry loaded, allowing all actions")
            return available_actions
        
        registry_names = self._registry
        return [
            action for action in available_actions
            if action.get("function", {}).get("name") in registry_names
        ]
    
    def _score_action_compatibility(
        self,
        problem_state: dict[str, Any],
        action: dict[str, Any],
    ) -> dict[str, float]:
        """Score action compatibility with problem state (stub)."""
        # In production: use rule engine or ML model
        return {"overall": 0.85}
    
    def _generate_rationale(
        self,
        problem_state: dict[str, Any],
        action: dict[str, Any],
        scores: dict[str, float],
    ) -> str | None:
        """Generate human-readable rationale for proposal."""
        action_name = action.get("function", {}).get("name", "")
        
        rationales = {
            "change_stand": "Reassignment aligns aircraft type with gate capacity requirements",
            "notify_teams": "Team notification ensures rapid execution of ground operations",
            "create_todo": "Structured tracking improves accountability for resolution",
        }
        
        return rationales.get(action_name, "Action recommended based on domain expertise")
    
    # Factory method for singleton access
    @staticmethod
    def create_instance() -> "OntologyTools":
        """Create new instance of ontology tools."""
        return OntologyTools()
