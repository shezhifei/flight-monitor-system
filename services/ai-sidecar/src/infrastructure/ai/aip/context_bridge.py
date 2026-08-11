"""
AIP Context Bridge - Ontology 与 LLM 上下文桥接

负责将 Ontology Schema、对象状态、关系信息注入到 LLM 上下文，
提升 LLM 对业务领域的理解能力。
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING, Any

from src.infrastructure.logging.core import get_logger

if TYPE_CHECKING:
    from ..ontology.schema import ActionDefinition, OntologyRegistry

logger = get_logger(__name__)


@dataclass
class ContextInjection:
    """上下文注入项"""

    name: str
    content: str
    priority: int = 0
    ttl_seconds: int = 300

    def to_dict(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "content": self.content,
            "priority": self.priority,
            "ttl_seconds": self.ttl_seconds,
        }


class OntologyContextBridge:
    """
    Ontology 上下文桥接器

    核心职责：
    1. 将 Ontology Schema 转换为 LLM 可理解的提示
    2. 注入对象关系图谱信息
    3. 动态上下文管理
    4. 基于查询意图的上下文选择
    """

    def __init__(self, ontology_registry: OntologyRegistry):
        self._ontology = ontology_registry
        self._injection_cache: dict[str, ContextInjection] = {}
        self._system_prompt_parts: list[str] = []

    def build_system_prompt(
        self, object_types: list[str] | None = None, include_actions: bool = True, include_relationships: bool = True
    ) -> str:
        """
        构建包含 Ontology 信息的系统提示词

        Args:
            object_types: 要包含的对象类型列表，None 表示全部
            include_actions: 是否包含 Actions
            include_relationships: 是否包含 Relationships

        Returns:
            格式化的系统提示词
        """
        prompt_parts = []

        prompt_parts.append("# 业务领域 Ontology\n")
        prompt_parts.append("你正在与航班地面保障监控系统交互。以下是系统中的业务对象定义：\n")

        schema = self._ontology.get_schema("default")
        if not schema:
            return "\n".join(prompt_parts)

        for obj_name, obj_def in schema.objects.items():
            if object_types and obj_name not in object_types:
                continue

            prompt_parts.append(f"## {obj_name}\n")
            prompt_parts.append(f"{obj_def.description}\n")

            if obj_def.properties:
                prompt_parts.append("### 属性:")
                for prop in obj_def.properties:
                    type_hint = prop.type
                    if prop.enum_values:
                        type_hint = f"enum({', '.join(prop.enum_values)})"
                    req_marker = "(必需)" if prop.required else ""
                    prompt_parts.append(f"- {prop.name}: {type_hint} {req_marker} - {prop.description}")

            if include_relationships and obj_def.relationships:
                prompt_parts.append("\n### 关系:")
                for rel in obj_def.relationships:
                    card = "[1]" if rel.cardinality == "one" else "[*]"
                    prompt_parts.append(f"- {rel.name} {card}→ {rel.target_object} - {rel.description}")

            if include_actions and obj_def.actions:
                prompt_parts.append("\n### 可执行动作:")
                actions = schema.get_object_actions(obj_name)
                for action in actions:
                    risk_marker = ""
                    if action.requires_approval:
                        risk_marker = " [需要审批]"
                    elif action.risk_level in ("HIGH", "CRITICAL"):
                        risk_marker = f" [{action.risk_level}风险]"
                    prompt_parts.append(f"- `{obj_name}.{action.name}`{risk_marker}: {action.description}")

            prompt_parts.append("")

        return "\n".join(prompt_parts)

    def build_object_context(
        self, object_type: str, object_id: str, object_data: dict[str, Any], depth: int = 1
    ) -> str:
        """
        构建单个对象的上下文描述

        Args:
            object_type: 对象类型
            object_id: 对象ID
            object_data: 对象数据
            depth: 关系深度

        Returns:
            格式化的对象描述
        """
        lines = []
        lines.append(f"**{object_type}** (ID: {object_id})")

        schema = self._ontology.get_object(object_type, "default")
        if not schema:
            for key, value in object_data.items():
                lines.append(f"  - {key}: {value}")
            return "\n".join(lines)

        for prop in schema.properties:
            value = object_data.get(prop.name)
            if value is not None:
                lines.append(f"  - {prop.name}: {value}")

        if depth > 0:
            for rel in schema.relationships:
                rel_data = object_data.get(rel.name)
                if rel_data:
                    if isinstance(rel_data, list):
                        lines.append(f"  - {rel.name}: {len(rel_data)} 个关联对象")
                        for item in rel_data[:3]:
                            if isinstance(item, dict):
                                lines.append(f"    - {item.get('id', 'unknown')}")
                    else:
                        lines.append(f"  - {rel.name}: {rel_data}")

        return "\n".join(lines)

    def build_query_context(self, query_type: str, relevant_objects: list[str]) -> str:
        """
        基于查询类型构建上下文

        Args:
            query_type: 查询类型（如 "flight_status", "team_availability"）
            relevant_objects: 相关的对象类型列表

        Returns:
            上下文提示
        """
        context_map = {
            "flight_status": [
                "Flight.status: 航班当前状态",
                "Flight.stand: 当前机位",
                "Flight.delay_minutes: 延误分钟数",
                "相关: Stand.available_capacity, Team.on_duty_count",
            ],
            "team_availability": [
                "Team.status: 班组当前状态 (on_duty/off_duty/break)",
                "Team.location: 当前位置",
                "Team.member_count: 成员数量",
                "相关: Flight.assigned_team, Equipment.assigned_team",
            ],
            "stand_allocation": [
                "Stand.status: 机位状态 (available/occupied/maintenance)",
                "Stand.size: 机位大小",
                "Flight.stand: 航班停靠机位",
                "相关: Flight.aircraft_type, Stand.max_wingspan",
            ],
            "anomaly_handling": [
                "Anomaly.severity: 异常严重程度 (low/medium/high/critical)",
                "Anomaly.status: 处理状态",
                "Anomaly.detected_at: 检测时间",
                "相关: Flight, Stand, Team",
            ],
        }

        context_parts = context_map.get(query_type, [])

        if not context_parts:
            schema = self._ontology.get_schema("default")
            if schema:
                for obj_name in relevant_objects:
                    obj = schema.get_object(obj_name)
                    if obj:
                        context_parts.append(f"\n**{obj_name}**: {obj.description}")

        return "\n".join(context_parts) if context_parts else ""

    def inject_object_schema(self, object_type: str, include_examples: bool = True) -> str:
        """
        生成对象 Schema 的 LLM 友好描述

        Args:
            object_type: 对象类型名称
            include_examples: 是否包含示例

        Returns:
            Schema 描述
        """
        cache_key = f"schema:{object_type}"
        if cache_key in self._injection_cache:
            return self._injection_cache[cache_key].content

        schema = self._ontology.get_object(object_type, "default")
        if not schema:
            return ""

        lines = [f"Object: {schema.name}"]
        lines.append(f"Description: {schema.description}\n")
        lines.append("Properties:")

        for prop in schema.properties:
            type_str = prop.type
            if prop.enum_values:
                type_str = f"{prop.type} ({'|'.join(prop.enum_values)})"
            req = " (required)" if prop.required else ""
            lines.append(f"  {prop.name}: {type_str}{req} - {prop.description}")

        if schema.relationships:
            lines.append("\nRelationships:")
            for rel in schema.relationships:
                lines.append(f"  {rel.name}: {rel.cardinality}→ {rel.target_object}")

        content = "\n".join(lines)

        self._injection_cache[cache_key] = ContextInjection(
            name=cache_key, content=content, priority=10, ttl_seconds=3600
        )

        return content

    def generate_action_prompt(self, action_def: ActionDefinition, include_risk_warning: bool = True) -> str:
        """
        生成 Action 的执行提示

        Args:
            action_def: Action 定义
            include_risk_warning: 是否包含风险警告

        Returns:
            Action 提示
        """
        lines = []

        lines.append(f"### {action_def.object_type}.{action_def.name}")
        lines.append(f"{action_def.description}\n")

        if action_def.parameters:
            lines.append("**Parameters:**")
            for param in action_def.parameters:
                type_str = param.type
                if param.enum_values:
                    type_str = f"{param.type} ({', '.join(param.enum_values)})"
                req = "(required)" if param.required else "(optional)"
                lines.append(f"- `{param.name}`: {type_str} {req} - {param.description}")

        if include_risk_warning and action_def.requires_approval:
            lines.append("\n⚠️ **Requires human approval before execution**")

        risk_level = getattr(action_def, "risk_level", "NORMAL")
        if risk_level in ("HIGH", "CRITICAL"):
            lines.append(f"\n🔴 **High Risk Action** - {risk_level} risk level")

        return "\n".join(lines)

    def build_few_shot_examples(self, object_type: str, action: str, count: int = 2) -> str:
        """
        生成 Few-shot 示例

        Args:
            object_type: 对象类型
            action: Action 名称
            count: 示例数量

        Returns:
            Few-shot 示例文本
        """
        examples = []

        if object_type == "Flight" and action == "change_stand":
            examples.append("""
Example 1:
User: 将 CA1234 航班的机位从 A1 改到 A5
Action: Flight.change_stand
Parameters: {"flight_id": "CA1234_20240101", "new_stand": "A5", "reason": "A1需要维护"}

Example 2:
User: CA5678 需要换到更大的机位
Action: Flight.change_stand
Parameters: {"flight_id": "CA5678_20240101", "new_stand": "B10", "reason": "原机位过小"}
""")

        return "\n".join(examples)

    def clear_cache(self) -> None:
        """清空注入缓存"""
        self._injection_cache.clear()

    def get_injected_contexts(self, max_priority: int | None = None) -> list[ContextInjection]:
        """获取已缓存的上下文注入项"""
        contexts = list(self._injection_cache.values())

        if max_priority is not None:
            contexts = [c for c in contexts if c.priority <= max_priority]

        contexts.sort(key=lambda x: x.priority, reverse=True)
        return contexts


__all__ = [
    "ContextInjection",
    "OntologyContextBridge",
]
