"""Module-level state and constants for the tool registry package."""

from __future__ import annotations

import contextvars
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from .service import ToolRegistry

# Context-local override for the tool registry (used for test isolation)
_registry_context: contextvars.ContextVar[ToolRegistry | None] = contextvars.ContextVar(
    "tool_registry_context",
    default=None,
)

# Global default tool registry instance
_registry: ToolRegistry | None = None
