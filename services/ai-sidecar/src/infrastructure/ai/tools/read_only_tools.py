"""Read-only tools that execute locally in the Python sidecar.

These tools query state but never mutate it. Write actions must go through
Rust DomainActionExecutor via the proposal-ingest path.

Production rule (docs/architecture/AGENT_RUNTIME_LOOP.md):
* No mock tool fallback on the production path.
* ``flight_status_lookup`` is a thin adapter that delegates to the read-only
  backend registered at composition time (``set_read_only_backend``).
* Executing a read-only tool without a wired backend fails closed with
  ``ReadOnlyBackendNotConfigured`` — fabricated data is never returned.
* The builtin query catalog tools (``query_tools.QUERY_TOOL_DEFINITIONS``) are
  classified as read-only and dispatched to the same backend.
"""

from __future__ import annotations

from typing import Any, Protocol

from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class ReadOnlyBackendNotConfigured(RuntimeError):  # noqa: N818 - public compatibility name
    """Raised when a read-only tool is executed without a wired backend."""


class ReadOnlyBackend(Protocol):
    """Real read-only data backend injected at composition time.

    Implementations query the ``ai_query`` views (or an equivalent read-only
    surface) and must include a ``source`` field in their results.
    """

    async def execute_read_only(self, tool_name: str, arguments: dict[str, Any]) -> dict[str, Any]: ...


# ---------------------------------------------------------------------------
# Backend wiring (composition time only; tests may override and restore)
# ---------------------------------------------------------------------------

_READ_ONLY_BACKEND: ReadOnlyBackend | None = None


def set_read_only_backend(backend: ReadOnlyBackend | None) -> None:
    """Register the read-only backend used by the thin adapters."""
    global _READ_ONLY_BACKEND
    _READ_ONLY_BACKEND = backend


def get_read_only_backend() -> ReadOnlyBackend | None:
    """Return the currently wired read-only backend (``None`` when absent)."""
    return _READ_ONLY_BACKEND


# ---------------------------------------------------------------------------
# Thin adapters (no fake data)
# ---------------------------------------------------------------------------


async def flight_status_lookup(flight_id: str) -> dict[str, Any]:
    """Look up the status of a flight by its identifier.

    Thin adapter: delegates to the wired read-only backend (``ai_query``
    read path). Fails closed with :class:`ReadOnlyBackendNotConfigured` when
    no backend is registered.

    Args:
        flight_id: The unique identifier for the flight (e.g., "CA1234").

    Returns:
        A dictionary containing flight status information; the result carries
        a ``source`` field naming the read surface it came from.
    """
    return await execute_read_only_tool("flight_status_lookup", {"flight_id": flight_id})


# ---------------------------------------------------------------------------
# Tool registry
# ---------------------------------------------------------------------------

# Mapping of locally-adapter tool names to their async implementations.
READ_ONLY_TOOLS: dict[str, Any] = {
    "flight_status_lookup": flight_status_lookup,
}

# Builtin query catalog tools are real read-only tools dispatched to the backend.
from .query_tools import QueryToolName as _QueryToolName  # noqa: E402

_QUERY_TOOL_NAMES: frozenset[str] = frozenset(t.value for t in _QueryToolName)


def get_read_only_tool_names() -> list[str]:
    """Return the list of available read-only tool names (adapters + catalog)."""
    return list(READ_ONLY_TOOLS.keys()) + sorted(_QUERY_TOOL_NAMES)


def is_read_only_tool(tool_name: str) -> bool:
    """Check if a tool name is a read-only tool (adapter or query catalog)."""
    return tool_name in READ_ONLY_TOOLS or tool_name in _QUERY_TOOL_NAMES


async def execute_read_only_tool(
    tool_name: str,
    arguments: dict[str, Any],
    backend: ReadOnlyBackend | None = None,
) -> dict[str, Any]:
    """Execute a read-only tool by name with the given arguments.

    Dispatch order: the passed-in backend wins, otherwise the composition-time
    backend. When neither is wired the call fails closed.

    Args:
        tool_name: The name of the read-only tool to execute.
        arguments: Keyword arguments to pass to the tool function.
        backend: Optional backend override (used by ToolExecutor injection).

    Returns:
        The result from the tool execution.

    Raises:
        ValueError: If the tool name is not a registered read-only tool.
        ReadOnlyBackendNotConfigured: If no backend is wired.
    """
    if not is_read_only_tool(tool_name):
        raise ValueError(f"Unknown read-only tool: {tool_name}")

    resolved_backend = backend if backend is not None else _READ_ONLY_BACKEND
    if resolved_backend is None:
        raise ReadOnlyBackendNotConfigured(
            "READ_ONLY_BACKEND_NOT_CONFIGURED: no read-only backend is wired; refusing to fabricate tool results"
        )

    try:
        return await resolved_backend.execute_read_only(tool_name, arguments)
    except TypeError as exc:
        # Handle mismatched arguments
        raise ValueError(f"Invalid arguments for tool '{tool_name}': {exc}") from exc
