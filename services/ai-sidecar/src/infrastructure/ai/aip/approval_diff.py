"""
Ontology-Aware Diff 计算引擎

提供基于 Schema 的变更差异计算，用于审批流程。
增强的 Diff 包含：
- Schema 感知的属性变更
- 关键属性标注
- 关系变更追踪
- 约束影响分析

使用方式:
    from src.infrastructure.ai.aip.approval_diff import OntologyAwareDiff

    diff_engine = OntologyAwareDiff(ontology_registry)
    diff = diff_engine.compute_diff("Flight", before_state, after_state)
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime
from enum import StrEnum
from typing import Any

from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class ChangeType(StrEnum):
    """变更类型"""

    ADDED = "added"
    REMOVED = "removed"
    MODIFIED = "modified"
    UNCHANGED = "unchanged"


class RiskLevel(StrEnum):
    """风险等级"""

    LOW = "LOW"
    NORMAL = "NORMAL"
    MEDIUM = "MEDIUM"
    HIGH = "HIGH"
    CRITICAL = "CRITICAL"


@dataclass
class PropertyChange:
    """属性变更"""

    property_name: str
    property_type: str
    change_type: ChangeType
    before_value: Any = None
    after_value: Any = None
    is_critical: bool = False
    is_safe: bool = True
    schema_path: str = ""
    description: str = ""

    def to_dict(self) -> dict[str, Any]:
        return {
            "property": self.property_name,
            "type": self.property_type,
            "change_type": self.change_type.value,
            "before": self.before_value,
            "after": self.after_value,
            "critical": self.is_critical,
            "safe": self.is_safe,
            "schema_path": self.schema_path,
            "description": self.description,
        }


@dataclass
class RelationshipChange:
    """关系变更"""

    relationship_name: str
    target_object: str
    change_type: ChangeType
    before_id: str | None = None
    after_id: str | None = None
    cardinality: str = "one"
    description: str = ""

    def to_dict(self) -> dict[str, Any]:
        return {
            "relationship": self.relationship_name,
            "target": self.target_object,
            "change_type": self.change_type.value,
            "before": self.before_id,
            "after": self.after_id,
            "cardinality": self.cardinality,
            "description": self.description,
        }


@dataclass
class RiskAssessment:
    """风险评估"""

    level: RiskLevel
    reasons: list[str] = field(default_factory=list)
    affected_constraints: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)
    score: float = 0.0

    def to_dict(self) -> dict[str, Any]:
        return {
            "level": self.level.value,
            "reasons": self.reasons,
            "affected_constraints": self.affected_constraints,
            "warnings": self.warnings,
            "score": self.score,
        }


@dataclass
class OntologyDiff:
    """Ontology 感知的变更差异"""

    object_type: str
    object_id: str
    action: str
    timestamp: datetime = field(default_factory=datetime.now)
    property_changes: list[PropertyChange] = field(default_factory=list)
    relationship_changes: list[RelationshipChange] = field(default_factory=list)
    risk_assessment: RiskAssessment | None = None
    before_state: dict[str, Any] = field(default_factory=dict)
    after_state: dict[str, Any] = field(default_factory=dict)
    metadata: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        return {
            "object_type": self.object_type,
            "object_id": self.object_id,
            "action": self.action,
            "timestamp": self.timestamp.isoformat(),
            "property_changes": [p.to_dict() for p in self.property_changes],
            "relationship_changes": [r.to_dict() for r in self.relationship_changes],
            "risk_assessment": self.risk_assessment.to_dict() if self.risk_assessment else None,
            "before_state": self.before_state,
            "after_state": self.after_state,
            "metadata": self.metadata,
        }

    def get_summary(self) -> str:
        """生成变更摘要"""
        parts = [f"{self.object_type} '{self.object_id}': {self.action}"]

        if self.property_changes:
            critical = [p for p in self.property_changes if p.is_critical]
            if critical:
                parts.append(f"\n关键属性变更 ({len(critical)}):")
                for c in critical[:3]:
                    parts.append(f"  • {c.property_name}: {c.before_value} → {c.after_value}")

        if self.risk_assessment:
            parts.append(f"\n风险等级: {self.risk_assessment.level.value}")
            if self.risk_assessment.warnings:
                parts.append("警告:")
                for w in self.risk_assessment.warnings[:2]:
                    parts.append(f"  ⚠️ {w}")

        return "\n".join(parts)


CRITICAL_PROPERTIES: set[str] = {
    "status",
    "assigned_team",
    "assigned_team_id",
    "stand",
    "gate",
    "permission",
    "cost",
    "safety",
}

SAFE_PROPERTIES: set[str] = {
    "location",
    "notes",
    "description",
    "delay_minutes",
    "actual_departure",
    "actual_arrival",
}

RISK_INCREASE_PROPERTIES: dict[str, float] = {
    "status": 0.3,
    "stand": 0.2,
    "gate": 0.15,
    "assigned_team": 0.25,
}


class OntologyAwareDiff:
    """
    Ontology-Aware Diff 计算引擎

    提供基于 Schema 的变更差异计算：
    - 自动识别关键属性
    - 计算风险等级
    - 生成人类可读的变更摘要
    """

    def __init__(self, ontology_registry=None):
        self._ontology = ontology_registry
        self._schema_cache: dict[str, Any] = {}

    def set_ontology_registry(self, registry) -> None:
        """设置 Ontology 注册表"""
        self._ontology = registry

    def compute_diff(
        self,
        object_type: str,
        before_state: dict[str, Any],
        after_state: dict[str, Any],
        action: str = "",
        object_id: str = "",
    ) -> OntologyDiff:
        """
        计算变更差异

        Args:
            object_type: 对象类型
            before_state: 变更前状态
            after_state: 变更后状态
            action: Action 名称
            object_id: 对象 ID

        Returns:
            OntologyDiff
        """
        schema = self._get_schema(object_type)

        property_changes = self._compute_property_changes(object_type, schema, before_state, after_state)

        relationship_changes = self._compute_relationship_changes(object_type, schema, before_state, after_state)

        risk_assessment = self._assess_risk(object_type, action, property_changes, relationship_changes)

        return OntologyDiff(
            object_type=object_type,
            object_id=object_id,
            action=action,
            property_changes=property_changes,
            relationship_changes=relationship_changes,
            risk_assessment=risk_assessment,
            before_state=before_state,
            after_state=after_state,
            metadata={
                "schema_version": schema.get("version", "1.0") if schema else "unknown",
                "change_count": len(property_changes) + len(relationship_changes),
            },
        )

    def _get_schema(self, object_type: str) -> dict[str, Any]:
        """获取对象 Schema"""
        if object_type in self._schema_cache:
            return self._schema_cache[object_type]

        if not self._ontology:
            return {}

        schema = self._ontology.get_object(object_type, "default")
        if schema:
            schema_dict = schema.to_schema_dict()
            self._schema_cache[object_type] = schema_dict
            return schema_dict

        return {}

    def _compute_property_changes(
        self, object_type: str, schema: dict[str, Any], before: dict[str, Any], after: dict[str, Any]
    ) -> list[PropertyChange]:
        """计算属性变更"""
        changes = []
        schema_props = {p["name"]: p for p in schema.get("properties", [])}

        all_keys = set(before.keys()) | set(after.keys())

        for key in all_keys:
            before_val = before.get(key)
            after_val = after.get(key)

            if before_val is None and after_val is not None:
                change_type = ChangeType.ADDED
            elif before_val is not None and after_val is None:
                change_type = ChangeType.REMOVED
            elif before_val != after_val:
                change_type = ChangeType.MODIFIED
            else:
                continue

            schema_prop = schema_props.get(key, {})
            prop_type = schema_prop.get("type", "string")
            is_critical = key in CRITICAL_PROPERTIES
            is_safe = key in SAFE_PROPERTIES

            change = PropertyChange(
                property_name=key,
                property_type=prop_type,
                change_type=change_type,
                before_value=before_val,
                after_value=after_val,
                is_critical=is_critical,
                is_safe=is_safe,
                schema_path=f"{object_type}.{key}",
                description=schema_prop.get("description", ""),
            )

            changes.append(change)

        return changes

    def _compute_relationship_changes(
        self, object_type: str, schema: dict[str, Any], before: dict[str, Any], after: dict[str, Any]
    ) -> list[RelationshipChange]:
        """计算关系变更"""
        changes = []
        schema_rels = {r["name"]: r for r in schema.get("relationships", [])}

        for rel_name, rel_schema in schema_rels.items():
            before_val = before.get(rel_name)
            after_val = after.get(rel_name)

            if before_val is None and after_val is not None:
                change_type = ChangeType.ADDED
            elif before_val is not None and after_val is None:
                change_type = ChangeType.REMOVED
            elif before_val != after_val:
                change_type = ChangeType.MODIFIED
            else:
                continue

            change = RelationshipChange(
                relationship_name=rel_name,
                target_object=rel_schema.get("target", ""),
                change_type=change_type,
                before_id=before_val.get("id") if isinstance(before_val, dict) else before_val,
                after_id=after_val.get("id") if isinstance(after_val, dict) else after_val,
                cardinality=rel_schema.get("cardinality", "one"),
                description=rel_schema.get("description", ""),
            )

            changes.append(change)

        return changes

    def _assess_risk(
        self,
        object_type: str,
        action: str,
        property_changes: list[PropertyChange],
        relationship_changes: list[RelationshipChange],
    ) -> RiskAssessment:
        """评估风险"""
        score = 0.0
        reasons = []
        warnings = []

        for change in property_changes:
            if change.is_critical:
                score += RISK_INCREASE_PROPERTIES.get(change.property_name, 0.2)
                reasons.append(f"关键属性变更: {change.property_name}")

            if change.property_name in RISK_INCREASE_PROPERTIES:
                score += RISK_INCREASE_PROPERTIES[change.property_name]

        for change in relationship_changes:
            score += 0.15
            reasons.append(f"关系变更: {change.relationship_name}")

        if len(property_changes) > 5:
            score += 0.1
            warnings.append(f"变更属性较多: {len(property_changes)} 个")

        if any(c.change_type == ChangeType.REMOVED for c in property_changes):
            score += 0.1
            warnings.append("涉及属性删除")

        score = min(score, 1.0)

        if score >= 0.7:
            level = RiskLevel.CRITICAL
        elif score >= 0.5:
            level = RiskLevel.HIGH
        elif score >= 0.3:
            level = RiskLevel.MEDIUM
        elif score >= 0.1:
            level = RiskLevel.NORMAL
        else:
            level = RiskLevel.LOW

        return RiskAssessment(level=level, reasons=reasons[:5], warnings=warnings[:3], score=score)

    def compute_json_patch(self, before_state: dict[str, Any], after_state: dict[str, Any]) -> list[dict[str, Any]]:
        """计算 RFC 6902 JSON Patch"""
        patch = []
        all_keys = set(before_state.keys()) | set(after_state.keys())

        for key in all_keys:
            before_val = before_state.get(key)
            after_val = after_state.get(key)

            if before_val is None and after_val is not None:
                patch.append({"op": "add", "path": f"/{key}", "value": after_val})
            elif before_val is not None and after_val is None:
                patch.append({"op": "remove", "path": f"/{key}"})
            elif before_val != after_val:
                patch.append({"op": "replace", "path": f"/{key}", "value": after_val})

        return patch

    def generate_approval_context(self, diff: OntologyDiff) -> dict[str, Any]:
        """
        生成审批上下文

        用于审批 UI 的增强展示。

        Args:
            diff: OntologyDiff 实例

        Returns:
            审批上下文字典
        """
        context = {
            "object_type": diff.object_type,
            "object_id": diff.object_id,
            "action": diff.action,
            "timestamp": diff.timestamp.isoformat(),
            "risk_level": diff.risk_assessment.level.value if diff.risk_assessment else "UNKNOWN",
            "risk_score": diff.risk_assessment.score if diff.risk_assessment else 0,
        }

        if diff.property_changes:
            critical_changes = [c for c in diff.property_changes if c.is_critical]
            context["critical_changes"] = [c.to_dict() for c in critical_changes]
            context["property_change_count"] = len(diff.property_changes)

        if diff.relationship_changes:
            context["relationship_changes"] = [r.to_dict() for r in diff.relationship_changes]

        if diff.risk_assessment:
            context["risk_warnings"] = diff.risk_assessment.warnings
            context["risk_reasons"] = diff.risk_assessment.reasons

        context["summary"] = diff.get_summary()
        context["json_patch"] = self.compute_json_patch(diff.before_state, diff.after_state)

        return context


_diff_engine: OntologyAwareDiff | None = None


def get_diff_engine() -> OntologyAwareDiff:
    """获取全局 Diff 引擎"""
    global _diff_engine
    if _diff_engine is None:
        _diff_engine = OntologyAwareDiff()
    return _diff_engine


__all__ = [
    "ChangeType",
    "OntologyAwareDiff",
    "OntologyDiff",
    "PropertyChange",
    "RelationshipChange",
    "RiskAssessment",
    "RiskLevel",
    "get_diff_engine",
]
