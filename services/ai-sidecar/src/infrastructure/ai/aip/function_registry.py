"""
AIP Function Registry - 统一函数注册表

将 Ontology Actions 注册为 LLM 可调用的 Functions，
支持基于对象类型的函数发现和权限过滤。
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import StrEnum
from typing import Any

from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class FunctionCategory(StrEnum):
    """函数类别"""

    OBJECT_ACTION = "object_action"
    QUERY = "query"
    REPORT = "report"
    UTILITY = "utility"


class RiskLevel(StrEnum):
    """风险等级"""

    LOW = "LOW"
    NORMAL = "NORMAL"
    MEDIUM = "MEDIUM"
    HIGH = "HIGH"
    CRITICAL = "CRITICAL"


@dataclass
class AIPFunction:
    """AIP Function 定义"""

    name: str
    category: FunctionCategory
    object_type: str
    action_name: str
    description: str = ""
    parameters_schema: dict[str, Any] = field(default_factory=dict)
    requires_approval: bool = False
    risk_level: RiskLevel = RiskLevel.NORMAL
    permission_required: str | None = None
    tags: list[str] = field(default_factory=list)
    examples: list[dict[str, Any]] = field(default_factory=list)

    def to_openai_schema(self) -> dict[str, Any]:
        """生成 OpenAI function calling 格式的 Schema"""
        return {
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": {
                    "type": "object",
                    "properties": self.parameters_schema.get("properties", {}),
                    "required": self.parameters_schema.get("required", []),
                },
            },
        }

    def to_function_def(self) -> dict[str, Any]:
        """生成完整的函数定义"""
        return {
            "name": self.name,
            "category": self.category.value,
            "object_type": self.object_type,
            "action": self.action_name,
            "description": self.description,
            "parameters": self.parameters_schema,
            "requires_approval": self.requires_approval,
            "risk_level": self.risk_level.value,
            "tags": self.tags,
        }


@dataclass
class ApprovalDecision:
    """审批决策结果"""

    required: bool
    reason: str = ""
    auto_escalation: bool = False
    diff_required: bool = True
    relationship_audit: bool = False


class AIPFunctionRegistry:
    """AIP Function 注册表"""

    def __init__(self):
        self._functions: dict[str, AIPFunction] = {}
        self._object_action_map: dict[str, set[str]] = {}
        self._category_functions: dict[FunctionCategory, set[str]] = {}
        self._initialized = False

    def register(self, func: AIPFunction) -> None:
        """注册一个 AIP Function"""
        self._functions[func.name] = func

        if func.object_type:
            if func.object_type not in self._object_action_map:
                self._object_action_map[func.object_type] = set()
            self._object_action_map[func.object_type].add(func.action_name)

        if func.category not in self._category_functions:
            self._category_functions[func.category] = set()
        self._category_functions[func.category].add(func.name)

        logger.debug(f"Registered AIP Function: {func.name} ({func.category.value})")

    def register_batch(self, functions: list[AIPFunction]) -> None:
        """批量注册 Functions"""
        for func in functions:
            self.register(func)

    def get(self, name: str) -> AIPFunction | None:
        """获取指定的 Function"""
        return self._functions.get(name)

    def get_for_object(self, object_type: str, category: FunctionCategory | None = None) -> list[AIPFunction]:
        """获取指定对象类型的所有 Functions"""
        action_names = self._object_action_map.get(object_type, set())
        result = []

        for action_name in action_names:
            for func in self._functions.values():
                if (
                    func.action_name == action_name
                    and func.object_type == object_type
                    and (category is None or func.category == category)
                ):
                    result.append(func)

        return result

    def get_by_category(self, category: FunctionCategory) -> list[AIPFunction]:
        """获取指定类别的所有 Functions"""
        names = self._category_functions.get(category, set())
        return [self._functions[name] for name in names if name in self._functions]

    def get_all(self) -> list[AIPFunction]:
        """获取所有已注册的 Functions"""
        return list(self._functions.values())

    def get_tool_schemas(
        self,
        user_id: str,
        user_roles: list[str],
        object_types: list[str] | None = None,
        categories: list[FunctionCategory] | None = None,
    ) -> list[dict[str, Any]]:
        """生成用户可用的 OpenAI 工具 Schema 列表"""
        schemas = []

        for func in self._functions.values():
            if object_types and func.object_type not in object_types:
                continue
            if categories and func.category not in categories:
                continue

            if self._check_permission(func, user_id, user_roles):
                schemas.append(func.to_openai_schema())

        return schemas

    def get_function_defs(
        self, user_id: str, user_roles: list[str], object_types: list[str] | None = None
    ) -> list[dict[str, Any]]:
        """生成用户可用的完整函数定义列表"""
        defs = []

        for func in self._functions.values():
            if object_types and func.object_type not in object_types:
                continue

            if self._check_permission(func, user_id, user_roles):
                defs.append(func.to_function_def())

        return defs

    def _check_permission(self, func: AIPFunction, user_id: str, user_roles: list[str]) -> bool:
        """检查用户是否有权限使用该 Function"""
        if not func.permission_required:
            return True

        normalized_roles = [str(role).strip().lower() for role in user_roles]
        if "admin" in normalized_roles:
            return True

        role_set = {role.lower() for role in normalized_roles}
        required_roles = {r.strip().lower() for r in func.permission_required.split(",") if r.strip()}

        return bool(role_set & required_roles)

    def resolve_action(self, function_name: str, parameters: dict[str, Any]) -> dict[str, Any]:
        """解析函数调用到具体的 Ontology Action"""
        func = self._functions.get(function_name)
        if not func:
            raise ValueError(f"Unknown function: {function_name}")

        object_id = parameters.get("object_id") or parameters.get(f"{func.object_type.lower()}_id")

        return {
            "function_name": function_name,
            "object_type": func.object_type,
            "action_name": func.action_name,
            "object_id": object_id,
            "parameters": parameters,
            "requires_approval": func.requires_approval,
            "risk_level": func.risk_level,
            "category": func.category.value,
        }

    def initialize_from_ontology(self) -> None:
        """从 Ontology Schema 初始化 Function 注册表"""
        from ..ontology.schema import get_ontology_registry

        ontology = get_ontology_registry()
        default_schema = ontology.get_schema("default")

        if not default_schema:
            logger.warning("Default ontology schema not found")
            return

        for _action_name, action_def in default_schema.actions.items():
            risk_level = RiskLevel(action_def.risk_level.upper())
            requires_approval = action_def.requires_approval

            func = AIPFunction(
                name=f"{action_def.object_type}.{action_def.name}",
                category=FunctionCategory.OBJECT_ACTION,
                object_type=action_def.object_type,
                action_name=action_def.name,
                description=action_def.description,
                parameters_schema={
                    "properties": {
                        p.name: {
                            "type": p.type,
                            "description": p.description,
                            **({"enum": p.enum_values} if p.enum_values else {}),
                        }
                        for p in action_def.parameters
                    },
                    "required": [p.name for p in action_def.parameters if p.required],
                },
                requires_approval=requires_approval,
                risk_level=risk_level,
            )

            self.register(func)

        self._initialized = True
        logger.info(f"Initialized {len(self._functions)} functions from ontology")

    @property
    def is_initialized(self) -> bool:
        return self._initialized


_aip_registry: AIPFunctionRegistry | None = None


def get_aip_registry() -> AIPFunctionRegistry:
    """获取全局 AIP Function 注册表"""
    global _aip_registry
    if _aip_registry is None:
        _aip_registry = AIPFunctionRegistry()
        _aip_registry.initialize_from_ontology()
    return _aip_registry


def set_aip_registry(registry: AIPFunctionRegistry) -> None:
    """设置全局 AIP Function 注册表"""
    global _aip_registry
    _aip_registry = registry


__all__ = [
    "AIPFunction",
    "AIPFunctionRegistry",
    "ApprovalDecision",
    "FunctionCategory",
    "RiskLevel",
    "get_aip_registry",
    "set_aip_registry",
]
