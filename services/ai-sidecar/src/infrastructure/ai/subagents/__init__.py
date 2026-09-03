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
    get_handoff_delegate_manager,
)
from src.infrastructure.ai.subagents.handoff import (
    SubagentResult as HandoffSubagentResult,
)

__all__ = [
    "SUBAGENT_TOOL_SCHEMA",
    # Handoff vs Delegate
    "DelegateRequest",
    "HandoffDelegateManager",
    "HandoffRequest",
    "HandoffSubagentResult",
    "SubagentDispatcher",
    "SubagentResult",
    "get_handoff_delegate_manager",
]
