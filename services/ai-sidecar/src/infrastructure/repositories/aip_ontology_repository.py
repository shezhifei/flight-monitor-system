"""
AIP Ontology 自定义配置 Repository 实现

提供数据库 CRUD 操作实现。
"""

from typing import TYPE_CHECKING, Any

from src.infrastructure.database.soft_delete_audit import record_soft_delete

from src.domain.models.aip_ontology_models import (
    AIPFunction,
    ConstraintDefinition,
    ObjectPolicy,
    OntologyActionDefinition,
    OntologyObjectDefinition,
    ToolMapping,
)
from src.infrastructure.logging.core import get_logger
from src.shared.id_generator import generate_id

if TYPE_CHECKING:
    from src.infrastructure.database.async_connection_pool import AsyncPooledDatabaseConnection

logger = get_logger(__name__)


class AIPOntologyRepository:
    """AIP Ontology 自定义配置 Repository"""

    def __init__(self, db_pool: "AsyncPooledDatabaseConnection"):
        self._db_pool = db_pool

    async def _init_tables(self) -> None:
        """初始化表结构"""
        logger.debug("AIP ontology tables expected to be created via migration")

    async def get_all_objects(
        self,
        include_inactive: bool = False,
        limit: int = 100,
        offset: int = 0,
    ) -> list[OntologyObjectDefinition]:
        """获取所有对象定义"""
        query = """
            SELECT * FROM aip_ontology_objects
            WHERE (is_active = TRUE OR %s = TRUE) AND deleted_at IS NULL
            ORDER BY name
            LIMIT %s OFFSET %s
        """
        async with self._db_pool.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(query, (include_inactive, limit, offset))
            rows = await cursor.fetchall()
            return [OntologyObjectDefinition.from_db_row(row) for row in rows]

    async def get_object_by_id(self, id: str) -> OntologyObjectDefinition | None:
        """根据ID获取对象定义"""
        query = "SELECT * FROM aip_ontology_objects WHERE id = %s AND deleted_at IS NULL"
        async with self._db_pool.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(query, (id,))
            row = await cursor.fetchone()
            return OntologyObjectDefinition.from_db_row(row) if row else None

    async def get_object_by_name(self, name: str) -> OntologyObjectDefinition | None:
        """根据名称获取对象定义"""
        query = "SELECT * FROM aip_ontology_objects WHERE name = %s AND deleted_at IS NULL"
        async with self._db_pool.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(query, (name,))
            row = await cursor.fetchone()
            return OntologyObjectDefinition.from_db_row(row) if row else None

    async def save_object(self, obj: OntologyObjectDefinition) -> OntologyObjectDefinition:
        """保存对象定义"""
        if not obj.id:
            obj.id = generate_id()

        query = """
            INSERT INTO aip_ontology_objects (
                id, name, plural_name, description, is_abstract,
                properties, relationships, actions, tags, metadata, is_active
            ) VALUES (
                %(id)s, %(name)s, %(plural_name)s, %(description)s, %(is_abstract)s,
                %(properties)s, %(relationships)s, %(actions)s, %(tags)s, %(metadata)s, %(is_active)s
            )
            ON CONFLICT (id) DO UPDATE SET
                plural_name = EXCLUDED.plural_name,
                description = EXCLUDED.description,
                is_abstract = EXCLUDED.is_abstract,
                properties = EXCLUDED.properties,
                relationships = EXCLUDED.relationships,
                actions = EXCLUDED.actions,
                tags = EXCLUDED.tags,
                metadata = EXCLUDED.metadata,
                is_active = EXCLUDED.is_active,
                deleted_at = NULL,
                updated_at = CURRENT_TIMESTAMP
        """
        import json

        params = {
            "id": obj.id,
            "name": obj.name,
            "plural_name": obj.plural_name or obj.name + "s",
            "description": obj.description,
            "is_abstract": obj.is_abstract,
            "properties": json.dumps([p.to_dict() for p in obj.properties]),
            "relationships": json.dumps([r.to_dict() for r in obj.relationships]),
            "actions": json.dumps(obj.actions),
            "tags": json.dumps(obj.tags),
            "metadata": json.dumps(obj.metadata),
            "is_active": obj.is_active,
        }

        async with self._db_pool.connection_context() as conn:
            await conn.execute(query, params)

        logger.debug(f"Saved ontology object: {obj.name}")
        return obj

    async def delete_object(self, id: str) -> bool:
        """删除对象定义（审计要求软删除：仅标记 deleted_at，行保留）"""
        query = "UPDATE aip_ontology_objects SET deleted_at = NOW(), updated_at = NOW() WHERE id = %s AND deleted_at IS NULL"
        async with self._db_pool.connection_context() as conn:
            result = await conn.execute(query, (id,))
            if result and result > 0:
                await record_soft_delete(conn, "aip_ontology_object", id)
                return True
            return False


class AIPActionRepository:
    """AIP Action Repository"""

    def __init__(self, db_pool: "AsyncPooledDatabaseConnection"):
        self._db_pool = db_pool

    async def get_all_actions(
        self,
        object_type: str | None = None,
        include_inactive: bool = False,
        limit: int = 100,
        offset: int = 0,
    ) -> list[OntologyActionDefinition]:
        """获取所有动作定义"""
        conditions = ["deleted_at IS NULL", "(is_active = TRUE OR %s = TRUE)"]
        params: list[Any] = [include_inactive]

        if object_type:
            conditions.append("object_type = %s")
            params.append(object_type)

        where_clause = " AND ".join(conditions)
        query = f"""
            SELECT * FROM aip_ontology_actions
            WHERE {where_clause}
            ORDER BY object_type, name
            LIMIT %s OFFSET %s
        """
        params.extend([limit, offset])

        async with self._db_pool.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(query, tuple(params))
            rows = await cursor.fetchall()
            return [OntologyActionDefinition.from_db_row(row) for row in rows]

    async def get_action_by_id(self, id: str) -> OntologyActionDefinition | None:
        """根据ID获取动作定义"""
        query = "SELECT * FROM aip_ontology_actions WHERE id = %s AND deleted_at IS NULL"
        async with self._db_pool.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(query, (id,))
            row = await cursor.fetchone()
            return OntologyActionDefinition.from_db_row(row) if row else None

    async def get_action_by_object_action(self, object_type: str, action_name: str) -> OntologyActionDefinition | None:
        """根据对象类型和动作名称获取动作定义"""
        query = (
            "SELECT * FROM aip_ontology_actions "
            "WHERE object_type = %s AND name = %s AND deleted_at IS NULL"
        )
        async with self._db_pool.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(query, (object_type, action_name))
            row = await cursor.fetchone()
            return OntologyActionDefinition.from_db_row(row) if row else None

    async def save_action(self, action: OntologyActionDefinition) -> OntologyActionDefinition:
        """保存动作定义"""
        if not action.id:
            action.id = generate_id()

        import json

        query = """
            INSERT INTO aip_ontology_actions (
                id, name, object_type, description, category,
                parameters, requires_approval, risk_level, constraint_rules, metadata, is_active
            ) VALUES (
                %(id)s, %(name)s, %(object_type)s, %(description)s, %(category)s,
                %(parameters)s, %(requires_approval)s, %(risk_level)s, %(constraint_rules)s, %(metadata)s, %(is_active)s
            )
            ON CONFLICT (object_type, name) DO UPDATE SET
                description = EXCLUDED.description,
                category = EXCLUDED.category,
                parameters = EXCLUDED.parameters,
                requires_approval = EXCLUDED.requires_approval,
                risk_level = EXCLUDED.risk_level,
                constraint_rules = EXCLUDED.constraint_rules,
                metadata = EXCLUDED.metadata,
                is_active = EXCLUDED.is_active,
                deleted_at = NULL,
                updated_at = CURRENT_TIMESTAMP
        """
        params = {
            "id": action.id,
            "name": action.name,
            "object_type": action.object_type,
            "description": action.description,
            "category": action.category.value,
            "parameters": json.dumps([p.to_dict() for p in action.parameters]),
            "requires_approval": action.requires_approval,
            "risk_level": action.risk_level.value,
            "constraint_rules": json.dumps(action.constraint_rules),
            "metadata": json.dumps(action.metadata),
            "is_active": action.is_active,
        }

        async with self._db_pool.connection_context() as conn:
            await conn.execute(query, params)

        logger.debug(f"Saved ontology action: {action.object_type}.{action.name}")
        return action

    async def delete_action(self, id: str) -> bool:
        """删除动作定义（审计要求软删除：仅标记 deleted_at，行保留）"""
        query = "UPDATE aip_ontology_actions SET deleted_at = NOW(), updated_at = NOW() WHERE id = %s AND deleted_at IS NULL"
        async with self._db_pool.connection_context() as conn:
            result = await conn.execute(query, (id,))
            if result and result > 0:
                await record_soft_delete(conn, "aip_ontology_action", id)
                return True
            return False


class AIPPolicyRepository:
    """AIP Policy Repository"""

    def __init__(self, db_pool: "AsyncPooledDatabaseConnection"):
        self._db_pool = db_pool

    async def get_all_policies(
        self,
        principal_id: str | None = None,
        object_type: str | None = None,
        limit: int = 100,
        offset: int = 0,
    ) -> list[ObjectPolicy]:
        """获取所有策略"""
        conditions = ["deleted_at IS NULL"]
        params: list[Any] = []

        if principal_id:
            conditions.append("principal_id = %s")
            params.append(principal_id)
        if object_type:
            conditions.append("object_type = %s")
            params.append(object_type)

        where_clause = " AND ".join(conditions)

        query = f"""
            SELECT * FROM aip_object_policies
            WHERE {where_clause}
            ORDER BY object_type, principal_id
            LIMIT %s OFFSET %s
        """
        params.extend([limit, offset])

        async with self._db_pool.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(query, tuple(params))
            rows = await cursor.fetchall()
            return [ObjectPolicy.from_db_row(row) for row in rows]

    async def get_policy_by_id(self, id: str) -> ObjectPolicy | None:
        """根据ID获取策略"""
        query = "SELECT * FROM aip_object_policies WHERE id = %s AND deleted_at IS NULL"
        async with self._db_pool.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(query, (id,))
            row = await cursor.fetchone()
            return ObjectPolicy.from_db_row(row) if row else None

    async def save_policy(self, policy: ObjectPolicy) -> ObjectPolicy:
        """保存策略"""
        if not policy.id:
            policy.id = generate_id()

        import json

        query = """
            INSERT INTO aip_object_policies (
                id, object_type, object_id, principal_type, principal_id,
                permission, granted, conditions, description, expires_at
            ) VALUES (
                %(id)s, %(object_type)s, %(object_id)s, %(principal_type)s, %(principal_id)s,
                %(permission)s, %(granted)s, %(conditions)s, %(description)s, %(expires_at)s
            )
            ON CONFLICT (principal_type, principal_id, object_type, object_id, permission)
            DO UPDATE SET
                object_id = EXCLUDED.object_id,
                granted = EXCLUDED.granted,
                conditions = EXCLUDED.conditions,
                description = EXCLUDED.description,
                expires_at = EXCLUDED.expires_at,
                deleted_at = NULL,
                updated_at = CURRENT_TIMESTAMP
        """
        params = {
            "id": policy.id,
            "object_type": policy.object_type,
            "object_id": policy.object_id,
            "principal_type": policy.principal_type.value,
            "principal_id": policy.principal_id,
            "permission": policy.permission.value,
            "granted": policy.granted,
            "conditions": json.dumps(policy.conditions) if policy.conditions else None,
            "description": policy.description,
            "expires_at": policy.expires_at,
        }

        async with self._db_pool.connection_context() as conn:
            await conn.execute(query, params)

        logger.debug(
            f"Saved object policy: {policy.principal_type.value}:{policy.principal_id} -> {policy.object_type}"
        )
        return policy

    async def delete_policy(self, id: str) -> bool:
        """删除策略（审计要求软删除：仅标记 deleted_at，行保留）"""
        query = "UPDATE aip_object_policies SET deleted_at = NOW(), updated_at = NOW() WHERE id = %s AND deleted_at IS NULL"
        async with self._db_pool.connection_context() as conn:
            result = await conn.execute(query, (id,))
            if result and result > 0:
                await record_soft_delete(conn, "aip_object_policy", id)
                return True
            return False


class AIPFunctionRepository:
    """AIP Function Repository"""

    def __init__(self, db_pool: "AsyncPooledDatabaseConnection"):
        self._db_pool = db_pool

    async def get_all_functions(
        self,
        object_type: str | None = None,
        category: str | None = None,
        include_inactive: bool = False,
        limit: int = 100,
        offset: int = 0,
    ) -> list[AIPFunction]:
        """获取所有函数"""
        conditions = ["deleted_at IS NULL", "(is_active = TRUE OR %s = TRUE)"]
        params: list[Any] = [include_inactive]

        if object_type:
            conditions.append("object_type = %s")
            params.append(object_type)
        if category:
            conditions.append("category = %s")
            params.append(category)

        where_clause = " AND ".join(conditions)
        query = f"""
            SELECT * FROM aip_functions
            WHERE {where_clause}
            ORDER BY object_type, name
            LIMIT %s OFFSET %s
        """
        params.extend([limit, offset])

        async with self._db_pool.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(query, tuple(params))
            rows = await cursor.fetchall()
            return [AIPFunction.from_db_row(row) for row in rows]

    async def get_function_by_id(self, id: str) -> AIPFunction | None:
        """根据ID获取函数"""
        query = "SELECT * FROM aip_functions WHERE id = %s AND deleted_at IS NULL"
        async with self._db_pool.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(query, (id,))
            row = await cursor.fetchone()
            return AIPFunction.from_db_row(row) if row else None

    async def get_function_by_name(self, name: str) -> AIPFunction | None:
        """根据名称获取函数"""
        query = "SELECT * FROM aip_functions WHERE name = %s AND deleted_at IS NULL"
        async with self._db_pool.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(query, (name,))
            row = await cursor.fetchone()
            return AIPFunction.from_db_row(row) if row else None

    async def save_function(self, func: AIPFunction) -> AIPFunction:
        """保存函数"""
        if not func.id:
            func.id = generate_id()

        import json

        query = """
            INSERT INTO aip_functions (
                id, name, category, object_type, action_name, description,
                parameters_schema, requires_approval, risk_level, permission_required,
                tags, examples, metadata, is_active
            ) VALUES (
                %(id)s, %(name)s, %(category)s, %(object_type)s, %(action_name)s, %(description)s,
                %(parameters_schema)s, %(requires_approval)s, %(risk_level)s, %(permission_required)s,
                %(tags)s, %(examples)s, %(metadata)s, %(is_active)s
            )
            ON CONFLICT (name) DO UPDATE SET
                category = EXCLUDED.category,
                object_type = EXCLUDED.object_type,
                action_name = EXCLUDED.action_name,
                description = EXCLUDED.description,
                parameters_schema = EXCLUDED.parameters_schema,
                requires_approval = EXCLUDED.requires_approval,
                risk_level = EXCLUDED.risk_level,
                permission_required = EXCLUDED.permission_required,
                tags = EXCLUDED.tags,
                examples = EXCLUDED.examples,
                metadata = EXCLUDED.metadata,
                is_active = EXCLUDED.is_active,
                deleted_at = NULL,
                updated_at = CURRENT_TIMESTAMP
        """
        params = {
            "id": func.id,
            "name": func.name,
            "category": func.category,
            "object_type": func.object_type,
            "action_name": func.action_name,
            "description": func.description,
            "parameters_schema": json.dumps(func.parameters_schema),
            "requires_approval": func.requires_approval,
            "risk_level": func.risk_level.value,
            "permission_required": func.permission_required,
            "tags": json.dumps(func.tags),
            "examples": json.dumps(func.examples),
            "metadata": json.dumps(func.metadata),
            "is_active": func.is_active,
        }

        async with self._db_pool.connection_context() as conn:
            await conn.execute(query, params)

        logger.debug(f"Saved AIP function: {func.name}")
        return func

    async def delete_function(self, id: str) -> bool:
        """删除函数（审计要求软删除：仅标记 deleted_at，行保留）"""
        query = "UPDATE aip_functions SET deleted_at = NOW(), updated_at = NOW() WHERE id = %s AND deleted_at IS NULL"
        async with self._db_pool.connection_context() as conn:
            result = await conn.execute(query, (id,))
            if result and result > 0:
                await record_soft_delete(conn, "aip_function", id)
                return True
            return False


class AIPToolMappingRepository:
    """AIP Tool Mapping Repository"""

    def __init__(self, db_pool: "AsyncPooledDatabaseConnection"):
        self._db_pool = db_pool

    async def get_all_mappings(
        self,
        object_type: str | None = None,
        migration_status: str | None = None,
        include_inactive: bool = False,
        limit: int = 100,
        offset: int = 0,
    ) -> list[ToolMapping]:
        """获取所有工具映射"""
        conditions = ["deleted_at IS NULL", "(is_active = TRUE OR %s = TRUE)"]
        params: list[Any] = [include_inactive]

        if object_type:
            conditions.append("object_type = %s")
            params.append(object_type)
        if migration_status:
            conditions.append("migration_status = %s")
            params.append(migration_status)

        where_clause = " AND ".join(conditions)
        query = f"""
            SELECT * FROM aip_tool_mappings
            WHERE {where_clause}
            ORDER BY tool_name
            LIMIT %s OFFSET %s
        """
        params.extend([limit, offset])

        async with self._db_pool.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(query, tuple(params))
            rows = await cursor.fetchall()
            return [ToolMapping.from_db_row(row) for row in rows]

    async def get_mapping_by_id(self, id: str) -> ToolMapping | None:
        """根据ID获取映射"""
        query = "SELECT * FROM aip_tool_mappings WHERE id = %s AND deleted_at IS NULL"
        async with self._db_pool.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(query, (id,))
            row = await cursor.fetchone()
            return ToolMapping.from_db_row(row) if row else None

    async def get_mapping_by_tool_name(self, tool_name: str) -> ToolMapping | None:
        """根据工具名称获取映射"""
        query = "SELECT * FROM aip_tool_mappings WHERE tool_name = %s AND deleted_at IS NULL"
        async with self._db_pool.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(query, (tool_name,))
            row = await cursor.fetchone()
            return ToolMapping.from_db_row(row) if row else None

    async def save_mapping(self, mapping: ToolMapping) -> ToolMapping:
        """保存工具映射"""
        if not mapping.id:
            mapping.id = generate_id()

        import json

        query = """
            INSERT INTO aip_tool_mappings (
                id, tool_name, object_type, action_name, requires_approval,
                risk_level, migration_status, custom_handler, metadata, is_active
            ) VALUES (
                %(id)s, %(tool_name)s, %(object_type)s, %(action_name)s, %(requires_approval)s,
                %(risk_level)s, %(migration_status)s, %(custom_handler)s, %(metadata)s, %(is_active)s
            )
            ON CONFLICT (tool_name) DO UPDATE SET
                object_type = EXCLUDED.object_type,
                action_name = EXCLUDED.action_name,
                requires_approval = EXCLUDED.requires_approval,
                risk_level = EXCLUDED.risk_level,
                migration_status = EXCLUDED.migration_status,
                custom_handler = EXCLUDED.custom_handler,
                metadata = EXCLUDED.metadata,
                is_active = EXCLUDED.is_active,
                deleted_at = NULL,
                updated_at = CURRENT_TIMESTAMP
        """
        params = {
            "id": mapping.id,
            "tool_name": mapping.tool_name,
            "object_type": mapping.object_type,
            "action_name": mapping.action_name,
            "requires_approval": mapping.requires_approval,
            "risk_level": mapping.risk_level.value,
            "migration_status": mapping.migration_status.value,
            "custom_handler": mapping.custom_handler,
            "metadata": json.dumps(mapping.metadata),
            "is_active": mapping.is_active,
        }

        async with self._db_pool.connection_context() as conn:
            await conn.execute(query, params)

        logger.debug(f"Saved tool mapping: {mapping.tool_name} -> {mapping.object_type}.{mapping.action_name}")
        return mapping

    async def delete_mapping(self, id: str) -> bool:
        """删除工具映射（审计要求软删除：仅标记 deleted_at，行保留）"""
        query = "UPDATE aip_tool_mappings SET deleted_at = NOW(), updated_at = NOW() WHERE id = %s AND deleted_at IS NULL"
        async with self._db_pool.connection_context() as conn:
            result = await conn.execute(query, (id,))
            if result and result > 0:
                await record_soft_delete(conn, "aip_tool_mapping", id)
                return True
            return False


class AIPConstraintRepository:
    """AIP Constraint Repository"""

    def __init__(self, db_pool: "AsyncPooledDatabaseConnection"):
        self._db_pool = db_pool

    async def get_all_constraints(
        self,
        object_type: str | None = None,
        action_name: str | None = None,
        include_inactive: bool = False,
        limit: int = 100,
        offset: int = 0,
    ) -> list[ConstraintDefinition]:
        """获取所有约束"""
        conditions = ["deleted_at IS NULL", "(is_active = TRUE OR %s = TRUE)"]
        params: list[Any] = [include_inactive]

        if object_type:
            conditions.append("object_type = %s")
            params.append(object_type)
        if action_name:
            conditions.append("action_name = %s")
            params.append(action_name)

        where_clause = " AND ".join(conditions)
        query = f"""
            SELECT * FROM aip_constraints
            WHERE {where_clause}
            ORDER BY object_type, action_name, name
            LIMIT %s OFFSET %s
        """
        params.extend([limit, offset])

        async with self._db_pool.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(query, tuple(params))
            rows = await cursor.fetchall()
            return [ConstraintDefinition.from_db_row(row) for row in rows]

    async def get_constraint_by_id(self, id: str) -> ConstraintDefinition | None:
        """根据ID获取约束"""
        query = "SELECT * FROM aip_constraints WHERE id = %s AND deleted_at IS NULL"
        async with self._db_pool.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(query, (id,))
            row = await cursor.fetchone()
            return ConstraintDefinition.from_db_row(row) if row else None

    async def save_constraint(self, constraint: ConstraintDefinition) -> ConstraintDefinition:
        """保存约束"""
        if not constraint.id:
            constraint.id = generate_id()

        import json

        query = """
            INSERT INTO aip_constraints (
                id, name, object_type, action_name, constraint_type,
                expression, error_message, severity, metadata, is_active
            ) VALUES (
                %(id)s, %(name)s, %(object_type)s, %(action_name)s, %(constraint_type)s,
                %(expression)s, %(error_message)s, %(severity)s, %(metadata)s, %(is_active)s
            )
            ON CONFLICT (object_type, action_name, name) DO UPDATE SET
                expression = EXCLUDED.expression,
                error_message = EXCLUDED.error_message,
                severity = EXCLUDED.severity,
                metadata = EXCLUDED.metadata,
                is_active = EXCLUDED.is_active,
                deleted_at = NULL,
                updated_at = CURRENT_TIMESTAMP
        """
        params = {
            "id": constraint.id,
            "name": constraint.name,
            "object_type": constraint.object_type,
            "action_name": constraint.action_name,
            "constraint_type": constraint.constraint_type.value,
            "expression": constraint.expression,
            "error_message": constraint.error_message,
            "severity": constraint.severity.value,
            "metadata": json.dumps(constraint.metadata),
            "is_active": constraint.is_active,
        }

        async with self._db_pool.connection_context() as conn:
            await conn.execute(query, params)

        logger.debug(f"Saved constraint: {constraint.object_type}.{constraint.action_name}.{constraint.name}")
        return constraint

    async def delete_constraint(self, id: str) -> bool:
        """删除约束（审计要求软删除：仅标记 deleted_at，行保留）"""
        query = "UPDATE aip_constraints SET deleted_at = NOW(), updated_at = NOW() WHERE id = %s AND deleted_at IS NULL"
        async with self._db_pool.connection_context() as conn:
            result = await conn.execute(query, (id,))
            if result and result > 0:
                await record_soft_delete(conn, "aip_constraint", id)
                return True
            return False
