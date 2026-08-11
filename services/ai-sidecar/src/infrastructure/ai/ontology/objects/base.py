"""
Ontology 对象基类定义
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from ..schema import ActionDefinition, PropertyDefinition, RelationshipDefinition


@dataclass
class ObjectType:
    """对象类型定义"""

    name: str
    plural_name: str = ""
    description: str = ""
    properties: list[PropertyDefinition] = field(default_factory=list)
    relationships: list[RelationshipDefinition] = field(default_factory=list)
    actions: list[str] = field(default_factory=list)
    tags: list[str] = field(default_factory=list)
    is_abstract: bool = False

    def __post_init__(self):
        if not self.plural_name:
            self.plural_name = f"{self.name}s"

    def get_property(self, name: str) -> PropertyDefinition | None:
        return next((p for p in self.properties if p.name == name), None)

    def get_relationship(self, name: str) -> RelationshipDefinition | None:
        return next((r for r in self.relationships if r.name == name), None)

    def get_required_properties(self) -> list[PropertyDefinition]:
        return [p for p in self.properties if p.required]

    def to_schema_dict(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "plural_name": self.plural_name,
            "description": self.description,
            "properties": [p.to_schema_dict() for p in self.properties],
            "relationships": [r.to_schema_dict() for r in self.relationships],
            "actions": self.actions,
            "tags": self.tags,
            "is_abstract": self.is_abstract,
        }


def create_property(
    name: str, type_str: str, required: bool = False, description: str = "", **kwargs
) -> PropertyDefinition:
    """属性定义工厂函数"""
    return PropertyDefinition(name=name, type=type_str, required=required, description=description, **kwargs)


def create_relationship(
    name: str, target: str, cardinality: str = "one", description: str = "", inverse: str | None = None
) -> RelationshipDefinition:
    """关系定义工厂函数"""
    return RelationshipDefinition(
        name=name, target_object=target, cardinality=cardinality, description=description, inverse=inverse
    )


def create_action(
    name: str,
    object_type: str,
    description: str = "",
    parameters: list[PropertyDefinition] | None = None,
    requires_approval: bool = False,
    risk_level: str = "NORMAL",
    category: str = "mutation",
) -> ActionDefinition:
    """动作定义工厂函数"""
    return ActionDefinition(
        name=name,
        object_type=object_type,
        description=description,
        parameters=parameters or [],
        requires_approval=requires_approval,
        risk_level=risk_level,
        category=category,
    )
