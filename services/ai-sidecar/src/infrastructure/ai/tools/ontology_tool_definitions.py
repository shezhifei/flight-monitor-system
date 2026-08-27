"""Ontology tool definitions (Task F4).

Three tools expose the Rust-owned ontology action surface to the agent
loop. They are thin schemas only: execution routes through
``ToolExecutor`` into the fail-closed :class:`OntologyActionClient`,
never into local stubs.

Governance fields (presets, required permissions) live in the catalog /
resolver layer, NOT in these definitions — ``to_openai_schema()`` emits
a pure function schema.
"""

from __future__ import annotations

from src.infrastructure.ai.tools.base import (
    BaseToolDefinition,
    OperationLevel,
    ToolCategory,
)

ONTOLOGY_LOOKUP = BaseToolDefinition(
    name="ontology.lookup",
    description=(
        "Look up a flight-ops object and its relations via the registered "
        "ontology read actions. entity_id like flight:<id> or stand:<id>."
    ),
    parameters={
        "entity_id": {
            "type": "string",
            "description": "Object identifier prefixed by type, e.g. flight:CA1598 or stand:A12",
        },
        "include_relations": {
            "type": "boolean",
            "description": "Include related objects in the response (default true)",
        },
    },
    required_params=["entity_id"],
    category=ToolCategory.ONTOLOGY,
    operation_level=OperationLevel.READ,
    side_effect=False,
)

ONTOLOGY_EXPLAIN_CONSTRAINTS = BaseToolDefinition(
    name="ontology.explain_constraints",
    description=(
        "Explain the constraints that apply to a proposed change using "
        "registered ontology read actions. Returns violations with evidence; "
        "never invents rules."
    ),
    parameters={
        "entity_type": {
            "type": "string",
            "description": "Object type being changed, e.g. Flight",
        },
        "proposed_change": {
            "type": "object",
            "description": (
                "Proposed change, e.g. {action: StandOccupation.allocate, stand_code: A12, "
                "time_window: {start, end}}"
            ),
        },
    },
    required_params=["entity_type", "proposed_change"],
    category=ToolCategory.ONTOLOGY,
    operation_level=OperationLevel.READ,
    side_effect=False,
)

ONTOLOGY_PROPOSE_ACTION = BaseToolDefinition(
    name="ontology.propose_action",
    description=(
        "Propose a governed ontology write. Controlled writes "
        "(e.g. StandOccupation.allocate) are proposal-only and are never executed directly."
    ),
    parameters={
        "action_name": {
            "type": "string",
            "description": (
                "Registered action name from envelope.allowed_actions, "
                "e.g. StandOccupation.allocate or Flight.add_note"
            ),
        },
        "parameters": {
            "type": "object",
            "description": "Arguments for the action",
        },
    },
    required_params=["action_name"],
    category=ToolCategory.ONTOLOGY,
    operation_level=OperationLevel.ASSISTED_WRITE,
    side_effect=False,
)

ONTOLOGY_TOOL_DEFINITIONS: list[BaseToolDefinition] = [
    ONTOLOGY_LOOKUP,
    ONTOLOGY_EXPLAIN_CONSTRAINTS,
    ONTOLOGY_PROPOSE_ACTION,
]

ONTOLOGY_TOOL_NAMES: list[str] = [definition.name for definition in ONTOLOGY_TOOL_DEFINITIONS]

_ONTOLOGY_TOOL_NAME_SET: frozenset[str] = frozenset(ONTOLOGY_TOOL_NAMES)


def is_ontology_tool(tool_name: str) -> bool:
    """Check if a tool name belongs to the ontology tool surface."""
    return tool_name in _ONTOLOGY_TOOL_NAME_SET


__all__ = [
    "ONTOLOGY_EXPLAIN_CONSTRAINTS",
    "ONTOLOGY_LOOKUP",
    "ONTOLOGY_PROPOSE_ACTION",
    "ONTOLOGY_TOOL_DEFINITIONS",
    "ONTOLOGY_TOOL_NAMES",
    "is_ontology_tool",
]
