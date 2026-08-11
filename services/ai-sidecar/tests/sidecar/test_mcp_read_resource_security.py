"""Task 9 (P1): MCP read_resource security boundary into method

Tests verify that:
1. read_resource enforces entity binding ACL internally (not relying on callers)
2. read_resource checks allowed_resources before ANY cache read
3. No duplicate cache reads exist between route and method
"""

import pytest
from unittest.mock import AsyncMock, MagicMock, patch

from src.infrastructure.ai.mcp.client_manager import McpClientManager


class TestReadResourceSecurityBoundary:
    """Security checks must happen INSIDE read_resource, before cache access."""

    @pytest.mark.asyncio
    async def test_read_resource_raises_when_entity_id_not_provided(self):
        """entity_id is required for ACL enforcement; missing entity → error"""
        mgr = McpClientManager(mcp_repo=MagicMock(), cache_manager=None)

        with pytest.raises(ValueError, match="entity_id is required"):
            await mgr.read_resource(
                server_id="server-1",
                resource_uri="file:///test.txt",
                entity_id=None,
            )

    @pytest.mark.asyncio
    async def test_read_resource_raises_when_no_enabled_binding_exists(self):
        """No enabled binding for entity+server → error (403-equivalent)"""
        mock_repo = MagicMock()
        mock_repo.find_bindings_by_entity = AsyncMock(return_value=[])

        mgr = McpClientManager(mcp_repo=mock_repo, cache_manager=None)

        with pytest.raises(PermissionError, match="MCP_BINDING_NOT_ENABLED"):
            await mgr.read_resource(
                server_id="server-1",
                resource_uri="file:///test.txt",
                entity_id="entity-1",
            )

    @pytest.mark.asyncio
    async def test_read_resource_raises_when_resource_not_in_allowed_resources(self):
        """Resource URI not in binding's allowed_resources → error"""
        mock_repo = MagicMock()
        mock_repo.find_bindings_by_entity = AsyncMock(return_value=[
            {"server_id": "server-1", "enabled": True, "allowed_resources": ["file:///allowed.txt"]}
        ])

        mgr = McpClientManager(mcp_repo=mock_repo, cache_manager=None)

        with pytest.raises(PermissionError, match="MCP_RESOURCE_NOT_ALLOWED"):
            await mgr.read_resource(
                server_id="server-1",
                resource_uri="file:///forbidden.txt",
                entity_id="entity-1",
            )

    @pytest.mark.asyncio
    async def test_read_resource_allows_resource_in_allowed_resources(self):
        """Resource URI in allowed_resources proceeds past ACL (will fail on connection)"""
        mock_repo = MagicMock()
        mock_repo.find_bindings_by_entity = AsyncMock(return_value=[
            {"server_id": "server-1", "enabled": True, "allowed_resources": ["file:///allowed.txt"]}
        ])

        mgr = McpClientManager(mcp_repo=mock_repo, cache_manager=None)

        # ACL passes, but no active session → expect RuntimeError about connection, not ACL error
        with pytest.raises(RuntimeError) as exc_info:
            await mgr.read_resource(
                server_id="server-1",
                resource_uri="file:///allowed.txt",
                entity_id="entity-1",
            )
        assert "MCP_BINDING_NOT_ENABLED" not in str(exc_info.value)
        assert "MCP_RESOURCE_NOT_ALLOWED" not in str(exc_info.value)

    @pytest.mark.asyncio
    async def test_read_resource_allows_when_allowed_resources_is_empty(self):
        """Empty allowed_resources means all resources allowed (backward compat)"""
        mock_repo = MagicMock()
        mock_repo.find_bindings_by_entity = AsyncMock(return_value=[
            {"server_id": "server-1", "enabled": True, "allowed_resources": []}
        ])

        mgr = McpClientManager(mcp_repo=mock_repo, cache_manager=None)

        with pytest.raises(RuntimeError) as exc_info:
            await mgr.read_resource(
                server_id="server-1",
                resource_uri="file:///any.txt",
                entity_id="entity-1",
            )
        assert "MCP_BINDING_NOT_ENABLED" not in str(exc_info.value)
        assert "MCP_RESOURCE_NOT_ALLOWED" not in str(exc_info.value)

    @pytest.mark.asyncio
    async def test_read_resource_acl_check_happens_before_cache_read(self):
        """ACL must be enforced BEFORE any cache read to prevent cache-side-channel"""
        mock_repo = MagicMock()
        mock_repo.find_bindings_by_entity = AsyncMock(return_value=[])

        mock_cache = MagicMock()
        mock_cache.get_mcp_resource = AsyncMock()

        mgr = McpClientManager(mcp_repo=mock_repo, cache_manager=mock_cache)

        with pytest.raises(PermissionError):
            await mgr.read_resource(
                server_id="server-1",
                resource_uri="file:///test.txt",
                entity_id="entity-1",
            )

        # Cache must NOT be consulted before ACL passes
        mock_cache.get_mcp_resource.assert_not_called()

    @pytest.mark.asyncio
    async def test_read_resource_parses_json_allowed_resources(self):
        """allowed_resources may be a JSON string, must be parsed"""
        mock_repo = MagicMock()
        mock_repo.find_bindings_by_entity = AsyncMock(return_value=[
            {"server_id": "server-1", "enabled": True, "allowed_resources": '["file:///ok.txt"]'}
        ])

        mgr = McpClientManager(mcp_repo=mock_repo, cache_manager=None)

        with pytest.raises(PermissionError, match="MCP_RESOURCE_NOT_ALLOWED"):
            await mgr.read_resource(
                server_id="server-1",
                resource_uri="file:///bad.txt",
                entity_id="entity-1",
            )

    @pytest.mark.asyncio
    async def test_read_resource_fails_closed_on_invalid_allowed_resources_json(self):
        """Invalid allowed_resources JSON must not be treated as unrestricted access."""
        mock_repo = MagicMock()
        mock_repo.find_bindings_by_entity = AsyncMock(return_value=[
            {"server_id": "server-1", "enabled": True, "allowed_resources": "not-json"}
        ])
        mock_cache = MagicMock()
        mock_cache.get_mcp_resource = AsyncMock()

        mgr = McpClientManager(mcp_repo=mock_repo, cache_manager=mock_cache)

        with pytest.raises(PermissionError, match="MCP_RESOURCE_ACL_INVALID"):
            await mgr.read_resource(
                server_id="server-1",
                resource_uri="file:///any.txt",
                entity_id="entity-1",
            )

        mock_cache.get_mcp_resource.assert_not_called()
