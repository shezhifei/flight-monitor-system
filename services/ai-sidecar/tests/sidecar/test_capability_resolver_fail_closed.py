"""Tests for MCP fail_closed environment enforcement in CapabilityResolver."""

from __future__ import annotations

import asyncio
from unittest.mock import AsyncMock, MagicMock

import pytest

from src.infrastructure.ai.capability_resolver import CapabilityResolver
from src.infrastructure.ai import capability_resolver as capability_resolver_mod


class _FakeMcpRepo:
    """Fake MCP repo that returns configured bindings and capabilities."""

    def __init__(self, bindings=None, capabilities=None):
        self._bindings = bindings or []
        self._capabilities = capabilities or {}

    async def find_bindings_by_entity(self, entity_id: str):
        return self._bindings

    async def get_capabilities(self, server_id: str):
        return self._capabilities.get(server_id)


def _enabled_binding(server_id: str, allowed_tools=None, denied_tools=None):
    return {
        "enabled": True,
        "server_id": server_id,
        "allowed_tools": allowed_tools or [],
        "denied_tools": denied_tools or [],
    }


def _resolve_tools(
    resolver: CapabilityResolver,
    entity_id: str = "test-entity",
    tooling_config=None,
    mcp_config=None,
    model_id: str = "gpt-4o",
):
    tooling = tooling_config or {"enabled": True, "allowed_tool_sources": ["mcp"]}
    mcp = mcp_config or {"enabled": True}
    return asyncio.run(resolver._resolve_tools(entity_id, tooling, model_id, mcp))


class TestMcpFailClosedEnvEnforcement:
    """Production env must force MCP fail_closed even when config omits it."""

    def test_production_env_raises_when_mcp_server_has_no_capabilities(self, monkeypatch):
        """RED: production env + missing caps -> ValueError must be raised even with fail_closed not in config."""
        monkeypatch.setattr(capability_resolver_mod, "is_production_environment", lambda: True)

        fake_repo = _FakeMcpRepo(
            bindings=[_enabled_binding("server-a")],
            capabilities={"server-a": None},
        )
        resolver = CapabilityResolver(mcp_repo=fake_repo, builtin_tools=[])

        with pytest.raises(ValueError, match="MCP_CAPABILITIES_NOT_DISCOVERED"):
            _resolve_tools(resolver, mcp_config={"enabled": True})

    def test_development_env_allows_skipping_when_server_has_no_capabilities(self, monkeypatch):
        """In dev without fail_closed config, missing caps should skip silently (not raise)."""
        monkeypatch.setattr(capability_resolver_mod, "is_production_environment", lambda: False)

        fake_repo = _FakeMcpRepo(
            bindings=[_enabled_binding("server-a")],
            capabilities={"server-a": None},
        )
        resolver = CapabilityResolver(mcp_repo=fake_repo, builtin_tools=[])

        tools, policy = _resolve_tools(resolver, mcp_config={"enabled": True})

        assert "mcp_skipped_servers" in policy
        assert "server-a" in policy["mcp_skipped_servers"]
        assert len(tools) == 0

    def test_explicit_fail_closed_true_in_dev_raises(self, monkeypatch):
        """Config fail_closed=True must raise even in dev."""
        monkeypatch.setattr(capability_resolver_mod, "is_production_environment", lambda: False)

        fake_repo = _FakeMcpRepo(
            bindings=[_enabled_binding("server-a")],
            capabilities={"server-a": None},
        )
        resolver = CapabilityResolver(mcp_repo=fake_repo, builtin_tools=[])

        with pytest.raises(ValueError, match="MCP_CAPABILITIES_NOT_DISCOVERED"):
            _resolve_tools(resolver, mcp_config={"enabled": True, "fail_closed": True})

    def test_production_env_cannot_relax_with_fail_closed_false(self, monkeypatch):
        """Config fail_closed=False in production must NOT relax - must still raise (config can only tighten)."""
        monkeypatch.setattr(capability_resolver_mod, "is_production_environment", lambda: True)

        fake_repo = _FakeMcpRepo(
            bindings=[_enabled_binding("server-a")],
            capabilities={"server-a": None},
        )
        resolver = CapabilityResolver(mcp_repo=fake_repo, builtin_tools=[])

        with pytest.raises(ValueError, match="MCP_CAPABILITIES_NOT_DISCOVERED"):
            _resolve_tools(resolver, mcp_config={"enabled": True, "fail_closed": False})

    def test_production_env_with_valid_capabilities_succeeds(self, monkeypatch):
        """Production env with valid discovered caps should succeed normally."""
        monkeypatch.setattr(capability_resolver_mod, "is_production_environment", lambda: True)

        fake_repo = _FakeMcpRepo(
            bindings=[_enabled_binding("server-a")],
            capabilities={
                "server-a": {
                    "tools": [
                        {"name": "query_flights", "description": "Query flights", "inputSchema": {}},
                    ],
                }
            },
        )
        resolver = CapabilityResolver(mcp_repo=fake_repo, builtin_tools=[])

        tools, policy = _resolve_tools(resolver, mcp_config={"enabled": True})

        mcp_tools = [t for t in tools if t.source == "mcp"]
        assert len(mcp_tools) == 1
        assert mcp_tools[0].name == "mcp.server-a.query_flights"
        assert "mcp_skipped_servers" not in policy
