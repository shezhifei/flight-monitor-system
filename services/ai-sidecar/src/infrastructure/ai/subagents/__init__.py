"""Subagents dispatcher - controlled recursive delegation to child entities."""

from src.infrastructure.ai.subagents.dispatcher import (
    SUBAGENT_TOOL_SCHEMA,
    SubagentDispatcher,
    SubagentResult,
)

__all__ = [
    "SUBAGENT_TOOL_SCHEMA",
    "SubagentDispatcher",
    "SubagentResult",
]
