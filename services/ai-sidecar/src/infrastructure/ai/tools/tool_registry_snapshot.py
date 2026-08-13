"""统一工具注册表快照 - 合并多种来源的工具

将 builtin、MCP、skill 来源的工具合并为统一的 ToolRegistrySnapshot。
"""

from __future__ import annotations

import hashlib
import json
import logging
from dataclasses import dataclass
from typing import Any, Literal

logger = logging.getLogger(__name__)


@dataclass
class ToolDefinition:
    """工具定义"""

    name: str
    display_name: str
    description: str
    parameters: dict[str, Any]
    source: Literal["builtin", "mcp", "skill"]
    category: str = ""
    risk_level: str = "low"
    cacheable: bool = False
    side_effect: bool = False

    # MCP 特有
    server_id: str | None = None
    original_name: str | None = None

    # Skill 特有
    skill_slug: str | None = None

    # 治理元数据（不在 LLM 可见的 function schema 中）
    governance: dict[str, Any] | None = None

    def to_schema(self) -> dict[str, Any]:
        """转换为 LLM 工具 schema"""
        schema = {
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters,
            },
        }
        return schema

    def to_dict(self) -> dict[str, Any]:
        """转换为字典"""
        return {
            "name": self.name,
            "display_name": self.display_name,
            "description": self.description,
            "parameters": self.parameters,
            "source": self.source,
            "category": self.category,
            "risk_level": self.risk_level,
            "cacheable": self.cacheable,
            "side_effect": self.side_effect,
            "server_id": self.server_id,
            "original_name": self.original_name,
            "skill_slug": self.skill_slug,
            "governance": self.governance,
        }


@dataclass
class ToolRegistrySnapshot:
    """工具注册表快照

    不可变的工具集合，在 run 开始前生成，运行中只读。
    """

    tools: list[ToolDefinition]
    schema_hash: str = ""
    builtin_count: int = 0
    mcp_count: int = 0
    skill_count: int = 0
    denied_count: int = 0

    def __post_init__(self):
        if not self.schema_hash:
            self.schema_hash = self._compute_hash()

    def _compute_hash(self) -> str:
        """计算 schema hash"""
        tool_schemas = [t.to_schema() for t in self.tools]
        hash_input = json.dumps(tool_schemas, sort_keys=True)
        return hashlib.sha256(hash_input.encode()).hexdigest()[:16]

    def get_tool(self, name: str) -> ToolDefinition | None:
        """根据名称获取工具"""
        for tool in self.tools:
            if tool.name == name:
                return tool
        return None

    def get_tools_by_source(self, source: str) -> list[ToolDefinition]:
        """根据来源获取工具列表"""
        return [t for t in self.tools if t.source == source]

    def get_tools_by_category(self, category: str) -> list[ToolDefinition]:
        """根据类别获取工具列表"""
        return [t for t in self.tools if t.category == category]

    def to_llm_schemas(self) -> list[dict[str, Any]]:
        """转换为 LLM 工具 schema 列表"""
        return [t.to_schema() for t in self.tools]

    def to_summary(self) -> dict[str, Any]:
        """返回摘要信息"""
        return {
            "total_tools": len(self.tools),
            "builtin_count": self.builtin_count,
            "mcp_count": self.mcp_count,
            "skill_count": self.skill_count,
            "denied_count": self.denied_count,
            "schema_hash": self.schema_hash,
        }


class ToolRegistrySnapshotBuilder:
    """工具注册表快照构建器"""

    def __init__(
        self,
        builtin_tools: list[dict[str, Any]] | None = None,
    ):
        self._builtin_tools = builtin_tools or []

    async def build(
        self,
        tooling_config: dict[str, Any],
        mcp_tools: list[dict[str, Any]] | None = None,
        skill_tools: list[dict[str, Any]] | None = None,
    ) -> ToolRegistrySnapshot:
        """构建工具注册表快照

        Args:
            tooling_config: 工具配置
            mcp_tools: MCP 工具列表
            skill_tools: Skill 工具列表（第一阶段通常为空）

        Returns:
            ToolRegistrySnapshot: 不可变的工具快照
        """
        if not tooling_config.get("enabled", True):
            return ToolRegistrySnapshot(tools=[])

        allowed_sources = tooling_config.get("allowed_tool_sources", ["builtin"])
        allowed_categories = tooling_config.get("allowed_tool_categories", [])
        allowed_tools = tooling_config.get("allowed_tools")
        denied_tools = tooling_config.get("denied_tools", [])

        tools = []
        denied_count = 0
        builtin_count = 0
        mcp_count = 0
        skill_count = 0

        # 1. Builtin tools
        if "builtin" in allowed_sources:
            for tool_data in self._builtin_tools:
                tool = self._build_builtin_tool(tool_data)
                if self._is_tool_allowed(tool, allowed_categories, allowed_tools, denied_tools):
                    tools.append(tool)
                    builtin_count += 1
                else:
                    denied_count += 1

        # 2. MCP tools
        if "mcp" in allowed_sources and mcp_tools:
            for tool_data in mcp_tools:
                tool = self._build_mcp_tool(tool_data)
                if self._is_tool_allowed(tool, allowed_categories, allowed_tools, denied_tools):
                    tools.append(tool)
                    mcp_count += 1
                else:
                    denied_count += 1

        # 3. Skill tools (第一阶段通常为空，skill 只作为指令上下文)
        if "skill" in allowed_sources and skill_tools:
            for tool_data in skill_tools:
                tool = self._build_skill_tool(tool_data)
                if self._is_tool_allowed(tool, allowed_categories, allowed_tools, denied_tools):
                    tools.append(tool)
                    skill_count += 1
                else:
                    denied_count += 1

        snapshot = ToolRegistrySnapshot(
            tools=tools,
            builtin_count=builtin_count,
            mcp_count=mcp_count,
            skill_count=skill_count,
            denied_count=denied_count,
        )

        logger.info(
            f"Built tool registry snapshot: "
            f"total={len(tools)}, builtin={builtin_count}, "
            f"mcp={mcp_count}, skill={skill_count}, denied={denied_count}"
        )

        return snapshot

    def _build_builtin_tool(self, data: dict[str, Any], governance: dict[str, Any] | None = None) -> ToolDefinition:
        """构建 builtin 工具"""
        return ToolDefinition(
            name=data.get("name", ""),
            display_name=data.get("name", ""),
            description=data.get("description", ""),
            parameters=data.get("parameters", {}),
            source="builtin",
            category=data.get("category", ""),
            risk_level=data.get("risk_level", "low"),
            cacheable=data.get("cacheable", False),
            side_effect=data.get("side_effect", False),
            governance=governance,
        )

    def _build_mcp_tool(self, data: dict[str, Any], governance: dict[str, Any] | None = None) -> ToolDefinition:
        """构建 MCP 工具"""
        server_id = data.get("server_id", "unknown")
        original_name = data.get("name", "")
        prefixed_name = f"mcp.{server_id}.{original_name}"

        return ToolDefinition(
            name=prefixed_name,
            display_name=f"MCP: {original_name}",
            description=data.get("description", ""),
            parameters=data.get("parameters", {}),
            source="mcp",
            category=data.get("category", "mcp"),
            risk_level=data.get("risk_level", "medium"),
            cacheable=data.get("cacheable", False),
            side_effect=data.get("side_effect", True),
            server_id=server_id,
            original_name=original_name,
            governance=governance,
        )

    def _build_skill_tool(self, data: dict[str, Any], governance: dict[str, Any] | None = None) -> ToolDefinition:
        """构建 Skill 工具"""
        skill_slug = data.get("skill_slug", "unknown")
        original_name = data.get("name", "")
        prefixed_name = f"skill.{skill_slug}.{original_name}"

        return ToolDefinition(
            name=prefixed_name,
            display_name=f"Skill: {original_name}",
            description=data.get("description", ""),
            parameters=data.get("parameters", {}),
            source="skill",
            category=data.get("category", "skill"),
            risk_level=data.get("risk_level", "medium"),
            cacheable=data.get("cacheable", False),
            side_effect=data.get("side_effect", True),
            skill_slug=skill_slug,
            original_name=original_name,
            governance=governance,
        )

    def _is_tool_allowed(
        self,
        tool: ToolDefinition,
        allowed_categories: list[str],
        allowed_tools: list[str] | None,
        denied_tools: list[str],
    ) -> bool:
        """检查工具是否被允许"""
        # 检查拒绝列表
        if tool.name in denied_tools:
            return False
        if tool.original_name and tool.original_name in denied_tools:
            return False

        # 检查允许列表
        if allowed_tools is not None and tool.name not in allowed_tools and tool.original_name not in allowed_tools:
            return False

        # 检查类别
        return not (allowed_categories and tool.category and tool.category not in allowed_categories)


__all__ = [
    "ToolDefinition",
    "ToolRegistrySnapshot",
    "ToolRegistrySnapshotBuilder",
]
