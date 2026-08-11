"""Readonly SQL query tool definitions."""

from enum import StrEnum
from typing import Any

from .base import BaseToolDefinition, OperationLevel, ToolCategory, build_openai_tools


class SQLQueryToolName(StrEnum):
    SQL_QUERY_READONLY = "sql_query_readonly"


SQL_QUERY_TOOL_DEFINITIONS: list[BaseToolDefinition] = [
    BaseToolDefinition(
        name=SQLQueryToolName.SQL_QUERY_READONLY.value,
        description=(
            "执行只读 SQL 查询。仅支持 SELECT / WITH ... SELECT，且只能访问 ai_query schema 下允许的只读视图。"
        ),
        parameters={
            "sql": {
                "type": "string",
                "description": "待执行的只读 SQL 语句（单条 SELECT 或 WITH ... SELECT）。",
            },
            "max_rows": {
                "type": "integer",
                "description": "返回行数上限（默认 200，最大 500）。",
                "minimum": 1,
                "maximum": 500,
            },
        },
        required_params=["sql"],
        category=ToolCategory.QUERY,
        operation_level=OperationLevel.READ,
        side_effect=False,
    ),
]

SQL_QUERY_TOOLS: list[dict[str, Any]] = build_openai_tools(SQL_QUERY_TOOL_DEFINITIONS)


def get_sql_query_tools() -> list[dict[str, Any]]:
    return SQL_QUERY_TOOLS


__all__ = [
    "SQL_QUERY_TOOLS",
    "SQL_QUERY_TOOL_DEFINITIONS",
    "SQLQueryToolName",
    "get_sql_query_tools",
]
