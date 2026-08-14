"""Task A2 — mock teardown: read-only tools must not be fake-data mocks.

Asserts (docs/plans/2026-08-14-hybrid-agent-architecture.md, Task A2 Step 1):

1. ``READ_ONLY_TOOLS`` implementations do not ``asyncio.sleep`` then return a
   hardcoded ``gate: A12``.
2. ``flight_status_lookup`` (when registered) must delegate to a real read-only
   backend and its result must carry a ``source`` field.
3. Executing a read-only tool without a wired backend fails closed with
   ``READ_ONLY_BACKEND_NOT_CONFIGURED`` instead of returning fabricated data.
4. The former mock names are no longer registered as read-only tools, and the
   real query catalog tools are classified as read-only.
"""

from __future__ import annotations

import asyncio
import inspect

import pytest

from src.infrastructure.ai.tools import read_only_tools as ro
from src.infrastructure.ai.tools.query_tools import QueryToolName


def _run(coro):
    loop = asyncio.new_event_loop()
    try:
        return loop.run_until_complete(coro)
    finally:
        loop.close()


def test_read_only_tools_have_no_mock_impls() -> None:
    src = inspect.getsource(ro)
    assert "asyncio.sleep" not in src, "read-only tools must not fake latency"
    assert '"A12"' not in src, "read-only tools must not return hardcoded gates"
    assert set(ro.READ_ONLY_TOOLS) == {"flight_status_lookup"}, (
        f"mock tools still registered: {sorted(ro.READ_ONLY_TOOLS)}"
    )


def test_former_mock_names_are_not_registered() -> None:
    for name in (
        "flight_list_by_date",
        "weather_at_airport",
        "get_flight_crew_info",
        "query_resource_availability",
    ):
        assert name not in ro.READ_ONLY_TOOLS, f"{name} must be removed"
        assert ro.is_read_only_tool(name) is False, f"{name} must not classify as read-only"


def test_query_catalog_tools_are_read_only() -> None:
    assert ro.is_read_only_tool(QueryToolName.SEARCH_FLIGHTS_ADVANCED.value) is True
    assert ro.is_read_only_tool(QueryToolName.GET_DELAYED_FLIGHTS.value) is True


class _StubBackend:
    """Backend stub asserting delegation; results carry ``source``."""

    def __init__(self) -> None:
        self.calls: list[tuple[str, dict]] = []

    async def execute_read_only(self, tool_name: str, arguments: dict) -> dict:
        self.calls.append((tool_name, arguments))
        return {
            "flight_id": arguments.get("flight_id", ""),
            "status": "scheduled",
            "source": "ai_query.v_flights",
        }


def test_flight_status_lookup_delegates_to_backend_with_source() -> None:
    backend = _StubBackend()
    ro.set_read_only_backend(backend)
    try:
        result = _run(ro.flight_status_lookup("CA1234"))
    finally:
        ro.set_read_only_backend(None)
    assert backend.calls == [("flight_status_lookup", {"flight_id": "CA1234"})]
    assert result["source"] == "ai_query.v_flights"


def test_execute_read_only_fails_closed_without_backend() -> None:
    ro.set_read_only_backend(None)
    with pytest.raises(RuntimeError, match="READ_ONLY_BACKEND_NOT_CONFIGURED"):
        _run(ro.execute_read_only_tool("flight_status_lookup", {"flight_id": "CA1"}))


def test_execute_read_only_unknown_tool_still_rejected() -> None:
    with pytest.raises(ValueError, match="Unknown read-only tool"):
        _run(ro.execute_read_only_tool("unknown_tool", {}))
