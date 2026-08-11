"""
Agent 执行仓储

提供 Agent 执行记录的持久化存储。
"""

import json
from abc import ABC, abstractmethod
from datetime import datetime
from typing import TYPE_CHECKING, Any

from src.domain.ai.agent_execution import (
    AgentExecution,
    AgentExecutionStatus,
    AgentStep,
    AgentStepType,
    TokenUsage,
    ToolCallRecord,
)
from src.infrastructure.logging.core import get_logger

if TYPE_CHECKING:
    from src.infrastructure.database.async_connection_pool import AsyncPooledDatabaseConnection

logger = get_logger(__name__)


class AgentExecutionRepository(ABC):
    """Agent 执行仓储接口"""

    @abstractmethod
    async def save_execution(self, execution: AgentExecution) -> None:
        """保存执行记录"""

    @abstractmethod
    async def get_execution(self, run_id: str) -> AgentExecution | None:
        """获取执行记录"""

    @abstractmethod
    async def update_execution(self, execution: AgentExecution) -> None:
        """更新执行记录"""

    @abstractmethod
    async def save_task_type(self, step: AgentStep) -> None:
        """保存执行作业类型"""

    @abstractmethod
    async def get_task_types(self, run_id: str) -> list[AgentStep]:
        """获取执行作业类型"""

    @abstractmethod
    async def get_task_types_batch(self, run_ids: list[str]) -> dict[str, list[AgentStep]]:
        """批量获取执行作业类型"""

    @abstractmethod
    async def list_executions(
        self,
        todo_id: str | None = None,
        entity_id: str | None = None,
        status: AgentExecutionStatus | None = None,
        started_after: datetime | None = None,
        limit: int = 50,
        offset: int = 0,
    ) -> list[AgentExecution]:
        """列出执行记录"""


class PostgresAgentExecutionRepository(AgentExecutionRepository):
    """PostgreSQL Agent 执行仓储实现（异步版本）"""

    def __init__(self, db_pool: "AsyncPooledDatabaseConnection"):
        """
        Args:
            db_pool: 异步数据库连接池
        """
        self._db_pool = db_pool

    async def save_execution(self, execution: AgentExecution) -> None:
        """保存执行记录"""
        query = """
            INSERT INTO agent_executions (
                run_id, todo_id, entity_id, entity_config, status,
                total_steps, total_tokens, total_tool_calls,
                started_at, finished_at, error_message, metadata
            ) VALUES (
                %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s
            )
            ON CONFLICT (run_id) DO UPDATE SET
                status = EXCLUDED.status,
                total_steps = EXCLUDED.total_steps,
                total_tokens = EXCLUDED.total_tokens,
                total_tool_calls = EXCLUDED.total_tool_calls,
                finished_at = EXCLUDED.finished_at,
                error_message = EXCLUDED.error_message,
                metadata = EXCLUDED.metadata
        """

        async with self._db_pool.connection_context() as conn:
            await conn.execute(
                query,
                (
                    execution.run_id,
                    execution.todo_id,
                    execution.entity_id,
                    json.dumps(execution.entity_config),
                    execution.status.value,
                    execution.total_steps,
                    execution.total_tokens,
                    execution.total_tool_calls,
                    execution.started_at,
                    execution.finished_at,
                    execution.error_message,
                    json.dumps(execution.metadata),
                ),
            )

        logger.debug(f"Saved execution {execution.run_id}")

    async def get_execution(self, run_id: str) -> AgentExecution | None:
        """获取执行记录"""
        query = """
            SELECT run_id, todo_id, entity_id, entity_config, status,
                   total_steps, total_tokens, total_tool_calls,
                   started_at, finished_at, error_message, metadata
            FROM agent_executions
            WHERE run_id = %s
        """

        async with self._db_pool.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(query, (run_id,))
            row = await cursor.fetchone()
            if not row:
                return None
            return self._row_to_execution(row)

    async def update_execution(self, execution: AgentExecution) -> None:
        """更新执行记录"""
        query = """
            UPDATE agent_executions SET
                status = %s,
                total_steps = %s,
                total_tokens = %s,
                total_tool_calls = %s,
                finished_at = %s,
                error_message = %s,
                metadata = %s
            WHERE run_id = %s
        """

        async with self._db_pool.connection_context() as conn:
            await conn.execute(
                query,
                (
                    execution.status.value,
                    execution.total_steps,
                    execution.total_tokens,
                    execution.total_tool_calls,
                    execution.finished_at,
                    execution.error_message,
                    json.dumps(execution.metadata),
                    execution.run_id,
                ),
            )

    async def save_task_type(self, step: AgentStep) -> None:
        """保存执行作业类型"""
        query = """
            INSERT INTO agent_steps (
                step_id, run_id, sequence, step_type, role, content,
                tool_calls, token_usage, latency_ms, created_at, metadata,
                thinking, decision_summary
            ) VALUES (
                %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s
            )
        """

        async with self._db_pool.connection_context() as conn:
            await conn.execute(
                query,
                (
                    step.step_id,
                    step.run_id,
                    step.sequence,
                    step.step_type.value,
                    step.role,
                    step.content,
                    json.dumps([tc.to_dict() for tc in step.tool_calls]) if step.tool_calls else None,
                    json.dumps(step.token_usage.to_dict()) if step.token_usage else None,
                    step.latency_ms,
                    step.created_at,
                    json.dumps(step.metadata),
                    step.thinking,
                    step.decision_summary,
                ),
            )

    async def get_task_types(self, run_id: str) -> list[AgentStep]:
        """获取执行作业类型"""
        query = """
            SELECT step_id, run_id, sequence, step_type, role, content,
                   tool_calls, token_usage, latency_ms, created_at, metadata,
                   thinking, decision_summary
            FROM agent_steps
            WHERE run_id = %s
            ORDER BY sequence
        """

        async with self._db_pool.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(query, (run_id,))
            rows = await cursor.fetchall()
            return [self._row_to_step(row) for row in rows]

    async def get_task_types_batch(self, run_ids: list[str]) -> dict[str, list[AgentStep]]:
        """批量获取执行作业类型。"""
        normalized_run_ids = [str(run_id).strip() for run_id in run_ids if str(run_id).strip()]
        if not normalized_run_ids:
            return {}

        placeholders = ", ".join(["%s"] * len(normalized_run_ids))
        query = f"""
            SELECT step_id, run_id, sequence, step_type, role, content,
                   tool_calls, token_usage, latency_ms, created_at, metadata,
                   thinking, decision_summary
            FROM agent_steps
            WHERE run_id IN ({placeholders})
            ORDER BY run_id, sequence
        """

        async with self._db_pool.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(query, tuple(normalized_run_ids))
            rows = await cursor.fetchall()

        grouped: dict[str, list[AgentStep]] = {run_id: [] for run_id in normalized_run_ids}
        for row in rows:
            step = self._row_to_step(row)
            grouped.setdefault(step.run_id, []).append(step)
        return grouped

    async def list_executions(
        self,
        todo_id: str | None = None,
        entity_id: str | None = None,
        status: AgentExecutionStatus | None = None,
        started_after: datetime | None = None,
        limit: int = 50,
        offset: int = 0,
    ) -> list[AgentExecution]:
        """列出执行记录"""
        query_parts = [
            "SELECT run_id, todo_id, entity_id, entity_config, status,",
            "       total_steps, total_tokens, total_tool_calls,",
            "       started_at, finished_at, error_message, metadata",
            "FROM agent_executions",
            "WHERE 1=1",
        ]
        params = []

        if todo_id:
            params.append(todo_id)
            query_parts.append("AND todo_id = %s")
        if entity_id:
            params.append(entity_id)
            query_parts.append("AND entity_id = %s")
        if status:
            params.append(status.value)
            query_parts.append("AND status = %s")
        if started_after is not None:
            params.append(started_after)
            query_parts.append("AND started_at >= %s")

        query_parts.append("ORDER BY started_at DESC")
        params.append(limit)
        query_parts.append("LIMIT %s")
        params.append(offset)
        query_parts.append("OFFSET %s")

        query = " ".join(query_parts)

        async with self._db_pool.connection_context() as conn, conn.cursor() as cursor:
            await cursor.execute(query, tuple(params))
            rows = await cursor.fetchall()
            return [self._row_to_execution(row) for row in rows]

    def _row_to_execution(self, row: dict[str, Any]) -> AgentExecution:
        """将数据库行转换为 AgentExecution"""
        entity_config = row.get("entity_config")
        if isinstance(entity_config, str):
            entity_config = json.loads(entity_config)

        metadata = row.get("metadata")
        if isinstance(metadata, str):
            metadata = json.loads(metadata)

        execution = AgentExecution(
            run_id=str(row["run_id"]),
            todo_id=str(row["todo_id"]),
            entity_id=str(row["entity_id"]),
            entity_config=entity_config or {},
            status=AgentExecutionStatus(row["status"]),
            total_steps=row["total_steps"],
            total_tokens=row["total_tokens"],
            total_tool_calls=row["total_tool_calls"],
            started_at=row["started_at"],
            finished_at=row["finished_at"],
            error_message=row.get("error_message"),
            metadata=metadata or {},
        )
        return execution

    def _row_to_step(self, row: dict[str, Any]) -> AgentStep:
        """将数据库行转换为 AgentStep"""
        tool_calls_data = row.get("tool_calls")
        tool_calls = None
        if tool_calls_data:
            if isinstance(tool_calls_data, str):
                tool_calls_data = json.loads(tool_calls_data)
            tool_calls = [ToolCallRecord.from_dict(tc) for tc in tool_calls_data]

        token_usage_data = row.get("token_usage")
        token_usage = None
        if token_usage_data:
            if isinstance(token_usage_data, str):
                token_usage_data = json.loads(token_usage_data)
            token_usage = TokenUsage.from_dict(token_usage_data)

        metadata = row.get("metadata")
        if isinstance(metadata, str):
            metadata = json.loads(metadata)

        return AgentStep(
            step_id=str(row["step_id"]),
            run_id=str(row["run_id"]),
            sequence=row["sequence"],
            step_type=AgentStepType(row["step_type"]),
            role=row["role"],
            content=row.get("content", ""),
            tool_calls=tool_calls,
            token_usage=token_usage,
            latency_ms=row.get("latency_ms"),
            created_at=row["created_at"],
            metadata=metadata or {},
            thinking=row.get("thinking"),
            decision_summary=row.get("decision_summary"),
        )


class MemoryAgentExecutionRepository(AgentExecutionRepository):
    """内存 Agent 执行仓储实现（用于测试）"""

    def __init__(self):
        self._executions: dict[str, AgentExecution] = {}
        self._steps: dict[str, list[AgentStep]] = {}

    async def save_execution(self, execution: AgentExecution) -> None:
        self._executions[execution.run_id] = execution

    async def get_execution(self, run_id: str) -> AgentExecution | None:
        return self._executions.get(run_id)

    async def update_execution(self, execution: AgentExecution) -> None:
        self._executions[execution.run_id] = execution

    async def save_task_type(self, step: AgentStep) -> None:
        if step.run_id not in self._steps:
            self._steps[step.run_id] = []
        self._steps[step.run_id].append(step)

    async def get_task_types(self, run_id: str) -> list[AgentStep]:
        return self._steps.get(run_id, [])

    async def get_task_types_batch(self, run_ids: list[str]) -> dict[str, list[AgentStep]]:
        normalized_run_ids = [str(run_id).strip() for run_id in run_ids if str(run_id).strip()]
        return {run_id: list(self._steps.get(run_id, [])) for run_id in normalized_run_ids}

    async def list_executions(
        self,
        todo_id: str | None = None,
        entity_id: str | None = None,
        status: AgentExecutionStatus | None = None,
        started_after: datetime | None = None,
        limit: int = 50,
        offset: int = 0,
    ) -> list[AgentExecution]:
        executions = list(self._executions.values())

        if todo_id:
            executions = [e for e in executions if e.todo_id == todo_id]
        if entity_id:
            executions = [e for e in executions if e.entity_id == entity_id]
        if status:
            executions = [e for e in executions if e.status == status]
        if started_after is not None:
            executions = [e for e in executions if e.started_at >= started_after]

        executions.sort(key=lambda e: e.started_at, reverse=True)
        return executions[offset : offset + limit]


__all__ = [
    "AgentExecutionRepository",
    "MemoryAgentExecutionRepository",
    "PostgresAgentExecutionRepository",
]
