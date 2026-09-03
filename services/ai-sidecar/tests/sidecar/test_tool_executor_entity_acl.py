"""Task 8 (P1): MCP entity-level ACL single authority at execution time

Tests verify that:
1. Tool execution rejects tools not in the allowed_tool_names set (defense-in-depth)
2. Denied tools are rejected even if LLM hallucinates/replays them
3. The is_tool_allowed function is shared as a single authority
"""

from unittest.mock import AsyncMock, MagicMock

import pytest

from src.infrastructure.ai.capability_resolver import is_tool_allowed
from src.infrastructure.ai.tools.tool_executor import ToolExecutor


class TestIsToolAllowedSingleAuthority:
    """Tests for is_tool_allowed as the single ACL authority."""

    def test_allows_tool_when_no_restrictions(self):
        """No restrictions → tool allowed"""
        tool = {"name": "get_flight_status", "category": "read"}
        assert is_tool_allowed(tool, {}) is True

    def test_denies_tool_in_denied_list(self):
        """Tool in denied_tools → rejected"""
        tool = {"name": "dangerous_tool", "category": "write"}
        config = {"denied_tools": ["dangerous_tool"]}
        assert is_tool_allowed(tool, config) is False

    def test_allows_tool_not_in_denied_list(self):
        """Tool not in denied_tools → allowed"""
        tool = {"name": "safe_tool", "category": "read"}
        config = {"denied_tools": ["dangerous_tool"]}
        assert is_tool_allowed(tool, config) is True

    def test_denies_tool_not_in_allowed_list_when_allowed_list_set(self):
        """When allowed_tools is set, tools not in it are denied"""
        tool = {"name": "unknown_tool", "category": "read"}
        config = {"allowed_tools": ["get_flight_status", "get_stand_status"]}
        assert is_tool_allowed(tool, config) is False

    def test_allows_tool_in_allowed_list(self):
        """Tool in allowed_tools → allowed"""
        tool = {"name": "get_flight_status", "category": "read"}
        config = {"allowed_tools": ["get_flight_status", "get_stand_status"]}
        assert is_tool_allowed(tool, config) is True

    def test_denies_tool_when_category_not_allowed(self):
        """Tool category not in allowed_tool_categories → denied"""
        tool = {"name": "write_action", "category": "write"}
        config = {"allowed_tool_categories": ["read"]}
        assert is_tool_allowed(tool, config) is False

    def test_denied_list_takes_precedence_over_allowed_list(self):
        """If a tool is in both allowed and denied, denied wins"""
        tool = {"name": "restricted_tool", "category": "read"}
        config = {
            "allowed_tools": ["restricted_tool", "safe_tool"],
            "denied_tools": ["restricted_tool"],
        }
        assert is_tool_allowed(tool, config) is False

    def test_allows_tool_when_category_in_allowed_categories(self):
        """Tool category in allowed_tool_categories → allowed"""
        tool = {"name": "get_flight", "category": "read"}
        config = {"allowed_tool_categories": ["read"]}
        assert is_tool_allowed(tool, config) is True


class TestToolExecutorEntityAclEnforcement:
    """Tests that ToolExecutor enforces allowed_tool_names at execution time."""

    @pytest.mark.asyncio
    async def test_rejects_tool_not_in_allowed_tool_names(self):
        """Tool not in allowed_tool_names must be rejected at execution time"""
        executor = ToolExecutor()

        result = await executor.execute(
            tool_call={
                "tool_call_id": "tc-1",
                "tool_name": "get_flight_status",
                "arguments": {"flight_id": "CA123"},
            },
            run_id="run-1",
            allowed_tool_names={"get_stand_status"},  # get_flight_status NOT in set
        )

        assert result.success is False
        assert "TOOL_NOT_IN_ALLOWED_SET" in result.error
        assert "get_flight_status" in result.error

    @pytest.mark.asyncio
    async def test_allows_tool_in_allowed_tool_names(self):
        """Tool in allowed_tool_names proceeds to normal execution"""
        executor = ToolExecutor()

        # We need to handle the fact that get_flight_status is a read-only tool
        # but it will fail without proper context. We just need to verify it's NOT
        # rejected for NOT_IN_ALLOWED_SET reason.
        result = await executor.execute(
            tool_call={
                "tool_call_id": "tc-2",
                "tool_name": "get_flight_status",
                "arguments": {"flight_id": "CA123"},
            },
            run_id="run-2",
            allowed_tool_names={"get_flight_status", "get_stand_status"},
        )

        # It should NOT fail with TOOL_NOT_IN_ALLOWED_SET (it may fail for other reasons
        # like missing data sources, which is fine)
        assert "TOOL_NOT_IN_ALLOWED_SET" not in (result.error or "")

    @pytest.mark.asyncio
    async def test_allows_all_tools_when_allowed_tool_names_is_none(self):
        """When allowed_tool_names is None (backward compat), no ACL enforcement at this layer"""
        executor = ToolExecutor()

        result = await executor.execute(
            tool_call={
                "tool_call_id": "tc-3",
                "tool_name": "get_flight_status",
                "arguments": {"flight_id": "CA123"},
            },
            run_id="run-3",
            allowed_tool_names=None,
        )

        # Should NOT fail with TOOL_NOT_IN_ALLOWED_SET
        assert "TOOL_NOT_IN_ALLOWED_SET" not in (result.error or "")

    @pytest.mark.asyncio
    async def test_rejects_unknown_tool_not_in_allowed_set(self):
        """Unknown/hallucinated tool not in allowed set must be rejected"""
        executor = ToolExecutor()

        result = await executor.execute(
            tool_call={
                "tool_call_id": "tc-4",
                "tool_name": "completely_fake_tool_that_does_not_exist",
                "arguments": {},
            },
            run_id="run-4",
            allowed_tool_names={"get_flight_status"},
        )

        assert result.success is False
        assert "TOOL_NOT_IN_ALLOWED_SET" in result.error

    @pytest.mark.asyncio
    async def test_execute_batch_enforces_allowed_tool_names(self):
        """execute_batch also enforces allowed_tool_names for all calls"""
        executor = ToolExecutor()

        results = await executor.execute_batch(
            tool_calls=[
                {"tool_call_id": "tc-a", "tool_name": "allowed_tool", "arguments": {}},
                {"tool_call_id": "tc-b", "tool_name": "disallowed_tool", "arguments": {}},
            ],
            run_id="run-5",
            allowed_tool_names={"allowed_tool"},
        )

        assert len(results) == 2
        # allowed_tool proceeds (may fail for other reasons but not ACL)
        assert "TOOL_NOT_IN_ALLOWED_SET" not in (results[0].error or "")
        # disallowed_tool is rejected
        assert results[1].success is False
        assert "TOOL_NOT_IN_ALLOWED_SET" in results[1].error

    @pytest.mark.asyncio
    async def test_mcp_execution_denies_category_not_allowed_by_binding(self):
        """Execution-time MCP ACL must enforce allowed_tool_categories, not only names."""
        mock_repo = MagicMock()
        mock_repo.get_capabilities = AsyncMock(
            return_value={
                "tools": [
                    {
                        "name": "write_action",
                        "category": "write",
                        "annotations": {"destructive": False},
                    }
                ]
            }
        )
        mock_repo.find_bindings_by_entity = AsyncMock(
            return_value=[
                {
                    "server_id": "srv-1",
                    "enabled": True,
                    "allowed_tools": None,
                    "denied_tools": [],
                    "allowed_tool_categories": ["read"],
                }
            ]
        )
        executor = ToolExecutor(mcp_repo=mock_repo, mcp_client_manager=MagicMock())
        envelope = MagicMock()
        envelope.entity_id = "entity-1"

        # Call _execute_mcp_tool directly to bypass MQ gate and test entity ACL
        result = await executor._execute_mcp_tool(
            tool_call_id="tc-cat",
            tool_name="mcp.srv-1.write_action",
            arguments={},
            run_id="run-cat",
            envelope=envelope,
        )

        assert result.success is False
        assert "MCP_TOOL_NOT_ALLOWED_BY_BINDING" in result.error

    @pytest.mark.asyncio
    async def test_mcp_execution_fails_closed_on_invalid_binding_acl_json(self):
        """Invalid security-sensitive binding JSON must not become an empty allowlist."""
        mock_repo = MagicMock()
        mock_repo.get_capabilities = AsyncMock(
            return_value={
                "tools": [
                    {
                        "name": "dangerous_tool",
                        "category": "write",
                        "annotations": {"destructive": False},
                    }
                ]
            }
        )
        mock_repo.find_bindings_by_entity = AsyncMock(
            return_value=[
                {
                    "server_id": "srv-1",
                    "enabled": True,
                    "allowed_tools": "not-json",
                    "denied_tools": [],
                }
            ]
        )
        executor = ToolExecutor(mcp_repo=mock_repo, mcp_client_manager=MagicMock())
        envelope = MagicMock()
        envelope.entity_id = "entity-1"

        # Call _execute_mcp_tool directly to bypass MQ gate and test entity ACL
        result = await executor._execute_mcp_tool(
            tool_call_id="tc-json",
            tool_name="mcp.srv-1.dangerous_tool",
            arguments={},
            run_id="run-json",
            envelope=envelope,
        )

        assert result.success is False
        assert "MCP_BINDING_ACL_INVALID" in result.error
