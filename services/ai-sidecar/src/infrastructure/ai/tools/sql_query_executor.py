"""Readonly SQL query tool executor."""

from __future__ import annotations

import hashlib
import json
import re
from collections.abc import Iterable
from typing import Any, ClassVar

from cachetools import TTLCache

from .base import BaseToolExecutor, ToolCategory, ToolExecutionError, ToolExecutionStatus
from .sql_query_tools import SQLQueryToolName

_tool_result_cache = TTLCache(maxsize=200, ttl=15)


class SQLQueryReadOnlyExecutor(BaseToolExecutor):
    """Execute readonly SQL against ai_query schema with strict validation."""

    _FORBIDDEN_KEYWORDS: ClassVar[set[str]] = {
        "insert",
        "update",
        "delete",
        "merge",
        "alter",
        "drop",
        "truncate",
        "create",
        "grant",
        "revoke",
        "copy",
        "call",
        "do",
    }

    _RELATION_PATTERN = re.compile(r"(?is)\b(?:from|join)\s+([a-zA-Z_][a-zA-Z0-9_$]*(?:\.[a-zA-Z_][a-zA-Z0-9_$]*)?)")
    _CTE_PATTERN = re.compile(r"(?is)(?:\bwith\b|,)\s*([a-zA-Z_][a-zA-Z0-9_$]*)\s+as\s*\(")
    _NUMBER_PATTERN = re.compile(r"\b\d+(?:\.\d+)?\b")
    _SINGLE_QUOTED_STRING_PATTERN = re.compile(r"'(?:''|[^'])*'")

    def __init__(
        self,
        db_pool: Any | None,
        *,
        allowed_relations: Iterable[str] | None = None,
        default_max_rows: int = 200,
        hard_max_rows: int = 500,
        statement_timeout_ms: int = 10000,
        default_user: str = "AI_Assistant",
    ):
        super().__init__(default_user=default_user)
        self._db_pool = db_pool
        self._allowed_relations = self._normalize_relation_allowlist(allowed_relations or [])
        self._default_max_rows = max(1, int(default_max_rows or 200))
        self._hard_max_rows = max(self._default_max_rows, int(hard_max_rows or 500))
        self._statement_timeout_ms = max(100, int(statement_timeout_ms or 10000))

    def _register_handlers(self) -> None:
        self._handlers = {
            SQLQueryToolName.SQL_QUERY_READONLY.value: self._handle_sql_query_readonly,
        }

    def get_category(self) -> ToolCategory:
        return ToolCategory.QUERY

    async def _handle_sql_query_readonly(self, args: dict[str, Any]) -> dict[str, Any]:
        cache_key = f"sql_ro:{json.dumps(args, sort_keys=True)}"
        if cache_key in _tool_result_cache:
            return _tool_result_cache[cache_key]

        result = await self._execute_sql_query_readonly(args)
        _tool_result_cache[cache_key] = result
        return result

    async def _execute_sql_query_readonly(self, args: dict[str, Any]) -> dict[str, Any]:
        self._ensure_db_pool()

        raw_sql = self._require_arg(
            args,
            "sql",
            "缺少必需参数: sql",
        )
        sql = self._normalize_sql(raw_sql)
        max_rows = self._resolve_max_rows(args.get("max_rows"))

        self._validate_sql(sql)
        relations_used = self._extract_relations(sql)
        self._validate_relations(relations_used)

        # Fetch one extra row to expose whether the response was truncated.
        fetch_cap = min(max_rows + 1, self._hard_max_rows + 1)
        executable_sql = self._wrap_query_with_limit(sql, fetch_cap)
        rows = await self._execute_query(executable_sql)

        normalized_rows = self._normalize_rows(rows)
        truncated = len(normalized_rows) > max_rows
        visible_rows = normalized_rows[:max_rows]

        return {
            "rows": visible_rows,
            "row_count": len(visible_rows),
            "truncated": truncated,
            "relations_used": sorted(relations_used),
            "sql_fingerprint": self._fingerprint_sql(sql),
        }

    def _ensure_db_pool(self) -> None:
        if self._db_pool is None:
            raise ToolExecutionError(
                "只读查询连接池未初始化",
                ToolExecutionStatus.ERROR,
            )
        if not hasattr(self._db_pool, "transaction_context"):
            raise ToolExecutionError(
                "只读查询连接池缺少事务能力",
                ToolExecutionStatus.ERROR,
            )

    def _resolve_max_rows(self, value: Any) -> int:
        try:
            if value is None:
                return self._default_max_rows
            resolved = int(value)
        except (TypeError, ValueError):
            resolved = self._default_max_rows
        return max(1, min(resolved, self._hard_max_rows))

    @staticmethod
    def _normalize_sql(value: Any) -> str:
        sql = str(value or "").strip()
        while sql.endswith(";"):
            sql = sql[:-1].rstrip()
        if not sql:
            raise ToolExecutionError(
                "sql 不能为空",
                ToolExecutionStatus.VALIDATION_ERROR,
            )
        return sql

    def _validate_sql(self, sql: str) -> None:
        if self._contains_delimiter_outside_literal(sql):
            raise ToolExecutionError(
                "仅允许执行单条 SQL 语句",
                ToolExecutionStatus.VALIDATION_ERROR,
            )

        lowered = sql.lstrip().lower()
        if not (lowered.startswith("select ") or lowered.startswith("with ")):
            raise ToolExecutionError(
                "仅允许 SELECT 或 WITH ... SELECT 语句",
                ToolExecutionStatus.VALIDATION_ERROR,
            )

        scrubbed = self._strip_literals_and_comments(sql).lower()
        for keyword in self._FORBIDDEN_KEYWORDS:
            if re.search(rf"\b{re.escape(keyword)}\b", scrubbed):
                raise ToolExecutionError(
                    f"检测到不允许的关键字: {keyword.upper()}",
                    ToolExecutionStatus.VALIDATION_ERROR,
                )

    @staticmethod
    def _contains_delimiter_outside_literal(sql: str) -> bool:
        i = 0
        in_single = False
        in_double = False
        in_line_comment = False
        in_block_comment = False
        size = len(sql)

        while i < size:
            ch = sql[i]
            nxt = sql[i + 1] if i + 1 < size else ""

            if in_line_comment:
                if ch == "\n":
                    in_line_comment = False
                i += 1
                continue

            if in_block_comment:
                if ch == "*" and nxt == "/":
                    in_block_comment = False
                    i += 2
                    continue
                i += 1
                continue

            if not in_single and not in_double:
                if ch == "-" and nxt == "-":
                    in_line_comment = True
                    i += 2
                    continue
                if ch == "/" and nxt == "*":
                    in_block_comment = True
                    i += 2
                    continue
                if ch == ";":
                    return True
                if ch == "'":
                    in_single = True
                    i += 1
                    continue
                if ch == '"':
                    in_double = True
                    i += 1
                    continue
                i += 1
                continue

            if in_single:
                if ch == "'" and nxt == "'":
                    i += 2
                    continue
                if ch == "'":
                    in_single = False
                i += 1
                continue

            if in_double:
                if ch == '"':
                    in_double = False
                i += 1
                continue

        return False

    @classmethod
    def _strip_literals_and_comments(cls, sql: str) -> str:
        # Remove block and line comments first.
        no_block_comments = re.sub(r"/\*.*?\*/", " ", sql, flags=re.DOTALL)
        no_comments = re.sub(r"--.*?$", " ", no_block_comments, flags=re.MULTILINE)
        # Replace string literals to avoid false positive keyword checks.
        return cls._SINGLE_QUOTED_STRING_PATTERN.sub("''", no_comments)

    @classmethod
    def _extract_relations(cls, sql: str) -> set[str]:
        cte_names = {
            cls._normalize_identifier(match.group(1)) for match in cls._CTE_PATTERN.finditer(sql) if match.group(1)
        }

        relations: set[str] = set()
        for match in cls._RELATION_PATTERN.finditer(sql):
            raw_relation = match.group(1) or ""
            relation = cls._normalize_identifier(raw_relation)
            if not relation:
                continue
            if "." not in relation and relation in cte_names:
                continue
            relations.add(relation)
        return relations

    def _validate_relations(self, relations: set[str]) -> None:
        for relation in relations:
            if "." not in relation:
                raise ToolExecutionError(
                    f"relation 必须带 schema 前缀: {relation}",
                    ToolExecutionStatus.VALIDATION_ERROR,
                )

            schema_name, _ = relation.split(".", 1)
            if schema_name != "ai_query":
                raise ToolExecutionError(
                    f"仅允许访问 ai_query schema，当前 relation: {relation}",
                    ToolExecutionStatus.VALIDATION_ERROR,
                )

            if self._allowed_relations and relation not in self._allowed_relations:
                raise ToolExecutionError(
                    f"relation 不在 allowlist 中: {relation}",
                    ToolExecutionStatus.VALIDATION_ERROR,
                )

    @staticmethod
    def _normalize_identifier(identifier: str) -> str:
        return str(identifier or "").strip().strip('"').lower()

    @classmethod
    def _normalize_relation_allowlist(cls, relations: Iterable[str]) -> set[str]:
        normalized: set[str] = set()
        for relation in relations:
            name = cls._normalize_identifier(str(relation or ""))
            if not name:
                continue
            if "." not in name:
                name = f"ai_query.{name}"
            normalized.add(name)
        return normalized

    @staticmethod
    def _wrap_query_with_limit(sql: str, limit: int) -> str:
        safe_limit = max(1, int(limit))
        return "SELECT * FROM (" + sql + f") AS ai_query_readonly_subquery LIMIT {safe_limit}"

    async def _execute_query(self, sql: str) -> list[dict[str, Any]]:
        try:
            async with self._db_pool.connection_context() as conn:
                await conn.execute("SET TRANSACTION READ ONLY")
                await conn.execute(f"SET LOCAL statement_timeout = {self._statement_timeout_ms}")
                async with conn.cursor() as cursor:
                    await cursor.execute(sql)
                    rows = await cursor.fetchall()
                    return rows or []
        except ToolExecutionError:
            raise
        except Exception as exc:
            message = str(exc or "").lower()
            if "statement timeout" in message:
                raise ToolExecutionError(
                    "查询执行超时，请尝试缩小查询范围或添加更具体的筛选条件后重试",
                    ToolExecutionStatus.TIMEOUT,
                ) from exc
            raise ToolExecutionError(
                f"只读 SQL 执行失败: {exc}",
                ToolExecutionStatus.ERROR,
            ) from exc

    @staticmethod
    def _normalize_rows(rows: list[Any]) -> list[dict[str, Any]]:
        normalized: list[dict[str, Any]] = []
        for row in rows:
            if isinstance(row, dict):
                normalized.append(dict(row))
            elif hasattr(row, "items"):
                normalized.append(dict(row.items()))
            else:
                normalized.append({"value": row})
        return normalized

    @classmethod
    def _fingerprint_sql(cls, sql: str) -> str:
        scrubbed = cls._SINGLE_QUOTED_STRING_PATTERN.sub("'?'", sql)
        scrubbed = cls._NUMBER_PATTERN.sub("?", scrubbed)
        normalized = re.sub(r"\s+", " ", scrubbed).strip().lower()
        digest = hashlib.sha256(normalized.encode("utf-8")).hexdigest()
        return digest[:16]


__all__ = ["SQLQueryReadOnlyExecutor"]
