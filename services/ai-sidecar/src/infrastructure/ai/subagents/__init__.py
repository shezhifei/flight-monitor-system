"""Subagents dispatcher - controlled recursive delegation to child entities."""

from src.infrastructure.ai.subagents.dispatcher import (
    SUBAGENT_TOOL_SCHEMA,
    SubagentDispatcher,
    SubagentResult,
)
from src.infrastructure.ai.subagents.handoff import (
    DelegateRequest,
    HandoffDelegateManager,
    HandoffRequest,
    SubagentResult as HandoffSubagentResult,
    get_handoff_delegate_manager,
)

__all__ = [
    "SUBAGENT_TOOL_SCHEMA",
    "SubagentDispatcher",
    "SubagentResult",
    # Handoff vs Delegate
    "DelegateRequest",
    "HandoffDelegateManager",
    "HandoffRequest",
    "HandoffSubagentResult",
    "get_handoff_delegate_manager",
]
