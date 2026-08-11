"""
Ontology 查询引擎

提供基于对象关系图的语义查询能力：
- 跨对象关联查询 (Flight → Team → Equipment)
- 基于约束的推理 (找所有可用的大机位)
- 路径查询 (查询某对象的完整上下文)

使用方式:
    from src.infrastructure.ai.ontology.query_engine import OntologyQueryEngine

    engine = OntologyQueryEngine()
    results = await engine.query("available_large_stands")
"""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass, field
from enum import StrEnum
from typing import Any

from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class QueryType(StrEnum):
    """查询类型"""

    OBJECT = "object"
    RELATIONSHIP = "relationship"
    PATH = "path"
    CONSTRAINT = "constraint"
    AGGREGATION = "aggregation"


@dataclass
class QueryContext:
    """查询上下文"""

    query_type: QueryType
    object_types: list[str] = field(default_factory=list)
    filters: dict[str, Any] = field(default_factory=dict)
    relationships: list[str] = field(default_factory=list)
    depth: int = 1
    limit: int = 100


@dataclass
class QueryResult:
    """查询结果"""

    success: bool
    query_type: QueryType
    results: list[dict[str, Any]] = field(default_factory=list)
    count: int = 0
    execution_time_ms: float = 0.0
    errors: list[str] = field(default_factory=list)
    metadata: dict[str, Any] = field(default_factory=dict)


class OntologyQueryEngine:
    """
    Ontology 查询引擎

    提供语义查询能力：
    - 对象查询
    - 关系遍历
    - 路径解析
    - 约束推理
    """

    def __init__(self, ontology_registry=None, data_accessor=None):
        self._ontology = ontology_registry
        self._data_accessor = data_accessor
        self._query_cache: dict[str, QueryResult] = {}
        self._cache_ttl = 60

    def set_ontology_registry(self, registry) -> None:
        """设置 Ontology 注册表"""
        self._ontology = registry

    def set_data_accessor(self, accessor) -> None:
        """设置数据访问器"""
        self._data_accessor = accessor

    async def execute(self, query: QueryContext) -> QueryResult:
        """
        执行查询

        Args:
            query: 查询上下文

        Returns:
            查询结果
        """
        import time

        start_time = time.time()

        try:
            if query.query_type == QueryType.OBJECT:
                results = await self._query_objects(query)
            elif query.query_type == QueryType.RELATIONSHIP:
                results = await self._query_relationships(query)
            elif query.query_type == QueryType.PATH:
                results = await self._query_paths(query)
            elif query.query_type == QueryType.CONSTRAINT:
                results = await self._query_with_constraints(query)
            elif query.query_type == QueryType.AGGREGATION:
                results = await self._query_aggregation(query)
            else:
                results = []

            execution_time = (time.time() - start_time) * 1000

            return QueryResult(
                success=True,
                query_type=query.query_type,
                results=results[: query.limit],
                count=len(results),
                execution_time_ms=execution_time,
            )

        except Exception as exc:  # noqa: BLE001 - query execution may fail in various ways
            logger.error(f"Query execution failed: {exc}")
            return QueryResult(
                success=False,
                query_type=query.query_type,
                errors=[str(exc)],
                execution_time_ms=(time.time() - start_time) * 1000,
            )

    async def _query_objects(self, query: QueryContext) -> list[dict[str, Any]]:
        """查询对象"""
        if not self._data_accessor:
            return []

        results = []
        for object_type in query.object_types:
            if query.filters:
                objects = await self._data_accessor.get_objects_by_query(object_type, query.filters)
                results.extend(objects)
            else:
                state = await self._data_accessor.get_object_state(object_type, query.filters.get("id", ""))
                if state:
                    results.append(state)

        return results

    async def _query_relationships(self, query: QueryContext) -> list[dict[str, Any]]:
        """查询关系"""
        if not self._ontology or not self._data_accessor:
            return []

        results = []

        for object_type in query.object_types:
            schema = self._ontology.get_object(object_type, "default")
            if not schema:
                continue

            for rel_name in query.relationships:
                rel = schema.get_relationship(rel_name)
                if not rel:
                    continue

                related_schema = self._ontology.get_object(rel.target_object, "default")
                if not related_schema:
                    continue

                if query.filters:
                    related_objects = await self._data_accessor.get_objects_by_query(rel.target_object, query.filters)
                    for obj in related_objects:
                        results.append(
                            {
                                "source_type": object_type,
                                "source_id": query.filters.get("id"),
                                "relationship": rel_name,
                                "target_type": rel.target_object,
                                "target": obj,
                            }
                        )

        return results

    async def _query_paths(self, query: QueryContext) -> list[dict[str, Any]]:
        """查询路径"""
        if not self._ontology or not self._data_accessor:
            return []

        results = []

        for object_type in query.object_types:
            path_results = await self._resolve_path(
                start_type=object_type, start_id=query.filters.get("id"), path=query.relationships, depth=query.depth
            )
            results.extend(path_results)

        return results

    async def _resolve_path(self, start_type: str, start_id: str, path: list[str], depth: int) -> list[dict[str, Any]]:
        """解析路径"""
        results = []

        current_type = start_type
        current_id = start_id
        visited: set[tuple[str, str]] = set()

        for rel_name in path:
            if depth <= 0:
                break

            schema = self._ontology.get_object(current_type, "default")
            if not schema:
                break

            rel = schema.get_relationship(rel_name)
            if not rel:
                break

            current_state = await self._data_accessor.get_object_state(current_type, current_id)
            if not current_state:
                break

            related_id = current_state.get(rel_name)
            if not related_id:
                continue

            if isinstance(related_id, list):
                for rid in related_id[:1]:
                    if (rel.target_object, rid) not in visited:
                        related_state = await self._data_accessor.get_object_state(rel.target_object, rid)
                        if related_state:
                            results.append(
                                {
                                    "path_segment": f"{current_type}.{rel_name} → {rel.target_object}",
                                    "from_id": current_id,
                                    "to_id": rid,
                                    "to_state": related_state,
                                }
                            )
                            visited.add((rel.target_object, rid))
                            current_type = rel.target_object
                            current_id = rid
            else:
                if (rel.target_object, related_id) not in visited:
                    related_state = await self._data_accessor.get_object_state(rel.target_object, related_id)
                    if related_state:
                        results.append(
                            {
                                "path_segment": f"{current_type}.{rel_name} → {rel.target_object}",
                                "from_id": current_id,
                                "to_id": related_id,
                                "to_state": related_state,
                            }
                        )
                        visited.add((rel.target_object, related_id))
                        current_type = rel.target_object
                        current_id = related_id

            depth -= 1

        return results

    async def _query_with_constraints(self, query: QueryContext) -> list[dict[str, Any]]:
        """带约束的查询"""
        if not self._data_accessor:
            return []

        all_results = []
        for object_type in query.object_types:
            objects = await self._data_accessor.get_objects_by_query(object_type, {})

            filtered = []
            for obj in objects:
                if self._check_constraints(obj, query.filters):
                    filtered.append(obj)

            all_results.extend(filtered)

        return all_results

    def _check_constraints(self, obj: dict[str, Any], constraints: dict[str, Any]) -> bool:
        """检查约束"""
        for key, expected in constraints.items():
            if key.startswith("not_"):
                actual_key = key[4:]
                if actual_key in obj and obj[actual_key] == expected:
                    return False
            elif key.startswith("min_"):
                actual_key = key[4:]
                if actual_key not in obj or obj[actual_key] < expected:
                    return False
            elif key.startswith("max_"):
                actual_key = key[4:]
                if actual_key not in obj or obj[actual_key] > expected:
                    return False
            else:
                if key in obj and obj[key] != expected:
                    return False

        return True

    async def _query_aggregation(self, query: QueryContext) -> list[dict[str, Any]]:
        """聚合查询"""
        if not self._data_accessor:
            return []

        aggregations = []

        for object_type in query.object_types:
            filters = query.filters.copy()
            agg_field = filters.pop("agg_field", None)
            agg_func = filters.pop("agg_func", "count")

            objects = await self._data_accessor.get_objects_by_query(object_type, filters)

            if agg_func == "count":
                result = {"object_type": object_type, "count": len(objects)}
                aggregations.append(result)
            elif agg_func == "group_by" and agg_field:
                groups: dict[str, int] = {}
                for obj in objects:
                    key = str(obj.get(agg_field, "unknown"))
                    groups[key] = groups.get(key, 0) + 1
                aggregations.append({"object_type": object_type, "group_by": agg_field, "groups": groups})

        return aggregations

    def build_query(self, natural_language: str) -> QueryContext:
        """
        从自然语言构建查询

        Args:
            natural_language: 自然语言查询

        Returns:
            QueryContext
        """
        query = QueryContext(query_type=QueryType.OBJECT)

        nl_lower = natural_language.lower()

        if "可用" in natural_language or "available" in nl_lower:
            query.filters["status"] = "available"

        if "大" in natural_language or "large" in nl_lower:
            query.filters["size"] = "large"

        if "小" in natural_language or "small" in nl_lower:
            query.filters["size"] = "small"

        if "延误" in natural_language or "delay" in nl_lower:
            query.object_types = ["Flight"]
            query.filters["status"] = "delayed"

        if "班组" in natural_language or "team" in nl_lower:
            query.object_types.append("Team")

        if "机位" in natural_language or "stand" in nl_lower:
            query.object_types.append("Stand")

        if "航班" in natural_language or "flight" in nl_lower:
            query.object_types.append("Flight")

        if not query.object_types:
            query.object_types = ["Flight"]

        return query


class ConstraintChecker:
    """
    约束检查器

    在 Action 执行前验证业务约束。
    """

    def __init__(self, ontology_registry=None, data_accessor=None):
        self._ontology = ontology_registry
        self._data_accessor = data_accessor
        self._constraints: dict[str, list[Callable]] = {}
        self._load_default_constraints()

    def _load_default_constraints(self) -> None:
        """加载默认约束"""
        self._constraints = {
            "Flight.change_stand": [
                self._check_stand_capacity,
                self._check_stand_availability,
            ],
            "Flight.assign_team": [
                self._check_team_availability,
            ],
            "Stand.close": [
                self._check_no_active_flights,
            ],
        }

    async def check_constraints(
        self, object_type: str, action: str, parameters: dict[str, Any]
    ) -> tuple[bool, list[str]]:
        """
        检查约束

        Args:
            object_type: 对象类型
            action: Action 名称
            parameters: Action 参数

        Returns:
            (是否通过, 违反的约束列表)
        """
        violations = []

        key = f"{object_type}.{action}"
        constraints = self._constraints.get(key, [])

        for constraint in constraints:
            try:
                violation = await constraint(object_type, action, parameters)
                if violation:
                    violations.append(violation)
            except Exception as exc:  # noqa: BLE001 - constraint check must not break validation
                logger.warning(f"Constraint check failed: {exc}")

        return len(violations) == 0, violations

    async def _check_stand_capacity(self, object_type: str, action: str, parameters: dict[str, Any]) -> str | None:
        """检查机位容量"""
        if action != "change_stand" or not self._data_accessor:
            return None

        new_stand_id = parameters.get("new_stand")
        flight_id = parameters.get("flight_id")

        if not new_stand_id or not flight_id:
            return None

        flight_state = await self._data_accessor.get_object_state("Flight", flight_id)
        stand_state = await self._data_accessor.get_object_state("Stand", new_stand_id)

        if not flight_state or not stand_state:
            return None

        aircraft_type = flight_state.get("aircraft_type", "")
        stand_size = stand_state.get("size", "medium")

        size_hierarchy = {"small": 0, "medium": 1, "large": 2, "xlarge": 3}
        aircraft_size_map = {"B737": "medium", "A320": "medium", "B747": "large", "A380": "xlarge"}

        required_size = aircraft_size_map.get(aircraft_type, "medium")
        if size_hierarchy.get(stand_size, 0) < size_hierarchy.get(required_size, 0):
            return f"机位 {new_stand_id} (size: {stand_size}) 无法容纳机型 {aircraft_type}"

        return None

    async def _check_stand_availability(self, object_type: str, action: str, parameters: dict[str, Any]) -> str | None:
        """检查机位可用性"""
        if action != "change_stand" or not self._data_accessor:
            return None

        new_stand_id = parameters.get("new_stand")
        if not new_stand_id:
            return None

        stand_state = await self._data_accessor.get_object_state("Stand", new_stand_id)
        if not stand_state:
            return f"机位 {new_stand_id} 不存在"

        status = stand_state.get("status")
        if status != "available":
            return f"机位 {new_stand_id} 当前不可用 (status: {status})"

        return None

    async def _check_team_availability(self, object_type: str, action: str, parameters: dict[str, Any]) -> str | None:
        """检查班组可用性"""
        if action != "assign_team" or not self._data_accessor:
            return None

        team_id = parameters.get("team_id")
        if not team_id:
            return None

        team_state = await self._data_accessor.get_object_state("Team", team_id)
        if not team_state:
            return f"班组 {team_id} 不存在"

        status = team_state.get("status")
        if status != "on_duty":
            return f"班组 {team_id} 当前不在岗 (status: {status})"

        return None

    async def _check_no_active_flights(self, object_type: str, action: str, parameters: dict[str, Any]) -> str | None:
        """检查没有活跃航班"""
        if action != "close" or not self._data_accessor:
            return None

        stand_id = parameters.get("stand_id")
        if not stand_id:
            return None

        stand_state = await self._data_accessor.get_object_state("Stand", stand_id)
        if not stand_state:
            return None

        current_flight_id = stand_state.get("current_flight_id")
        if current_flight_id:
            return f"机位 {stand_id} 有航班 {current_flight_id} 停靠，无法关闭"

        return None

    def register_constraint(self, object_type: str, action: str, constraint: Callable) -> None:
        """注册自定义约束"""
        key = f"{object_type}.{action}"
        if key not in self._constraints:
            self._constraints[key] = []
        self._constraints[key].append(constraint)


_query_engine: OntologyQueryEngine | None = None
_constraint_checker: ConstraintChecker | None = None


def get_query_engine() -> OntologyQueryEngine:
    """获取全局查询引擎"""
    global _query_engine
    if _query_engine is None:
        _query_engine = OntologyQueryEngine()
    return _query_engine


def get_constraint_checker() -> ConstraintChecker:
    """获取全局约束检查器"""
    global _constraint_checker
    if _constraint_checker is None:
        _constraint_checker = ConstraintChecker()
    return _constraint_checker


__all__ = [
    "ConstraintChecker",
    "OntologyQueryEngine",
    "QueryContext",
    "QueryResult",
    "QueryType",
    "get_constraint_checker",
    "get_query_engine",
]
