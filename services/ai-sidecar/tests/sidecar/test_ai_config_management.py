"""Tests for AI Config management endpoints.

Covers: capabilities, MCP server CRUD, MCP bindings, skill registry,
skill bindings, cache metrics, cache invalidate, config_revision,
multimodal inference, prompt cache key stability, resolver fail-closed.
"""

from __future__ import annotations

import asyncio
import hashlib
import json
from dataclasses import dataclass, field
from typing import Any
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from tests.sidecar.tool_executor_test_support import authorized_tool_executor


def _run(coro):
    """Run a coroutine synchronously."""
    loop = asyncio.new_event_loop()
    try:
        return loop.run_until_complete(coro)
    finally:
        loop.close()


# ---------------------------------------------------------------------------
# Fake repositories (sync wrappers for test setup)
# ---------------------------------------------------------------------------


class FakeMcpRepository:
    """In-memory MCP repository for testing."""

    def __init__(self):
        self._servers: dict[str, dict[str, Any]] = {}
        self._caps: dict[str, dict[str, Any]] = {}
        self._bindings: dict[str, dict[str, Any]] = {}

    async def find_all_servers(self):
        return list(self._servers.values())

    async def find_server_by_id(self, server_id: str):
        return self._servers.get(server_id)

    async def upsert_server(self, server_id: str, data: dict[str, Any]):
        row = {"server_id": server_id, **data}
        self._servers[server_id] = row
        return row

    async def delete_server(self, server_id: str):
        if server_id in self._servers:
            del self._servers[server_id]
            return True
        return False

    async def get_capabilities(self, server_id: str):
        return self._caps.get(server_id)

    async def upsert_capabilities(self, server_id: str, data: dict[str, Any]):
        self._caps[server_id] = {"server_id": server_id, **data}
        return self._caps[server_id]

    async def find_bindings_by_entity(self, entity_id: str):
        return [b for b in self._bindings.values() if b.get("entity_id") == entity_id]

    async def upsert_binding(self, binding_id: str, data: dict[str, Any]):
        row = {"binding_id": binding_id, **data}
        self._bindings[binding_id] = row
        return row

    async def delete_binding(self, binding_id: str):
        if binding_id in self._bindings:
            del self._bindings[binding_id]
            return True
        return False

    # Sync helpers for test setup
    def sync_upsert_server(self, server_id, data):
        self._servers[server_id] = {"server_id": server_id, **data}

    def sync_upsert_capabilities(self, server_id, data):
        self._caps[server_id] = {"server_id": server_id, **data}

    def sync_upsert_skill(self, slug, data):
        pass  # handled by FakeSkillRepository

    def sync_upsert_binding(self, binding_id, data):
        self._bindings[binding_id] = {"binding_id": binding_id, **data}


class FakeSkillRepository:
    """In-memory skill repository for testing."""

    def __init__(self):
        self._skills: dict[str, dict[str, Any]] = {}
        self._bindings: dict[str, dict[str, Any]] = {}

    async def find_all_skills(self):
        return list(self._skills.values())

    async def find_skill(self, skill_slug: str):
        return self._skills.get(skill_slug)

    async def upsert_skill(self, skill_slug: str, data: dict[str, Any]):
        row = {"skill_slug": skill_slug, **data}
        self._skills[skill_slug] = row
        return row

    async def delete_skill(self, skill_slug: str):
        if skill_slug in self._skills:
            del self._skills[skill_slug]
            return True
        return False

    async def find_bindings_by_entity(self, entity_id: str):
        return [b for b in self._bindings.values() if b.get("entity_id") == entity_id]

    async def upsert_binding(self, binding_id: str, data: dict[str, Any]):
        row = {"binding_id": binding_id, **data}
        self._bindings[binding_id] = row
        return row

    async def delete_binding(self, binding_id: str):
        if binding_id in self._bindings:
            del self._bindings[binding_id]
            return True
        return False

    def sync_upsert_skill(self, slug, data):
        self._skills[slug] = {"skill_slug": slug, **data}

    def sync_upsert_binding(self, binding_id, data):
        self._bindings[binding_id] = {"binding_id": binding_id, **data}


class FakeCacheMetricsRepository:
    """In-memory cache metrics repository for testing."""

    def __init__(self):
        self._records: list[dict[str, Any]] = []

    async def record(self, data: dict[str, Any]):
        self._records.append(data)

    async def query(self, entity_id=None, cache_type=None, limit=100):
        results = self._records
        if entity_id:
            results = [r for r in results if r.get("entity_id") == entity_id]
        if cache_type:
            results = [r for r in results if r.get("cache_type") == cache_type]
        return results[:limit]

    async def get_summary(self, entity_id=None, hours=24):
        records = self._records
        if entity_id:
            records = [r for r in records if r.get("entity_id") == entity_id]
        by_type: dict[str, dict[str, Any]] = {}
        for r in records:
            ct = r.get("cache_type", "unknown")
            if ct not in by_type:
                by_type[ct] = {
                    "cache_type": ct,
                    "total_events": 0,
                    "hits": 0,
                    "misses": 0,
                    "total_cached_tokens": 0,
                    "total_read_tokens": 0,
                    "total_write_tokens": 0,
                }
            entry = by_type[ct]
            entry["total_events"] += 1
            if r.get("hit"):
                entry["hits"] += 1
            else:
                entry["misses"] += 1
            entry["total_cached_tokens"] += r.get("cached_tokens", 0)
            entry["total_read_tokens"] += r.get("read_tokens", 0)
            entry["total_write_tokens"] += r.get("write_tokens", 0)
        return {
            "period_hours": hours,
            "by_cache_type": list(by_type.values()),
        }


class FakeCacheManager:
    """Fake cache manager for testing."""

    def __init__(self, available: bool = True):
        self._available = available

    async def invalidate(self, entity_id: str, cache_type=None):
        if not self._available:
            raise RuntimeError("cache backend not configured")
        return 1


# ---------------------------------------------------------------------------
# Fake CapabilityResolver
# ---------------------------------------------------------------------------


@dataclass
class FakeResolvedModel:
    model_id: str = "gpt-4o"
    provider_model: str = "gpt-4o"
    api_format: str = "chat_completions"
    context_window: int = 128000
    max_output_tokens: int = 4096
    input_modalities: list[str] = field(default_factory=lambda: ["text"])
    output_modalities: list[str] = field(default_factory=lambda: ["text"])
    capabilities: dict[str, Any] = field(default_factory=dict)
    cost: dict[str, Any] = field(default_factory=dict)


@dataclass
class FakeResolvedTool:
    source: str = "builtin"
    name: str = "test_tool"
    display_name: str = "test_tool"
    description: str = "A test tool"
    parameters: dict[str, Any] = field(default_factory=dict)
    risk_level: str = "low"

    def to_schema(self):
        return {
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters,
            },
        }


@dataclass
class FakeResolvedMcp:
    enabled: bool = False
    server_count: int = 0
    tool_count: int = 0
    resource_count: int = 0


@dataclass
class FakeResolvedSkills:
    enabled: bool = False
    skill_count: int = 0


@dataclass
class FakeResolvedSubagents:
    enabled: bool = False
    allowed_entity_ids: list[str] = field(default_factory=list)
    max_depth: int = 1
    max_concurrency: int = 2
    inherit_parent_context: bool = True


@dataclass
class FakeResolvedContextPolicy:
    strategy: str = "hybrid"
    max_context_tokens: int = 64000
    compression_threshold_tokens: int = 48000
    preserve_recent_messages: int = 12
    summary_model: str | None = None
    summary_max_tokens: int = 1200
    persist_summaries: bool = True


@dataclass
class FakeResolvedCachePolicy:
    enabled: bool = True
    provider_prompt_cache_enabled: bool = False
    provider_prompt_cache_retention: str | None = "24h"
    provider_prompt_cache_namespace: str | None = "flight_monitor"
    context_cache_enabled: bool = False
    context_cache_ttl: int = 86400
    tool_result_cache_enabled: bool = False
    tool_result_cache_ttl: int = 60
    tool_result_cacheable_tools: list[str] = field(default_factory=list)
    mcp_resource_cache_enabled: bool = False
    mcp_resource_cache_ttl: int = 300


@dataclass
class FakeResolvedConfig:
    entity_id: str = "default"
    config_version: int = 2
    config_revision: int = 1
    model_id: str = "gpt-4o"
    model: Any = None
    provider_type: str = "openai_compatible"
    base_url: str = "https://api.openai.com/v1"
    api_format: str = "chat_completions"
    timeout: float = 30.0
    max_retries: int = 3
    retry_delay: float = 0.5
    tools: list[Any] = field(default_factory=list)
    tool_policy: dict[str, Any] = field(default_factory=dict)
    mcp: Any = None
    skills: Any = None
    subagents: Any = None
    context_policy: Any = None
    cache_policy: Any = None
    security: dict[str, Any] = field(default_factory=dict)
    system_prompt: str = "test prompt"
    task_template: str | None = None
    snapshot_hash: str = "abc123"

    def __post_init__(self):
        if self.model is None:
            self.model = FakeResolvedModel()
        if self.mcp is None:
            self.mcp = FakeResolvedMcp()
        if self.skills is None:
            self.skills = FakeResolvedSkills()
        if self.subagents is None:
            self.subagents = FakeResolvedSubagents()
        if self.context_policy is None:
            self.context_policy = FakeResolvedContextPolicy()
        if self.cache_policy is None:
            self.cache_policy = FakeResolvedCachePolicy()


class FakeCapabilityResolver:
    """Fake capability resolver for testing."""

    def __init__(self, should_fail: bool = False, fail_with: Exception | None = None):
        self._should_fail = should_fail
        self._fail_with = fail_with
        self._last_input_modalities = None

    async def resolve(self, entity_id, model_purpose="chat", input_modalities=None):
        self._last_input_modalities = input_modalities
        if self._should_fail:
            if self._fail_with:
                raise self._fail_with
            raise RuntimeError("Resolver failure")
        return FakeResolvedConfig(entity_id=entity_id)


# ---------------------------------------------------------------------------
# Helper: build test app and client
# ---------------------------------------------------------------------------


def _make_test_app_and_client(
    mcp_repo=None, skill_repo=None, cache_metrics_repo=None, cache_manager=None, resolver=None
):
    from fastapi import FastAPI
    from fastapi.testclient import TestClient

    from src.infrastructure.ai.management_routes import router

    app = FastAPI()
    app.include_router(router)

    repos = {
        "capability_resolver": resolver,
        "mcp_repo": mcp_repo,
        "skill_repo": skill_repo,
        "cache_metrics_repo": cache_metrics_repo,
        "cache_manager": cache_manager,
        "model_catalog_repo": None,
    }

    stack = patch(
        "src.infrastructure.ai.management_routes._resolve_repos",
        return_value=repos,
    )
    stack2 = patch(
        "src.infrastructure.ai.management_routes.require_service_identity",
        return_value=None,
    )
    stack.start()
    stack2.start()
    client = TestClient(app)
    return client, stack, stack2


# ---------------------------------------------------------------------------
# Tests: Capabilities endpoint
# ---------------------------------------------------------------------------


class TestCapabilitiesEndpoint:
    def test_get_capabilities_returns_snapshot(self):
        resolver = FakeCapabilityResolver()
        client, s1, s2 = _make_test_app_and_client(resolver=resolver)
        try:
            resp = client.get("/internal/ai/v1/entities/default/capabilities")
            assert resp.status_code == 200
            body = resp.json()
            assert body["success"] is True
            data = body["data"]
            assert data["entity_id"] == "default"
            assert data["model_id"] == "gpt-4o"
            assert data["config_version"] == 2
            assert data["snapshot_hash"] == "abc123"
            assert "tool_count" in data
            assert "mcp_enabled" in data
            assert "skills_enabled" in data
        finally:
            s1.stop()
            s2.stop()

    def test_validate_returns_valid_on_success(self):
        resolver = FakeCapabilityResolver()
        client, s1, s2 = _make_test_app_and_client(resolver=resolver)
        try:
            resp = client.post("/internal/ai/v1/entities/default/capabilities/validate")
            assert resp.status_code == 200
            body = resp.json()
            assert body["success"] is True
            assert body["data"]["valid"] is True
            assert body["data"]["errors"] == []
        finally:
            s1.stop()
            s2.stop()

    def test_validate_returns_errors_on_resolver_failure(self):
        resolver = FakeCapabilityResolver(should_fail=True, fail_with=ValueError("AI_MODALITY_NOT_SUPPORTED: image"))
        client, s1, s2 = _make_test_app_and_client(resolver=resolver)
        try:
            resp = client.post("/internal/ai/v1/entities/default/capabilities/validate")
            assert resp.status_code == 200
            body = resp.json()
            assert body["data"]["valid"] is False
            assert len(body["data"]["errors"]) > 0
            err = body["data"]["errors"][0]
            assert "AI_MODALITY_NOT_SUPPORTED" in err["code"]
        finally:
            s1.stop()
            s2.stop()


# ---------------------------------------------------------------------------
# Tests: MCP Server CRUD
# ---------------------------------------------------------------------------


class TestMcpServerCrud:
    def test_create_and_list_mcp_servers(self):
        repo = FakeMcpRepository()
        client, s1, s2 = _make_test_app_and_client(mcp_repo=repo)
        try:
            resp = client.post(
                "/internal/ai/v1/entities/default/mcp/servers",
                json={
                    "id": "test-srv",
                    "display_name": "Test Server",
                    "transport": "stdio",
                    "command_ref": "npx",
                    "enabled": True,
                },
            )
            assert resp.status_code == 200
            body = resp.json()
            assert body["success"] is True
            assert body["data"]["id"] == "test-srv"

            resp = client.get("/internal/ai/v1/entities/default/mcp/servers")
            assert resp.status_code == 200
            servers = resp.json()["data"]
            assert len(servers) == 1
            assert servers[0]["id"] == "test-srv"
        finally:
            s1.stop()
            s2.stop()

    def test_update_mcp_server(self):
        repo = FakeMcpRepository()
        client, s1, s2 = _make_test_app_and_client(mcp_repo=repo)
        try:
            client.post(
                "/internal/ai/v1/entities/default/mcp/servers",
                json={"id": "srv-1", "display_name": "Original"},
            )
            resp = client.put(
                "/internal/ai/v1/entities/default/mcp/servers/srv-1",
                json={"id": "srv-1", "display_name": "Updated"},
            )
            assert resp.status_code == 200
            assert resp.json()["data"]["display_name"] == "Updated"
        finally:
            s1.stop()
            s2.stop()

    def test_delete_mcp_server(self):
        repo = FakeMcpRepository()
        client, s1, s2 = _make_test_app_and_client(mcp_repo=repo)
        try:
            client.post(
                "/internal/ai/v1/entities/default/mcp/servers",
                json={"id": "srv-del", "display_name": "To Delete"},
            )
            resp = client.delete("/internal/ai/v1/entities/default/mcp/servers/srv-del")
            assert resp.status_code == 200
            assert resp.json()["data"] is True

            resp = client.get("/internal/ai/v1/entities/default/mcp/servers")
            assert len(resp.json()["data"]) == 0
        finally:
            s1.stop()
            s2.stop()

    def test_delete_nonexistent_server_returns_404(self):
        repo = FakeMcpRepository()
        client, s1, s2 = _make_test_app_and_client(mcp_repo=repo)
        try:
            resp = client.delete("/internal/ai/v1/entities/default/mcp/servers/nonexistent")
            assert resp.status_code == 404
        finally:
            s1.stop()
            s2.stop()


# ---------------------------------------------------------------------------
# Tests: MCP Probe
# ---------------------------------------------------------------------------


class TestMcpProbe:
    def test_probe_returns_discovered_when_capabilities_exist(self):
        repo = FakeMcpRepository()
        repo.sync_upsert_server("srv-1", {"display_name": "Test", "status": "active"})
        repo.sync_upsert_capabilities(
            "srv-1",
            {
                "tools": [{"name": "tool1", "description": "desc"}],
                "resources": [],
                "prompts": [],
                "schema_hash": "abc",
            },
        )
        client, s1, s2 = _make_test_app_and_client(mcp_repo=repo)
        try:
            resp = client.post("/internal/ai/v1/entities/default/mcp/servers/srv-1/probe")

            assert resp.status_code == 200
            body = resp.json()["data"]
            assert body["status"] == "discovered"
            assert body["capabilities"]["server_id"] == "srv-1"
            assert len(body["capabilities"]["tools"]) == 1
        finally:
            s1.stop()
            s2.stop()

    def test_probe_returns_not_discovered_when_no_capabilities(self):
        repo = FakeMcpRepository()
        repo.sync_upsert_server("srv-2", {"display_name": "NoCaps", "status": "active"})
        client, s1, s2 = _make_test_app_and_client(mcp_repo=repo)
        try:
            resp = client.post("/internal/ai/v1/entities/default/mcp/servers/srv-2/probe")
            assert resp.status_code == 200
            body = resp.json()["data"]
            assert body["status"] == "not_discovered"
            assert body["capabilities"] is None
        finally:
            s1.stop()
            s2.stop()

    def test_probe_returns_404_for_missing_server(self):
        repo = FakeMcpRepository()
        client, s1, s2 = _make_test_app_and_client(mcp_repo=repo)
        try:
            resp = client.post("/internal/ai/v1/entities/default/mcp/servers/nonexistent/probe")
            assert resp.status_code == 404
        finally:
            s1.stop()
            s2.stop()


# ---------------------------------------------------------------------------
# Tests: MCP Bindings
# ---------------------------------------------------------------------------


class TestMcpBindings:
    def test_save_and_list_bindings(self):
        repo = FakeMcpRepository()
        client, s1, s2 = _make_test_app_and_client(mcp_repo=repo)
        try:
            resp = client.post(
                "/internal/ai/v1/entities/default/mcp/bindings",
                json={
                    "server_id": "srv-1",
                    "enabled": True,
                    "allowed_tools": ["tool1"],
                },
            )
            assert resp.status_code == 200
            binding_id = resp.json()["data"]["binding_id"]
            assert binding_id

            resp = client.get("/internal/ai/v1/entities/default/mcp/bindings")
            bindings = resp.json()["data"]
            assert len(bindings) == 1
            assert bindings[0]["server_id"] == "srv-1"
        finally:
            s1.stop()
            s2.stop()


# ---------------------------------------------------------------------------
# Tests: Skills
# ---------------------------------------------------------------------------


class TestSkills:
    def test_list_skill_registry(self):
        skill_repo = FakeSkillRepository()
        skill_repo.sync_upsert_skill(
            "test-skill",
            {
                "name": "Test Skill",
                "description": "A test skill",
                "content_hash": "abc123",
                "version": "1.0.0",
                "source": "local",
                "status": "active",
            },
        )
        client, s1, s2 = _make_test_app_and_client(skill_repo=skill_repo)
        try:
            resp = client.get("/internal/ai/v1/skills")
            assert resp.status_code == 200
            skills = resp.json()["data"]
            assert len(skills) == 1
            assert skills[0]["skill_slug"] == "test-skill"
        finally:
            s1.stop()
            s2.stop()

    def test_save_and_list_entity_skills(self):
        skill_repo = FakeSkillRepository()
        client, s1, s2 = _make_test_app_and_client(skill_repo=skill_repo)
        try:
            resp = client.post(
                "/internal/ai/v1/entities/default/skills/bindings",
                json={
                    "skill_slug": "test-skill",
                    "enabled": True,
                    "priority": 10,
                },
            )
            assert resp.status_code == 200
            assert resp.json()["data"]["skill_slug"] == "test-skill"

            resp = client.get("/internal/ai/v1/entities/default/skills")
            bindings = resp.json()["data"]
            assert len(bindings) == 1
        finally:
            s1.stop()
            s2.stop()

    def test_delete_skill_binding(self):
        skill_repo = FakeSkillRepository()
        client, s1, s2 = _make_test_app_and_client(skill_repo=skill_repo)
        try:
            resp = client.post(
                "/internal/ai/v1/entities/default/skills/bindings",
                json={"skill_slug": "del-skill", "enabled": True},
            )
            binding_id = resp.json()["data"]["binding_id"]

            resp = client.delete(f"/internal/ai/v1/entities/default/skills/bindings/{binding_id}")
            assert resp.status_code == 200
            assert resp.json()["data"] is True
        finally:
            s1.stop()
            s2.stop()

    def test_delete_nonexistent_binding_returns_404(self):
        skill_repo = FakeSkillRepository()
        client, s1, s2 = _make_test_app_and_client(skill_repo=skill_repo)
        try:
            resp = client.delete("/internal/ai/v1/entities/default/skills/bindings/nonexistent")
            assert resp.status_code == 404
        finally:
            s1.stop()
            s2.stop()

    def test_probe_skill_found_in_registry(self):
        skill_repo = FakeSkillRepository()
        skill_repo.sync_upsert_skill(
            "my-skill",
            {
                "name": "My Skill",
                "content_hash": "hash123",
            },
        )
        client, s1, s2 = _make_test_app_and_client(skill_repo=skill_repo)
        try:
            resp = client.post("/internal/ai/v1/entities/default/skills/my-skill/probe")
            assert resp.status_code == 200
            body = resp.json()["data"]
            assert body["status"] == "ok"
            assert body["content_hash"] == "hash123"
        finally:
            s1.stop()
            s2.stop()

    def test_probe_skill_not_found(self):
        skill_repo = FakeSkillRepository()
        client, s1, s2 = _make_test_app_and_client(skill_repo=skill_repo)
        try:
            resp = client.post("/internal/ai/v1/entities/default/skills/nonexistent/probe")
            assert resp.status_code == 200
            body = resp.json()["data"]
            assert body["status"] == "not_found"
        finally:
            s1.stop()
            s2.stop()

    def test_probe_skill_rejects_path_traversal_slug(self):
        client, s1, s2 = _make_test_app_and_client(skill_repo=FakeSkillRepository())
        try:
            resp = client.post("/internal/ai/v1/entities/default/skills/..%5C..%5Ctests%5Cfixtures/probe")
            assert resp.status_code == 400
            body = resp.json()
            assert body["code"] == "INVALID_SKILL_SLUG"
        finally:
            s1.stop()
            s2.stop()


# ---------------------------------------------------------------------------
# Tests: Cache
# ---------------------------------------------------------------------------


# ---------------------------------------------------------------------------
# Tests: MCP Probe — Real Discovery (stdio)
# ---------------------------------------------------------------------------


class TestMcpProbeDiscovery:
    """Tests for the real MCP discovery in probe_mcp_server."""

    def test_probe_stdio_discovery_success(self):
        """Probe via stdio: fake client manager returns tools/resources/prompts."""
        from src.infrastructure.ai.mcp.command_allowlist import reset_cache

        repo = FakeMcpRepository()
        repo.sync_upsert_server(
            "srv-probe",
            {
                "display_name": "Probe Server",
                "transport": "stdio",
                "command_ref": "npx",
                "args": ["-y", "@modelcontextprotocol/test-server"],
                "status": "active",
                "timeout_seconds": 10,
                "startup_timeout_seconds": 5,
            },
        )
        client, s1, s2 = _make_test_app_and_client(mcp_repo=repo)
        try:

            async def fake_connect(server_id, server_config, timeout=None, startup_timeout=None):
                return MagicMock()

            async def fake_discover_all(server_id, timeout=None):
                from types import SimpleNamespace

                return {
                    "tools": [
                        SimpleNamespace(
                            name="tool_a", description="Tool A", parameters={}, cacheable=False, side_effect=False
                        ),
                    ],
                    "resources": [
                        SimpleNamespace(uri="file:///test", name="test-res", description="", mime_type="text/plain"),
                    ],
                    "prompts": [{"name": "greeting", "description": "A greeting prompt"}],
                    "schema_hash": "hash123",
                }

            mock_mgr = AsyncMock()
            mock_mgr.connect_server = AsyncMock(side_effect=fake_connect)
            mock_mgr.discover_all = AsyncMock(side_effect=fake_discover_all)
            mock_mgr.disconnect_server = AsyncMock()

            allowlist_json = json.dumps({"npx": {"executable": "npx", "args_prefix": []}})
            with patch.dict("os.environ", {"AI_MCP_COMMAND_ALLOWLIST_JSON": allowlist_json}):
                reset_cache()
                with patch("src.infrastructure.ai.management_routes.McpClientManager", return_value=mock_mgr):
                    resp = client.post("/internal/ai/v1/entities/default/mcp/servers/srv-probe/probe")

            assert resp.status_code == 200
            body = resp.json()["data"]
            assert body["status"] == "discovered"
            assert body["capabilities"]["server_id"] == "srv-probe"
        finally:
            reset_cache()
            s1.stop()
            s2.stop()

    def test_probe_stdio_executable_not_found(self):
        """Probe with non-existent command returns discovery_failed."""
        from src.infrastructure.ai.mcp.command_allowlist import reset_cache

        repo = FakeMcpRepository()
        repo.sync_upsert_server(
            "srv-blocked",
            {
                "display_name": "Blocked Server",
                "transport": "stdio",
                "command_ref": "nonexistent-command-that-does-not-exist",
                "args": [],
                "status": "active",
            },
        )
        client, s1, s2 = _make_test_app_and_client(mcp_repo=repo)
        try:
            allowlist_json = json.dumps(
                {
                    "nonexistent-command-that-does-not-exist": {
                        "executable": "nonexistent-command-that-does-not-exist",
                        "args_prefix": [],
                    }
                }
            )
            with patch.dict("os.environ", {"AI_MCP_COMMAND_ALLOWLIST_JSON": allowlist_json}):
                reset_cache()
                resp = client.post("/internal/ai/v1/entities/default/mcp/servers/srv-blocked/probe")
            assert resp.status_code == 200
            body = resp.json()["data"]
            assert body["status"] == "discovery_failed"
        finally:
            s1.stop()
            s2.stop()

    def test_probe_unsupported_transport(self):
        """Probe with http/sse transport returns unsupported_transport."""
        repo = FakeMcpRepository()
        repo.sync_upsert_server(
            "srv-http",
            {
                "display_name": "HTTP Server",
                "transport": "http",
                "endpoint_url": "http://localhost:8080/mcp",
                "status": "active",
            },
        )
        client, s1, s2 = _make_test_app_and_client(mcp_repo=repo)
        try:
            resp = client.post("/internal/ai/v1/entities/default/mcp/servers/srv-http/probe")
            assert resp.status_code == 200
            body = resp.json()["data"]
            assert body["status"] == "unsupported_transport"
        finally:
            s1.stop()
            s2.stop()

    def test_probe_rejects_draft_status(self):
        """Probe only allows active/enabled servers."""
        repo = FakeMcpRepository()
        repo.sync_upsert_server(
            "srv-draft",
            {
                "display_name": "Draft Server",
                "transport": "stdio",
                "command_ref": "npx",
                "status": "draft",
            },
        )
        client, s1, s2 = _make_test_app_and_client(mcp_repo=repo)
        try:
            resp = client.post("/internal/ai/v1/entities/default/mcp/servers/srv-draft/probe")
            assert resp.status_code == 400
        finally:
            s1.stop()
            s2.stop()


# ---------------------------------------------------------------------------
# Tests: CapabilityResolver — MCP Tool Merging
# ---------------------------------------------------------------------------


class TestCapabilityResolverMcpMerging:
    """Tests for CapabilityResolver._resolve_tools with MCP sources."""

    def _make_resolver_with_mcp_repo(self, mcp_repo, entity_config=None):
        import sys

        from src.infrastructure.ai.capability_resolver import CapabilityResolver

        mock_normalizer = MagicMock()
        mock_normalizer.normalize_config = lambda x: x
        sys.modules["src.infrastructure.ai.config.config_normalizer"] = mock_normalizer

        class FakeConfigStore:
            async def get(self, entity_id):
                return entity_config or {
                    "config_version": 2,
                    "model_routing": {"default": "gpt-4o"},
                    "providers": {"default": {"type": "openai_compatible", "api_format": "chat_completions"}},
                    "tooling": {
                        "enabled": True,
                        "allowed_tool_sources": ["builtin", "mcp"],
                    },
                    "mcp": {
                        "enabled": True,
                        "fail_closed": False,
                    },
                }

        resolver = CapabilityResolver(
            config_store=FakeConfigStore(),
            mcp_repo=mcp_repo,
        )
        return resolver

    def test_merges_mcp_tools_from_capabilities(self):
        """MCP tools from capabilities are merged with mcp. prefix."""
        repo = FakeMcpRepository()
        repo.sync_upsert_binding(
            "bind-1",
            {
                "entity_id": "default",
                "server_id": "srv-a",
                "enabled": True,
            },
        )
        repo.sync_upsert_capabilities(
            "srv-a",
            {
                "tools": [
                    {"name": "tool1", "description": "desc1", "inputSchema": {}},
                ],
                "resources": [],
                "prompts": [],
                "schema_hash": "abc",
            },
        )
        resolver = self._make_resolver_with_mcp_repo(repo)
        try:
            snapshot = _run(resolver.resolve("default"))
            mcp_tools = [t for t in snapshot.tools if t.source == "mcp"]
            assert len(mcp_tools) == 1
            assert mcp_tools[0].name == "mcp.srv-a.tool1"
            assert mcp_tools[0].display_name == "tool1"
        finally:
            import sys

            sys.modules.pop("src.infrastructure.ai.config.config_normalizer", None)

    def test_mcp_allowed_tools_filter(self):
        """allowed_tools at binding level filters MCP tools."""
        repo = FakeMcpRepository()
        repo.sync_upsert_binding(
            "bind-1",
            {
                "entity_id": "default",
                "server_id": "srv-a",
                "enabled": True,
                "allowed_tools": ["tool1"],
            },
        )
        repo.sync_upsert_capabilities(
            "srv-a",
            {
                "tools": [
                    {"name": "tool1", "description": "desc1", "inputSchema": {}},
                    {"name": "tool2", "description": "desc2", "inputSchema": {}},
                ],
                "resources": [],
                "prompts": [],
                "schema_hash": "abc",
            },
        )
        resolver = self._make_resolver_with_mcp_repo(repo)
        try:
            snapshot = _run(resolver.resolve("default"))
            mcp_tools = [t for t in snapshot.tools if t.source == "mcp"]
            assert len(mcp_tools) == 1
            assert mcp_tools[0].name == "mcp.srv-a.tool1"
        finally:
            import sys

            sys.modules.pop("src.infrastructure.ai.config.config_normalizer", None)

    def test_mcp_denied_tools_filter(self):
        """denied_tools at binding level filters out MCP tools."""
        repo = FakeMcpRepository()
        repo.sync_upsert_binding(
            "bind-1",
            {
                "entity_id": "default",
                "server_id": "srv-a",
                "enabled": True,
                "denied_tools": ["tool2"],
            },
        )
        repo.sync_upsert_capabilities(
            "srv-a",
            {
                "tools": [
                    {"name": "tool1", "description": "desc1", "inputSchema": {}},
                    {"name": "tool2", "description": "desc2", "inputSchema": {}},
                ],
                "resources": [],
                "prompts": [],
                "schema_hash": "abc",
            },
        )
        resolver = self._make_resolver_with_mcp_repo(repo)
        try:
            snapshot = _run(resolver.resolve("default"))
            mcp_tools = [t for t in snapshot.tools if t.source == "mcp"]
            assert len(mcp_tools) == 1
            assert mcp_tools[0].name == "mcp.srv-a.tool1"
        finally:
            import sys

            sys.modules.pop("src.infrastructure.ai.config.config_normalizer", None)

    def test_mcp_fail_closed_no_capabilities(self):
        """mcp enabled + fail_closed=true + no capabilities raises ValueError."""
        repo = FakeMcpRepository()
        repo.sync_upsert_binding(
            "bind-1",
            {
                "entity_id": "default",
                "server_id": "srv-a",
                "enabled": True,
            },
        )
        # No capabilities stored

        import sys

        mock_normalizer = MagicMock()
        mock_normalizer.normalize_config = lambda x: x
        sys.modules["src.infrastructure.ai.config.config_normalizer"] = mock_normalizer

        from src.infrastructure.ai.capability_resolver import CapabilityResolver

        class FakeConfigStore:
            async def get(self, entity_id):
                return {
                    "config_version": 2,
                    "model_routing": {"default": "gpt-4o"},
                    "providers": {"default": {"type": "openai_compatible", "api_format": "chat_completions"}},
                    "tooling": {
                        "enabled": True,
                        "allowed_tool_sources": ["builtin", "mcp"],
                    },
                    "mcp": {
                        "enabled": True,
                        "fail_closed": True,
                    },
                }

        resolver = CapabilityResolver(config_store=FakeConfigStore(), mcp_repo=repo)
        try:
            with pytest.raises(ValueError, match="MCP_CAPABILITIES_NOT_DISCOVERED"):
                _run(resolver.resolve("default"))
        finally:
            sys.modules.pop("src.infrastructure.ai.config.config_normalizer", None)

    def test_mcp_not_enabled_ignores_mcp_tools(self):
        """mcp.enabled=false skips MCP tool resolution even if source is listed."""
        repo = FakeMcpRepository()
        repo.sync_upsert_binding(
            "bind-1",
            {
                "entity_id": "default",
                "server_id": "srv-a",
                "enabled": True,
            },
        )
        repo.sync_upsert_capabilities(
            "srv-a",
            {
                "tools": [{"name": "tool1", "description": "desc1", "inputSchema": {}}],
                "resources": [],
                "prompts": [],
                "schema_hash": "abc",
            },
        )

        import sys

        mock_normalizer = MagicMock()
        mock_normalizer.normalize_config = lambda x: x
        sys.modules["src.infrastructure.ai.config.config_normalizer"] = mock_normalizer

        from src.infrastructure.ai.capability_resolver import CapabilityResolver

        class FakeConfigStore:
            async def get(self, entity_id):
                return {
                    "config_version": 2,
                    "model_routing": {"default": "gpt-4o"},
                    "providers": {"default": {"type": "openai_compatible", "api_format": "chat_completions"}},
                    "tooling": {
                        "enabled": True,
                        "allowed_tool_sources": ["builtin", "mcp"],
                    },
                    "mcp": {
                        "enabled": False,
                    },
                }

        resolver = CapabilityResolver(config_store=FakeConfigStore(), mcp_repo=repo)
        try:
            snapshot = _run(resolver.resolve("default"))
            mcp_tools = [t for t in snapshot.tools if t.source == "mcp"]
            assert len(mcp_tools) == 0
        finally:
            sys.modules.pop("src.infrastructure.ai.config.config_normalizer", None)

    def test_mcp_tool_count_reflects_filtered_tools(self):
        """_resolve_mcp tool_count matches actual merged tools after filtering."""
        repo = FakeMcpRepository()
        repo.sync_upsert_binding(
            "bind-1",
            {
                "entity_id": "default",
                "server_id": "srv-a",
                "enabled": True,
                "allowed_tools": ["tool1"],
            },
        )
        repo.sync_upsert_capabilities(
            "srv-a",
            {
                "tools": [
                    {"name": "tool1", "description": "desc1", "inputSchema": {}},
                    {"name": "tool2", "description": "desc2", "inputSchema": {}},
                    {"name": "tool3", "description": "desc3", "inputSchema": {}},
                ],
                "resources": [],
                "prompts": [],
                "schema_hash": "abc",
            },
        )

        import sys

        mock_normalizer = MagicMock()
        mock_normalizer.normalize_config = lambda x: x
        sys.modules["src.infrastructure.ai.config.config_normalizer"] = mock_normalizer

        from src.infrastructure.ai.capability_resolver import CapabilityResolver

        class FakeConfigStore:
            async def get(self, entity_id):
                return {
                    "config_version": 2,
                    "model_routing": {"default": "gpt-4o"},
                    "providers": {"default": {"type": "openai_compatible", "api_format": "chat_completions"}},
                    "tooling": {
                        "enabled": True,
                        "allowed_tool_sources": ["builtin", "mcp"],
                    },
                    "mcp": {
                        "enabled": True,
                        "fail_closed": False,
                    },
                }

        resolver = CapabilityResolver(config_store=FakeConfigStore(), mcp_repo=repo)
        try:
            snapshot = _run(resolver.resolve("default"))
            mcp_tools = [t for t in snapshot.tools if t.source == "mcp"]
            assert len(mcp_tools) == 1  # only tool1 allowed
            assert mcp_tools[0].name == "mcp.srv-a.tool1"
            assert snapshot.mcp.tool_count == 1  # matches filtered count
        finally:
            sys.modules.pop("src.infrastructure.ai.config.config_normalizer", None)


# ---------------------------------------------------------------------------
# Tests: ToolExecutor — MCP Tool Execution
# ---------------------------------------------------------------------------


class TestToolExecutorMcpExecution:
    """Tests for ToolExecutor MCP tool routing."""

    def test_parse_mcp_tool_name(self):
        from src.infrastructure.ai.tools.tool_executor import is_mcp_tool, parse_mcp_tool_name

        assert is_mcp_tool("mcp.srv-a.my_tool")
        assert not is_mcp_tool("my_tool")
        assert not is_mcp_tool("builtin.read_flight")
        server_id, tool_name = parse_mcp_tool_name("mcp.srv-a.my_tool")
        assert server_id == "srv-a"
        assert tool_name == "my_tool"

    def test_mcp_tool_without_client_manager_returns_error(self):
        executor = authorized_tool_executor()

        async def run():
            result = await executor.execute(
                {
                    "tool_call_id": "call-1",
                    "tool_name": "mcp.srv-a.my_tool",
                    "arguments": {"key": "value"},
                },
                "run-1",
            )
            return result

        result = _run(run())
        assert result.success is False
        assert "MCP client manager not configured" in result.error

    def test_mcp_tool_side_effect_creates_proposal(self):
        from types import SimpleNamespace

        from src.infrastructure.ai.mcp.client_manager import McpClientManager, McpServerSession, McpToolInfo

        # Create a session with a side_effect tool
        session = McpServerSession(server_id="srv-a", transport="stdio", status="connected")
        session.tools = [
            McpToolInfo(name="my_tool", description="", parameters={}, server_id="srv-a", side_effect=True),
        ]

        # Create repo with capabilities (destructive=True) and an enabled binding
        repo = FakeMcpRepository()
        repo.sync_upsert_binding(
            "bind-1",
            {
                "entity_id": "default",
                "server_id": "srv-a",
                "enabled": True,
            },
        )
        repo.sync_upsert_capabilities(
            "srv-a",
            {
                "tools": [
                    {"name": "my_tool", "description": "", "inputSchema": {}, "annotations": {"destructive": True}},
                ],
                "resources": [],
                "prompts": [],
                "schema_hash": "hash",
            },
        )

        mock_mgr = MagicMock(spec=McpClientManager)
        mock_mgr.get_session.return_value = session

        executor = authorized_tool_executor(mcp_client_manager=mock_mgr, mcp_repo=repo)
        envelope = SimpleNamespace(entity_id="default")

        async def run():
            result = await executor.execute(
                {
                    "tool_call_id": "call-side",
                    "tool_name": "mcp.srv-a.my_tool",
                    "arguments": {},
                },
                "run-side",
                envelope=envelope,
            )
            return result

        result = _run(run())
        assert result.success is True
        assert result.proposal is not None
        assert result.proposal["action_name"] == "my_tool"
        assert result.proposal["requires_approval"] is True

    def test_mcp_tool_no_side_effect_executes_via_client(self):
        from types import SimpleNamespace

        from src.infrastructure.ai.mcp.client_manager import McpClientManager, McpServerSession, McpToolInfo
        from src.infrastructure.ai.mcp.command_allowlist import reset_cache

        session = McpServerSession(server_id="srv-b", transport="stdio", status="connected")
        session.tools = [
            McpToolInfo(name="read_tool", description="", parameters={}, server_id="srv-b", side_effect=False),
        ]

        # Create repo with capabilities (destructive=False) and an enabled binding
        repo = FakeMcpRepository()
        repo.sync_upsert_binding(
            "bind-1",
            {
                "entity_id": "default",
                "server_id": "srv-b",
                "enabled": True,
            },
        )
        repo.sync_upsert_server(
            "srv-b",
            {
                "display_name": "B Server",
                "transport": "stdio",
                "command_ref": "npx",
                "status": "active",
                "timeout_seconds": 10,
                "startup_timeout_seconds": 5,
            },
        )
        repo.sync_upsert_capabilities(
            "srv-b",
            {
                "tools": [
                    {
                        "name": "read_tool",
                        "description": "",
                        "inputSchema": {},
                        "annotations": {"destructive": False, "side_effect": False},
                    },
                ],
                "resources": [],
                "prompts": [],
                "schema_hash": "hash",
            },
        )

        mock_mgr = MagicMock(spec=McpClientManager)
        mock_mgr.get_session.return_value = session

        async def fake_call_tool(server_id, tool_name, arguments, timeout=None):
            return {"content": [{"type": "text", "text": "result data"}]}

        mock_mgr.call_tool = fake_call_tool

        executor = authorized_tool_executor(mcp_client_manager=mock_mgr, mcp_repo=repo)
        envelope = SimpleNamespace(entity_id="default")

        async def run():
            allowlist_json = json.dumps({"npx": {"executable": "npx", "args_prefix": []}})
            with patch.dict("os.environ", {"AI_MCP_COMMAND_ALLOWLIST_JSON": allowlist_json}):
                reset_cache()
                result = await executor.execute(
                    {
                        "tool_call_id": "call-read",
                        "tool_name": "mcp.srv-b.read_tool",
                        "arguments": {"query": "test"},
                    },
                    "run-read",
                    envelope=envelope,
                )
            return result

        result = _run(run())
        assert result.success is True
        assert result.result is not None
        reset_cache()


class TestCacheEndpoints:
    def test_cache_metrics_summary(self):
        cache_metrics_repo = FakeCacheMetricsRepository()
        cache_metrics_repo._records = [
            {
                "entity_id": "default",
                "cache_type": "prompt",
                "hit": True,
                "cached_tokens": 100,
                "read_tokens": 0,
                "write_tokens": 0,
            },
            {
                "entity_id": "default",
                "cache_type": "prompt",
                "hit": False,
                "cached_tokens": 0,
                "read_tokens": 50,
                "write_tokens": 50,
            },
        ]
        client, s1, s2 = _make_test_app_and_client(cache_metrics_repo=cache_metrics_repo)
        try:
            resp = client.get("/internal/ai/v1/cache/metrics?entity_id=default&hours=24")
            assert resp.status_code == 200
            body = resp.json()["data"]
            assert body["period_hours"] == 24
            assert len(body["by_cache_type"]) == 1
            assert body["by_cache_type"][0]["cache_type"] == "prompt"
            assert body["by_cache_type"][0]["hits"] == 1
            assert body["by_cache_type"][0]["misses"] == 1
        finally:
            s1.stop()
            s2.stop()

    def test_cache_invalidate_with_backend(self):
        cache_manager = FakeCacheManager(available=True)
        client, s1, s2 = _make_test_app_and_client(cache_manager=cache_manager)
        try:
            resp = client.post(
                "/internal/ai/v1/cache/invalidate",
                json={"entity_id": "default"},
            )
            assert resp.status_code == 200
            body = resp.json()["data"]
            assert body["invalidated"] == 1
            assert body["skipped"] is False
        finally:
            s1.stop()
            s2.stop()

    def test_cache_invalidate_without_backend(self):
        cache_manager = FakeCacheManager(available=False)
        client, s1, s2 = _make_test_app_and_client(cache_manager=cache_manager)
        try:
            resp = client.post(
                "/internal/ai/v1/cache/invalidate",
                json={"entity_id": "default"},
            )
            assert resp.status_code == 200
            body = resp.json()["data"]
            assert body["invalidated"] == 0
            assert body["skipped"] is True
            assert body["reason"] == "internal_error"
        finally:
            s1.stop()
            s2.stop()

    def test_cache_metrics_empty_when_no_repo(self):
        client, s1, s2 = _make_test_app_and_client()
        try:
            resp = client.get("/internal/ai/v1/cache/metrics")
            assert resp.status_code == 200
            body = resp.json()["data"]
            assert body["by_cache_type"] == []
        finally:
            s1.stop()
            s2.stop()


# ---------------------------------------------------------------------------
# Tests: config_revision
# ---------------------------------------------------------------------------


class TestConfigRevision:
    def _patch_normalizer(self):
        import sys

        mock_normalizer = MagicMock()
        mock_normalizer.normalize_config = lambda x: x
        sys.modules["src.infrastructure.ai.config.config_normalizer"] = mock_normalizer
        return sys.modules

    def test_resolver_reads_config_revision(self):
        from src.infrastructure.ai.capability_resolver import CapabilityResolver

        class FakeConfigStore:
            async def get(self, entity_id):
                return {
                    "config_version": 2,
                    "_config_revision": 42,
                    "model_routing": {"default": "gpt-4o"},
                    "providers": {"default": {"type": "openai_compatible", "api_format": "chat_completions"}},
                }

        resolver = CapabilityResolver(config_store=FakeConfigStore())
        modules = self._patch_normalizer()
        try:
            snapshot = _run(resolver.resolve("default"))
            assert snapshot.config_revision == 42
        finally:
            del modules["src.infrastructure.ai.config.config_normalizer"]

    def test_resolver_falls_back_to_configRevision(self):  # noqa: N802 - compatibility fixture name
        from src.infrastructure.ai.capability_resolver import CapabilityResolver

        class FakeConfigStore:
            async def get(self, entity_id):
                return {
                    "config_version": 2,
                    "configRevision": 7,
                    "model_routing": {"default": "gpt-4o"},
                    "providers": {"default": {"type": "openai_compatible", "api_format": "chat_completions"}},
                }

        resolver = CapabilityResolver(config_store=FakeConfigStore())
        modules = self._patch_normalizer()
        try:
            snapshot = _run(resolver.resolve("default"))
            assert snapshot.config_revision == 7
        finally:
            del modules["src.infrastructure.ai.config.config_normalizer"]

    def test_resolver_defaults_to_1(self):
        from src.infrastructure.ai.capability_resolver import CapabilityResolver

        class FakeConfigStore:
            async def get(self, entity_id):
                return {
                    "config_version": 2,
                    "model_routing": {"default": "gpt-4o"},
                    "providers": {"default": {"type": "openai_compatible", "api_format": "chat_completions"}},
                }

        resolver = CapabilityResolver(config_store=FakeConfigStore())
        modules = self._patch_normalizer()
        try:
            snapshot = _run(resolver.resolve("default"))
            assert snapshot.config_revision == 1
        finally:
            del modules["src.infrastructure.ai.config.config_normalizer"]


# ---------------------------------------------------------------------------
# Tests: Multimodal inference
# ---------------------------------------------------------------------------


class TestMultimodalInference:
    def _make_envelope(self, context_attachments=None):
        from types import SimpleNamespace

        from src.infrastructure.ai.context_envelope import (
            EnvelopeOntology,
            EnvelopeRequester,
            EnvelopeTask,
        )

        # Build a lightweight envelope-like object.
        # _infer_input_modalities uses getattr() so duck typing works.
        ctx = SimpleNamespace()
        if context_attachments:
            ctx.attachments = context_attachments
        else:
            ctx.attachments = []
        return SimpleNamespace(
            run_id="test",
            entity_id="default",
            requester=EnvelopeRequester(user_id="test-user"),
            ontology=EnvelopeOntology(),
            context=ctx,
            task=EnvelopeTask(task_type="chat", user_message="hello"),
        )

    def test_text_only_envelope(self):
        from src.infrastructure.ai.runtime_service import _infer_input_modalities

        envelope = self._make_envelope()
        modalities = _infer_input_modalities(envelope)
        assert modalities == ["text"]

    def test_image_attachment_infers_image(self):
        from src.infrastructure.ai.runtime_service import _infer_input_modalities

        envelope = self._make_envelope(
            context_attachments=[{"mime_type": "image/png", "url": "test.png"}],
        )
        modalities = _infer_input_modalities(envelope)
        assert "text" in modalities
        assert "image" in modalities

    def test_audio_attachment_infers_audio(self):
        from src.infrastructure.ai.runtime_service import _infer_input_modalities

        envelope = self._make_envelope(
            context_attachments=[{"mime_type": "audio/wav", "url": "test.wav"}],
        )
        modalities = _infer_input_modalities(envelope)
        assert "audio" in modalities

    def test_text_only_model_rejects_image(self):
        # Replace normalize_config at the module level via sys.modules
        import sys

        from src.infrastructure.ai.capability_resolver import CapabilityResolver

        mock_normalizer = MagicMock()
        mock_normalizer.normalize_config = lambda x: x
        sys.modules["src.infrastructure.ai.config.config_normalizer"] = mock_normalizer
        try:

            class FakeConfigStore:
                async def get(self, entity_id):
                    return {
                        "config_version": 2,
                        "model_routing": {"default": "gpt-4o"},
                        "models": {
                            "gpt-4o": {
                                "modalities": {"input": ["text"], "output": ["text"]},
                            }
                        },
                        "providers": {"default": {"type": "openai_compatible", "api_format": "chat_completions"}},
                    }

            resolver = CapabilityResolver(config_store=FakeConfigStore())
            with pytest.raises(ValueError, match="AI_MODALITY_NOT_SUPPORTED"):
                _run(resolver.resolve("default", input_modalities=["text", "image"]))
        finally:
            del sys.modules["src.infrastructure.ai.config.config_normalizer"]


# ---------------------------------------------------------------------------
# Tests: Prompt cache key stability
# ---------------------------------------------------------------------------


class TestPromptCacheKeyStability:
    def test_key_is_deterministic(self):
        from src.infrastructure.ai.capability_resolver import generate_prompt_cache_key

        key1 = generate_prompt_cache_key(
            namespace="ns",
            entity_id="ent",
            api_format="chat_completions",
            model_id="gpt-4o",
            system_prompt_hash="abc",
            tool_schema_hash="def",
        )
        key2 = generate_prompt_cache_key(
            namespace="ns",
            entity_id="ent",
            api_format="chat_completions",
            model_id="gpt-4o",
            system_prompt_hash="abc",
            tool_schema_hash="def",
        )
        assert key1 == key2

    def test_key_changes_with_different_inputs(self):
        from src.infrastructure.ai.capability_resolver import generate_prompt_cache_key

        key1 = generate_prompt_cache_key(
            namespace="ns",
            entity_id="ent",
            api_format="chat_completions",
            model_id="gpt-4o",
            system_prompt_hash="abc",
            tool_schema_hash="def",
        )
        key2 = generate_prompt_cache_key(
            namespace="ns",
            entity_id="ent",
            api_format="chat_completions",
            model_id="gpt-4o",
            system_prompt_hash="xyz",
            tool_schema_hash="def",
        )
        assert key1 != key2

    def test_tools_canonical_json_stable(self):
        tools = [
            {"type": "function", "function": {"name": "b_tool", "parameters": {}}},
            {"type": "function", "function": {"name": "a_tool", "parameters": {}}},
        ]
        canonical1 = json.dumps(tools, sort_keys=True, separators=(",", ":"), default=str)
        canonical2 = json.dumps(tools, sort_keys=True, separators=(",", ":"), default=str)
        assert canonical1 == canonical2
        assert (
            hashlib.sha256(canonical1.encode()).hexdigest()[:16] == hashlib.sha256(canonical2.encode()).hexdigest()[:16]
        )


# ---------------------------------------------------------------------------
# Tests: Resolver fail-closed
# ---------------------------------------------------------------------------


class TestResolverFailClosed:
    def test_resolver_exception_causes_fail_closed(self):
        from src.infrastructure.ai.context_envelope import (
            ContextEnvelope,
            EnvelopeContext,
            EnvelopeLimits,
            EnvelopeOntology,
            EnvelopeRequester,
            EnvelopeTask,
        )
        from src.infrastructure.ai.runtime_service import RuntimeService

        resolver = FakeCapabilityResolver(should_fail=True, fail_with=RuntimeError("DB connection lost"))
        svc = RuntimeService(capability_resolver=resolver)
        envelope = ContextEnvelope(
            job_id="job-fail",
            run_id="test-fail",
            entity_id="default",
            requester=EnvelopeRequester(user_id="test-user"),
            ontology=EnvelopeOntology(),
            context=EnvelopeContext(limits=EnvelopeLimits(redaction="standard")),
            task=EnvelopeTask(task_type="chat", user_message="hello"),
        )

        async def collect():
            events = []
            async for evt in svc.stream_run_with_tools(envelope):
                events.append(evt)
            return events

        events = _run(collect())
        event_types = [e["event"] for e in events]
        assert "run.fail" in event_types
        assert "run.complete" not in event_types
        fail_event = next(e for e in events if e["event"] == "run.fail")
        assert "AI_CAPABILITY_RESOLUTION_FAILED" in fail_event["data"]["answer"]


# ---------------------------------------------------------------------------
# Tests: Default CapabilityResolver injection
# ---------------------------------------------------------------------------


class TestDefaultInjection:
    def test_get_runtime_service_injects_resolver(self):
        import src.infrastructure.ai.runtime_service as rt_mod

        rt_mod._default_runtime_service = None

        with patch(
            "src.infrastructure.ai.runtime_service._build_default_capability_resolver",
            return_value=FakeCapabilityResolver(),
        ):
            svc = rt_mod.get_runtime_service()
            assert svc._capability_resolver is not None

        rt_mod._default_runtime_service = None


# ---------------------------------------------------------------------------
# Tests: Security — command_ref allowlist + side_effect fail-closed
# ---------------------------------------------------------------------------


class TestMcpCommandAllowlistSecurity:
    """P0-1: command_ref must come from secure config, not dynamically bypassed."""

    def test_probe_command_ref_not_in_allowlist(self):
        """Server with command_ref not in env allowlist → MCP_COMMAND_NOT_ALLOWLISTED."""
        from src.infrastructure.ai.mcp.command_allowlist import reset_cache

        repo = FakeMcpRepository()
        repo.sync_upsert_server(
            "srv-bad",
            {
                "display_name": "Bad Server",
                "transport": "stdio",
                "command_ref": "evil-command",
                "status": "active",
            },
        )
        client, s1, s2 = _make_test_app_and_client(mcp_repo=repo)
        try:
            with patch.dict(
                "os.environ", {"AI_MCP_COMMAND_ALLOWLIST_JSON": '{"npx": {"executable": "npx", "args_prefix": []}}'}
            ):
                reset_cache()
                resp = client.post("/internal/ai/v1/entities/default/mcp/servers/srv-bad/probe")
            assert resp.status_code == 200
            body = resp.json()["data"]
            assert body["status"] == "not_discovered"
            assert "MCP_COMMAND_NOT_ALLOWLISTED" in body["error"]
        finally:
            reset_cache()
            s1.stop()
            s2.stop()

    def test_probe_command_ref_in_allowlist_succeeds(self):
        """Server with command_ref in env allowlist → discovery proceeds."""
        from src.infrastructure.ai.mcp.command_allowlist import reset_cache

        repo = FakeMcpRepository()
        repo.sync_upsert_server(
            "srv-ok",
            {
                "display_name": "OK Server",
                "transport": "stdio",
                "command_ref": "npx",
                "args": ["-y", "@scope/server"],
                "status": "active",
                "timeout_seconds": 10,
                "startup_timeout_seconds": 5,
            },
        )
        client, s1, s2 = _make_test_app_and_client(mcp_repo=repo)
        try:
            allowlist_json = json.dumps({"npx": {"executable": "npx", "args_prefix": []}})
            with patch.dict("os.environ", {"AI_MCP_COMMAND_ALLOWLIST_JSON": allowlist_json}):
                reset_cache()
                mock_mgr = AsyncMock()
                mock_mgr.connect_server = AsyncMock()
                mock_mgr.discover_all = AsyncMock(
                    return_value={
                        "tools": [],
                        "resources": [],
                        "prompts": [],
                        "schema_hash": "hash",
                    }
                )
                mock_mgr.disconnect_server = AsyncMock()
                with patch("src.infrastructure.ai.management_routes.McpClientManager", return_value=mock_mgr):
                    resp = client.post("/internal/ai/v1/entities/default/mcp/servers/srv-ok/probe")
            assert resp.status_code == 200
            body = resp.json()["data"]
            assert body["status"] == "discovered"
        finally:
            reset_cache()
            s1.stop()
            s2.stop()

    def test_tool_execution_command_ref_not_in_allowlist(self):
        """Tool exec: command_ref not in allowlist → MCP_COMMAND_NOT_ALLOWLISTED, no connect/call."""
        from types import SimpleNamespace

        from src.infrastructure.ai.mcp.command_allowlist import reset_cache

        repo = FakeMcpRepository()
        repo.sync_upsert_binding(
            "bind-bad",
            {
                "entity_id": "default",
                "server_id": "srv-bad",
                "enabled": True,
            },
        )
        repo.sync_upsert_server(
            "srv-bad",
            {
                "display_name": "Bad Server",
                "transport": "stdio",
                "command_ref": "evil-command",
                "status": "active",
            },
        )
        repo.sync_upsert_capabilities(
            "srv-bad",
            {
                "tools": [
                    {
                        "name": "safe_tool",
                        "description": "desc",
                        "inputSchema": {},
                        "annotations": {"destructive": False},
                    },
                ],
                "resources": [],
                "prompts": [],
                "schema_hash": "hash",
            },
        )

        mock_client = MagicMock()
        mock_client.get_session = MagicMock(return_value=None)
        mock_client.connect_server = AsyncMock()
        mock_client.call_tool = AsyncMock()

        executor = authorized_tool_executor(mcp_client_manager=mock_client, mcp_repo=repo)
        envelope = SimpleNamespace(entity_id="default")

        async def run():
            with patch.dict(
                "os.environ", {"AI_MCP_COMMAND_ALLOWLIST_JSON": '{"npx": {"executable": "npx", "args_prefix": []}}'}
            ):
                reset_cache()
                result = await executor.execute(
                    {
                        "tool_call_id": "tc-1",
                        "tool_name": "mcp.srv-bad.safe_tool",
                        "arguments": {},
                    },
                    run_id="run-1",
                    envelope=envelope,
                )
            return result

        result = _run(run())
        assert result.success is False
        assert "MCP_COMMAND_NOT_ALLOWLISTED" in result.error
        mock_client.connect_server.assert_not_called()
        mock_client.call_tool.assert_not_called()
        reset_cache()

    def test_side_effect_tool_first_execution_proposal_only(self):
        """P0-2: side_effect tool with no session → proposal, no connect/call."""
        from types import SimpleNamespace

        repo = FakeMcpRepository()
        repo.sync_upsert_binding(
            "bind-se",
            {
                "entity_id": "default",
                "server_id": "srv-se",
                "enabled": True,
            },
        )
        repo.sync_upsert_server(
            "srv-se",
            {
                "display_name": "Side Effect Server",
                "transport": "stdio",
                "command_ref": "npx",
                "status": "active",
            },
        )
        repo.sync_upsert_capabilities(
            "srv-se",
            {
                "tools": [
                    {
                        "name": "destructive_tool",
                        "description": "desc",
                        "inputSchema": {},
                        "annotations": {"destructive": True},
                    },
                ],
                "resources": [],
                "prompts": [],
                "schema_hash": "hash",
            },
        )

        mock_client = MagicMock()
        mock_client.get_session = MagicMock(return_value=None)
        mock_client.connect_server = AsyncMock()
        mock_client.call_tool = AsyncMock()

        executor = authorized_tool_executor(mcp_client_manager=mock_client, mcp_repo=repo)
        envelope = SimpleNamespace(entity_id="default")

        async def run():
            return await executor.execute(
                {
                    "tool_call_id": "tc-se",
                    "tool_name": "mcp.srv-se.destructive_tool",
                    "arguments": {"key": "val"},
                },
                run_id="run-se",
                envelope=envelope,
            )

        result = _run(run())
        assert result.success is True
        assert result.result["status"] == "proposal_created"
        assert result.proposal is not None
        assert result.proposal["object_id"] == "srv-se"
        mock_client.connect_server.assert_not_called()
        mock_client.call_tool.assert_not_called()

    def test_capabilities_missing_returns_error(self):
        """No capabilities in repo → MCP_TOOL_CAPABILITIES_NOT_DISCOVERED, no connect/call."""
        repo = FakeMcpRepository()
        repo.sync_upsert_server(
            "srv-nocaps",
            {
                "display_name": "No Caps Server",
                "transport": "stdio",
                "command_ref": "npx",
                "status": "active",
            },
        )
        # No capabilities stored

        mock_client = MagicMock()
        mock_client.get_session = MagicMock(return_value=None)
        mock_client.connect_server = AsyncMock()
        mock_client.call_tool = AsyncMock()

        executor = authorized_tool_executor(mcp_client_manager=mock_client, mcp_repo=repo)

        async def run():
            return await executor.execute(
                {
                    "tool_call_id": "tc-nocaps",
                    "tool_name": "mcp.srv-nocaps.any_tool",
                    "arguments": {},
                },
                run_id="run-nocaps",
            )

        result = _run(run())
        assert result.success is False
        assert "MCP_TOOL_CAPABILITIES_NOT_DISCOVERED" in result.error
        mock_client.connect_server.assert_not_called()
        mock_client.call_tool.assert_not_called()

    def test_tool_not_in_discovered_capabilities(self):
        """Tool name not in capabilities → MCP_TOOL_NOT_DISCOVERED."""
        from types import SimpleNamespace

        repo = FakeMcpRepository()
        repo.sync_upsert_binding(
            "bind-some",
            {
                "entity_id": "default",
                "server_id": "srv-some",
                "enabled": True,
            },
        )
        repo.sync_upsert_server(
            "srv-some",
            {
                "display_name": "Some Server",
                "transport": "stdio",
                "command_ref": "npx",
                "status": "active",
            },
        )
        repo.sync_upsert_capabilities(
            "srv-some",
            {
                "tools": [
                    {
                        "name": "known_tool",
                        "description": "desc",
                        "inputSchema": {},
                        "annotations": {"destructive": False},
                    },
                ],
                "resources": [],
                "prompts": [],
                "schema_hash": "hash",
            },
        )

        mock_client = MagicMock()
        mock_client.get_session = MagicMock(return_value=None)
        mock_client.connect_server = AsyncMock()
        mock_client.call_tool = AsyncMock()

        executor = authorized_tool_executor(mcp_client_manager=mock_client, mcp_repo=repo)
        envelope = SimpleNamespace(entity_id="default")

        async def run():
            return await executor.execute(
                {
                    "tool_call_id": "tc-unknown",
                    "tool_name": "mcp.srv-some.unknown_tool",
                    "arguments": {},
                },
                run_id="run-unknown",
                envelope=envelope,
            )

        result = _run(run())
        assert result.success is False
        assert "MCP_TOOL_NOT_DISCOVERED" in result.error
        mock_client.connect_server.assert_not_called()
        mock_client.call_tool.assert_not_called()

    def test_non_side_effect_tool_lazy_connect_and_call(self):
        """explicit destructive=false + command_ref in allowlist → lazy connect + call_tool."""
        from types import SimpleNamespace

        from src.infrastructure.ai.mcp.command_allowlist import reset_cache

        repo = FakeMcpRepository()
        repo.sync_upsert_binding(
            "bind-safe",
            {
                "entity_id": "default",
                "server_id": "srv-safe",
                "enabled": True,
            },
        )
        repo.sync_upsert_server(
            "srv-safe",
            {
                "display_name": "Safe Server",
                "transport": "stdio",
                "command_ref": "npx",
                "status": "active",
                "timeout_seconds": 10,
                "startup_timeout_seconds": 5,
            },
        )
        repo.sync_upsert_capabilities(
            "srv-safe",
            {
                "tools": [
                    {
                        "name": "read_tool",
                        "description": "desc",
                        "inputSchema": {},
                        "annotations": {"destructive": False, "side_effect": False},
                    },
                ],
                "resources": [],
                "prompts": [],
                "schema_hash": "hash",
            },
        )

        mock_session = MagicMock()
        mock_session.status = "disconnected"

        mock_client = MagicMock()
        mock_client.get_session = MagicMock(return_value=None)
        mock_client.connect_server = AsyncMock()
        mock_client.call_tool = AsyncMock(return_value={"content": "ok"})

        executor = authorized_tool_executor(mcp_client_manager=mock_client, mcp_repo=repo)
        envelope = SimpleNamespace(entity_id="default")

        async def run():
            allowlist_json = json.dumps({"npx": {"executable": "npx", "args_prefix": []}})
            with patch.dict("os.environ", {"AI_MCP_COMMAND_ALLOWLIST_JSON": allowlist_json}):
                reset_cache()
                return await executor.execute(
                    {
                        "tool_call_id": "tc-safe",
                        "tool_name": "mcp.srv-safe.read_tool",
                        "arguments": {"query": "test"},
                    },
                    run_id="run-safe",
                    envelope=envelope,
                )

        result = _run(run())
        assert result.success is True
        assert result.result == {"content": "ok"}
        mock_client.connect_server.assert_called_once()
        mock_client.call_tool.assert_called_once_with(
            "srv-safe",
            "read_tool",
            {"query": "test"},
            timeout=30.0,
        )
        reset_cache()


# ---------------------------------------------------------------------------
# Tests: Snapshot enrichment — new fields
# ---------------------------------------------------------------------------


class TestSnapshotEnrichment:
    """Test enriched fields in capabilities endpoint."""

    def test_capabilities_returns_builtin_and_mcp_tool_count(self):
        """Snapshot includes builtin_tool_count and mcp_tool_count."""
        resolver = FakeCapabilityResolver()
        client, s1, s2 = _make_test_app_and_client(resolver=resolver)
        try:
            resp = client.get("/internal/ai/v1/entities/default/capabilities")
            assert resp.status_code == 200
            data = resp.json()["data"]
            assert "builtin_tool_count" in data
            assert "mcp_tool_count" in data
            assert isinstance(data["builtin_tool_count"], int)
            assert isinstance(data["mcp_tool_count"], int)
        finally:
            s1.stop()
            s2.stop()

    def test_capabilities_returns_enriched_cache_backends(self):
        """Snapshot cache_backends includes retention, namespace, ttl, cacheable_tools."""
        resolver = FakeCapabilityResolver()
        client, s1, s2 = _make_test_app_and_client(resolver=resolver)
        try:
            resp = client.get("/internal/ai/v1/entities/default/capabilities")
            data = resp.json()["data"]
            cb = data["cache_backends"]
            assert "provider_prompt_cache_retention" in cb
            assert "provider_prompt_cache_namespace" in cb
            assert "tool_result_cache_ttl" in cb
            assert "tool_result_cacheable_tools" in cb
            assert "context_cache_ttl" in cb
            assert cb["mcp_resource_cache_note"] in ("runtime_integrated", "disabled")
            assert cb["context_cache_note"] in ("runtime_integrated", "disabled")
        finally:
            s1.stop()
            s2.stop()

    def test_capabilities_returns_mcp_servers_enrichment(self):
        """Snapshot includes mcp_servers with discovery status."""
        repo = FakeMcpRepository()
        repo.sync_upsert_server(
            "srv-1",
            {
                "display_name": "Test Server",
                "transport": "stdio",
                "command_ref": "npx",
                "status": "active",
            },
        )
        repo.sync_upsert_binding(
            "bind-1",
            {
                "entity_id": "default",
                "server_id": "srv-1",
                "enabled": True,
            },
        )
        repo.sync_upsert_capabilities(
            "srv-1",
            {
                "tools": [{"name": "tool1", "description": "desc", "inputSchema": {}}],
                "resources": [],
                "prompts": [],
                "schema_hash": "abc",
            },
        )
        resolver = FakeCapabilityResolver()
        client, s1, s2 = _make_test_app_and_client(resolver=resolver, mcp_repo=repo)
        try:
            resp = client.get("/internal/ai/v1/entities/default/capabilities")
            data = resp.json()["data"]
            assert "mcp_servers" in data
            assert "mcp_server_count" in data
            assert "mcp_allowlist_configured" in data
            assert isinstance(data["mcp_servers"], list)
        finally:
            s1.stop()
            s2.stop()

    def test_capabilities_returns_skill_bindings_enrichment(self):
        """Snapshot includes skill_bindings list."""
        skill_repo = FakeSkillRepository()
        skill_repo.sync_upsert_skill(
            "test-skill",
            {
                "name": "Test Skill",
                "content_hash": "hash",
                "version": "1.0.0",
            },
        )
        skill_repo.sync_upsert_binding(
            "skill-bind-1",
            {
                "entity_id": "default",
                "skill_slug": "test-skill",
                "enabled": True,
                "priority": 10,
                "activation_policy": "always",
                "max_instruction_tokens": 3000,
            },
        )
        resolver = FakeCapabilityResolver()
        client, s1, s2 = _make_test_app_and_client(resolver=resolver, skill_repo=skill_repo)
        try:
            resp = client.get("/internal/ai/v1/entities/default/capabilities")
            data = resp.json()["data"]
            assert "skill_bindings" in data
            assert isinstance(data["skill_bindings"], list)
        finally:
            s1.stop()
            s2.stop()

    def test_capabilities_returns_subagent_risk(self):
        """Snapshot includes subagent_risk indicator."""
        resolver = FakeCapabilityResolver()
        client, s1, s2 = _make_test_app_and_client(resolver=resolver)
        try:
            resp = client.get("/internal/ai/v1/entities/default/capabilities")
            data = resp.json()["data"]
            assert "subagent_risk" in data
            risk = data["subagent_risk"]
            assert "enabled_no_allowlist" in risk
            assert "high_depth" in risk
        finally:
            s1.stop()
            s2.stop()


# ---------------------------------------------------------------------------
# Tests: Validate structured errors
# ---------------------------------------------------------------------------


class TestValidateStructuredErrors:
    """Test that validate endpoint returns structured error objects with code/message/severity."""

    def test_validate_valid_returns_empty_errors(self):
        resolver = FakeCapabilityResolver()
        client, s1, s2 = _make_test_app_and_client(resolver=resolver)
        try:
            resp = client.post("/internal/ai/v1/entities/default/capabilities/validate")
            body = resp.json()
            assert body["data"]["valid"] is True
            assert body["data"]["errors"] == []
        finally:
            s1.stop()
            s2.stop()

    def test_validate_value_error_returns_structured_error(self):
        resolver = FakeCapabilityResolver(should_fail=True, fail_with=ValueError("AI_MODALITY_NOT_SUPPORTED: image"))
        client, s1, s2 = _make_test_app_and_client(resolver=resolver)
        try:
            resp = client.post("/internal/ai/v1/entities/default/capabilities/validate")
            body = resp.json()
            errors = body["data"]["errors"]
            assert len(errors) > 0
            err = errors[0]
            assert "code" in err
            assert "message" in err
            assert "severity" in err
            assert err["code"] == "AI_MODALITY_NOT_SUPPORTED"
            assert err["severity"] == "error"
        finally:
            s1.stop()
            s2.stop()

    def test_validate_runtime_error_returns_structured_error(self):
        resolver = FakeCapabilityResolver(should_fail=True, fail_with=RuntimeError("DB connection lost"))
        client, s1, s2 = _make_test_app_and_client(resolver=resolver)
        try:
            resp = client.post("/internal/ai/v1/entities/default/capabilities/validate")
            body = resp.json()
            errors = body["data"]["errors"]
            assert len(errors) > 0
            err = errors[0]
            assert err["code"] == "CAPABILITY_RESOLUTION_FAILED"
            assert err["severity"] == "error"
        finally:
            s1.stop()
            s2.stop()

    def test_validate_warning_does_not_invalidate(self):
        """If only warnings exist, valid should still be True."""
        resolver = FakeCapabilityResolver()
        skill_repo = FakeSkillRepository()
        client, s1, s2 = _make_test_app_and_client(resolver=resolver, skill_repo=skill_repo)
        try:
            # The snapshot has skills_enabled=False, no warnings for that
            # But subagents with empty allowed_entity_ids should warn
            resp = client.post("/internal/ai/v1/entities/default/capabilities/validate")
            body = resp.json()
            # Default FakeResolvedConfig has subagents.enabled=False, so no warning
            assert body["data"]["valid"] is True
        finally:
            s1.stop()
            s2.stop()

    def test_validate_service_not_configured_returns_structured_error(self):
        client, s1, s2 = _make_test_app_and_client(resolver=None)
        try:
            resp = client.post("/internal/ai/v1/entities/default/capabilities/validate")
            body = resp.json()
            errors = body["data"]["errors"]
            assert len(errors) > 0
            assert errors[0]["code"] == "SERVICE_NOT_CONFIGURED"
            assert errors[0]["severity"] == "error"
        finally:
            s1.stop()
            s2.stop()


# ---------------------------------------------------------------------------
# Tests: MCP resource-read endpoint (contract tests, no live infra)
#
#   POST /internal/ai/v1/entities/{entity_id}/mcp/servers/{server_id}/resource
#   Handler: management_routes.read_mcp_resource
#
# Response shape note: the handler uses _err()/_ok(), which produce a flat
# body {"success": bool, "error": str, "code": str} (error code at TOP level),
# NOT FastAPI's {"detail": ...}. The service-identity guard, by contrast,
# raises HTTPException, so its body is {"detail": {"code": ...}}.
# ---------------------------------------------------------------------------


_MCP_RESOURCE_PATH = "/internal/ai/v1/entities/default/mcp/servers/srv-r/resource"


class _FakeMcpResourceCacheManager:
    """Cache manager exposing only the method the resource-read path calls.

    ``hit`` controls whether get_mcp_resource returns content (cache hit) or
    None (miss). ``calls`` records invocations for assertions.
    """

    def __init__(self, hit=None):
        self._hit = hit
        self.calls = []

    async def get_mcp_resource(self, server_id, resource_uri, ttl_seconds=None, entity_id=None):
        self.calls.append((server_id, resource_uri, ttl_seconds, entity_id))
        return self._hit


def _make_resource_app_and_client(mcp_repo=None, cache_manager=None, patch_identity=True):
    """Build app/client for the resource endpoint.

    When patch_identity is False, require_service_identity is NOT patched so
    the real auth guard runs (used for the 401 auth tests).
    """
    from fastapi import FastAPI
    from fastapi.testclient import TestClient

    from src.infrastructure.ai.management_routes import router

    app = FastAPI()
    app.include_router(router)

    repos = {
        "capability_resolver": None,
        "mcp_repo": mcp_repo,
        "skill_repo": None,
        "cache_metrics_repo": None,
        "cache_manager": cache_manager,
        "model_catalog_repo": None,
    }

    stack = patch(
        "src.infrastructure.ai.management_routes._resolve_repos",
        return_value=repos,
    )
    stack.start()
    stack2 = None
    if patch_identity:
        stack2 = patch(
            "src.infrastructure.ai.management_routes.require_service_identity",
            return_value=None,
        )
        stack2.start()
    client = TestClient(app, raise_server_exceptions=False)
    return client, stack, stack2


class TestMcpResourceReadAuth:
    """Service-identity enforcement on the resource-read endpoint."""

    def test_missing_service_identity_header_returns_401(self):
        # Real guard runs; no X-Service-Identity header present.
        repo = FakeMcpRepository()
        client, s1, s2 = _make_resource_app_and_client(mcp_repo=repo, patch_identity=False)
        try:
            resp = client.post(
                _MCP_RESOURCE_PATH,
                json={"resource_uri": "file:///x.txt"},
            )
            assert resp.status_code == 401, resp.text
            body = resp.json()
            assert body["detail"]["code"] == "MISSING_SERVICE_IDENTITY"
        finally:
            s1.stop()
            if s2:
                s2.stop()

    def test_garbage_service_identity_token_returns_401(self):
        # Real guard runs with a JWT_SECRET set so decode is reached and the
        # malformed token surfaces as InvalidTokenError -> 401.
        import src.infrastructure.ai.service_identity as si

        repo = FakeMcpRepository()
        client, s1, s2 = _make_resource_app_and_client(mcp_repo=repo, patch_identity=False)
        try:
            with patch.dict("os.environ", {"JWT_SECRET": "test-secret-value"}):
                si.get_jwt_secret.cache_clear()
                try:
                    resp = client.post(
                        _MCP_RESOURCE_PATH,
                        json={"resource_uri": "file:///x.txt"},
                        headers={"X-Service-Identity": "not-a-valid-jwt"},
                    )
                finally:
                    si.get_jwt_secret.cache_clear()
            assert resp.status_code == 401, resp.text
            body = resp.json()
            assert body["detail"]["code"] == "INVALID_SERVICE_IDENTITY"
        finally:
            s1.stop()
            if s2:
                s2.stop()


class TestMcpResourceReadEnforcement:
    """Binding / allowlist / transport enforcement (identity patched out)."""

    def test_no_enabled_binding_returns_403_binding_not_enabled(self):
        repo = FakeMcpRepository()
        repo.sync_upsert_server(
            "srv-r",
            {
                "display_name": "Resource Server",
                "transport": "stdio",
                "command_ref": "npx",
                "status": "active",
            },
        )
        # No binding at all -> binding lookup finds none.
        client, s1, s2 = _make_resource_app_and_client(mcp_repo=repo)
        try:
            resp = client.post(
                _MCP_RESOURCE_PATH,
                json={"resource_uri": "file:///allowed.txt"},
            )
            assert resp.status_code == 403, resp.text
            body = resp.json()
            assert body["success"] is False
            assert body["code"] == "MCP_BINDING_NOT_ENABLED"
        finally:
            s1.stop()
            if s2:
                s2.stop()

    def test_resource_not_in_allowed_resources_returns_403(self):
        repo = FakeMcpRepository()
        repo.sync_upsert_server(
            "srv-r",
            {
                "display_name": "Resource Server",
                "transport": "stdio",
                "command_ref": "npx",
                "status": "active",
            },
        )
        repo.sync_upsert_binding(
            "bind-r",
            {
                "entity_id": "default",
                "server_id": "srv-r",
                "enabled": True,
                "allowed_resources": ["file:///allowed.txt"],
            },
        )
        client, s1, s2 = _make_resource_app_and_client(mcp_repo=repo)
        try:
            resp = client.post(
                _MCP_RESOURCE_PATH,
                json={"resource_uri": "file:///forbidden.txt"},
            )
            assert resp.status_code == 403, resp.text
            body = resp.json()
            assert body["code"] == "MCP_RESOURCE_NOT_ALLOWED"
        finally:
            s1.stop()
            if s2:
                s2.stop()

    def test_command_ref_not_allowlisted_returns_403(self):
        from src.infrastructure.ai.mcp.command_allowlist import reset_cache

        repo = FakeMcpRepository()
        repo.sync_upsert_server(
            "srv-r",
            {
                "display_name": "Resource Server",
                "transport": "stdio",
                "command_ref": "evil-command",
                "status": "active",
            },
        )
        repo.sync_upsert_binding(
            "bind-r",
            {
                "entity_id": "default",
                "server_id": "srv-r",
                "enabled": True,
                # No allowed_resources -> whitelist not enforced; passes to command check.
            },
        )
        # Cache manager present but reports a miss so we reach the command check.
        cache = _FakeMcpResourceCacheManager(hit=None)
        client, s1, s2 = _make_resource_app_and_client(mcp_repo=repo, cache_manager=cache)
        try:
            with patch.dict("os.environ", {"AI_MCP_COMMAND_ALLOWLIST_JSON": "{}"}):
                reset_cache()
                resp = client.post(
                    _MCP_RESOURCE_PATH,
                    json={"resource_uri": "file:///anything.txt"},
                )
            assert resp.status_code == 403, resp.text
            body = resp.json()
            assert body["code"] == "MCP_COMMAND_NOT_ALLOWLISTED"
        finally:
            reset_cache()
            s1.stop()
            if s2:
                s2.stop()

    def test_unsupported_transport_returns_400(self):
        repo = FakeMcpRepository()
        repo.sync_upsert_server(
            "srv-r",
            {
                "display_name": "HTTP Resource Server",
                "transport": "http",
                "endpoint_url": "http://localhost:8080/mcp",
                "status": "active",
            },
        )
        repo.sync_upsert_binding(
            "bind-r",
            {
                "entity_id": "default",
                "server_id": "srv-r",
                "enabled": True,
            },
        )
        # Cache miss so the transport check is reached.
        cache = _FakeMcpResourceCacheManager(hit=None)
        client, s1, s2 = _make_resource_app_and_client(mcp_repo=repo, cache_manager=cache)
        try:
            resp = client.post(
                _MCP_RESOURCE_PATH,
                json={"resource_uri": "file:///x.txt"},
            )
            assert resp.status_code == 400, resp.text
            body = resp.json()
            assert body["code"] == "MCP_UNSUPPORTED_TRANSPORT"
        finally:
            s1.stop()
            if s2:
                s2.stop()


class TestMcpResourceReadCacheHit:
    """A cache hit returns 200 without constructing/using the MCP client."""

    def test_cache_hit_returns_cached_without_spawning_client(self):
        from src.infrastructure.ai.mcp.command_allowlist import reset_cache

        repo = FakeMcpRepository()
        repo.sync_upsert_server(
            "srv-r",
            {
                "display_name": "Resource Server",
                "transport": "stdio",
                "command_ref": "npx",
                "status": "active",
            },
        )
        repo.sync_upsert_binding(
            "bind-r",
            {
                "entity_id": "default",
                "server_id": "srv-r",
                "enabled": True,
                "allowed_resources": ["file:///allowed.txt"],
            },
        )
        cached_content = {"contents": [{"uri": "file:///allowed.txt", "text": "hello"}]}
        cache = _FakeMcpResourceCacheManager(hit=cached_content)
        client, s1, s2 = _make_resource_app_and_client(mcp_repo=repo, cache_manager=cache)
        try:
            # If the client were constructed, this MagicMock would record it.
            mock_mgr_cls = MagicMock(name="McpClientManager")
            # command_ref must be allowlisted: the allowlist check now precedes
            # the cache read, so a hit can no longer bypass it.
            allowlist_json = json.dumps({"npx": {"executable": "npx", "args_prefix": []}})
            with (
                patch.dict("os.environ", {"AI_MCP_COMMAND_ALLOWLIST_JSON": allowlist_json}),
                patch(
                    "src.infrastructure.ai.management_routes.McpClientManager",
                    mock_mgr_cls,
                ),
            ):
                reset_cache()
                resp = client.post(
                    _MCP_RESOURCE_PATH,
                    json={"resource_uri": "file:///allowed.txt"},
                )
            assert resp.status_code == 200, resp.text
            body = resp.json()
            assert body["success"] is True
            assert body["data"]["cached"] is True
            assert body["data"]["content"] == cached_content
            assert body["data"]["resource_uri"] == "file:///allowed.txt"
            # No MCP client/subprocess path was invoked on a cache hit.
            mock_mgr_cls.assert_not_called()
            assert cache.calls, "cache lookup should have been performed"
        finally:
            reset_cache()
            s1.stop()
            if s2:
                s2.stop()


class TestMcpResourceReadCacheBypassGuards:
    """A warm cache must NOT let a request bypass the security boundaries.

    These regressions pin the ordering fix: server-status, transport, and
    command-allowlist checks all run BEFORE the cache read, so a cache hit can
    never short-circuit them. allowed_resources must likewise gate the hit.
    """

    def test_cache_hit_blocked_when_command_not_allowlisted(self):
        """Cache hit + de-allowlisted command_ref => 403, content never served."""
        from src.infrastructure.ai.mcp.command_allowlist import reset_cache

        repo = FakeMcpRepository()
        repo.sync_upsert_server(
            "srv-r",
            {
                "display_name": "Resource Server",
                "transport": "stdio",
                "command_ref": "evil-command",
                "status": "active",
            },
        )
        repo.sync_upsert_binding(
            "bind-r",
            {
                "entity_id": "default",
                "server_id": "srv-r",
                "enabled": True,
                # No allowed_resources -> whitelist not enforced; reach command check.
            },
        )
        # Cache WOULD hit, but the command check must fire first.
        cache = _FakeMcpResourceCacheManager(hit={"contents": [{"text": "cached-secret"}]})
        client, s1, s2 = _make_resource_app_and_client(mcp_repo=repo, cache_manager=cache)
        try:
            mock_mgr_cls = MagicMock(name="McpClientManager")
            with (
                patch.dict("os.environ", {"AI_MCP_COMMAND_ALLOWLIST_JSON": "{}"}),
                patch("src.infrastructure.ai.management_routes.McpClientManager", mock_mgr_cls),
            ):
                reset_cache()
                resp = client.post(
                    _MCP_RESOURCE_PATH,
                    json={"resource_uri": "file:///anything.txt"},
                )
            assert resp.status_code == 403, resp.text
            body = resp.json()
            assert body["code"] == "MCP_COMMAND_NOT_ALLOWLISTED"
            # The cached secret must never have been read, let alone returned.
            assert cache.calls == [], "command check must precede the cache read"
            mock_mgr_cls.assert_not_called()
        finally:
            reset_cache()
            s1.stop()
            if s2:
                s2.stop()

    def test_cache_hit_blocked_when_server_status_draft(self):
        """Cache hit on a draft/inactive server => 403 MCP_SERVER_NOT_ACTIVE."""
        repo = FakeMcpRepository()
        repo.sync_upsert_server(
            "srv-r",
            {
                "display_name": "Resource Server",
                "transport": "stdio",
                "command_ref": "npx",
                "status": "draft",
            },
        )
        repo.sync_upsert_binding(
            "bind-r",
            {
                "entity_id": "default",
                "server_id": "srv-r",
                "enabled": True,
                "allowed_resources": ["file:///allowed.txt"],
            },
        )
        cache = _FakeMcpResourceCacheManager(hit={"contents": [{"text": "cached-secret"}]})
        client, s1, s2 = _make_resource_app_and_client(mcp_repo=repo, cache_manager=cache)
        try:
            resp = client.post(
                _MCP_RESOURCE_PATH,
                json={"resource_uri": "file:///allowed.txt"},
            )
            assert resp.status_code == 403, resp.text
            body = resp.json()
            assert body["success"] is False
            assert body["code"] == "MCP_SERVER_NOT_ACTIVE"
            # Disabled server: stale cache must not be read.
            assert cache.calls == [], "status check must precede the cache read"
        finally:
            s1.stop()
            if s2:
                s2.stop()

    def test_allowed_resources_enforced_before_cache_hit(self):
        """A forbidden URI is rejected even when its content is cached."""
        from src.infrastructure.ai.mcp.command_allowlist import reset_cache

        repo = FakeMcpRepository()
        repo.sync_upsert_server(
            "srv-r",
            {
                "display_name": "Resource Server",
                "transport": "stdio",
                "command_ref": "npx",
                "status": "active",
            },
        )
        repo.sync_upsert_binding(
            "bind-r",
            {
                "entity_id": "default",
                "server_id": "srv-r",
                "enabled": True,
                "allowed_resources": ["file:///allowed.txt"],
            },
        )
        cache = _FakeMcpResourceCacheManager(hit={"contents": [{"text": "cached-secret"}]})
        client, s1, s2 = _make_resource_app_and_client(mcp_repo=repo, cache_manager=cache)
        try:
            allowlist_json = json.dumps({"npx": {"executable": "npx", "args_prefix": []}})
            with patch.dict("os.environ", {"AI_MCP_COMMAND_ALLOWLIST_JSON": allowlist_json}):
                reset_cache()
                resp = client.post(
                    _MCP_RESOURCE_PATH,
                    json={"resource_uri": "file:///forbidden.txt"},
                )
            assert resp.status_code == 403, resp.text
            body = resp.json()
            assert body["code"] == "MCP_RESOURCE_NOT_ALLOWED"
            assert cache.calls == [], "allowed_resources must gate the cache read"
        finally:
            reset_cache()
            s1.stop()
            if s2:
                s2.stop()

    def test_active_server_allowlisted_command_cache_hit_serves_without_subprocess(self):
        """The happy path still works post-reorder: 200, cached, no subprocess."""
        from src.infrastructure.ai.mcp.command_allowlist import reset_cache

        repo = FakeMcpRepository()
        repo.sync_upsert_server(
            "srv-r",
            {
                "display_name": "Resource Server",
                "transport": "stdio",
                "command_ref": "npx",
                "status": "active",
            },
        )
        repo.sync_upsert_binding(
            "bind-r",
            {
                "entity_id": "default",
                "server_id": "srv-r",
                "enabled": True,
                "allowed_resources": ["file:///allowed.txt"],
            },
        )
        cached_content = {"contents": [{"uri": "file:///allowed.txt", "text": "hello"}]}
        cache = _FakeMcpResourceCacheManager(hit=cached_content)
        client, s1, s2 = _make_resource_app_and_client(mcp_repo=repo, cache_manager=cache)
        try:
            mock_mgr_cls = MagicMock(name="McpClientManager")
            allowlist_json = json.dumps({"npx": {"executable": "npx", "args_prefix": []}})
            with (
                patch.dict("os.environ", {"AI_MCP_COMMAND_ALLOWLIST_JSON": allowlist_json}),
                patch("src.infrastructure.ai.management_routes.McpClientManager", mock_mgr_cls),
            ):
                reset_cache()
                resp = client.post(
                    _MCP_RESOURCE_PATH,
                    json={"resource_uri": "file:///allowed.txt"},
                )
            assert resp.status_code == 200, resp.text
            body = resp.json()
            assert body["data"]["cached"] is True
            assert body["data"]["content"] == cached_content
            assert cache.calls, "cache lookup should have been performed on the happy path"
            mock_mgr_cls.assert_not_called()
        finally:
            reset_cache()
            s1.stop()
            if s2:
                s2.stop()
