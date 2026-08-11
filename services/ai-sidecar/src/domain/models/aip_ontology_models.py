"""
AIP Ontology 自定义配置领域模型

定义Ontology对象、动作、策略、函数、约束等可自定义配置的领域模型。
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from datetime import datetime
from enum import StrEnum
from typing import Any


class RiskLevel(StrEnum):
    LOW = "LOW"
    NORMAL = "NORMAL"
    MEDIUM = "MEDIUM"
    HIGH = "HIGH"
    CRITICAL = "CRITICAL"


class PrincipalType(StrEnum):
    USER = "user"
    ROLE = "role"
    GROUP = "group"


class Permission(StrEnum):
    READ = "read"
    WRITE = "write"
    DELETE = "delete"
    EXECUTE = "execute"
    ADMIN = "admin"


class MigrationStatus(StrEnum):
    NOT_STARTED = "not_started"
    IN_PROGRESS = "in_progress"
    COMPLETED = "completed"


class ConstraintType(StrEnum):
    VALIDATION = "validation"
    BUSINESS_RULE = "business_rule"
    CAPACITY = "capacity"
    AVAILABILITY = "availability"


class ActionCategory(StrEnum):
    QUERY = "query"
    MUTATION = "mutation"
    ADMIN = "admin"


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

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> PropertyDefinition:
        return cls(
            name=data.get("name", ""),
            type=data.get("type", "string"),
            required=data.get("required", False),
            description=data.get("description", ""),
            enum_values=data.get("enum_values"),
            reference_object=data.get("reference_object"),
            default=data.get("default"),
        )

    def to_dict(self) -> dict[str, Any]:
        result: dict[str, Any] = {
            "name": self.name,
            "type": self.type,
            "required": self.required,
            "description": self.description,
        }
        if self.enum_values:
            result["enum_values"] = self.enum_values
        if self.reference_object:
            result["reference_object"] = self.reference_object
        if self.default is not None:
            result["default"] = self.default
        return result


@dataclass
class RelationshipDefinition:
    """关系定义"""

    name: str
    target_object: str
    cardinality: str = "one"
    description: str = ""
    inverse: str | None = None

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> RelationshipDefinition:
        return cls(
            name=data.get("name", ""),
            target_object=data.get("target_object", data.get("target", "")),
            cardinality=data.get("cardinality", "one"),
            description=data.get("description", ""),
            inverse=data.get("inverse"),
        )

    def to_dict(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "target_object": self.target_object,
            "cardinality": self.cardinality,
            "description": self.description,
            "inverse": self.inverse,
        }


@dataclass
class OntologyObjectDefinition:
    """Ontology对象定义"""

    id: str
    name: str
    plural_name: str = ""
    description: str = ""
    is_abstract: bool = False
    properties: list[PropertyDefinition] = field(default_factory=list)
    relationships: list[RelationshipDefinition] = field(default_factory=list)
    actions: list[str] = field(default_factory=list)
    tags: list[str] = field(default_factory=list)
    metadata: dict[str, Any] = field(default_factory=dict)
    is_active: bool = True
    created_at: datetime | None = None
    updated_at: datetime | None = None

    @classmethod
    def from_db_row(cls, row: dict[str, Any]) -> OntologyObjectDefinition:
        props_raw = row.get("properties", [])
        if isinstance(props_raw, str):
            props_raw = json.loads(props_raw)
        props = [PropertyDefinition.from_dict(p) for p in props_raw] if props_raw else []

        rels_raw = row.get("relationships", [])
        if isinstance(rels_raw, str):
            rels_raw = json.loads(rels_raw)
        rels = [RelationshipDefinition.from_dict(r) for r in rels_raw] if rels_raw else []

        actions_raw = row.get("actions", [])
        if isinstance(actions_raw, str):
            actions_raw = json.loads(actions_raw)

        tags_raw = row.get("tags", [])
        if isinstance(tags_raw, str):
            tags_raw = json.loads(tags_raw)

        metadata_raw = row.get("metadata", {})
        if isinstance(metadata_raw, str):
            metadata_raw = json.loads(metadata_raw)

        return cls(
            id=row["id"],
            name=row["name"],
            plural_name=row.get("plural_name", ""),
            description=row.get("description", ""),
            is_abstract=row.get("is_abstract", False),
            properties=props,
            relationships=rels,
            actions=actions_raw if isinstance(actions_raw, list) else [],
            tags=tags_raw if isinstance(tags_raw, list) else [],
            metadata=metadata_raw,
            is_active=row.get("is_active", True),
            created_at=row.get("created_at"),
            updated_at=row.get("updated_at"),
        )

    def to_db_dict(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "name": self.name,
            "plural_name": self.plural_name,
            "description": self.description,
            "is_abstract": self.is_abstract,
            "properties": json.dumps([p.to_dict() for p in self.properties]),
            "relationships": json.dumps([r.to_dict() for r in self.relationships]),
            "actions": json.dumps(self.actions),
            "tags": json.dumps(self.tags),
            "metadata": json.dumps(self.metadata),
            "is_active": self.is_active,
        }


@dataclass
class OntologyActionDefinition:
    """Ontology动作定义"""

    id: str
    name: str
    object_type: str
    description: str = ""
    category: ActionCategory = ActionCategory.MUTATION
    parameters: list[PropertyDefinition] = field(default_factory=list)
    requires_approval: bool = False
    risk_level: RiskLevel = RiskLevel.NORMAL
    constraint_rules: list[dict[str, Any]] = field(default_factory=list)
    metadata: dict[str, Any] = field(default_factory=dict)
    is_active: bool = True
    created_at: datetime | None = None
    updated_at: datetime | None = None

    @classmethod
    def from_db_row(cls, row: dict[str, Any]) -> OntologyActionDefinition:
        params_raw = row.get("parameters", [])
        if isinstance(params_raw, str):
            params_raw = json.loads(params_raw)
        params = [PropertyDefinition.from_dict(p) for p in params_raw] if params_raw else []

        constraints_raw = row.get("constraint_rules", [])
        if isinstance(constraints_raw, str):
            constraints_raw = json.loads(constraints_raw)

        metadata_raw = row.get("metadata", {})
        if isinstance(metadata_raw, str):
            metadata_raw = json.loads(metadata_raw)

        return cls(
            id=row["id"],
            name=row["name"],
            object_type=row["object_type"],
            description=row.get("description", ""),
            category=ActionCategory(row.get("category", "mutation")),
            parameters=params,
            requires_approval=row.get("requires_approval", False),
            risk_level=RiskLevel(row.get("risk_level", "NORMAL")),
            constraint_rules=constraints_raw,
            metadata=metadata_raw,
            is_active=row.get("is_active", True),
            created_at=row.get("created_at"),
            updated_at=row.get("updated_at"),
        )

    def to_db_dict(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "name": self.name,
            "object_type": self.object_type,
            "description": self.description,
            "category": self.category.value,
            "parameters": json.dumps([p.to_dict() for p in self.parameters]),
            "requires_approval": self.requires_approval,
            "risk_level": self.risk_level.value,
            "constraint_rules": json.dumps(self.constraint_rules),
            "metadata": json.dumps(self.metadata),
            "is_active": self.is_active,
        }


@dataclass
class ObjectPolicy:
    """对象级安全策略"""

    id: str
    object_type: str
    principal_type: PrincipalType
    principal_id: str
    permission: Permission
    object_id: str | None = None
    conditions: dict[str, Any] | None = None
    granted: bool = True
    description: str = ""
    expires_at: datetime | None = None
    created_at: datetime | None = None
    updated_at: datetime | None = None

    @classmethod
    def from_db_row(cls, row: dict[str, Any]) -> ObjectPolicy:
        conditions_raw = row.get("conditions")
        if isinstance(conditions_raw, str):
            conditions_raw = json.loads(conditions_raw)

        return cls(
            id=row["id"],
            object_type=row["object_type"],
            object_id=row.get("object_id"),
            principal_type=PrincipalType(row.get("principal_type", "user")),
            principal_id=row["principal_id"],
            permission=Permission(row["permission"]),
            conditions=conditions_raw,
            granted=row.get("granted", True),
            description=row.get("description", ""),
            expires_at=row.get("expires_at"),
            created_at=row.get("created_at"),
            updated_at=row.get("updated_at"),
        )

    def to_db_dict(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "object_type": self.object_type,
            "object_id": self.object_id,
            "principal_type": self.principal_type.value,
            "principal_id": self.principal_id,
            "permission": self.permission.value,
            "granted": self.granted,
            "conditions": json.dumps(self.conditions) if self.conditions else None,
            "description": self.description,
            "expires_at": self.expires_at,
        }


@dataclass
class AIPFunction:
    """AIP函数定义"""

    id: str
    name: str
    category: str = "object_action"
    object_type: str = ""
    action_name: str = ""
    description: str = ""
    parameters_schema: dict[str, Any] = field(default_factory=dict)
    requires_approval: bool = False
    risk_level: RiskLevel = RiskLevel.NORMAL
    permission_required: str | None = None
    tags: list[str] = field(default_factory=list)
    examples: list[dict[str, Any]] = field(default_factory=list)
    metadata: dict[str, Any] = field(default_factory=dict)
    is_active: bool = True
    created_at: datetime | None = None
    updated_at: datetime | None = None

    @classmethod
    def from_db_row(cls, row: dict[str, Any]) -> AIPFunction:
        params_raw = row.get("parameters_schema", {})
        if isinstance(params_raw, str):
            params_raw = json.loads(params_raw)

        tags_raw = row.get("tags", [])
        if isinstance(tags_raw, str):
            tags_raw = json.loads(tags_raw)

        examples_raw = row.get("examples", [])
        if isinstance(examples_raw, str):
            examples_raw = json.loads(examples_raw)

        metadata_raw = row.get("metadata", {})
        if isinstance(metadata_raw, str):
            metadata_raw = json.loads(metadata_raw)

        return cls(
            id=row["id"],
            name=row["name"],
            category=row.get("category", "object_action"),
            object_type=row.get("object_type", ""),
            action_name=row.get("action_name", ""),
            description=row.get("description", ""),
            parameters_schema=params_raw,
            requires_approval=row.get("requires_approval", False),
            risk_level=RiskLevel(row.get("risk_level", "NORMAL")),
            permission_required=row.get("permission_required"),
            tags=tags_raw if isinstance(tags_raw, list) else [],
            examples=examples_raw if isinstance(examples_raw, list) else [],
            metadata=metadata_raw,
            is_active=row.get("is_active", True),
            created_at=row.get("created_at"),
            updated_at=row.get("updated_at"),
        )

    def to_db_dict(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "name": self.name,
            "category": self.category,
            "object_type": self.object_type,
            "action_name": self.action_name,
            "description": self.description,
            "parameters_schema": json.dumps(self.parameters_schema),
            "requires_approval": self.requires_approval,
            "risk_level": self.risk_level.value,
            "permission_required": self.permission_required,
            "tags": json.dumps(self.tags),
            "examples": json.dumps(self.examples),
            "metadata": json.dumps(self.metadata),
            "is_active": self.is_active,
        }


@dataclass
class ToolMapping:
    """工具映射配置"""

    id: str
    tool_name: str
    object_type: str
    action_name: str
    requires_approval: bool = False
    risk_level: RiskLevel = RiskLevel.NORMAL
    migration_status: MigrationStatus = MigrationStatus.NOT_STARTED
    custom_handler: str | None = None
    metadata: dict[str, Any] = field(default_factory=dict)
    is_active: bool = True
    created_at: datetime | None = None
    updated_at: datetime | None = None

    @classmethod
    def from_db_row(cls, row: dict[str, Any]) -> ToolMapping:
        metadata_raw = row.get("metadata", {})
        if isinstance(metadata_raw, str):
            metadata_raw = json.loads(metadata_raw)

        return cls(
            id=row["id"],
            tool_name=row["tool_name"],
            object_type=row["object_type"],
            action_name=row["action_name"],
            requires_approval=row.get("requires_approval", False),
            risk_level=RiskLevel(row.get("risk_level", "NORMAL")),
            migration_status=MigrationStatus(row.get("migration_status", "not_started")),
            custom_handler=row.get("custom_handler"),
            metadata=metadata_raw,
            is_active=row.get("is_active", True),
            created_at=row.get("created_at"),
            updated_at=row.get("updated_at"),
        )

    def to_db_dict(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "tool_name": self.tool_name,
            "object_type": self.object_type,
            "action_name": self.action_name,
            "requires_approval": self.requires_approval,
            "risk_level": self.risk_level.value,
            "migration_status": self.migration_status.value,
            "custom_handler": self.custom_handler,
            "metadata": json.dumps(self.metadata),
            "is_active": self.is_active,
        }


@dataclass
class ConstraintDefinition:
    """业务约束定义"""

    id: str
    name: str
    object_type: str
    constraint_type: ConstraintType
    expression: str
    action_name: str | None = None
    error_message: str | None = None
    severity: RiskLevel = RiskLevel.NORMAL
    metadata: dict[str, Any] = field(default_factory=dict)
    is_active: bool = True
    created_at: datetime | None = None
    updated_at: datetime | None = None

    @classmethod
    def from_db_row(cls, row: dict[str, Any]) -> ConstraintDefinition:
        metadata_raw = row.get("metadata", {})
        if isinstance(metadata_raw, str):
            metadata_raw = json.loads(metadata_raw)

        return cls(
            id=row["id"],
            name=row["name"],
            object_type=row["object_type"],
            action_name=row.get("action_name"),
            constraint_type=ConstraintType(row.get("constraint_type", "validation")),
            expression=row["expression"],
            error_message=row.get("error_message"),
            severity=RiskLevel(row.get("severity", "NORMAL")),
            metadata=metadata_raw,
            is_active=row.get("is_active", True),
            created_at=row.get("created_at"),
            updated_at=row.get("updated_at"),
        )

    def to_db_dict(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "name": self.name,
            "object_type": self.object_type,
            "action_name": self.action_name,
            "constraint_type": self.constraint_type.value,
            "expression": self.expression,
            "error_message": self.error_message,
            "severity": self.severity.value,
            "metadata": json.dumps(self.metadata),
            "is_active": self.is_active,
        }
