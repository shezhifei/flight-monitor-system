"""Pending action queue for human-approval workflows.

Facade package that re-exports the public API split across submodules:
- :mod:`.models`: dataclasses, enums, exceptions, and protocols.
- :mod:`.service`: store implementations and runtime accessors.
"""

from __future__ import annotations

from .models import (
    PendingAction,
    PendingActionConflictError,
    PendingActionStatus,
    PendingActionStoreProtocol,
)
from .service import (
    MemoryPendingActionStore,
    PendingActionStore,
    PostgresPendingActionStore,
    get_pending_action_store,
    set_pending_action_store,
)

__all__ = [
    "MemoryPendingActionStore",
    "PendingAction",
    "PendingActionConflictError",
    "PendingActionStatus",
    "PendingActionStore",
    "PendingActionStoreProtocol",
    "PostgresPendingActionStore",
    "get_pending_action_store",
    "set_pending_action_store",
]
