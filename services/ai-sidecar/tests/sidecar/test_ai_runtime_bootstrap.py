"""Tests for the AI runtime DI bootstrap and the asyncpg-native config store.

These tests verify the *wiring* logic with fakes only — they do NOT touch a live
Postgres. Live-DB activation must be verified in the deployment environment.
"""

from __future__ import annotations

import json

import pytest

# ---------------------------------------------------------------------------
# Fake asyncpg primitives
# ---------------------------------------------------------------------------


class _AcquireCtx:
    def __init__(self, conn):
        self._conn = conn

    async def __aenter__(self):
        return self._conn

    async def __aexit__(self, *exc):
        return False


class FakePool:
    def __init__(self, conn):
        self._conn = conn

    def acquire(self):
        return _AcquireCtx(self._conn)


class FakeConn:
    """Minimal asyncpg-connection stand-in driven by canned responses."""

    def __init__(self, *, fetchrow_result=None, fetch_result=None, execute_result="OK"):
        self._fetchrow_result = fetchrow_result
        self._fetch_result = fetch_result or []
        self._execute_result = execute_result
        self.execute_calls = []
        self.fetchrow_calls = []

    async def fetchrow(self, query, *args):
        self.fetchrow_calls.append((query, args))
        return self._fetchrow_result

    async def fetch(self, query, *args):
        return self._fetch_result

    async def execute(self, query, *args):
        self.execute_calls.append((query, args))
        return self._execute_result


# ---------------------------------------------------------------------------
# AsyncpgAIConfigStore
# ---------------------------------------------------------------------------


@pytest.fixture(autouse=True)
def _no_encryption(monkeypatch):
    # Keep crypto out of the way: no key, no requirement -> base64 fallback.
    for var in (
        "AI_CONFIG_ENCRYPTION_KEY",
        "AI_CONFIG_REQUIRE_ENCRYPTION",
        "APP_ENV",
        "APP_ENVIRONMENT",
        "ENVIRONMENT",
        "FLIGHT_ENV",
    ):
        monkeypatch.delenv(var, raising=False)
    from src.infrastructure.ai.config.ai_config_crypto import reset_config_encryptor

    reset_config_encryptor()
    yield
    reset_config_encryptor()


def _run(coro):
    import asyncio

    return asyncio.run(coro)


class TestAsyncpgConfigStore:
    def test_get_returns_config_with_revision_from_json_string(self):
        from src.infrastructure.ai.asyncpg_config_store import AsyncpgAIConfigStore

        # JSONB returned as a string (asyncpg default) with a non-trivial revision.
        row = {"config": json.dumps({"default_model": "gpt-4o"}), "config_revision": 7}
        store = AsyncpgAIConfigStore(FakePool(FakeConn(fetchrow_result=row)), seed_on_start=False)

        result = _run(store.get("default"))
        assert result is not None
        assert result["default_model"] == "gpt-4o"
        assert result["_config_revision"] == 7

    def test_get_handles_dict_jsonb(self):
        from src.infrastructure.ai.asyncpg_config_store import AsyncpgAIConfigStore

        row = {"config": {"default_model": "gpt-4o-mini"}, "config_revision": 1}
        store = AsyncpgAIConfigStore(FakePool(FakeConn(fetchrow_result=row)), seed_on_start=False)
        result = _run(store.get("default"))
        assert result["default_model"] == "gpt-4o-mini"

    def test_get_missing_entity_returns_none(self):
        from src.infrastructure.ai.asyncpg_config_store import AsyncpgAIConfigStore

        store = AsyncpgAIConfigStore(FakePool(FakeConn(fetchrow_result=None)), seed_on_start=False)
        assert _run(store.get("missing")) is None

    def test_get_decrypts_base64_api_key(self):
        import base64

        from src.infrastructure.ai.asyncpg_config_store import AsyncpgAIConfigStore

        stored = {
            "api_key": base64.b64encode(b"sk-secret").decode("utf-8"),
            "_key_encoded": True,
        }
        row = {"config": json.dumps(stored), "config_revision": 2}
        store = AsyncpgAIConfigStore(FakePool(FakeConn(fetchrow_result=row)), seed_on_start=False)
        result = _run(store.get("default"))
        assert result["api_key"] == "sk-secret"
        assert "_key_encoded" not in result

    def test_get_all_maps_rows(self):
        from src.infrastructure.ai.asyncpg_config_store import AsyncpgAIConfigStore

        rows = [
            {"id": "default", "config": json.dumps({"a": 1}), "config_revision": 1},
            {"id": "pilot", "config": {"b": 2}, "config_revision": 5},
        ]
        store = AsyncpgAIConfigStore(FakePool(FakeConn(fetch_result=rows)), seed_on_start=False)
        result = _run(store.get_all())
        assert set(result.keys()) == {"default", "pilot"}
        assert result["pilot"]["_config_revision"] == 5

    def test_delete_parses_command_tag(self):
        from src.infrastructure.ai.asyncpg_config_store import AsyncpgAIConfigStore

        store_hit = AsyncpgAIConfigStore(FakePool(FakeConn(execute_result="DELETE 1")), seed_on_start=False)
        assert _run(store_hit.delete("default")) is True

        store_miss = AsyncpgAIConfigStore(FakePool(FakeConn(execute_result="DELETE 0")), seed_on_start=False)
        assert _run(store_miss.delete("nope")) is False

    def test_update_merges_and_strips_revision_marker(self):
        from src.infrastructure.ai.asyncpg_config_store import AsyncpgAIConfigStore

        existing = {"config": json.dumps({"default_model": "old", "base_url": "u"}), "config_revision": 3}
        conn = FakeConn(fetchrow_result=existing, execute_result="INSERT 0 1")
        store = AsyncpgAIConfigStore(FakePool(conn), seed_on_start=False)

        merged = _run(store.update("default", {"default_model": "new", "_config_revision": 99}))
        assert merged["default_model"] == "new"
        assert merged["base_url"] == "u"
        # Transient revision marker must not be persisted.
        assert "_config_revision" not in merged
        # The persisted JSON payload (2nd execute arg) must not carry the marker.
        persisted = json.loads(conn.execute_calls[-1][1][1])
        assert "_config_revision" not in persisted


# ---------------------------------------------------------------------------
# build_and_register_runtime
# ---------------------------------------------------------------------------


class _StubConfigStore:
    """Returns a fixed (legacy-shaped) config so the resolver can normalize it."""

    def __init__(self, config):
        self._config = config

    async def get(self, entity_id):
        return dict(self._config)

    async def get_all(self):
        return {"default": dict(self._config)}

    async def update(self, entity_id, config):
        return dict(self._config)

    async def delete(self, entity_id):
        return True

    async def reload(self):
        return None


@pytest.fixture
def _clean_container():
    import src.infrastructure.ai.runtime_service as rs
    from src.infrastructure.ai.ai_container import reset_ai_container

    reset_ai_container()
    rs._default_runtime_service = None
    yield
    reset_ai_container()
    rs._default_runtime_service = None


class TestBuildAndRegisterRuntime:
    def test_registers_full_capability_stack(self, _clean_container):
        from src.infrastructure.ai.ai_container import get_ai_container
        from src.infrastructure.ai.ai_runtime_bootstrap import build_and_register_runtime
        from src.infrastructure.ai.capability_resolver import CapabilityResolver

        build_and_register_runtime(db_pool=FakePool(FakeConn()))

        container = get_ai_container()
        for key in (
            "config_store",
            "mcp_repo",
            "skill_repo",
            "model_catalog_repo",
            "cache_metrics_repo",
            "cache_manager",
            "context_budget_planner",
            "skill_loader",
            "skill_instruction_composer",
            "mcp_client_manager",
            "capability_resolver",
        ):
            assert container.has(key), f"missing registration: {key}"

        assert isinstance(container.resolve("capability_resolver"), CapabilityResolver)

    def test_resolve_repos_no_longer_returns_none_resolver(self, _clean_container):
        """The production wiring closes the management /capabilities 503 path."""
        from src.infrastructure.ai.ai_runtime_bootstrap import build_and_register_runtime
        from src.infrastructure.ai.management_routes import _resolve_repos

        build_and_register_runtime(db_pool=FakePool(FakeConn()))
        repos = _resolve_repos()
        assert repos["capability_resolver"] is not None
        assert repos["mcp_repo"] is not None
        assert repos["cache_manager"] is not None

    def test_injected_config_store_drives_resolver_end_to_end(self, _clean_container):
        from src.infrastructure.ai.ai_runtime_bootstrap import build_and_register_runtime

        stub = _StubConfigStore(
            {
                "config_version": 2,
                "default_model": "gpt-4o",
                "base_url": "https://api.openai.com/v1",
                "_config_revision": 11,
            }
        )
        build_and_register_runtime(db_pool=FakePool(FakeConn()), config_store=stub)

        from src.infrastructure.ai.ai_container import resolve_capability_resolver

        resolver = resolve_capability_resolver()
        snapshot = _run(resolver.resolve(entity_id="default", model_purpose="chat"))
        assert snapshot.model_id  # a model was resolved from the routing
        assert snapshot.config_revision == 11

    def test_capabilities_route_returns_200_after_bootstrap(self, _clean_container):
        from unittest.mock import patch

        from fastapi import FastAPI
        from fastapi.testclient import TestClient

        from src.infrastructure.ai.ai_runtime_bootstrap import build_and_register_runtime
        from src.infrastructure.ai.management_routes import router

        stub = _StubConfigStore(
            {
                "config_version": 2,
                "default_model": "gpt-4o",
                "base_url": "https://api.openai.com/v1",
            }
        )
        build_and_register_runtime(db_pool=FakePool(FakeConn()), config_store=stub)

        app = FastAPI()
        app.include_router(router)

        # Only auth is patched; the REAL _resolve_repos must read the bootstrapped container.
        with patch(
            "src.infrastructure.ai.management_routes.require_service_identity",
            return_value=None,
        ):
            client = TestClient(app)
            resp = client.get("/internal/ai/v1/entities/default/capabilities")

        assert resp.status_code == 200, resp.text
        body = resp.json()
        assert body.get("success") is True
