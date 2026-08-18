"""Contract tests for the fail-closed ontology action client (Task F2).

The client is the sidecar's only path to execute registered ontology
read/advisory actions: it calls the Rust internal endpoints with a
Service Identity token scoped to the exact path. Failures must surface
as ``OntologyActionClientError`` — never as stub entities.
"""

from __future__ import annotations

import httpx
import pytest

from src.infrastructure.ai.ontology.action_client import (
    ADVISORY_PATH,
    READ_PATH,
    OntologyActionClient,
    OntologyActionClientError,
)
from src.infrastructure.ai.service_identity import SERVICE_IDENTITY_HEADER
from src.infrastructure.ai.service_identity_issuer import ServiceIdentityIssuer

BASE_URL_ENV_KEYS = ("AI_INTERNAL_API_URL", "RUST_API_BASE_URL", "AI_API_BASE_URL")


def _clear_base_url_env(monkeypatch: pytest.MonkeyPatch) -> None:
    for key in BASE_URL_ENV_KEYS:
        monkeypatch.delenv(key, raising=False)


def _issuer() -> ServiceIdentityIssuer:
    return ServiceIdentityIssuer("test-secret")


def _client_with_handler(handler, *, issuer=None) -> OntologyActionClient:
    transport_client = httpx.AsyncClient(transport=httpx.MockTransport(handler))
    return OntologyActionClient(
        base_url="http://localhost:8080",
        issuer=issuer or _issuer(),
        client=transport_client,
    )


def test_missing_base_url_fails_at_construction(monkeypatch: pytest.MonkeyPatch) -> None:
    _clear_base_url_env(monkeypatch)
    with pytest.raises(OntologyActionClientError) as excinfo:
        OntologyActionClient(issuer=_issuer())
    assert excinfo.value.code == "no_rust_api_base_url"


@pytest.mark.asyncio
async def test_read_returns_body_and_signs_exact_path() -> None:
    captured: dict = {}

    def handler(request: httpx.Request) -> httpx.Response:
        captured["request"] = request
        return httpx.Response(200, json={"flights": [], "total": 0})

    client = _client_with_handler(handler)
    result = await client.read(run_id="run_1", action_name="flight.search", arguments={"limit": 5})

    assert result == {"flights": [], "total": 0}
    request = captured["request"]
    assert request.url.path == READ_PATH
    assert SERVICE_IDENTITY_HEADER in request.headers
    body = request.read()
    import json as _json

    payload = _json.loads(body)
    assert payload["run_id"] == "run_1"
    assert payload["action_name"] == "flight.search"
    assert payload["arguments"] == {"limit": 5}


@pytest.mark.asyncio
async def test_advisory_hits_advisory_path() -> None:
    captured: dict = {}

    def handler(request: httpx.Request) -> httpx.Response:
        captured["request"] = request
        return httpx.Response(200, json={"proposal": {}})

    client = _client_with_handler(handler)
    await client.advisory(run_id="run_1", action_name="dispatch.suggest_replan", arguments={})

    assert captured["request"].url.path == ADVISORY_PATH


@pytest.mark.asyncio
async def test_forbidden_maps_to_typed_error_with_error_code() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        return httpx.Response(
            403,
            json={
                "success": False,
                "error_code": "TOOL_ACTOR_PERMISSION_DENIED",
                "error": "requester lacks permission 'flight:read'",
            },
        )

    client = _client_with_handler(handler)
    with pytest.raises(OntologyActionClientError) as excinfo:
        await client.read(run_id="run_1", action_name="flight.search", arguments={})

    error = excinfo.value
    assert error.status_code == 403
    assert error.error_code == "TOOL_ACTOR_PERMISSION_DENIED"


@pytest.mark.asyncio
async def test_not_found_maps_to_typed_error_with_error_code() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        return httpx.Response(
            404,
            json={"success": False, "error_code": "AI_RUN_NOT_FOUND", "error": "run not found"},
        )

    client = _client_with_handler(handler)
    with pytest.raises(OntologyActionClientError) as excinfo:
        await client.read(run_id="run_missing", action_name="flight.search", arguments={})

    assert excinfo.value.status_code == 404
    assert excinfo.value.error_code == "AI_RUN_NOT_FOUND"


@pytest.mark.asyncio
async def test_missing_issuer_fails_closed(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv("JWT_SECRET", raising=False)
    monkeypatch.delenv("JWT_SECRET_KEY", raising=False)
    from src.infrastructure.ai.service_identity import get_jwt_secret

    get_jwt_secret.cache_clear()
    try:
        transport_client = httpx.AsyncClient(
            transport=httpx.MockTransport(lambda request: httpx.Response(200, json={}))
        )
        client = OntologyActionClient(
            base_url="http://localhost:8080",
            issuer=None,
            client=transport_client,
        )
        with pytest.raises(OntologyActionClientError) as excinfo:
            await client.read(run_id="run_1", action_name="flight.search", arguments={})
        assert excinfo.value.code == "no_service_identity_issuer"
    finally:
        get_jwt_secret.cache_clear()


@pytest.mark.asyncio
async def test_transport_error_fails_closed() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        raise httpx.ConnectError("connection refused", request=request)

    client = _client_with_handler(handler)
    with pytest.raises(OntologyActionClientError) as excinfo:
        await client.read(run_id="run_1", action_name="flight.search", arguments={})

    assert excinfo.value.code == "transport_error"
    assert excinfo.value.status_code is None
