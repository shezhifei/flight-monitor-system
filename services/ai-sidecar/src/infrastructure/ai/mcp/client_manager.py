"""MCP 客户端管理器 - 管理 MCP server 连接和工具发现"""

from __future__ import annotations

import asyncio
import hashlib
import json
import logging
from dataclasses import dataclass, field
from typing import Any

from src.infrastructure.common.runtime_utils import decode_jsonb_or_raise

logger = logging.getLogger(__name__)


@dataclass
class McpToolInfo:
    """MCP 工具信息"""

    name: str
    description: str
    parameters: dict[str, Any]
    server_id: str
    cacheable: bool = False
    side_effect: bool = True


@dataclass
class McpResourceInfo:
    """MCP 资源信息"""

    uri: str
    name: str
    description: str
    mime_type: str
    server_id: str


@dataclass
class McpServerSession:
    """MCP 服务器会话"""

    server_id: str
    transport: str
    status: str = "disconnected"  # disconnected, connecting, connected, error
    tools: list[McpToolInfo] = field(default_factory=list)
    resources: list[McpResourceInfo] = field(default_factory=list)
    error: str | None = None
    _process: Any = None
    _reader: Any = None
    _writer: Any = None


class McpClientManager:
    """MCP 客户端管理器

    职责：
    1. 管理 MCP server 的连接生命周期
    2. 执行 tool/resource discovery
    3. 调用 MCP tools
    4. 读取 MCP resources
    5. 缓存 discovery 结果

    第一阶段只支持 stdio transport。
    """

    def __init__(
        self,
        mcp_repo=None,
        command_allowlist: dict[str, Any] | None = None,
        cache_manager=None,
    ):
        self._mcp_repo = mcp_repo
        self._command_allowlist = command_allowlist or {}
        self._cache_manager = cache_manager
        self._sessions: dict[str, McpServerSession] = {}
        self._discovery_cache: dict[str, dict[str, Any]] = {}
        self._lock = asyncio.Lock()

    async def connect_server(
        self,
        server_id: str,
        server_config: dict[str, Any],
        timeout: float | None = None,
        startup_timeout: float | None = None,
    ) -> McpServerSession:
        """连接到 MCP server

        Args:
            server_id: 服务器 ID
            server_config: 服务器配置
            timeout: 请求超时秒数
            startup_timeout: 启动和初始化超时秒数

        Returns:
            McpServerSession: 服务器会话
        """
        async with self._lock:
            # 检查是否已连接
            session = self._sessions.get(server_id)
            if session and session.status == "connected":
                return session

            # 创建新会话
            session = McpServerSession(
                server_id=server_id,
                transport=server_config.get("transport", "stdio"),
            )
            self._sessions[server_id] = session

            try:
                transport = server_config.get("transport", "stdio")

                if transport == "stdio":
                    await self._connect_stdio(session, server_config, startup_timeout=startup_timeout)
                else:
                    raise ValueError(f"Unsupported transport: {transport}")

                session.status = "connected"
                logger.info(f"Connected to MCP server: {server_id}")

            except Exception as e:
                session.status = "error"
                session.error = str(e)
                logger.error(f"Failed to connect to MCP server {server_id}: {e}")
                raise

            return session

    @staticmethod
    def _validate_args(
        command_ref: str,
        caller_args: list[str],
        allowlist_entry: dict[str, Any],
    ) -> list[str]:
        """Validate caller-supplied args against the command allowlist entry.

        If the entry defines ``allowed_args``, each caller arg must appear in
        that set.  If ``allowed_args`` is absent, *no* caller args are permitted
        (safe default — only the admin-controlled ``args_prefix`` is used).
        """
        allowed = allowlist_entry.get("allowed_args")
        if allowed is None:
            if caller_args:
                raise ValueError(
                    f"Command ref '{command_ref}' does not accept caller-supplied args. "
                    f"Set 'allowed_args' in the allowlist entry to permit specific values."
                )
            return []
        allowed_set = set(allowed)
        for arg in caller_args:
            if arg not in allowed_set:
                raise ValueError(
                    f"Arg '{arg}' is not in allowed_args for command_ref '{command_ref}'. "
                    f"Allowed: {sorted(allowed_set)}"
                )
        return caller_args

    async def _connect_stdio(
        self,
        session: McpServerSession,
        config: dict[str, Any],
        startup_timeout: float | None = None,
    ) -> None:
        """通过 stdio 连接 MCP server"""
        command_ref = config.get("command_ref")

        # 验证命令在 allowlist 中
        if command_ref not in self._command_allowlist:
            raise ValueError(
                f"Command ref '{command_ref}' not in allowlist. Allowed: {list(self._command_allowlist.keys())}"
            )

        allowlist_entry = self._command_allowlist[command_ref]
        executable = allowlist_entry.get("executable")
        args_prefix = allowlist_entry.get("args_prefix", [])
        working_dir = allowlist_entry.get("working_dir")

        # 构建完整命令 — validate caller args before use
        caller_args = config.get("args", [])
        validated_args = self._validate_args(command_ref, caller_args, allowlist_entry)
        full_args = args_prefix + validated_args

        session.status = "connecting"

        # 启动进程
        process = await asyncio.create_subprocess_exec(
            executable,
            *full_args,
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            cwd=working_dir,
        )

        session._process = process
        session._reader = process.stdout
        session._writer = process.stdin

        # 发送 initialize 请求
        await self._send_initialize(session, config, timeout=startup_timeout)

    async def _send_initialize(
        self,
        session: McpServerSession,
        config: dict[str, Any],
        timeout: float | None = None,
    ) -> None:
        """发送 MCP initialize 请求"""
        request = {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "roots": {"listChanged": True},
                },
                "clientInfo": {
                    "name": "flight-monitor-ai-sidecar",
                    "version": "1.0.0",
                },
            },
        }

        response = await self._send_request(session, request, timeout=timeout)

        if "error" in response:
            raise RuntimeError(f"MCP initialize failed: {response['error']}")

        logger.info(f"MCP server {session.server_id} initialized: {response.get('result', {})}")

    async def _send_request(
        self,
        session: McpServerSession,
        request: dict[str, Any],
        timeout: float | None = None,
    ) -> dict[str, Any]:
        """发送 JSON-RPC 请求并等待响应"""
        if not session._writer or not session._reader:
            raise RuntimeError("Session not connected")

        # 发送请求
        request_bytes = json.dumps(request).encode() + b"\n"
        session._writer.write(request_bytes)
        await session._writer.drain()

        # 读取响应
        response_line = await asyncio.wait_for(
            session._reader.readline(),
            timeout=timeout or 30.0,
        )

        if not response_line:
            raise RuntimeError("No response from MCP server")

        return json.loads(response_line.decode())

    async def discover_tools(self, server_id: str, timeout: float | None = None) -> list[McpToolInfo]:
        """发现 MCP server 的工具"""
        session = self._sessions.get(server_id)
        if not session or session.status != "connected":
            raise RuntimeError(f"Server {server_id} not connected")

        # 检查缓存
        cache_key = f"tools:{server_id}"
        if cache_key in self._discovery_cache:
            cached = self._discovery_cache[cache_key]
            return cached.get("tools", [])

        request = {
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
        }

        response = await self._send_request(session, request, timeout=timeout)

        if "error" in response:
            raise RuntimeError(f"tools/list failed: {response['error']}")

        tools_data = response.get("result", {}).get("tools", [])
        tools = []

        for tool_data in tools_data:
            tool = McpToolInfo(
                name=tool_data.get("name", ""),
                description=tool_data.get("description", ""),
                parameters=tool_data.get("inputSchema", {}),
                server_id=server_id,
                cacheable=tool_data.get("annotations", {}).get("cacheable", False),
                side_effect=tool_data.get("annotations", {}).get("destructive", True),
            )
            tools.append(tool)

        session.tools = tools

        # 缓存结果
        self._discovery_cache[cache_key] = {
            "tools": tools,
            "schema_hash": self._compute_tools_hash(tools),
        }

        logger.info(f"Discovered {len(tools)} tools from MCP server {server_id}")
        return tools

    async def discover_resources(self, server_id: str, timeout: float | None = None) -> dict[str, Any]:
        """发现 MCP server 的资源

        Returns:
            dict with "resources" (List[McpResourceInfo]) and "schema_hash"
        """
        session = self._sessions.get(server_id)
        if not session or session.status != "connected":
            raise RuntimeError(f"Server {server_id} not connected")

        request = {
            "jsonrpc": "2.0",
            "id": 3,
            "method": "resources/list",
        }

        response = await self._send_request(session, request, timeout=timeout)

        if "error" in response:
            raise RuntimeError(f"resources/list failed: {response['error']}")

        resources_data = response.get("result", {}).get("resources", [])
        resources = []
        for res_data in resources_data:
            resource = McpResourceInfo(
                uri=res_data.get("uri", ""),
                name=res_data.get("name", ""),
                description=res_data.get("description", ""),
                mime_type=res_data.get("mimeType", ""),
                server_id=server_id,
            )
            resources.append(resource)

        session.resources = resources
        logger.info(f"Discovered {len(resources)} resources from MCP server {server_id}")

        return {
            "resources": resources,
            "schema_hash": hashlib.sha256(
                json.dumps([{"uri": r.uri, "name": r.name} for r in resources], sort_keys=True).encode()
            ).hexdigest()[:16],
        }

    async def discover_prompts(self, server_id: str, timeout: float | None = None) -> dict[str, Any]:
        """发现 MCP server 的提示模板"""
        session = self._sessions.get(server_id)
        if not session or session.status != "connected":
            raise RuntimeError(f"Server {server_id} not connected")

        request = {
            "jsonrpc": "2.0",
            "id": 4,
            "method": "prompts/list",
        }

        response = await self._send_request(session, request, timeout=timeout)

        if "error" in response:
            raise RuntimeError(f"prompts/list failed: {response['error']}")

        prompts_data = response.get("result", {}).get("prompts", [])
        logger.info(f"Discovered {len(prompts_data)} prompts from MCP server {server_id}")

        return {
            "prompts": prompts_data,
            "schema_hash": hashlib.sha256(json.dumps(prompts_data, sort_keys=True).encode()).hexdigest()[:16],
        }

    async def discover_all(self, server_id: str, timeout: float | None = None) -> dict[str, Any]:
        """全面发现 MCP server 的 tools / resources / prompts

        Returns dict with keys: tools, resources, prompts, schema_hash
        """
        tools = await self.discover_tools(server_id, timeout=timeout)
        resources_result = await self.discover_resources(server_id, timeout=timeout)
        prompts_result = await self.discover_prompts(server_id, timeout=timeout)

        combined_hash_input = {
            "tools": self._compute_tools_hash(tools),
            "resources": resources_result.get("schema_hash", ""),
            "prompts": prompts_result.get("schema_hash", ""),
        }
        schema_hash = hashlib.sha256(json.dumps(combined_hash_input, sort_keys=True).encode()).hexdigest()[:16]

        return {
            "tools": tools,
            "resources": resources_result.get("resources", []),
            "prompts": prompts_result.get("prompts", []),
            "schema_hash": schema_hash,
        }

    async def call_tool(
        self,
        server_id: str,
        tool_name: str,
        arguments: dict[str, Any],
        timeout: float | None = None,
    ) -> dict[str, Any]:
        """调用 MCP tool"""
        session = self._sessions.get(server_id)
        if not session or session.status != "connected":
            raise RuntimeError(f"Server {server_id} not connected")

        request = {
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": arguments,
            },
        }

        response = await self._send_request(session, request, timeout=timeout)

        if "error" in response:
            raise RuntimeError(f"tools/call failed: {response['error']}")

        return response.get("result", {})

    @staticmethod
    def _extract_resource_text(result: dict[str, Any]) -> str:
        """Flatten an MCP ``resources/read`` result into a single text string.

        The MCP spec returns ``{"contents": [{"uri": ..., "text": ...} | {"blob": ...}]}``.
        We concatenate text parts; binary blobs are represented by a short marker so the
        cached value stays a plain string.
        """
        contents = result.get("contents", []) if isinstance(result, dict) else []
        parts: list[str] = []
        for item in contents:
            if not isinstance(item, dict):
                continue
            if "text" in item and item["text"] is not None:
                parts.append(str(item["text"]))
            elif "blob" in item:
                parts.append(f"[binary:{item.get('mimeType', 'application/octet-stream')}]")
        return "\n".join(parts)

    async def read_resource(
        self,
        server_id: str,
        resource_uri: str,
        ttl_seconds: int = 300,
        timeout: float | None = None,
        entity_id: str | None = None,
    ) -> dict[str, Any]:
        """读取 MCP 资源内容，带缓存读写。

        安全边界（本方法内强制执行，不依赖调用方）：
          1. entity_id 必填（用于实体级 ACL）
          2. entity 对该 server 必须有 enabled binding
          3. 若 binding 声明了 allowed_resources，resource_uri 必须在列表内
        所有安全检查在 cache read **之前**完成，防止缓存侧信道绕过授权。

        路径：ACL check → cache read -> (miss) JSON-RPC ``resources/read`` -> cache write。
        当注入了 ``cache_manager`` 时，命中缓存直接返回缓存内容，未命中则真实请求
        MCP server 并写回缓存（key=``ai:mcp:{entity}:{server}:{uri_hash}``，按 entity
        隔离，TTL 由调用方/MCP server 策略给定）。

        Returns:
            ``{"content": str, "cached": bool, "server_id": str, "resource_uri": str}``

        Raises:
            ValueError: entity_id 未提供
            PermissionError: entity 无 enabled binding 或 resource 不在 allowed_resources 中
            RuntimeError: server 未连接或 MCP 请求失败
        """
        await self.authorize_resource_read(server_id, resource_uri, entity_id)

        # 1. cache read (only after ACL passes)
        if self._cache_manager is not None:
            cached = await self._cache_manager.get_mcp_resource(
                server_id, resource_uri, ttl_seconds=ttl_seconds, entity_id=entity_id
            )
            if cached is not None:
                return {
                    "content": cached,
                    "cached": True,
                    "server_id": server_id,
                    "resource_uri": resource_uri,
                }

        # 2. real fetch via JSON-RPC resources/read
        session = self._sessions.get(server_id)
        if not session or session.status != "connected":
            raise RuntimeError(f"Server {server_id} not connected")

        request = {
            "jsonrpc": "2.0",
            "id": 6,
            "method": "resources/read",
            "params": {"uri": resource_uri},
        }
        response = await self._send_request(session, request, timeout=timeout)
        if "error" in response:
            raise RuntimeError(f"resources/read failed: {response['error']}")

        result = response.get("result", {})
        content = self._extract_resource_text(result)

        if self._cache_manager is not None:
            await self._cache_manager.set_mcp_resource(
                server_id, resource_uri, content, ttl_seconds=ttl_seconds, entity_id=entity_id
            )

        return {
            "content": content,
            "cached": False,
            "server_id": server_id,
            "resource_uri": resource_uri,
        }

    async def authorize_resource_read(
        self,
        server_id: str,
        resource_uri: str,
        entity_id: str | None = None,
    ) -> None:
        """Enforce entity binding and allowed_resources ACL without cache or IO."""
        if not entity_id:
            raise ValueError("entity_id is required for MCP resource read ACL enforcement")

        if not self._mcp_repo:
            raise RuntimeError("MCP repository not configured; cannot enforce resource ACL")

        bindings = await self._mcp_repo.find_bindings_by_entity(entity_id)
        binding = next(
            (b for b in bindings if b.get("server_id") == server_id and b.get("enabled")),
            None,
        )
        if not binding:
            raise PermissionError(
                f"MCP_BINDING_NOT_ENABLED: No enabled MCP binding for entity '{entity_id}' and server '{server_id}'"
            )

        try:
            allowed_resources = (
                decode_jsonb_or_raise(
                    binding.get("allowed_resources"),
                    "allowed_resources",
                )
                or []
            )
        except ValueError as exc:
            raise PermissionError(f"MCP_RESOURCE_ACL_INVALID: {exc}") from exc

        if allowed_resources and resource_uri not in allowed_resources:
            raise PermissionError(
                f"MCP_RESOURCE_NOT_ALLOWED: Resource '{resource_uri}' is not in the "
                f"binding's allowed_resources for entity '{entity_id}' and server '{server_id}'"
            )

    async def get_cached_resource_after_acl(
        self,
        server_id: str,
        resource_uri: str,
        ttl_seconds: int = 300,
        entity_id: str | None = None,
    ) -> dict[str, Any] | None:
        """Return cached resource content after ACL passes, or None on miss."""
        await self.authorize_resource_read(server_id, resource_uri, entity_id)
        if self._cache_manager is None:
            return None
        cached = await self._cache_manager.get_mcp_resource(
            server_id, resource_uri, ttl_seconds=ttl_seconds, entity_id=entity_id
        )
        if cached is None:
            return None
        return {
            "content": cached,
            "cached": True,
            "server_id": server_id,
            "resource_uri": resource_uri,
        }

    async def disconnect_server(self, server_id: str) -> None:
        """断开 MCP server 连接"""
        async with self._lock:
            session = self._sessions.pop(server_id, None)
            if session:
                if session._process:
                    try:
                        session._process.terminate()
                        await asyncio.wait_for(
                            session._process.wait(),
                            timeout=5.0,
                        )
                    except (TimeoutError, ProcessLookupError):
                        session._process.kill()

                session.status = "disconnected"
                logger.info(f"Disconnected from MCP server: {server_id}")

    async def disconnect_all(self) -> None:
        """断开所有 MCP server 连接"""
        server_ids = list(self._sessions.keys())
        for server_id in server_ids:
            await self.disconnect_server(server_id)

    def _compute_tools_hash(self, tools: list[McpToolInfo]) -> str:
        """计算工具列表 hash"""
        tool_dicts = [{"name": t.name, "description": t.description, "parameters": t.parameters} for t in tools]
        hash_input = json.dumps(tool_dicts, sort_keys=True)
        return hashlib.sha256(hash_input.encode()).hexdigest()[:16]

    def get_session(self, server_id: str) -> McpServerSession | None:
        """获取服务器会话"""
        return self._sessions.get(server_id)

    def get_all_sessions(self) -> dict[str, McpServerSession]:
        """获取所有服务器会话"""
        return dict(self._sessions)


__all__ = [
    "McpClientManager",
    "McpResourceInfo",
    "McpServerSession",
    "McpToolInfo",
]
