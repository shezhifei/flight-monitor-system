"""
Ontology 数据加载器

从数据库动态加载 Ontology 配置，使配置变更能实时影响 AI 行为。
这是 Ontology 真正发挥作用的核心组件。
"""

import asyncio
from typing import TYPE_CHECKING, Any, Optional

from src.domain.models.aip_ontology_models import (
    AIPFunction as AIPFunctionModel,
)
from src.domain.models.aip_ontology_models import (
    ConstraintDefinition,
    OntologyActionDefinition,
    OntologyObjectDefinition,
)
from src.infrastructure.logging.core import get_logger
from src.infrastructure.repositories.aip_ontology_repository import (
    AIPActionRepository,
    AIPConstraintRepository,
    AIPFunctionRepository,
    AIPOntologyRepository,
)

if TYPE_CHECKING:
    from src.infrastructure.database.async_connection_pool import AsyncPooledDatabaseConnection

logger = get_logger(__name__)


class OntologyDataLoader:
    """
    Ontology 数据加载器

    核心职责：
    1. 从数据库加载 Ontology 配置
    2. 将配置应用到运行时组件
    3. 支持热更新（修改数据库后重新加载）

    Ontology 的真正价值：
    - 修改属性定义 -> LLM 知道有哪些字段
    - 修改动作定义 -> LLM 知道能执行什么操作
    - 修改约束 -> 执行前自动校验
    - 修改权限 -> 不同用户看到不同工具
    """

    def __init__(self, db_pool: "AsyncPooledDatabaseConnection"):
        self._db_pool = db_pool
        self._ontology_repo = AIPOntologyRepository(db_pool)
        self._action_repo = AIPActionRepository(db_pool)
        self._function_repo = AIPFunctionRepository(db_pool)
        self._constraint_repo = AIPConstraintRepository(db_pool)
        self._cache: dict[str, Any] = {}
        self._last_load_time: float | None = None
        self._load_tasks: set[asyncio.Task[Any]] = set()

    async def load_all(self) -> dict[str, Any]:
        """
        加载所有 Ontology 配置

        Returns:
            包含所有配置的字典
        """
        logger.info("Loading Ontology data from database...")

        objects = await self._ontology_repo.get_all_objects(include_inactive=False, limit=1000)
        actions = await self._action_repo.get_all_actions(include_inactive=False, limit=1000)
        functions = await self._function_repo.get_all_functions(include_inactive=False, limit=1000)
        constraints = await self._constraint_repo.get_all_constraints(include_inactive=False, limit=1000)

        self._cache = {
            "objects": {obj.id: obj for obj in objects},
            "actions": {action.id: action for action in actions},
            "functions": {func.id: func for func in functions},
            "constraints": {c.id: c for c in constraints},
            "object_map": {obj.name: obj for obj in objects},
            "action_map": {(a.object_type, a.name): a for a in actions},
        }

        import time

        self._last_load_time = time.time()

        logger.info(
            f"Loaded: {len(objects)} objects, {len(actions)} actions, "
            f"{len(functions)} functions, {len(constraints)} constraints"
        )

        return self._cache

    async def reload(self) -> dict[str, Any]:
        """重新加载配置（热更新）"""
        logger.info("Reloading Ontology data from database...")
        return await self.load_all()

    def get_object(self, name: str) -> OntologyObjectDefinition | None:
        """获取对象定义"""
        return self._cache.get("object_map", {}).get(name)

    def get_action(self, object_type: str, action_name: str) -> OntologyActionDefinition | None:
        """获取动作定义"""
        return self._cache.get("action_map", {}).get((object_type, action_name))

    def get_object_actions(self, object_type: str) -> list[OntologyActionDefinition]:
        """获取对象的所有动作"""
        return [action for action in self._cache.get("actions", {}).values() if action.object_type == object_type]

    def get_functions_for_object(self, object_type: str) -> list[AIPFunctionModel]:
        """获取对象关联的函数"""
        return [func for func in self._cache.get("functions", {}).values() if func.object_type == object_type]

    def get_constraints(self, object_type: str, action_name: str | None = None) -> list[ConstraintDefinition]:
        """获取约束定义"""
        constraints = [c for c in self._cache.get("constraints", {}).values() if c.object_type == object_type]
        if action_name:
            constraints = [c for c in constraints if c.action_name == action_name]
        return constraints

    def validate_action(self, object_type: str, action_name: str, parameters: dict[str, Any]) -> dict[str, Any]:
        """
        验证动作参数是否符合 Ontology 定义

        这是 Ontology 驱动行为的核心：
        - 检查必填参数
        - 验证参数类型
        - 检查枚举值

        Args:
            object_type: 对象类型
            action_name: 动作名称
            parameters: 传入的参数

        Returns:
            验证结果
        """
        action = self.get_action(object_type, action_name)
        if not action:
            return {"valid": False, "error": f"Unknown action: {object_type}.{action_name}"}

        errors = []
        warnings = []

        for param_def in action.parameters:
            param_name = param_def.name
            param_value = parameters.get(param_name)

            if param_def.required and param_value is None:
                errors.append(f"Missing required parameter: {param_name}")
                continue

            if param_value is None:
                continue

            if param_def.enum_values and param_value not in param_def.enum_values:
                errors.append(
                    f"Invalid value for {param_name}: '{param_value}'. Must be one of: {param_def.enum_values}"
                )

        return {
            "valid": len(errors) == 0,
            "errors": errors,
            "warnings": warnings,
            "action": action.to_db_dict() if hasattr(action, "to_db_dict") else None,
        }

    def build_llm_context(self, object_types: list[str] | None = None) -> dict[str, Any]:
        """
        构建 LLM 上下文

        这是将 Ontology 注入到 AI 的核心方法：

        Returns:
            LLM 可理解的 Ontology 描述
        """
        if not self._cache:
            logger.warning("Ontology cache is empty, loading...")
            task = asyncio.create_task(self.load_all())
            self._load_tasks.add(task)
            task.add_done_callback(self._load_tasks.discard)

        context = {
            "ontology_version": self._cache.get("ontology_version") or "flight-ops.v1",
            "loaded_at": self._last_load_time,
            "object_types": [],
        }

        objects = self._cache.get("object_map", {})

        for obj_name, obj_def in objects.items():
            if object_types and obj_name not in object_types:
                continue

            obj_context = {
                "name": obj_def.name,
                "description": obj_def.description,
                "properties": [],
                "relationships": [],
                "actions": [],
            }

            for prop in obj_def.properties:
                prop_desc = {
                    "name": prop.name,
                    "type": prop.type,
                    "description": prop.description,
                }
                if prop.enum_values:
                    prop_desc["enum_values"] = prop.enum_values
                if prop.required:
                    prop_desc["required"] = True
                obj_context["properties"].append(prop_desc)

            for rel in obj_def.relationships:
                obj_context["relationships"].append(
                    {
                        "name": rel.name,
                        "target": rel.target_object,
                        "cardinality": rel.cardinality,
                        "description": rel.description,
                    }
                )

            actions = self.get_object_actions(obj_name)
            for action in actions:
                action_desc = {
                    "name": action.name,
                    "description": action.description,
                    "parameters": [],
                    "requires_approval": action.requires_approval,
                    "risk_level": action.risk_level.value if hasattr(action.risk_level, "value") else action.risk_level,
                }
                for param in action.parameters:
                    param_desc = {
                        "name": param.name,
                        "type": param.type,
                        "description": param.description,
                    }
                    if param.enum_values:
                        param_desc["enum_values"] = param.enum_values
                    if param.required:
                        param_desc["required"] = True
                    action_desc["parameters"].append(param_desc)

                obj_context["actions"].append(action_desc)

            context["object_types"].append(obj_context)

        return context

    def generate_tool_schemas(self, object_types: list[str] | None = None) -> list[dict[str, Any]]:
        """
        从 Ontology 动态生成 LLM Tool Schemas

        这是 Ontology 驱动 AI 工具的核心：
        - 根据数据库配置生成 OpenAI function calling 格式
        - 修改数据库 -> LLM 自动获得新工具

        Returns:
            OpenAI function calling 格式的 tool schemas
        """
        if not self._cache:
            task = asyncio.create_task(self.load_all())
            self._load_tasks.add(task)
            task.add_done_callback(self._load_tasks.discard)
            return []

        schemas = []
        objects = self._cache.get("object_map", {})

        for obj_name, obj_def in objects.items():
            if object_types and obj_name not in object_types:
                continue

            actions = self.get_object_actions(obj_name)
            for action in actions:
                if not action.is_active:
                    continue

                tool_name = f"{obj_name}.{action.name}"
                properties: dict[str, Any] = {}
                required: list[str] = []

                for param in action.parameters:
                    param_schema: dict[str, Any] = {
                        "type": self._map_type_to_openai(param.type),
                        "description": param.description,
                    }
                    if param.enum_values:
                        param_schema["enum"] = param.enum_values

                    properties[param.name] = param_schema

                    if param.required:
                        required.append(param.name)

                schema = {
                    "type": "function",
                    "function": {
                        "name": tool_name,
                        "description": f"{obj_def.description}: {action.description}",
                        "parameters": {
                            "type": "object",
                            "properties": properties,
                        },
                    },
                }

                if required:
                    schema["function"]["parameters"]["required"] = required

                schemas.append(schema)

        return schemas

    def _map_type_to_openai(self, ontology_type: str) -> str:
        """映射 Ontology 类型到 OpenAI 类型"""
        type_map = {
            "string": "string",
            "integer": "integer",
            "number": "number",
            "boolean": "boolean",
            "array": "array",
            "object": "object",
            "datetime": "string",
        }
        return type_map.get(ontology_type, "string")


class OntologyRuntimeBridge:
    """
    Ontology 运行时桥接器

    将数据库中的 Ontology 配置应用到 AIPApplication
    """

    def __init__(self, data_loader: OntologyDataLoader):
        self._data_loader = data_loader
        self._runtime_components: dict[str, Any] = {}

    async def initialize(self, aip_app: Any) -> None:
        """
        初始化运行时桥接

        将数据库配置应用到 AIPApplication

        Args:
            aip_app: AIPApplication 实例
        """
        logger.info("Initializing Ontology Runtime Bridge...")

        await self._data_loader.load_all()
        self._runtime_components["aip_app"] = aip_app

        await self._sync_function_registry(aip_app)
        await self._sync_object_schema(aip_app)
        await self._sync_constraints(aip_app)

        logger.info("Ontology Runtime Bridge initialized")

    async def _sync_function_registry(self, aip_app: Any) -> None:
        """同步 Function Registry"""
        if not aip_app.function_registry:
            logger.warning("Function Registry not available")
            return

        functions = self._data_loader.generate_tool_schemas()
        logger.info(f"Generated {len(functions)} tool schemas from Ontology")

        for schema in functions:
            try:
                aip_app.function_registry.register_from_ontology(schema)
            except Exception as e:  # noqa: BLE001 - individual function registration failures must not abort the sync loop
                logger.error(f"Failed to register function {schema['function']['name']}: {e}")

    async def _sync_object_schema(self, aip_app: Any) -> None:
        """同步对象 Schema"""
        if not aip_app.ontology_registry:
            logger.warning("Ontology Registry not available")
            return

        context = self._data_loader.build_llm_context()
        logger.info(f"Built LLM context with {len(context['object_types'])} object types")

        aip_app.ontology_registry.update_context(context)

    async def _sync_constraints(self, aip_app: Any) -> None:
        """同步约束"""
        constraints = self._data_loader._cache.get("constraints", {})
        logger.info(f"Loaded {len(constraints)} constraints from database")

        aip_app.ontology_registry.update_constraints(constraints)

    async def hot_reload(self) -> None:
        """
        热更新 Ontology 配置

        修改数据库后调用此方法，AI 立即感知变化
        """
        logger.info("Hot reloading Ontology configuration...")
        await self._data_loader.reload()

        if "aip_app" in self._runtime_components:
            aip_app = self._runtime_components["aip_app"]
            await self._sync_function_registry(aip_app)
            await self._sync_object_schema(aip_app)

        logger.info("Hot reload complete")

    def get_llm_context(self, object_types: list[str] | None = None) -> dict[str, Any]:
        """获取 LLM 上下文"""
        return self._data_loader.build_llm_context(object_types)

    def validate_action(self, object_type: str, action: str, parameters: dict[str, Any]) -> dict[str, Any]:
        """验证动作参数"""
        return self._data_loader.validate_action(object_type, action, parameters)


_ontology_data_loader: OntologyDataLoader | None = None


def get_ontology_data_loader(db_pool: Optional["AsyncPooledDatabaseConnection"] = None) -> OntologyDataLoader:
    """获取全局 Ontology Data Loader"""
    global _ontology_data_loader
    if _ontology_data_loader is None:
        if db_pool is None:
            from src.di.container import get_container

            container = get_container()
            db_pool = container.async_connection_pool
        _ontology_data_loader = OntologyDataLoader(db_pool)
    return _ontology_data_loader


def get_ontology_runtime_bridge() -> OntologyRuntimeBridge | None:
    """获取 Ontology Runtime Bridge"""
    global _ontology_data_loader
    if _ontology_data_loader is None:
        return None
    return OntologyRuntimeBridge(_ontology_data_loader)
