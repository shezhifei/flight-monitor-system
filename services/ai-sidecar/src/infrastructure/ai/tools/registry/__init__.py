"""AI tool registry and execution coordination.

Facade package that re-exports the public API surface of the tool registry.
Implementation lives in :mod:`.service` and module-level state in :mod:`.models`.
"""

from .service import (
    DynamicToolRegistry,
    ToolRegistry,
    create_tool_registry,
    get_tool_registry,
    set_tool_registry,
    use_tool_registry,
)

__all__ = [
    "DynamicToolRegistry",
    "ToolRegistry",
    "create_tool_registry",
    "get_tool_registry",
    "set_tool_registry",
    "use_tool_registry",
]
