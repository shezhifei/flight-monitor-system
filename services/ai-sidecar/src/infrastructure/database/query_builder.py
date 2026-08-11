"""SQL query builder utilities."""

import re
from enum import Enum
from typing import Any


class JoinType(Enum):
    """连接类型枚举"""

    INNER = "INNER JOIN"
    LEFT = "LEFT JOIN"
    RIGHT = "RIGHT JOIN"
    FULL = "FULL OUTER JOIN"
    CROSS = "CROSS JOIN"


class OrderDirection(Enum):
    """排序方向枚举"""

    ASC = "ASC"
    DESC = "DESC"


class ComparisonOperator(Enum):
    """比较操作符枚举"""

    EQ = "="
    NE = "!="
    GT = ">"
    GE = ">="
    GTE = ">="
    LT = "<"
    LE = "<="
    LTE = "<="
    LIKE = "LIKE"
    ILIKE = "ILIKE"
    IN = "IN"
    NOT_IN = "NOT IN"
    IS_NULL = "IS NULL"
    IS_NOT_NULL = "IS NOT NULL"
    BETWEEN = "BETWEEN"
    EXISTS = "EXISTS"
    NOT_EXISTS = "NOT EXISTS"


class LogicalOperator(Enum):
    """逻辑操作符枚举"""

    AND = "AND"
    OR = "OR"
    NOT = "NOT"


class QueryBuilder:
    """Composable SELECT query builder."""

    def __init__(self):
        self._select_fields: list[str] = []
        self._from_table: str | None = None
        self._joins: list[dict[str, Any]] = []
        self._where_conditions: list[dict[str, Any]] = []
        self._group_by_fields: list[str] = []
        self._having_conditions: list[dict[str, Any]] = []
        self._order_by_fields: list[dict[str, Any]] = []
        self._limit_count: int | None = None
        self._offset_count: int | None = None
        self._distinct: bool = False

    def select(self, *fields: str) -> "QueryBuilder":
        """
        设置查询字段

        Args:
            *fields: 要查询的字段列表

        Returns:
            QueryBuilder: 查询构建器实例
        """
        for field in fields:
            if not self._is_valid_select_field(field):
                raise ValueError(f"Invalid select field: {field}")
        self._select_fields.extend(fields)
        return self

    def select_raw(self, expression: str) -> "QueryBuilder":
        """
        添加受控原始 SELECT 表达式。

        仅用于代码内白名单表达式；用户输入必须继续使用 select() 标识符路径。
        """
        if not isinstance(expression, str) or not expression.strip():
            raise ValueError("Raw SELECT expression cannot be empty")
        if not self._is_safe_raw_select_expression(expression):
            raise ValueError("Unsafe raw SELECT expression")
        self._select_fields.append(expression.strip())
        return self

    def select_all(self) -> "QueryBuilder":
        """查询所有字段"""
        self._select_fields = ["*"]
        return self

    def distinct(self) -> "QueryBuilder":
        """设置去重"""
        self._distinct = True
        return self

    def from_table(self, table: str) -> "QueryBuilder":
        """
        设置查询表

        Args:
            table: 表名

        Returns:
            QueryBuilder: 查询构建器实例
        """
        if not self._is_valid_identifier(table):
            raise ValueError(f"Invalid table name: {table}")
        self._from_table = table
        return self

    def join(self, table: str, on_condition: str, join_type: JoinType = JoinType.INNER) -> "QueryBuilder":
        """
        添加连接

        Args:
            table: 连接的表名
            on_condition: 连接条件
            join_type: 连接类型

        Returns:
            QueryBuilder: 查询构建器实例
        """
        if not self._is_valid_identifier(table):
            raise ValueError(f"Invalid table name: {table}")

        self._joins.append({"type": join_type, "table": table, "condition": on_condition})
        # 简单的安全检查：防止危险字符进入JOIN条件
        # 注意：这只是一个基础防护，复杂的ON条件应考虑重构为参数化方式
        dangerous = [";", "--", "/*", "*/", "xp_"]
        for char in dangerous:
            if char in on_condition:
                raise ValueError(f"Dangerous character '{char}' detected in JOIN condition")

        return self

    def where(self, field: str, operator: ComparisonOperator, value: Any = None) -> "QueryBuilder":
        """
        添加WHERE条件

        Args:
            field: 字段名
            operator: 比较操作符
            value: 比较值

        Returns:
            QueryBuilder: 查询构建器实例
        """
        if not self._is_valid_identifier(field):
            raise ValueError(f"Invalid field name: {field}")

        condition = {"field": field, "operator": operator, "value": value, "logical_op": LogicalOperator.AND}
        self._where_conditions.append(condition)
        return self

    def where_and(self, field: str, operator: ComparisonOperator, value: Any = None) -> "QueryBuilder":
        """添加AND条件的便捷方法"""
        return self.where(field, operator, value)

    def where_or(self, field: str, operator: ComparisonOperator, value: Any = None) -> "QueryBuilder":
        """添加OR条件"""
        if not self._is_valid_identifier(field):
            raise ValueError(f"Invalid field name: {field}")

        condition = {"field": field, "operator": operator, "value": value, "logical_op": LogicalOperator.OR}
        self._where_conditions.append(condition)
        return self

    def where_in(self, field: str, values: list[Any]) -> "QueryBuilder":
        """添加IN条件"""
        if not values:
            # 空列表时使用不可能的条件
            return self.where(field, ComparisonOperator.EQ, None)
        return self.where(field, ComparisonOperator.IN, values)

    def where_not_in(self, field: str, values: list[Any]) -> "QueryBuilder":
        """添加NOT IN条件"""
        if not values:
            # 空列表时跳过条件
            return self
        return self.where(field, ComparisonOperator.NOT_IN, values)

    def where_between(self, field: str, start_value: Any, end_value: Any) -> "QueryBuilder":
        """添加BETWEEN条件"""
        return self.where(field, ComparisonOperator.BETWEEN, [start_value, end_value])

    def where_null(self, field: str) -> "QueryBuilder":
        """添加IS NULL条件"""
        return self.where(field, ComparisonOperator.IS_NULL)

    def where_not_null(self, field: str) -> "QueryBuilder":
        """添加IS NOT NULL条件"""
        return self.where(field, ComparisonOperator.IS_NOT_NULL)

    def where_like(self, field: str, pattern: str) -> "QueryBuilder":
        """添加LIKE条件"""
        return self.where(field, ComparisonOperator.LIKE, pattern)

    def where_raw(
        self, condition_sql: str, params: list[Any] | None = None, logical_op: LogicalOperator = LogicalOperator.AND
    ) -> "QueryBuilder":
        """
        添加原始 WHERE 条件（需使用参数化占位符 %s）

        Args:
            condition_sql: 原始条件 SQL 片段
            params: 参数列表
            logical_op: 与前一条件的逻辑关系（默认 AND）

        Returns:
            QueryBuilder: 查询构建器实例
        """
        if not isinstance(condition_sql, str) or not condition_sql.strip():
            raise ValueError("Raw WHERE condition cannot be empty")
        if not self._is_safe_raw_condition(condition_sql):
            raise ValueError("Unsafe raw WHERE condition")

        normalized_params = params or []
        if not isinstance(normalized_params, list):
            raise ValueError("Raw WHERE params must be a list")

        placeholder_count = condition_sql.count("%s")
        if placeholder_count != len(normalized_params):
            raise ValueError(
                f"Raw WHERE placeholder count ({placeholder_count}) does not match "
                f"params count ({len(normalized_params)})"
            )

        self._where_conditions.append(
            {
                "raw_sql": condition_sql.strip(),
                "params": normalized_params,
                "logical_op": logical_op,
            }
        )
        return self

    def group_by(self, *fields: str) -> "QueryBuilder":
        """
        添加GROUP BY字段

        Args:
            *fields: 分组字段列表

        Returns:
            QueryBuilder: 查询构建器实例
        """
        for field in fields:
            if not self._is_valid_identifier(field):
                raise ValueError(f"Invalid field name: {field}")
        self._group_by_fields.extend(fields)
        return self

    def having(self, field: str, operator: ComparisonOperator, value: Any = None) -> "QueryBuilder":
        """
        添加HAVING条件

        Args:
            field: 字段名
            operator: 比较操作符
            value: 比较值

        Returns:
            QueryBuilder: 查询构建器实例
        """
        if not self._is_valid_identifier(field):
            raise ValueError(f"Invalid field name: {field}")

        condition = {"field": field, "operator": operator, "value": value, "logical_op": LogicalOperator.AND}
        self._having_conditions.append(condition)
        return self

    def order_by(self, field: str, direction: OrderDirection = OrderDirection.ASC) -> "QueryBuilder":
        """
        添加排序字段

        Args:
            field: 排序字段
            direction: 排序方向

        Returns:
            QueryBuilder: 查询构建器实例
        """
        if not self._is_valid_identifier(field):
            raise ValueError(f"Invalid field name: {field}")

        self._order_by_fields.append({"field": field, "direction": direction})
        return self

    def limit(self, count: int) -> "QueryBuilder":
        """
        设置限制数量

        Args:
            count: 限制数量

        Returns:
            QueryBuilder: 查询构建器实例
        """
        if count <= 0:
            raise ValueError("Limit count must be positive")
        self._limit_count = count
        return self

    def offset(self, count: int) -> "QueryBuilder":
        """
        设置偏移数量

        Args:
            count: 偏移数量

        Returns:
            QueryBuilder: 查询构建器实例
        """
        if count < 0:
            raise ValueError("Offset count cannot be negative")
        self._offset_count = count
        return self

    def build(self) -> tuple[str, list[Any]]:
        """
        构建SQL查询和参数列表

        Returns:
            Tuple[str, List[Any]]: SQL查询字符串和参数列表
        """
        if not self._from_table:
            raise ValueError("FROM table is required")

        query_parts = []
        parameters = []

        # SELECT部分
        select_clause = self._build_select_clause()
        query_parts.append(select_clause)

        # FROM部分
        query_parts.append(f"FROM {self._from_table}")

        # JOIN部分
        join_clause = self._build_join_clause()
        if join_clause:
            query_parts.append(join_clause)

        # WHERE部分
        where_clause, where_params = self._build_where_clause()
        if where_clause:
            query_parts.append(f"WHERE {where_clause}")
            parameters.extend(where_params)

        # GROUP BY部分
        group_by_clause = self._build_group_by_clause()
        if group_by_clause:
            query_parts.append(group_by_clause)

        # HAVING部分
        having_clause, having_params = self._build_having_clause()
        if having_clause:
            query_parts.append(f"HAVING {having_clause}")
            parameters.extend(having_params)

        # ORDER BY部分
        order_by_clause = self._build_order_by_clause()
        if order_by_clause:
            query_parts.append(order_by_clause)

        # LIMIT和OFFSET部分
        limit_offset_clause = self._build_limit_offset_clause()
        if limit_offset_clause:
            query_parts.append(limit_offset_clause)
            if self._limit_count:
                parameters.append(self._limit_count)
            if self._offset_count:
                parameters.append(self._offset_count)

        query = " ".join(query_parts)
        return query, parameters

    def _build_select_clause(self) -> str:
        """构建SELECT子句"""
        if not self._select_fields:
            fields = "*"
        else:
            fields = ", ".join(self._select_fields)

        distinct = "DISTINCT " if self._distinct else ""
        return f"SELECT {distinct}{fields}"

    def _build_join_clause(self) -> str:
        """构建JOIN子句"""
        if not self._joins:
            return ""

        join_clauses = []
        for join in self._joins:
            join_clause = f"{join['type'].value} {join['table']} ON {join['condition']}"
            join_clauses.append(join_clause)

        return " ".join(join_clauses)

    def _build_where_clause(self) -> tuple[str, list[Any]]:
        """构建WHERE子句"""
        if not self._where_conditions:
            return "", []

        conditions = []
        parameters = []

        for i, condition in enumerate(self._where_conditions):
            # 添加逻辑操作符（除了第一个条件）
            if i > 0:
                conditions.append(condition["logical_op"].value)

            # 构建条件
            condition_sql, condition_params = self._build_condition(condition)
            conditions.append(f"({condition_sql})")
            parameters.extend(condition_params)

        return " ".join(conditions), parameters

    def _build_having_clause(self) -> tuple[str, list[Any]]:
        """构建HAVING子句"""
        if not self._having_conditions:
            return "", []

        conditions = []
        parameters = []

        for i, condition in enumerate(self._having_conditions):
            # 添加逻辑操作符（除了第一个条件）
            if i > 0:
                conditions.append(condition["logical_op"].value)

            # 构建条件
            condition_sql, condition_params = self._build_condition(condition)
            conditions.append(f"({condition_sql})")
            parameters.extend(condition_params)

        return " ".join(conditions), parameters

    def _build_condition(self, condition: dict[str, Any]) -> tuple[str, list[Any]]:
        """构建单个条件"""
        if "raw_sql" in condition:
            return condition["raw_sql"], condition.get("params", [])

        field = condition["field"]
        operator = condition["operator"]
        value = condition["value"]

        if operator in [ComparisonOperator.IS_NULL, ComparisonOperator.IS_NOT_NULL]:
            return f"{field} {operator.value}", []

        if operator == ComparisonOperator.IN:
            if not isinstance(value, list):
                raise ValueError("IN operator requires a list of values")
            placeholders = ", ".join(["%s" for _ in value])  # PostgreSQL使用%s而不是?
            return f"{field} {operator.value} ({placeholders})", value

        if operator == ComparisonOperator.NOT_IN:
            if not isinstance(value, list):
                raise ValueError("NOT IN operator requires a list of values")
            placeholders = ", ".join(["%s" for _ in value])  # PostgreSQL使用%s而不是?
            return f"{field} {operator.value} ({placeholders})", value

        if operator == ComparisonOperator.BETWEEN:
            if not isinstance(value, list) or len(value) != 2:
                raise ValueError("BETWEEN operator requires a list of two values")
            return f"{field} {operator.value} %s AND %s", value  # PostgreSQL使用%s而不是?

        # 对于其他操作符，使用单个参数
        return f"{field} {operator.value} %s", [value]  # PostgreSQL使用%s而不是?

    def _build_group_by_clause(self) -> str:
        """构建GROUP BY子句"""
        if not self._group_by_fields:
            return ""

        return f"GROUP BY {', '.join(self._group_by_fields)}"

    def _build_order_by_clause(self) -> str:
        """构建ORDER BY子句"""
        if not self._order_by_fields:
            return ""

        order_clauses = []
        for order in self._order_by_fields:
            order_clause = f"{order['field']} {order['direction'].value}"
            order_clauses.append(order_clause)

        return f"ORDER BY {', '.join(order_clauses)}"

    def _build_limit_offset_clause(self) -> str:
        """构建LIMIT和OFFSET子句"""
        clauses = []

        if self._limit_count:
            clauses.append("LIMIT %s")  # PostgreSQL使用%s而不是?

        if self._offset_count:
            clauses.append("OFFSET %s")  # PostgreSQL使用%s而不是?

        return " ".join(clauses)

    @staticmethod
    def _is_valid_identifier(identifier: str) -> bool:
        """
        验证SQL标识符是否有效

        Args:
            identifier: 要验证的标识符

        Returns:
            bool: 是否有效
        """
        if not identifier or not isinstance(identifier, str):
            return False

        # 检查是否包含危险字符
        dangerous_chars = [";", "--", "/*", "*/", "xp_", "sp_"]
        for dangerous in dangerous_chars:
            if dangerous in identifier.lower():
                return False

        # 检查是否为有效的标识符格式
        # 允许字母、数字、下划线，且不以数字开头
        pattern = r"^[a-zA-Z_][a-zA-Z0-9_]*(\.[a-zA-Z_][a-zA-Z0-9_]*)*$"
        return bool(re.match(pattern, identifier))

    @classmethod
    def _is_valid_select_field(cls, field: str) -> bool:
        if field == "*":
            return True
        return cls._is_valid_identifier(field)

    @staticmethod
    def _is_safe_raw_select_expression(expression: str) -> bool:
        """对显式原始 SELECT 表达式做基础安全检查。"""
        lowered = expression.lower()
        dangerous_tokens = [";", "--", "/*", "*/", " xp_", " sp_"]
        if any(token in lowered for token in dangerous_tokens):
            return False
        if re.search(r"\b(from|where|join|union|select|insert|update|delete|drop|alter)\b", lowered):
            return False
        allowed = re.compile(
            r"^[a-zA-Z0-9_.,\s()*+\-/]+(\s+AS\s+[a-zA-Z_][a-zA-Z0-9_]*)?$",
            re.IGNORECASE,
        )
        return bool(allowed.match(expression.strip()))

    @staticmethod
    def _is_safe_raw_condition(condition_sql: str) -> bool:
        """对原始条件进行基础安全检查。"""
        lowered = condition_sql.lower()
        dangerous_tokens = [";", "--", "/*", "*/"]
        return not any(token in lowered for token in dangerous_tokens)


class DeleteBuilder:
    """DELETE查询构建器"""

    def __init__(self):
        self._table: str | None = None
        self._where_conditions: list[dict[str, Any]] = []

    def from_table(self, table: str) -> "DeleteBuilder":
        """设置删除的表"""
        if not QueryBuilder._is_valid_identifier(table):
            raise ValueError(f"Invalid table name: {table}")
        self._table = table
        return self

    def where(self, field: str, operator: ComparisonOperator, value: Any = None) -> "DeleteBuilder":
        """添加WHERE条件"""
        if not QueryBuilder._is_valid_identifier(field):
            raise ValueError(f"Invalid field name: {field}")

        condition = {"field": field, "operator": operator, "value": value, "logical_op": LogicalOperator.AND}
        self._where_conditions.append(condition)
        return self

    def where_not_in(self, field: str, values: list[Any]) -> "DeleteBuilder":
        """添加NOT IN条件"""
        if not values:
            return self
        return self.where(field, ComparisonOperator.NOT_IN, values)

    def build(self) -> tuple[str, list[Any]]:
        """构建DELETE查询"""
        if not self._table:
            raise ValueError("FROM table is required")

        query_parts = [f"DELETE FROM {self._table}"]
        parameters = []

        # WHERE部分
        if self._where_conditions:
            where_clause, where_params = self._build_where_clause()
            query_parts.append(f"WHERE {where_clause}")
            parameters.extend(where_params)

        query = " ".join(query_parts)
        return query, parameters

    def _build_where_clause(self) -> tuple[str, list[Any]]:
        """构建WHERE子句"""
        if not self._where_conditions:
            return "", []

        conditions = []
        parameters = []

        for i, condition in enumerate(self._where_conditions):
            if i > 0:
                conditions.append(condition["logical_op"].value)

            condition_sql, condition_params = self._build_condition(condition)
            conditions.append(f"({condition_sql})")
            parameters.extend(condition_params)

        return " ".join(conditions), parameters

    def _build_condition(self, condition: dict[str, Any]) -> tuple[str, list[Any]]:
        """构建单个条件"""
        field = condition["field"]
        operator = condition["operator"]
        value = condition["value"]

        if operator in [ComparisonOperator.IS_NULL, ComparisonOperator.IS_NOT_NULL]:
            return f"{field} {operator.value}", []

        if operator == ComparisonOperator.NOT_IN:
            if not isinstance(value, list):
                raise ValueError("NOT IN operator requires a list of values")
            placeholders = ", ".join(["%s" for _ in value])  # PostgreSQL使用%s而不是?
            return f"{field} {operator.value} ({placeholders})", value

        return f"{field} {operator.value} %s", [value]  # PostgreSQL使用%s而不是?


class InsertBuilder:
    """INSERT查询构建器"""

    def __init__(self):
        self._table: str | None = None
        self._columns: list[str] = []
        self._values: list[list[Any]] = []
        self._on_conflict: str | None = None

    def into_table(self, table: str) -> "InsertBuilder":
        """设置插入的表"""
        if not QueryBuilder._is_valid_identifier(table):
            raise ValueError(f"Invalid table name: {table}")
        self._table = table
        return self

    def columns(self, *columns: str) -> "InsertBuilder":
        """设置插入的列"""
        for column in columns:
            if not QueryBuilder._is_valid_identifier(column):
                raise ValueError(f"Invalid column name: {column}")
        self._columns.extend(columns)
        return self

    def values(self, *values: Any) -> "InsertBuilder":
        """添加一行值"""
        if len(values) != len(self._columns):
            raise ValueError("Number of values must match number of columns")
        self._values.append(list(values))
        return self

    def values_batch(self, values_list: list[list[Any]]) -> "InsertBuilder":
        """批量添加值"""
        for values in values_list:
            if len(values) != len(self._columns):
                raise ValueError("Number of values must match number of columns")
        self._values.extend(values_list)
        return self

    def on_conflict_replace(self) -> "InsertBuilder":
        """设置冲突时替换"""
        self._on_conflict = "ON CONFLICT DO UPDATE"
        return self

    def on_conflict_ignore(self) -> "InsertBuilder":
        """设置冲突时忽略"""
        self._on_conflict = "ON CONFLICT DO NOTHING"
        return self

    def on_conflict_update(self) -> "InsertBuilder":
        """设置冲突时更新（PostgreSQL语法）"""
        self._on_conflict = "ON CONFLICT DO UPDATE"
        return self

    def build(self) -> tuple[str, list[Any]]:
        """构建INSERT查询"""
        if not self._table:
            raise ValueError("INTO table is required")
        if not self._columns:
            raise ValueError("Columns are required")
        if not self._values:
            raise ValueError("Values are required")

        # 构建列部分
        columns_str = ", ".join(self._columns)

        # 构建值部分
        placeholders = ", ".join(["%s" for _ in self._columns])  # PostgreSQL使用%s而不是?
        values_str = ", ".join([f"({placeholders})" for _ in self._values])

        # 构建查询
        if self._on_conflict:
            if self._on_conflict == "ON CONFLICT DO UPDATE":
                # PostgreSQL的ON CONFLICT DO UPDATE语法
                # 假设第一列是主键
                primary_key = self._columns[0] if self._columns else "id"
                update_clauses = []
                for col in self._columns:
                    if col != primary_key:
                        update_clauses.append(f"{col} = EXCLUDED.{col}")

                update_clause = (
                    ", ".join(update_clauses) if update_clauses else f"{primary_key} = EXCLUDED.{primary_key}"
                )
                query = f"INSERT INTO {self._table} ({columns_str}) VALUES {values_str} ON CONFLICT ({primary_key}) DO UPDATE SET {update_clause}"
            elif self._on_conflict == "ON CONFLICT DO NOTHING":
                # PostgreSQL的ON CONFLICT DO NOTHING语法
                primary_key = self._columns[0] if self._columns else "id"
                query = f"INSERT INTO {self._table} ({columns_str}) VALUES {values_str} ON CONFLICT ({primary_key}) DO NOTHING"
            else:
                # 其他情况
                query = f"INSERT {self._on_conflict} INTO {self._table} ({columns_str}) VALUES {values_str}"
        else:
            query = f"INSERT INTO {self._table} ({columns_str}) VALUES {values_str}"

        # 展平所有值
        parameters = []
        for value_row in self._values:
            parameters.extend(value_row)

        return query, parameters


class UpdateBuilder:
    """UPDATE查询构建器"""

    def __init__(self):
        self._table: str | None = None
        self._set_clauses: list[dict[str, Any]] = []
        self._where_conditions: list[dict[str, Any]] = []

    def table(self, table: str) -> "UpdateBuilder":
        """设置更新的表"""
        if not QueryBuilder._is_valid_identifier(table):
            raise ValueError(f"Invalid table name: {table}")
        self._table = table
        return self

    def set(self, field: str, value: Any) -> "UpdateBuilder":
        """设置更新字段"""
        if not QueryBuilder._is_valid_identifier(field):
            raise ValueError(f"Invalid field name: {field}")
        self._set_clauses.append({"field": field, "value": value})
        return self

    def where(self, field: str, operator: ComparisonOperator, value: Any = None) -> "UpdateBuilder":
        """添加WHERE条件"""
        if not QueryBuilder._is_valid_identifier(field):
            raise ValueError(f"Invalid field name: {field}")

        condition = {"field": field, "operator": operator, "value": value, "logical_op": LogicalOperator.AND}
        self._where_conditions.append(condition)
        return self

    def build(self) -> tuple[str, list[Any]]:
        """构建UPDATE查询"""
        if not self._table:
            raise ValueError("Table is required")
        if not self._set_clauses:
            raise ValueError("SET clauses are required")

        query_parts = [f"UPDATE {self._table}"]
        parameters = []

        # SET部分
        set_clauses = []
        for clause in self._set_clauses:
            set_clauses.append(f"{clause['field']} = %s")  # PostgreSQL使用%s而不是?
            parameters.append(clause["value"])
        query_parts.append(f"SET {', '.join(set_clauses)}")

        # WHERE部分
        if self._where_conditions:
            where_clause, where_params = self._build_where_clause()
            query_parts.append(f"WHERE {where_clause}")
            parameters.extend(where_params)

        query = " ".join(query_parts)
        return query, parameters

    def _build_where_clause(self) -> tuple[str, list[Any]]:
        """构建WHERE子句"""
        if not self._where_conditions:
            return "", []

        conditions = []
        parameters = []

        for i, condition in enumerate(self._where_conditions):
            if i > 0:
                conditions.append(condition["logical_op"].value)

            condition_sql, condition_params = self._build_condition(condition)
            conditions.append(f"({condition_sql})")
            parameters.extend(condition_params)

        return " ".join(conditions), parameters

    def _build_condition(self, condition: dict[str, Any]) -> tuple[str, list[Any]]:
        """构建单个条件"""
        field = condition["field"]
        operator = condition["operator"]
        value = condition["value"]

        if operator in [ComparisonOperator.IS_NULL, ComparisonOperator.IS_NOT_NULL]:
            return f"{field} {operator.value}", []

        return f"{field} {operator.value} %s", [value]  # PostgreSQL使用%s而不是?


def select(*fields: str) -> QueryBuilder:
    """Create a SELECT query builder."""
    return QueryBuilder().select(*fields)


def delete() -> DeleteBuilder:
    """Create a DELETE query builder."""
    return DeleteBuilder()


def insert() -> InsertBuilder:
    """Create an INSERT query builder."""
    return InsertBuilder()


def update() -> UpdateBuilder:
    """Create an UPDATE query builder."""
    return UpdateBuilder()
