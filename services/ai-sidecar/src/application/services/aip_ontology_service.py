"""
AIP Ontology 自定义配置服务

提供业务逻辑层，封装 Repository 操作。
"""

from typing import Any

from src.domain.models.aip_ontology_models import (
    AIPFunction,
    ConstraintDefinition,
    ObjectPolicy,
    OntologyActionDefinition,
    OntologyObjectDefinition,
    PropertyDefinition,
    RelationshipDefinition,
    ToolMapping,
)
from src.infrastructure.logging.core import get_logger
from src.infrastructure.repositories.aip_ontology_repository import (
    AIPActionRepository,
    AIPConstraintRepository,
    AIPFunctionRepository,
    AIPOntologyRepository,
    AIPPolicyRepository,
    AIPToolMappingRepository,
)

logger = get_logger(__name__)


class AIPOntologyService:
    """AIP Ontology 服务"""

    def __init__(self, db_pool: Any):
        self._ontology_repo = AIPOntologyRepository(db_pool)
        self._action_repo = AIPActionRepository(db_pool)
        self._policy_repo = AIPPolicyRepository(db_pool)
        self._function_repo = AIPFunctionRepository(db_pool)
        self._mapping_repo = AIPToolMappingRepository(db_pool)
        self._constraint_repo = AIPConstraintRepository(db_pool)

    async def get_all_objects(
        self,
        include_inactive: bool = False,
        limit: int = 100,
        offset: int = 0,
    ) -> list[OntologyObjectDefinition]:
        """获取所有对象定义"""
        return await self._ontology_repo.get_all_objects(include_inactive, limit, offset)

    async def get_object_by_id(self, id: str) -> OntologyObjectDefinition | None:
        """获取对象定义"""
        return await self._ontology_repo.get_object_by_id(id)

    async def get_object_by_name(self, name: str) -> OntologyObjectDefinition | None:
        """获取对象定义"""
        return await self._ontology_repo.get_object_by_name(name)

    async def create_object(self, data: dict[str, Any]) -> OntologyObjectDefinition:
        """创建对象定义"""
        properties = [
            PropertyDefinition(
                name=p.get("name", ""),
                type=p.get("type", "string"),
                required=p.get("required", False),
                description=p.get("description", ""),
                enum_values=p.get("enum_values"),
                reference_object=p.get("reference_object"),
                default=p.get("default"),
            )
            for p in data.get("properties", [])
        ]

        relationships = [
            RelationshipDefinition(
                name=r.get("name", ""),
                target_object=r.get("target_object", ""),
                cardinality=r.get("cardinality", "one"),
                description=r.get("description", ""),
                inverse=r.get("inverse"),
            )
            for r in data.get("relationships", [])
        ]

        obj = OntologyObjectDefinition(
            id="",
            name=data.get("name", ""),
            plural_name=data.get("plural_name"),
            description=data.get("description", ""),
            is_abstract=data.get("is_abstract", False),
            properties=properties,
            relationships=relationships,
            actions=data.get("actions", []),
            tags=data.get("tags", []),
            metadata=data.get("metadata", {}),
            is_active=data.get("is_active", True),
        )

        return await self._ontology_repo.save_object(obj)

    async def update_object(self, id: str, data: dict[str, Any]) -> OntologyObjectDefinition | None:
        """更新对象定义"""
        obj = await self._ontology_repo.get_object_by_id(id)
        if not obj:
            return None

        if "plural_name" in data:
            obj.plural_name = data["plural_name"]
        if "description" in data:
            obj.description = data["description"]
        if "is_abstract" in data:
            obj.is_abstract = data["is_abstract"]
        if "properties" in data:
            obj.properties = [
                PropertyDefinition(
                    name=p.get("name", ""),
                    type=p.get("type", "string"),
                    required=p.get("required", False),
                    description=p.get("description", ""),
                    enum_values=p.get("enum_values"),
                    reference_object=p.get("reference_object"),
                    default=p.get("default"),
                )
                for p in data["properties"]
            ]
        if "relationships" in data:
            obj.relationships = [
                RelationshipDefinition(
                    name=r.get("name", ""),
                    target_object=r.get("target_object", ""),
                    cardinality=r.get("cardinality", "one"),
                    description=r.get("description", ""),
                    inverse=r.get("inverse"),
                )
                for r in data["relationships"]
            ]
        if "actions" in data:
            obj.actions = data["actions"]
        if "tags" in data:
            obj.tags = data["tags"]
        if "metadata" in data:
            obj.metadata = data["metadata"]
        if "is_active" in data:
            obj.is_active = data["is_active"]

        return await self._ontology_repo.save_object(obj)

    async def delete_object(self, id: str) -> bool:
        """删除对象定义"""
        return await self._ontology_repo.delete_object(id)

    async def get_all_actions(
        self,
        object_type: str | None = None,
        include_inactive: bool = False,
        limit: int = 100,
        offset: int = 0,
    ) -> list[OntologyActionDefinition]:
        """获取所有动作定义"""
        return await self._action_repo.get_all_actions(object_type, include_inactive, limit, offset)

    async def get_action_by_id(self, id: str) -> OntologyActionDefinition | None:
        """获取动作定义"""
        return await self._action_repo.get_action_by_id(id)

    async def create_action(self, data: dict[str, Any]) -> OntologyActionDefinition:
        """创建动作定义"""
        from src.domain.models.aip_ontology_models import ActionCategory, RiskLevel

        parameters = [
            PropertyDefinition(
                name=p.get("name", ""),
                type=p.get("type", "string"),
                required=p.get("required", False),
                description=p.get("description", ""),
                enum_values=p.get("enum_values"),
            )
            for p in data.get("parameters", [])
        ]

        action = OntologyActionDefinition(
            id="",
            name=data.get("name", ""),
            object_type=data.get("object_type", ""),
            description=data.get("description", ""),
            category=ActionCategory(data.get("category", "mutation")),
            parameters=parameters,
            requires_approval=data.get("requires_approval", False),
            risk_level=RiskLevel(data.get("risk_level", "NORMAL")),
            constraint_rules=data.get("constraint_rules", []),
            metadata=data.get("metadata", {}),
            is_active=data.get("is_active", True),
        )

        return await self._action_repo.save_action(action)

    async def update_action(self, id: str, data: dict[str, Any]) -> OntologyActionDefinition | None:
        """更新动作定义"""
        from src.domain.models.aip_ontology_models import ActionCategory, RiskLevel

        action = await self._action_repo.get_action_by_id(id)
        if not action:
            return None

        if "description" in data:
            action.description = data["description"]
        if "category" in data:
            action.category = ActionCategory(data["category"])
        if "parameters" in data:
            action.parameters = [
                PropertyDefinition(
                    name=p.get("name", ""),
                    type=p.get("type", "string"),
                    required=p.get("required", False),
                    description=p.get("description", ""),
                    enum_values=p.get("enum_values"),
                )
                for p in data["parameters"]
            ]
        if "requires_approval" in data:
            action.requires_approval = data["requires_approval"]
        if "risk_level" in data:
            action.risk_level = RiskLevel(data["risk_level"])
        if "constraint_rules" in data:
            action.constraint_rules = data["constraint_rules"]
        if "metadata" in data:
            action.metadata = data["metadata"]
        if "is_active" in data:
            action.is_active = data["is_active"]

        return await self._action_repo.save_action(action)

    async def delete_action(self, id: str) -> bool:
        """删除动作定义"""
        return await self._action_repo.delete_action(id)

    async def get_all_policies(
        self,
        principal_id: str | None = None,
        object_type: str | None = None,
        limit: int = 100,
        offset: int = 0,
    ) -> list[ObjectPolicy]:
        """获取所有策略"""
        return await self._policy_repo.get_all_policies(principal_id, object_type, limit, offset)

    async def get_policy_by_id(self, id: str) -> ObjectPolicy | None:
        """获取策略"""
        return await self._policy_repo.get_policy_by_id(id)

    async def create_policy(self, data: dict[str, Any]) -> ObjectPolicy:
        """创建策略"""
        from src.domain.models.aip_ontology_models import Permission, PrincipalType

        policy = ObjectPolicy(
            id="",
            object_type=data.get("object_type", ""),
            object_id=data.get("object_id"),
            principal_type=PrincipalType(data.get("principal_type", "user")),
            principal_id=data.get("principal_id", ""),
            permission=Permission(data.get("permission", "read")),
            granted=data.get("granted", True),
            conditions=data.get("conditions"),
            description=data.get("description", ""),
            expires_at=data.get("expires_at"),
        )

        return await self._policy_repo.save_policy(policy)

    async def update_policy(self, id: str, data: dict[str, Any]) -> ObjectPolicy | None:
        """更新策略"""
        from src.domain.models.aip_ontology_models import Permission, PrincipalType

        policy = await self._policy_repo.get_policy_by_id(id)
        if not policy:
            return None

        if "object_id" in data:
            policy.object_id = data["object_id"]
        if "principal_type" in data:
            policy.principal_type = PrincipalType(data["principal_type"])
        if "principal_id" in data:
            policy.principal_id = data["principal_id"]
        if "permission" in data:
            policy.permission = Permission(data["permission"])
        if "granted" in data:
            policy.granted = data["granted"]
        if "conditions" in data:
            policy.conditions = data["conditions"]
        if "description" in data:
            policy.description = data["description"]
        if "expires_at" in data:
            policy.expires_at = data["expires_at"]

        return await self._policy_repo.save_policy(policy)

    async def delete_policy(self, id: str) -> bool:
        """删除策略"""
        return await self._policy_repo.delete_policy(id)

    async def get_all_functions(
        self,
        object_type: str | None = None,
        category: str | None = None,
        include_inactive: bool = False,
        limit: int = 100,
        offset: int = 0,
    ) -> list[AIPFunction]:
        """获取所有函数"""
        return await self._function_repo.get_all_functions(object_type, category, include_inactive, limit, offset)

    async def get_function_by_id(self, id: str) -> AIPFunction | None:
        """获取函数"""
        return await self._function_repo.get_function_by_id(id)

    async def get_function_by_name(self, name: str) -> AIPFunction | None:
        """获取函数"""
        return await self._function_repo.get_function_by_name(name)

    async def create_function(self, data: dict[str, Any]) -> AIPFunction:
        """创建函数"""
        from src.domain.models.aip_ontology_models import RiskLevel

        func = AIPFunction(
            id="",
            name=data.get("name", ""),
            category=data.get("category", "object_action"),
            object_type=data.get("object_type", ""),
            action_name=data.get("action_name", ""),
            description=data.get("description", ""),
            parameters_schema=data.get("parameters_schema", {}),
            requires_approval=data.get("requires_approval", False),
            risk_level=RiskLevel(data.get("risk_level", "NORMAL")),
            permission_required=data.get("permission_required"),
            tags=data.get("tags", []),
            examples=data.get("examples", []),
            metadata=data.get("metadata", {}),
            is_active=data.get("is_active", True),
        )

        return await self._function_repo.save_function(func)

    async def update_function(self, id: str, data: dict[str, Any]) -> AIPFunction | None:
        """更新函数"""
        from src.domain.models.aip_ontology_models import RiskLevel

        func = await self._function_repo.get_function_by_id(id)
        if not func:
            return None

        if "description" in data:
            func.description = data["description"]
        if "parameters_schema" in data:
            func.parameters_schema = data["parameters_schema"]
        if "requires_approval" in data:
            func.requires_approval = data["requires_approval"]
        if "risk_level" in data:
            func.risk_level = RiskLevel(data["risk_level"])
        if "permission_required" in data:
            func.permission_required = data["permission_required"]
        if "tags" in data:
            func.tags = data["tags"]
        if "examples" in data:
            func.examples = data["examples"]
        if "metadata" in data:
            func.metadata = data["metadata"]
        if "is_active" in data:
            func.is_active = data["is_active"]

        return await self._function_repo.save_function(func)

    async def delete_function(self, id: str) -> bool:
        """删除函数"""
        return await self._function_repo.delete_function(id)

    async def get_all_mappings(
        self,
        object_type: str | None = None,
        migration_status: str | None = None,
        include_inactive: bool = False,
        limit: int = 100,
        offset: int = 0,
    ) -> list[ToolMapping]:
        """获取所有工具映射"""
        return await self._mapping_repo.get_all_mappings(object_type, migration_status, include_inactive, limit, offset)

    async def get_mapping_by_id(self, id: str) -> ToolMapping | None:
        """获取工具映射"""
        return await self._mapping_repo.get_mapping_by_id(id)

    async def create_mapping(self, data: dict[str, Any]) -> ToolMapping:
        """创建工具映射"""
        from src.domain.models.aip_ontology_models import MigrationStatus, RiskLevel

        mapping = ToolMapping(
            id="",
            tool_name=data.get("tool_name", ""),
            object_type=data.get("object_type", ""),
            action_name=data.get("action_name", ""),
            requires_approval=data.get("requires_approval", False),
            risk_level=RiskLevel(data.get("risk_level", "NORMAL")),
            migration_status=MigrationStatus(data.get("migration_status", "not_started")),
            custom_handler=data.get("custom_handler"),
            metadata=data.get("metadata", {}),
            is_active=data.get("is_active", True),
        )

        return await self._mapping_repo.save_mapping(mapping)

    async def update_mapping(self, id: str, data: dict[str, Any]) -> ToolMapping | None:
        """更新工具映射"""
        from src.domain.models.aip_ontology_models import MigrationStatus, RiskLevel

        mapping = await self._mapping_repo.get_mapping_by_id(id)
        if not mapping:
            return None

        if "object_type" in data:
            mapping.object_type = data["object_type"]
        if "action_name" in data:
            mapping.action_name = data["action_name"]
        if "requires_approval" in data:
            mapping.requires_approval = data["requires_approval"]
        if "risk_level" in data:
            mapping.risk_level = RiskLevel(data["risk_level"])
        if "migration_status" in data:
            mapping.migration_status = MigrationStatus(data["migration_status"])
        if "custom_handler" in data:
            mapping.custom_handler = data["custom_handler"]
        if "metadata" in data:
            mapping.metadata = data["metadata"]
        if "is_active" in data:
            mapping.is_active = data["is_active"]

        return await self._mapping_repo.save_mapping(mapping)

    async def delete_mapping(self, id: str) -> bool:
        """删除工具映射"""
        return await self._mapping_repo.delete_mapping(id)

    async def get_all_constraints(
        self,
        object_type: str | None = None,
        action_name: str | None = None,
        include_inactive: bool = False,
        limit: int = 100,
        offset: int = 0,
    ) -> list[ConstraintDefinition]:
        """获取所有约束"""
        return await self._constraint_repo.get_all_constraints(
            object_type, action_name, include_inactive, limit, offset
        )

    async def get_constraint_by_id(self, id: str) -> ConstraintDefinition | None:
        """获取约束"""
        return await self._constraint_repo.get_constraint_by_id(id)

    async def create_constraint(self, data: dict[str, Any]) -> ConstraintDefinition:
        """创建约束"""
        from src.domain.models.aip_ontology_models import ConstraintType, RiskLevel

        constraint = ConstraintDefinition(
            id="",
            name=data.get("name", ""),
            object_type=data.get("object_type", ""),
            action_name=data.get("action_name"),
            constraint_type=ConstraintType(data.get("constraint_type", "validation")),
            expression=data.get("expression", ""),
            error_message=data.get("error_message"),
            severity=RiskLevel(data.get("severity", "NORMAL")),
            metadata=data.get("metadata", {}),
            is_active=data.get("is_active", True),
        )

        return await self._constraint_repo.save_constraint(constraint)

    async def update_constraint(self, id: str, data: dict[str, Any]) -> ConstraintDefinition | None:
        """更新约束"""
        from src.domain.models.aip_ontology_models import RiskLevel

        constraint = await self._constraint_repo.get_constraint_by_id(id)
        if not constraint:
            return None

        if "name" in data:
            constraint.name = data["name"]
        if "expression" in data:
            constraint.expression = data["expression"]
        if "error_message" in data:
            constraint.error_message = data["error_message"]
        if "severity" in data:
            constraint.severity = RiskLevel(data["severity"])
        if "metadata" in data:
            constraint.metadata = data["metadata"]
        if "is_active" in data:
            constraint.is_active = data["is_active"]

        return await self._constraint_repo.save_constraint(constraint)

    async def delete_constraint(self, id: str) -> bool:
        """删除约束"""
        return await self._constraint_repo.delete_constraint(id)

    async def get_summary(self) -> dict[str, Any]:
        """获取 Ontology 汇总信息"""
        objects = await self._ontology_repo.get_all_objects(include_inactive=False, limit=1000)
        actions = await self._action_repo.get_all_actions(include_inactive=False, limit=1000)
        policies = await self._policy_repo.get_all_policies(limit=1000)
        functions = await self._function_repo.get_all_functions(include_inactive=False, limit=1000)
        mappings = await self._mapping_repo.get_all_mappings(include_inactive=False, limit=1000)
        constraints = await self._constraint_repo.get_all_constraints(include_inactive=False, limit=1000)

        object_types = [obj.name for obj in objects]

        recent_updates = []
        for obj in sorted(objects, key=lambda x: x.updated_at or "", reverse=True)[:5]:
            if obj.updated_at:
                recent_updates.append(
                    {
                        "type": "object",
                        "name": obj.name,
                        "updated_at": obj.updated_at.isoformat(),
                    }
                )
        for action in sorted(actions, key=lambda x: x.updated_at or "", reverse=True)[:3]:
            if action.updated_at:
                recent_updates.append(
                    {
                        "type": "action",
                        "name": f"{action.object_type}.{action.name}",
                        "updated_at": action.updated_at.isoformat(),
                    }
                )

        return {
            "object_count": len(objects),
            "action_count": len(actions),
            "policy_count": len(policies),
            "function_count": len(functions),
            "mapping_count": len(mappings),
            "constraint_count": len(constraints),
            "object_types": object_types,
            "recent_updates": recent_updates,
        }
