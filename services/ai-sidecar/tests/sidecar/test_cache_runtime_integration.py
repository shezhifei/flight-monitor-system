"""Phase C tests: real MCP resource cache + context cache read/write paths."""

from __future__ import annotations

import asyncio
from types import SimpleNamespace

import pytest

from src.infrastructure.ai.context_envelope import (
    ContextEnvelope,
    EnvelopeContext,
    EnvelopeOntology,
    EnvelopeRequester,
    EnvelopeTask,
)
from src.infrastructure.ai.mcp.client_manager import McpClientManager
from src.infrastructure.ai.openai_client import Message, MessageRole
from src.infrastructure.ai.runtime_service import RuntimeService


def _run(coro):
    return asyncio.run(coro)


# ---------------------------------------------------------------------------
# Fakes
# ---------------------------------------------------------------------------


class FakeCacheManager:
    """In-memory async cache stand-in tracking call counts."""

    def __init__(self):
        self._mcp = {}
        self._ctx = {}
        self.mcp_get_calls = 0
        self.mcp_set_calls = 0
        self.ctx_get_calls = 0
        self.ctx_set_calls = 0

    async def get_mcp_resource(self, server_id, resource_uri, ttl_seconds=300, entity_id=None):
        self.mcp_get_calls += 1
        return self._mcp.get((entity_id, server_id, resource_uri))

    async def set_mcp_resource(self, server_id, resource_uri, content, ttl_seconds=300, entity_id=None):
        self.mcp_set_calls += 1
        self._mcp[(entity_id, server_id, resource_uri)] = content

    async def get_context(self, entity_id, conversation_id):
        self.ctx_get_calls += 1
        return self._ctx.get((entity_id, conversation_id))

    async def set_context(self, entity_id, conversation_id, context, ttl_seconds=86400):
        self.ctx_set_calls += 1
        self._ctx[(entity_id, conversation_id)] = context


class FakeMcpAclRepo:
    async def find_bindings_by_entity(self, entity_id):
        return [
            {
                "entity_id": entity_id,
                "server_id": "srv1",
                "enabled": True,
                "allowed_resources": [],
            }
        ]


def _connected_manager(cache_manager, rpc_result):
    mgr = McpClientManager(mcp_repo=FakeMcpAclRepo(), cache_manager=cache_manager)
    mgr._sessions["srv1"] = SimpleNamespace(status="connected")
    calls = {"n": 0}

    async def _fake_send(session, request, timeout=None):
        calls["n"] += 1
        return {"result": rpc_result}

    mgr._send_request = _fake_send  # type: ignore[assignment]
    return mgr, calls


# ---------------------------------------------------------------------------
# MCP resource cache
# ---------------------------------------------------------------------------


class TestMcpResourceCache:
    def test_miss_then_hit(self):
        cache = FakeCacheManager()
        rpc_result = {"contents": [{"uri": "file://a", "text": "hello world"}]}
        mgr, calls = _connected_manager(cache, rpc_result)

        # First read: cache miss -> RPC -> cache write.
        first = _run(mgr.read_resource("srv1", "file://a", entity_id="default"))
        assert first["cached"] is False
        assert first["content"] == "hello world"
        assert calls["n"] == 1
        assert cache.mcp_set_calls == 1

        # Second read: cache hit -> NO RPC.
        second = _run(mgr.read_resource("srv1", "file://a", entity_id="default"))
        assert second["cached"] is True
        assert second["content"] == "hello world"
        assert calls["n"] == 1  # unchanged -> no second RPC

    def test_multipart_text_concatenated(self):
        cache = FakeCacheManager()
        rpc_result = {
            "contents": [
                {"text": "part1"},
                {"text": "part2"},
                {"blob": "x", "mimeType": "image/png"},
            ]
        }
        mgr, _ = _connected_manager(cache, rpc_result)
        out = _run(mgr.read_resource("srv1", "file://multi", entity_id="default"))
        assert "part1" in out["content"]
        assert "part2" in out["content"]
        assert "[binary:image/png]" in out["content"]

    def test_no_cache_manager_still_reads(self):
        mgr = McpClientManager(mcp_repo=FakeMcpAclRepo(), cache_manager=None)
        mgr._sessions["srv1"] = SimpleNamespace(status="connected")

        async def _fake_send(session, request, timeout=None):
            return {"result": {"contents": [{"text": "no-cache"}]}}

        mgr._send_request = _fake_send  # type: ignore[assignment]
        out = _run(mgr.read_resource("srv1", "file://x", entity_id="default"))
        assert out["content"] == "no-cache"
        assert out["cached"] is False

    def test_disconnected_server_raises(self):
        cache = FakeCacheManager()
        mgr = McpClientManager(mcp_repo=FakeMcpAclRepo(), cache_manager=cache)
        with pytest.raises(RuntimeError):
            _run(mgr.read_resource("srv1", "file://x", entity_id="default"))


# ---------------------------------------------------------------------------
# Context cache (RuntimeService helpers)
# ---------------------------------------------------------------------------


def _resolved_with_context_cache(enabled=True, ttl=123):
    cache_policy = SimpleNamespace(
        enabled=True,
        context_cache_enabled=enabled,
        context_cache_ttl=ttl,
    )
    return SimpleNamespace(cache_policy=cache_policy)


def _envelope(correlation_id="corr-1"):
    return ContextEnvelope(
        run_id="run-1",
        correlation_id=correlation_id,
        requester=EnvelopeRequester(user_id="u1"),
        ontology=EnvelopeOntology(),
        context=EnvelopeContext(),
        task=EnvelopeTask(task_type="chat", user_message="hi"),
    )


class TestContextCache:
    def test_write_then_read_round_trip(self):
        cache = FakeCacheManager()
        rs = RuntimeService(cache_manager=cache)
        resolved = _resolved_with_context_cache()
        messages = [
            Message(role=MessageRole.SYSTEM, content="sys"),
            Message(role=MessageRole.USER, content="hello"),
        ]

        wrote = _run(rs._write_context_cache(resolved, "default", "conv-1", messages))
        assert wrote is True
        assert cache.ctx_set_calls == 1

        cached = _run(rs._read_context_cache(resolved, "default", "conv-1"))
        assert cached is not None
        assert cached["messages"][0]["role"] == "system"

    def test_disabled_policy_is_noop(self):
        cache = FakeCacheManager()
        rs = RuntimeService(cache_manager=cache)
        resolved = _resolved_with_context_cache(enabled=False)
        wrote = _run(rs._write_context_cache(resolved, "default", "c", []))
        assert wrote is False
        assert cache.ctx_set_calls == 0
        assert _run(rs._read_context_cache(resolved, "default", "c")) is None
        assert cache.ctx_get_calls == 0

    def test_no_cache_manager_is_noop(self):
        rs = RuntimeService(cache_manager=None)
        resolved = _resolved_with_context_cache()
        assert rs._context_cache_enabled(resolved) is False
        assert _run(rs._read_context_cache(resolved, "default", "c")) is None

    def test_conversation_id_prefers_correlation_id(self):
        rs = RuntimeService()
        env = _envelope(correlation_id="corr-xyz")
        assert rs._context_conversation_id(env, "run-1") == "corr-xyz"

    def test_conversation_id_falls_back_to_run_id(self):
        rs = RuntimeService()
        env = _envelope(correlation_id="")
        # job_id also empty by default -> run_id.
        assert rs._context_conversation_id(env, "run-9") == "run-9"


# ---------------------------------------------------------------------------
# MCP resource-read endpoint (security + cache-first)
# ---------------------------------------------------------------------------


class FakeMcpRepo:
    def __init__(self, server=None, bindings=None):
        self._server = server
        self._bindings = bindings or []

    async def find_server_by_id(self, server_id):
        return self._server

    async def find_bindings_by_entity(self, entity_id):
        return self._bindings


def _resource_app(mcp_repo, cache_manager):
    from unittest.mock import patch

    from fastapi import FastAPI
    from fastapi.testclient import TestClient

    from src.infrastructure.ai.management_routes import router

    app = FastAPI()
    app.include_router(router)
    repos = {
        "capability_resolver": None,
        "mcp_repo": mcp_repo,
        "mcp_client_manager": None,
        "skill_repo": None,
        "cache_metrics_repo": None,
        "cache_manager": cache_manager,
        "model_catalog_repo": None,
    }
    s1 = patch("src.infrastructure.ai.management_routes._resolve_repos", return_value=repos)
    s2 = patch(
        "src.infrastructure.ai.management_routes.require_service_identity",
        return_value=None,
    )
    s1.start()
    s2.start()
    return TestClient(app), s1, s2


class TestMcpResourceEndpoint:
    def test_cache_hit_returns_without_subprocess(self):
        import json
        from unittest.mock import patch

        from src.infrastructure.ai.mcp.command_allowlist import reset_cache

        cache = FakeCacheManager()
        cache._mcp[("default", "srv1", "file://ok")] = "cached-content"
        server = {
            "server_id": "srv1",
            "status": "active",
            "transport": "stdio",
            "command_ref": "npx",
            "cache": {"resources_ttl_seconds": 120},
        }
        bindings = [{"server_id": "srv1", "enabled": True, "allowed_resources": ["file://ok"]}]
        client, s1, s2 = _resource_app(FakeMcpRepo(server, bindings), cache)
        try:
            # command_ref must be allowlisted: the allowlist check now precedes
            # the cache read so a warm cache cannot bypass it.
            allowlist_json = json.dumps({"npx": {"executable": "npx", "args_prefix": []}})
            with patch.dict("os.environ", {"AI_MCP_COMMAND_ALLOWLIST_JSON": allowlist_json}):
                reset_cache()
                resp = client.post(
                    "/internal/ai/v1/entities/default/mcp/servers/srv1/resource",
                    json={"resource_uri": "file://ok"},
                )
            assert resp.status_code == 200, resp.text
            data = resp.json()["data"]
            assert data["cached"] is True
            assert data["content"] == "cached-content"
        finally:
            reset_cache()
            s1.stop()
            s2.stop()

    def test_resource_not_in_allowed_resources_is_denied(self):
        cache = FakeCacheManager()
        server = {"server_id": "srv1", "status": "active", "transport": "stdio", "command_ref": "npx"}
        bindings = [{"server_id": "srv1", "enabled": True, "allowed_resources": ["file://ok"]}]
        client, s1, s2 = _resource_app(FakeMcpRepo(server, bindings), cache)
        try:
            resp = client.post(
                "/internal/ai/v1/entities/default/mcp/servers/srv1/resource",
                json={"resource_uri": "file://blocked"},
            )
            assert resp.status_code == 403, resp.text
            assert cache.mcp_get_calls == 0  # denied before any cache/subprocess work
        finally:
            s1.stop()
            s2.stop()

    def test_no_enabled_binding_is_denied(self):
        cache = FakeCacheManager()
        server = {"server_id": "srv1", "status": "active", "transport": "stdio", "command_ref": "npx"}
        bindings = [{"server_id": "srv1", "enabled": False, "allowed_resources": []}]
        client, s1, s2 = _resource_app(FakeMcpRepo(server, bindings), cache)
        try:
            resp = client.post(
                "/internal/ai/v1/entities/default/mcp/servers/srv1/resource",
                json={"resource_uri": "file://x"},
            )
            assert resp.status_code == 403, resp.text
        finally:
            s1.stop()
            s2.stop()

    def test_missing_resource_uri_is_422(self):
        cache = FakeCacheManager()
        client, s1, s2 = _resource_app(FakeMcpRepo({}, []), cache)
        try:
            resp = client.post(
                "/internal/ai/v1/entities/default/mcp/servers/srv1/resource",
                json={},
            )
            assert resp.status_code == 422, resp.text
        finally:
            s1.stop()
            s2.stop()
