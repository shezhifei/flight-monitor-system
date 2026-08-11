"""MCP 服务器和绑定仓储 - 操作 ai_mcp_servers, ai_mcp_server_capabilities, ai_entity_mcp_bindings 表"""

import logging
from abc import ABC, abstractmethod
from typing import Any

from src.infrastructure.common.runtime_utils import decode_jsonb_or_raise, parse_json_field

logger = logging.getLogger(__name__)

# JSONB fields in ai_entity_mcp_bindings that are security-sensitive.
# Parse failures here must raise rather than silently default to empty,
# because an empty allow/deny list weakens ACL enforcement.
_SECURITY_JSONB_FIELDS = frozenset({"allowed_tools", "denied_tools", "allowed_resources"})

# Non-security JSONB fields and their default values when parse fails.
_NON_SECURITY_JSONB_DEFAULTS: dict[str, Any] = {
    "args": [],
    "env_secret_refs": [],
    "risk_policy": {},
    "tools": [],
    "resources": [],
    "prompts": [],
    "allowed_prompts": [],
    "tool_defaults": {},
}


def _decode_row_jsonb(row: dict[str, Any]) -> dict[str, Any]:
    """Decode all known JSONB fields in a row dict at the repository boundary.

    Security fields (allowed_tools, denied_tools, allowed_resources) raise on
    parse failure.  Non-security fields fall back to a safe default.
    """
    for field_name in _SECURITY_JSONB_FIELDS:
        if field_name in row:
            row[field_name] = decode_jsonb_or_raise(row[field_name], field_name)
    for field_name, default in _NON_SECURITY_JSONB_DEFAULTS.items():
        if field_name in row:
            row[field_name] = parse_json_field(row[field_name], default=default)
    return row


class McpRepository(ABC):
    """MCP 仓储接口"""

    # === Server ===
    @abstractmethod
    async def find_all_servers(self) -> list[dict[str, Any]]:
        pass

    @abstractmethod
    async def find_server_by_id(self, server_id: str) -> dict[str, Any] | None:
        pass

    @abstractmethod
    async def upsert_server(self, server_id: str, data: dict[str, Any]) -> dict[str, Any]:
        pass

    @abstractmethod
    async def delete_server(self, server_id: str) -> bool:
        pass

    # === Capabilities ===
    @abstractmethod
    async def get_capabilities(self, server_id: str) -> dict[str, Any] | None:
        pass

    @abstractmethod
    async def upsert_capabilities(self, server_id: str, data: dict[str, Any]) -> dict[str, Any]:
        pass

    # === Bindings ===
    @abstractmethod
    async def find_bindings_by_entity(self, entity_id: str) -> list[dict[str, Any]]:
        pass

    @abstractmethod
    async def upsert_binding(self, binding_id: str, data: dict[str, Any]) -> dict[str, Any]:
        pass

    @abstractmethod
    async def delete_binding(self, binding_id: str) -> bool:
        pass


class PostgresMcpRepository(McpRepository):
    """PostgreSQL MCP 仓储实现"""

    def __init__(self, pool):
        self._pool = pool

    async def find_all_servers(self) -> list[dict[str, Any]]:
        query = "SELECT * FROM ai_mcp_servers ORDER BY server_id"
        async with self._pool.acquire() as conn:
            rows = await conn.fetch(query)
            return [_decode_row_jsonb(dict(row)) for row in rows]

    async def find_server_by_id(self, server_id: str) -> dict[str, Any] | None:
        query = "SELECT * FROM ai_mcp_servers WHERE server_id = $1"
        async with self._pool.acquire() as conn:
            row = await conn.fetchrow(query, server_id)
            return _decode_row_jsonb(dict(row)) if row else None

    async def upsert_server(self, server_id: str, data: dict[str, Any]) -> dict[str, Any]:
        import json

        query = """
            INSERT INTO ai_mcp_servers (
                server_id, display_name, description, transport,
                command_ref, endpoint_url, args, env_secret_refs,
                risk_policy, timeout_seconds, startup_timeout_seconds,
                max_concurrency, status
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            ON CONFLICT (server_id) DO UPDATE SET
                display_name = EXCLUDED.display_name,
                description = EXCLUDED.description,
                transport = EXCLUDED.transport,
                command_ref = EXCLUDED.command_ref,
                endpoint_url = EXCLUDED.endpoint_url,
                args = EXCLUDED.args,
                env_secret_refs = EXCLUDED.env_secret_refs,
                risk_policy = EXCLUDED.risk_policy,
                timeout_seconds = EXCLUDED.timeout_seconds,
                startup_timeout_seconds = EXCLUDED.startup_timeout_seconds,
                max_concurrency = EXCLUDED.max_concurrency,
                status = EXCLUDED.status,
                updated_at = now()
            RETURNING *
        """
        async with self._pool.acquire() as conn:
            row = await conn.fetchrow(
                query,
                server_id,
                data.get("display_name", server_id),
                data.get("description"),
                data.get("transport", "stdio"),
                data.get("command_ref"),
                data.get("endpoint_url"),
                json.dumps(data.get("args", [])),
                json.dumps(data.get("env_secret_refs", [])),
                json.dumps(data.get("risk_policy", {})),
                data.get("timeout_seconds", 10),
                data.get("startup_timeout_seconds", 5),
                data.get("max_concurrency", 4),
                data.get("status", "draft"),
            )
            return _decode_row_jsonb(dict(row))

    async def delete_server(self, server_id: str) -> bool:
        query = "DELETE FROM ai_mcp_servers WHERE server_id = $1"
        async with self._pool.acquire() as conn:
            result = await conn.execute(query, server_id)
            return result == "DELETE 1"

    async def get_capabilities(self, server_id: str) -> dict[str, Any] | None:
        query = "SELECT * FROM ai_mcp_server_capabilities WHERE server_id = $1"
        async with self._pool.acquire() as conn:
            row = await conn.fetchrow(query, server_id)
            return _decode_row_jsonb(dict(row)) if row else None

    async def upsert_capabilities(self, server_id: str, data: dict[str, Any]) -> dict[str, Any]:
        import json

        query = """
            INSERT INTO ai_mcp_server_capabilities (
                server_id, protocol_version, tools, resources, prompts,
                schema_hash, expires_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (server_id) DO UPDATE SET
                protocol_version = EXCLUDED.protocol_version,
                tools = EXCLUDED.tools,
                resources = EXCLUDED.resources,
                prompts = EXCLUDED.prompts,
                schema_hash = EXCLUDED.schema_hash,
                discovered_at = now(),
                expires_at = EXCLUDED.expires_at
            RETURNING *
        """
        async with self._pool.acquire() as conn:
            row = await conn.fetchrow(
                query,
                server_id,
                data.get("protocol_version"),
                json.dumps(data.get("tools", [])),
                json.dumps(data.get("resources", [])),
                json.dumps(data.get("prompts", [])),
                data.get("schema_hash", ""),
                data.get("expires_at"),
            )
            return _decode_row_jsonb(dict(row))

    async def find_bindings_by_entity(self, entity_id: str) -> list[dict[str, Any]]:
        query = """
            SELECT * FROM ai_entity_mcp_bindings
            WHERE entity_id = $1
            ORDER BY binding_id
        """
        async with self._pool.acquire() as conn:
            rows = await conn.fetch(query, entity_id)
            return [_decode_row_jsonb(dict(row)) for row in rows]

    async def upsert_binding(self, binding_id: str, data: dict[str, Any]) -> dict[str, Any]:
        import json

        query = """
            INSERT INTO ai_entity_mcp_bindings (
                binding_id, entity_id, server_id, enabled,
                allowed_tools, denied_tools, allowed_resources,
                allowed_prompts, tool_defaults
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (binding_id) DO UPDATE SET
                entity_id = EXCLUDED.entity_id,
                server_id = EXCLUDED.server_id,
                enabled = EXCLUDED.enabled,
                allowed_tools = EXCLUDED.allowed_tools,
                denied_tools = EXCLUDED.denied_tools,
                allowed_resources = EXCLUDED.allowed_resources,
                allowed_prompts = EXCLUDED.allowed_prompts,
                tool_defaults = EXCLUDED.tool_defaults,
                updated_at = now()
            RETURNING *
        """
        async with self._pool.acquire() as conn:
            row = await conn.fetchrow(
                query,
                binding_id,
                data.get("entity_id"),
                data.get("server_id"),
                data.get("enabled", False),
                json.dumps(data.get("allowed_tools", [])),
                json.dumps(data.get("denied_tools", [])),
                json.dumps(data.get("allowed_resources", [])),
                json.dumps(data.get("allowed_prompts", [])),
                json.dumps(data.get("tool_defaults", {})),
            )
            return _decode_row_jsonb(dict(row))

    async def delete_binding(self, binding_id: str) -> bool:
        query = "DELETE FROM ai_entity_mcp_bindings WHERE binding_id = $1"
        async with self._pool.acquire() as conn:
            result = await conn.execute(query, binding_id)
            return result == "DELETE 1"
