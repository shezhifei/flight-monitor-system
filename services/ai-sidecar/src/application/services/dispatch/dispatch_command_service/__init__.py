"""Dispatch Command Service package — facade re-exporting public symbols.

External code should continue to import from
``src.application.services.dispatch.dispatch_command_service``; this
``__init__`` delegates to the :mod:`.service` submodule.
"""

from __future__ import annotations

from .service import DispatchCommandApplicationService

__all__ = ["DispatchCommandApplicationService"]
