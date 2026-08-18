"""Read-only solver candidate tool for dispatch_ops (Task I1).

Asserts:

1. The sidecar client calls the Rust internal endpoint
   ``POST /internal/ai/v1/dispatch/replan-snapshot`` with a path-scoped
   Service Identity token — never the user-JWT face, never ``replan-apply``.
2. Failures surface as ``SolverCandidateClientError`` (fail-closed).
3. ``SolverTools.list_solver_candidates`` shapes the snapshot into
   candidates carrying ``source=dispatch.list_solver_candidates``, per-order
   ``object_id`` and ``as_of``.
4. ``tool_executor`` routes the tool as a distinct ``solver`` type through
   the injected adapter; without an adapter the call fails closed.
5. The tool is registered in the builtin catalog; ``replan-apply`` is NOT on
   the agent tool surface.
"""

from __future__ import annotations

import json as _json
from typing import Any

import httpx
import pytest

from src.infrastructure.ai.service_identity import SERVICE_IDENTITY_HEADER
from src.infrastructure.ai.service_identity_issuer import ServiceIdentityIssuer
from src.infrastructure.ai.tools.solver_tools import (
    REPLAN_SNAPSHOT_PATH,
    SOLVER_TOOL_NAME,
    SolverCandidateClient,
    SolverCandidateClientError,
    SolverTools,
    is_solver_tool,
)
from src.infrastructure.ai.tools.tool_executor import ToolExecutor
from tests.sidecar.tool_executor_test_support import AuthorizedToolMqGate

BASE_URL_ENV_KEYS = ("AI_INTERNAL_API_URL", "RUST_API_BASE_URL", "AI_API_BASE_URL")


def _clear_base_url_env(monkeypatch: pytest.MonkeyPatch) -> None:
    for key in BASE_URL_ENV_KEYS:
        monkeypatch.delenv(key, raising=False)


def _issuer() -> ServiceIdentityIssuer:
    return ServiceIdentityIssuer("test-secret")


def _client_with_handler(handler) -> SolverCandidateClient:
    transport_client = httpx.AsyncClient(transport=httpx.MockTransport(handler))
    return SolverCandidateClient(
        base_url="http://localhost:8080",
        issuer=_issuer(),
        client=transport_client,
    )


def _snapshot_payload() -> dict[str, Any]:
    return {
        "snapshot_id": "snap-1",
        "strategy": "balanced",
        "window_start": "2026-08-18T00:00:00Z",
        "window_end": "2026-08-18T06:00:00Z",
        "orders": [
            {"dispatch_order_id": "o-1", "flight_id": "F101", "status": "pending"},
            {"dispatch_order_id": "o-2", "flight_id": "F102", "status": "assigned"},
        ],
        "optimizable_orders": [],
    }


# ---------------------------------------------------------------------------
# Fail-closed client
# ---------------------------------------------------------------------------


def test_missing_base_url_fails_at_construction(monkeypatch: pytest.MonkeyPatch) -> None:
    _clear_base_url_env(monkeypatch)
    with pytest.raises(SolverCandidateClientError) as excinfo:
        SolverCandidateClient(issuer=_issuer())
    assert excinfo.value.code == "no_rust_api_base_url"


@pytest.mark.asyncio
async def test_replan_snapshot_posts_signed_path_and_window() -> None:
    captured: dict = {}

    def handler(request: httpx.Request) -> httpx.Response:
        captured["request"] = request
        return httpx.Response(200, json=_snapshot_payload())

    client = _client_with_handler(handler)
    result = await client.replan_snapshot(
        run_id="run_1",
        window_start="2026-08-18T00:00:00Z",
        window_end="2026-08-18T06:00:00Z",
        strategy="balanced",
        order_ids=["o-1"],
    )

    assert result["snapshot_id"] == "snap-1"
    request = captured["request"]
    assert request.url.path == REPLAN_SNAPSHOT_PATH
    assert SERVICE_IDENTITY_HEADER in request.headers
    payload = _json.loads(request.read())
    assert payload["run_id"] == "run_1"
    assert payload["window_start"] == "2026-08-18T00:00:00Z"
    assert payload["window_end"] == "2026-08-18T06:00:00Z"
    assert payload["strategy"] == "balanced"
    assert payload["order_ids"] == ["o-1"]


@pytest.mark.asyncio
async def test_rejected_response_maps_to_typed_error() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        return httpx.Response(
            403,
            json={
                "success": False,
                "error_code": "TOOL_ACTOR_PERMISSION_DENIED",
                "error": "requester lacks permission 'dispatch:read'",
            },
        )

    client = _client_with_handler(handler)
    with pytest.raises(SolverCandidateClientError) as excinfo:
        await client.replan_snapshot(
            run_id="run_1",
            window_start="2026-08-18T00:00:00Z",
            window_end="2026-08-18T06:00:00Z",
        )
    assert excinfo.value.status_code == 403
    assert excinfo.value.error_code == "TOOL_ACTOR_PERMISSION_DENIED"


@pytest.mark.asyncio
async def test_transport_error_fails_closed() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        raise httpx.ConnectError("connection refused", request=request)

    client = _client_with_handler(handler)
    with pytest.raises(SolverCandidateClientError) as excinfo:
        await client.replan_snapshot(
            run_id="run_1",
            window_start="2026-08-18T00:00:00Z",
            window_end="2026-08-18T06:00:00Z",
        )
    assert excinfo.value.code == "transport_error"


# ---------------------------------------------------------------------------
# Result shaping: source / object_id / as_of
# ---------------------------------------------------------------------------


class _RecordingClient:
    def __init__(self, payload: dict[str, Any] | None = None) -> None:
        self.calls: list[dict] = []
        self._payload = payload if payload is not None else _snapshot_payload()

    async def replan_snapshot(self, **kwargs: Any) -> dict[str, Any]:
        self.calls.append(kwargs)
        return dict(self._payload)


@pytest.mark.asyncio
async def test_candidates_carry_source_object_id_and_as_of() -> None:
    client = _RecordingClient()
    tools = SolverTools(client=client)
    result = await tools.list_solver_candidates(
        run_id="run_1",
        window_start="2026-08-18T00:00:00Z",
        window_end="2026-08-18T06:00:00Z",
        strategy="balanced",
    )

    assert result["source"] == SOLVER_TOOL_NAME
    assert result["as_of"]
    assert result["snapshot_id"] == "snap-1"
    candidates = result["candidates"]
    assert [c["object_id"] for c in candidates] == ["o-1", "o-2"]
    assert client.calls[0]["run_id"] == "run_1"
    # The evidence block lets grounding/freshness hooks see the provenance.
    assert result["evidence"]["source"] == SOLVER_TOOL_NAME
    assert result["evidence"]["as_of"] == result["as_of"]


@pytest.mark.asyncio
async def test_order_ids_filter_narrows_candidates() -> None:
    tools = SolverTools(client=_RecordingClient())
    result = await tools.list_solver_candidates(
        run_id="run_1",
        window_start="2026-08-18T00:00:00Z",
        window_end="2026-08-18T06:00:00Z",
        order_ids=["o-2"],
    )
    assert [c["object_id"] for c in result["candidates"]] == ["o-2"]


@pytest.mark.asyncio
async def test_orders_fall_back_to_optimizable_orders() -> None:
    payload = _snapshot_payload()
    payload["orders"] = []
    payload["optimizable_orders"] = [{"dispatch_order_id": "o-9"}]
    tools = SolverTools(client=_RecordingClient(payload))
    result = await tools.list_solver_candidates(
        run_id="run_1",
        window_start="2026-08-18T00:00:00Z",
        window_end="2026-08-18T06:00:00Z",
    )
    assert [c["object_id"] for c in result["candidates"]] == ["o-9"]


# ---------------------------------------------------------------------------
# Executor routing
# ---------------------------------------------------------------------------


def _executor(**kwargs: Any) -> ToolExecutor:
    kwargs.setdefault("mq_gate", AuthorizedToolMqGate())
    return ToolExecutor(**kwargs)


def test_tool_type_is_solver() -> None:
    executor = _executor()
    assert is_solver_tool(SOLVER_TOOL_NAME)
    assert executor.get_tool_type(SOLVER_TOOL_NAME) == "solver"
    assert not is_solver_tool("ontology.lookup")
    assert not is_solver_tool("flight_status_lookup")
    assert SOLVER_TOOL_NAME in executor.get_available_tools()


@pytest.mark.asyncio
async def test_executor_routes_to_solver_adapter() -> None:
    client = _RecordingClient()
    executor = _executor(solver_tools=SolverTools(client=client))
    result = await executor.execute(
        {
            "tool_call_id": "c1",
            "tool_name": SOLVER_TOOL_NAME,
            "arguments": {
                "window_start": "2026-08-18T00:00:00Z",
                "window_end": "2026-08-18T06:00:00Z",
                "strategy": "efficiency",
            },
        },
        run_id="run_9",
    )
    assert result.success is True
    assert result.result["source"] == SOLVER_TOOL_NAME
    assert client.calls[0]["strategy"] == "efficiency"


@pytest.mark.asyncio
async def test_executor_fails_closed_without_solver_adapter() -> None:
    executor = _executor()
    result = await executor.execute(
        {
            "tool_call_id": "c2",
            "tool_name": SOLVER_TOOL_NAME,
            "arguments": {
                "window_start": "2026-08-18T00:00:00Z",
                "window_end": "2026-08-18T06:00:00Z",
            },
        },
        run_id="run_9",
    )
    assert result.success is False
    assert "SOLVER_CLIENT_NOT_CONFIGURED" in (result.error or "")


@pytest.mark.asyncio
async def test_executor_rejects_missing_window_bounds() -> None:
    executor = _executor(solver_tools=SolverTools(client=_RecordingClient()))
    result = await executor.execute(
        {"tool_call_id": "c3", "tool_name": SOLVER_TOOL_NAME, "arguments": {}},
        run_id="run_9",
    )
    assert result.success is False
    assert "INVALID_TOOL_ARGUMENTS" in (result.error or "")


@pytest.mark.asyncio
async def test_client_error_surfaces_as_failed_result() -> None:
    class _Boom:
        async def replan_snapshot(self, **kwargs: Any) -> dict[str, Any]:
            raise SolverCandidateClientError(
                "solver_snapshot_rejected", "denied", status_code=403,
                error_code="TOOL_ACTOR_PERMISSION_DENIED",
            )

    executor = _executor(solver_tools=SolverTools(client=_Boom()))
    result = await executor.execute(
        {
            "tool_call_id": "c4",
            "tool_name": SOLVER_TOOL_NAME,
            "arguments": {
                "window_start": "2026-08-18T00:00:00Z",
                "window_end": "2026-08-18T06:00:00Z",
            },
        },
        run_id="run_9",
    )
    assert result.success is False
    assert "TOOL_ACTOR_PERMISSION_DENIED" in (result.error or "")


# ---------------------------------------------------------------------------
# Catalog registration; replan-apply never on the agent surface
# ---------------------------------------------------------------------------


def test_catalog_registers_solver_candidates_tool() -> None:
    from src.infrastructure.ai.ai_runtime_bootstrap import _builtin_tool_catalog

    catalog = {entry["name"]: entry for entry in _builtin_tool_catalog()}
    assert SOLVER_TOOL_NAME in catalog
    entry = catalog[SOLVER_TOOL_NAME]
    properties = entry["parameters"]["properties"]
    assert properties["strategy"]["enum"] == ["stability", "balanced", "efficiency"]
    assert "window_start" in properties and "window_end" in properties
    assert entry["side_effect"] is False
    assert entry["operation_level"] == "l0_read"


def test_replan_apply_is_not_on_agent_surface() -> None:
    from src.infrastructure.ai.ai_runtime_bootstrap import _builtin_tool_catalog

    names = {entry["name"] for entry in _builtin_tool_catalog()}
    assert not any("replan" in name and name != SOLVER_TOOL_NAME for name in names)
    executor = _executor()
    assert not any("replan_apply" in name.replace("-", "_") for name in executor.get_available_tools())


def test_freshness_limits_cover_solver_candidates() -> None:
    from src.infrastructure.ai.templates.shadow_mode_config import TOOL_FRESHNESS_LIMITS

    assert SOLVER_TOOL_NAME in TOOL_FRESHNESS_LIMITS
