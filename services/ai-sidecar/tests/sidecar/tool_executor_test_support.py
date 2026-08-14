"""Test support for exercising ToolExecutor after a Rust authorization decision."""

from __future__ import annotations

from types import SimpleNamespace
from typing import Any

from src.infrastructure.ai.tools.read_only_tools import ReadOnlyBackend
from src.infrastructure.ai.tools.tool_executor import ToolExecutor


class FakeReadOnlyBackend(ReadOnlyBackend):
    """Deterministic read-only backend stand-in for tests that execute tools."""

    async def execute_read_only(self, tool_name: str, arguments: dict[str, Any]) -> dict[str, Any]:
        return {
            **arguments,
            "status": "on_time",
            "source": "test_fake",
        }


class AuthorizedToolMqGate:
    """Minimal gate fake representing a successful Rust ``tool_lease`` decision."""

    async def request_authorization(self, **_: Any) -> SimpleNamespace:
        return SimpleNamespace(mode="execute", context=None)

    async def start_heartbeat(self, *, context: Any) -> tuple[None, None]:
        return None, None

    async def stop_heartbeat(self, task: Any, stop_event: Any) -> None:
        return None

    async def publish_result(self, **_: Any) -> None:
        return None


def authorized_tool_executor(**kwargs: Any) -> ToolExecutor:
    """Build a ToolExecutor whose protected calls have explicit test authorization."""

    kwargs.setdefault("read_only_backend", FakeReadOnlyBackend())
    return ToolExecutor(mq_gate=AuthorizedToolMqGate(), **kwargs)
