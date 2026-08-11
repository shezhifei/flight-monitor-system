"""Application-layer re-exports for AI infrastructure types.

Routes and application services should import AI-related enums,
exceptions, and utilities from this module instead of reaching
directly into ``src.infrastructure.ai.*``.

This keeps the dependency direction compliant with the North Star
architecture: delivery → application → domain/ports.
"""

# ── Tool enums ──────────────────────────────────────────────
# ── Config store interface ──────────────────────────────────
from src.infrastructure.ai.config_store import AIConfigStoreInterface

# ── Conversation types ──────────────────────────────────────
from src.infrastructure.ai.conversation_manager import (
    ConversationNotFoundError,
    ConversationStatus,
)

# ── Observability / metrics ─────────────────────────────────
from src.infrastructure.ai.monitoring.metrics import (
    get_execution_visibility_snapshot,
    get_query_observability_snapshot,
    get_report_schema_validation_snapshot,
    metrics,
)

# ── Prompts ─────────────────────────────────────────────────
from src.infrastructure.ai.prompts import PLANNER_SYSTEM_PROMPT
from src.infrastructure.ai.tools.base import InvocationMode, ToolCategory

# ── Pending-action exceptions ───────────────────────────────
from src.infrastructure.ai.tools.pending_actions import PendingActionConflictError

__all__ = [
    # prompts
    "PLANNER_SYSTEM_PROMPT",
    # config
    "AIConfigStoreInterface",
    # conversation
    "ConversationNotFoundError",
    "ConversationStatus",
    # tools
    "InvocationMode",
    "PendingActionConflictError",
    "ToolCategory",
    "get_execution_visibility_snapshot",
    "get_query_observability_snapshot",
    "get_report_schema_validation_snapshot",
    # metrics
    "metrics",
]
