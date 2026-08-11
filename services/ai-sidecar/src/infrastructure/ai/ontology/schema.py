"""
Ontology 核心 Schema 和注册表
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from .objects.base import ObjectType


@dataclass
class PropertyDefinition:
    """属性定义"""

    name: str
    type: str
    required: bool = False
    description: str = ""
    enum_values: list[str] | None = None
    reference_object: str | None = None
    default: Any | None = None

    def to_schema_dict(self) -> dict[str, Any]:
        schema: dict[str, Any] = {
            "name": self.name,
            "type": self.type,
            "required": self.required,
            "description": self.description,
        }
        if self.enum_values:
            schema["enum"] = self.enum_values
        if self.reference_object:
            schema["reference"] = self.reference_object
        if self.default is not None:
            schema["default"] = self.default
        return schema


@dataclass
class RelationshipDefinition:
    """关系定义"""

    name: str
    target_object: str
    cardinality: str = "one"
    description: str = ""
    inverse: str | None = None

    def to_schema_dict(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "target": self.target_object,
            "cardinality": self.cardinality,
            "description": self.description,
            "inverse": self.inverse,
        }


@dataclass
class ActionDefinition:
    """动作定义"""

    name: str
    object_type: str
    description: str = ""
    parameters: list[PropertyDefinition] = field(default_factory=list)
    requires_approval: bool = False
    risk_level: str = "NORMAL"
    category: str = "mutation"

    def to_schema_dict(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "object_type": self.object_type,
            "description": self.description,
            "parameters": [p.to_schema_dict() for p in self.parameters],
            "requires_approval": self.requires_approval,
            "risk_level": self.risk_level,
            "category": self.category,
        }


@dataclass
class OntologySchema:
    """Ontology Schema，包含所有对象类型定义"""

    name: str
    version: str = "1.0.0"
    description: str = ""
    objects: dict[str, ObjectType] = field(default_factory=dict)
    actions: dict[str, ActionDefinition] = field(default_factory=dict)

    def get_object(self, name: str) -> ObjectType | None:
        return self.objects.get(name)

    def get_action(self, name: str) -> ActionDefinition | None:
        return self.actions.get(name)

    def get_object_actions(self, object_type: str) -> list[ActionDefinition]:
        return [action for action in self.actions.values() if action.object_type == object_type]

    def to_schema_dict(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "version": self.version,
            "description": self.description,
            "objects": {name: obj.to_schema_dict() for name, obj in self.objects.items()},
            "actions": {name: action.to_schema_dict() for name, action in self.actions.items()},
        }


class OntologyRegistry:
    """Ontology 注册表，全局单例"""

    def __init__(self):
        self._schemas: dict[str, OntologySchema] = {}
        self._initialized = False

    def register_schema(self, schema: OntologySchema) -> None:
        """注册一个完整的 Ontology Schema"""
        self._schemas[schema.name] = schema

    def register_object(self, obj: ObjectType) -> None:
        """注册单个对象类型到默认 Schema"""
        default_schema = self._schemas.get("default")
        if default_schema:
            default_schema.objects[obj.name] = obj

    def register_action(self, action: ActionDefinition) -> None:
        """注册单个动作到默认 Schema"""
        default_schema = self._schemas.get("default")
        if default_schema:
            default_schema.actions[action.name] = action

    def get_schema(self, name: str = "default") -> OntologySchema | None:
        return self._schemas.get(name)

    def get_object(self, object_type: str, schema_name: str = "default") -> ObjectType | None:
        schema = self.get_schema(schema_name)
        return schema.get_object(object_type) if schema else None

    def get_action(self, action_name: str, schema_name: str = "default") -> ActionDefinition | None:
        schema = self.get_schema(schema_name)
        return schema.get_action(action_name) if schema else None

    def get_object_actions(self, object_type: str, schema_name: str = "default") -> list[ActionDefinition]:
        schema = self.get_schema(schema_name)
        return schema.get_object_actions(object_type) if schema else []

    def initialize_default_schema(self) -> OntologySchema:
        """初始化默认 Schema"""
        from .objects import Anomaly, Equipment, Flight, Stand, Team, Todo

        default_schema = OntologySchema(
            name="default",
            version="1.0.0",
            description="Flight Monitor System 默认 Ontology",
            objects={
                "Flight": Flight.OBJECT,
                "Stand": Stand.OBJECT,
                "Team": Team.OBJECT,
                "Equipment": Equipment.OBJECT,
                "Anomaly": Anomaly.OBJECT,
                "Todo": Todo.OBJECT,
            },
            actions={},
        )

        for action_def in [Flight, Stand, Team, Equipment, Anomaly, Todo]:
            if hasattr(action_def, "ACTIONS"):
                for action in action_def.ACTIONS:
                    default_schema.actions[action.name] = action

        self.register_schema(default_schema)
        self._initialized = True
        return default_schema

    @property
    def is_initialized(self) -> bool:
        return self._initialized


_ontology_registry: OntologyRegistry | None = None


def get_ontology_registry() -> OntologyRegistry:
    """获取全局 Ontology 注册表"""
    global _ontology_registry
    if _ontology_registry is None:
        _ontology_registry = OntologyRegistry()
        _ontology_registry.initialize_default_schema()
    return _ontology_registry


def set_ontology_registry(registry: OntologyRegistry) -> None:
    """设置全局 Ontology 注册表"""
    global _ontology_registry
    _ontology_registry = registry
