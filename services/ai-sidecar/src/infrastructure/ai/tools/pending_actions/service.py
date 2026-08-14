"""Pending action store implementations and runtime accessors."""

from __future__ import annotations

import asyncio
import json
from datetime import datetime, timedelta
from typing import TYPE_CHECKING, Any

from src.domain.utils.time_utils import utc_now
from src.infrastructure.common.exceptions import POSTGRES_EXCEPTIONS
from src.infrastructure.common.runtime_utils import get_runtime_holder
from src.infrastructure.logging.core import get_logger
from src.shared.id_generator import generate_id

from .models import (
    PendingAction,
    PendingActionConflictError,
    PendingActionStatus,
    PendingActionStoreProtocol,
)

if TYPE_CHECKING:
    from src.infrastructure.database.async_connection_pool import AsyncPooledDatabaseConnection

logger = get_logger("src.infrastructure.ai.tools.pending_actions")


def _coerce_timestamp(value: Any) -> datetime | None:
    if value is None:
        return None
    if isinstance(value, datetime):
        return value
    text = str(value).strip()
    if not text:
        return None
    if text.endswith("Z"):
        text = f"{text[:-1]}+00:00"
    try:
        return datetime.fromisoformat(text)
    except ValueError:
        return None


class MemoryPendingActionStore:
    """In-memory pending action store with async-safe access."""

    def __init__(self, max_actions: int = 2000):
        self._max_actions = max(100, int(max_actions or 100))
        self._actions: dict[str, PendingAction] = {}
        self._order: list[str] = []
        self._lock = asyncio.Lock()

    async def create_action(
        self,
        *,
        tool_call_id: str,
        tool_name: str,
        arguments: str,
        operation_level: str,
        invocation_mode: str,
        requester_user_id: str | None,
        requester_user_roles: list[str] | None,
        reason: str,
        risk_level: str = "NORMAL",
        entity_type: str | None = None,
        entity_id: str | None = None,
        before_snapshot: Any | None = None,
        after_snapshot: Any | None = None,
        json_patch: Any | None = None,
        diff_summary: Any | None = None,
        correlation_id: str | None = None,
        ui_hints: dict[str, Any] | None = None,
        ttl_seconds: int | None = None,
    ) -> PendingAction:
        now = utc_now()
        expires_at = now + timedelta(seconds=ttl_seconds) if ttl_seconds and ttl_seconds > 0 else None
        action = PendingAction(
            action_id=generate_id("pending_action"),
            tool_call_id=tool_call_id,
            tool_name=tool_name,
            arguments=arguments,
            operation_level=operation_level,
            invocation_mode=invocation_mode,
            requester_user_id=requester_user_id,
            requester_user_roles=list(requester_user_roles or []),
            reason=reason,
            status=PendingActionStatus.PENDING,
            created_at=now,
            updated_at=now,
            risk_level=str(risk_level or "NORMAL"),
            entity_type=entity_type,
            entity_id=entity_id,
            before_snapshot=before_snapshot,
            after_snapshot=after_snapshot,
            json_patch=json_patch,
            diff_summary=diff_summary,
            correlation_id=correlation_id,
            ui_hints=dict(ui_hints or {}),
            expires_at=expires_at,
            diff_source=str((ui_hints or {}).get("diff_source") or "").strip() or None,
        )

        async with self._lock:
            self._actions[action.action_id] = action
            self._order.append(action.action_id)
            self._trim_if_needed()

        return action

    async def get_action(self, action_id: str) -> PendingAction | None:
        async with self._lock:
            return self._actions.get(action_id)

    async def list_actions(
        self,
        *,
        status: str | None = None,
        tool_name: str | None = None,
        entity_id: str | None = None,
        requester_user_id: str | None = None,
        created_after: datetime | None = None,
        created_before: datetime | None = None,
        limit: int = 50,
        offset: int = 0,
    ) -> list[PendingAction]:
        normalized_status = str(status or "").strip().lower()
        normalized_tool_name = str(tool_name or "").strip()
        normalized_entity_id = str(entity_id or "").strip()
        normalized_requester = str(requester_user_id or "").strip()
        bounded_limit = max(1, min(int(limit or 50), 200))
        bounded_offset = max(0, int(offset or 0))

        async with self._lock:
            ordered = [self._actions[action_id] for action_id in reversed(self._order) if action_id in self._actions]

            filtered: list[PendingAction] = []
            for action in ordered:
                if normalized_status and action.status.value != normalized_status:
                    continue
                if normalized_tool_name and action.tool_name != normalized_tool_name:
                    continue
                if normalized_entity_id and str(action.entity_id or "").strip() != normalized_entity_id:
                    continue
                if normalized_requester and str(action.requester_user_id or "").strip() != normalized_requester:
                    continue
                action_created_at = _coerce_timestamp(action.created_at)
                if created_after is not None and (action_created_at is None or action_created_at < created_after):
                    continue
                if created_before is not None and (action_created_at is None or action_created_at > created_before):
                    continue
                filtered.append(action)

            return filtered[bounded_offset : bounded_offset + bounded_limit]

    async def count_actions(
        self,
        *,
        status: str | None = None,
        tool_name: str | None = None,
        entity_id: str | None = None,
        created_after: datetime | None = None,
        created_before: datetime | None = None,
    ) -> int:
        normalized_status = str(status or "").strip().lower()
        normalized_tool_name = str(tool_name or "").strip()
        normalized_entity_id = str(entity_id or "").strip()

        async with self._lock:
            count = 0
            for action_id in self._order:
                action = self._actions.get(action_id)
                if action is None:
                    continue
                if normalized_status and action.status.value != normalized_status:
                    continue
                if normalized_tool_name and action.tool_name != normalized_tool_name:
                    continue
                if normalized_entity_id and str(action.entity_id or "").strip() != normalized_entity_id:
                    continue
                action_created_at = _coerce_timestamp(action.created_at)
                if created_after is not None and (action_created_at is None or action_created_at < created_after):
                    continue
                if created_before is not None and (action_created_at is None or action_created_at > created_before):
                    continue
                count += 1
            return count

    async def approve_action(self, action_id: str, approver_id: str) -> PendingAction:
        now = utc_now()
        async with self._lock:
            action = self._actions.get(action_id)
            if action is None:
                raise KeyError(action_id)
            if action.status == PendingActionStatus.EXPIRED:
                raise PendingActionConflictError(
                    "pending action has expired and can no longer be approved",
                    code="PENDING_ACTION_EXPIRED",
                    decision_blocked_reason="expired",
                )
            if action.status != PendingActionStatus.PENDING:
                raise PendingActionConflictError(
                    f"pending action state invalid: {action.status.value}",
                    code="PENDING_ACTION_STATE_CONFLICT",
                )
            if action.expires_at is not None and now >= action.expires_at:
                action.status = PendingActionStatus.EXPIRED
                action.updated_at = now
                action.status_code = "PENDING_ACTION_EXPIRED"
                action.error_payload = {"decision_blocked_reason": "expired"}
                action.decision_blocked_reason = "expired"
                raise PendingActionConflictError(
                    "pending action has expired and can no longer be approved",
                    code="PENDING_ACTION_EXPIRED",
                    decision_blocked_reason="expired",
                )

            action.status = PendingActionStatus.APPROVED
            action.approved_by = approver_id
            action.approved_at = now
            action.updated_at = now
            return action

    async def reject_action(self, action_id: str, approver_id: str, reason: str | None = None) -> PendingAction:
        now = utc_now()
        async with self._lock:
            action = self._actions.get(action_id)
            if action is None:
                raise KeyError(action_id)
            if action.status == PendingActionStatus.EXPIRED:
                raise PendingActionConflictError(
                    "pending action has expired and can no longer be rejected",
                    code="PENDING_ACTION_EXPIRED",
                    decision_blocked_reason="expired",
                )
            if action.status != PendingActionStatus.PENDING:
                raise PendingActionConflictError(
                    f"pending action state invalid: {action.status.value}",
                    code="PENDING_ACTION_STATE_CONFLICT",
                )
            if action.expires_at is not None and now >= action.expires_at:
                action.status = PendingActionStatus.EXPIRED
                action.updated_at = now
                action.status_code = "PENDING_ACTION_EXPIRED"
                action.error_payload = {"decision_blocked_reason": "expired"}
                action.decision_blocked_reason = "expired"
                raise PendingActionConflictError(
                    "pending action has expired and can no longer be rejected",
                    code="PENDING_ACTION_EXPIRED",
                    decision_blocked_reason="expired",
                )

            action.status = PendingActionStatus.REJECTED
            action.rejected_by = approver_id
            action.rejected_reason = (reason or "").strip() or "rejected_by_human"
            action.rejected_at = now
            action.updated_at = now
            return action

    async def mark_executed(
        self,
        action_id: str,
        execution_result: Any,
        status_code: str | None = None,
        execution_receipt: Any | None = None,
    ) -> PendingAction:
        now = utc_now()
        async with self._lock:
            action = self._actions.get(action_id)
            if action is None:
                raise KeyError(action_id)
            action.status = PendingActionStatus.EXECUTED
            action.execution_result = execution_result
            action.execution_error = None
            action.status_code = status_code
            action.error_payload = None
            action.execution_receipt = execution_receipt
            action.updated_at = now
            return action

    async def mark_failed(
        self,
        action_id: str,
        error_message: str,
        status_code: str | None = None,
        error_payload: Any | None = None,
        execution_receipt: Any | None = None,
    ) -> PendingAction:
        now = utc_now()
        async with self._lock:
            action = self._actions.get(action_id)
            if action is None:
                raise KeyError(action_id)
            action.status = PendingActionStatus.FAILED
            action.execution_error = str(error_message or "execution_failed")
            action.status_code = status_code
            action.error_payload = error_payload
            action.execution_receipt = execution_receipt
            action.updated_at = now
            return action

    async def expire_stale_actions(self, before: Any) -> list[PendingAction]:
        """Mark all pending actions whose expires_at < before as expired."""
        now = utc_now()
        expired_list: list[PendingAction] = []
        async with self._lock:
            for action in self._actions.values():
                if (
                    action.status == PendingActionStatus.PENDING
                    and action.expires_at is not None
                    and action.expires_at < before
                ):
                    action.status = PendingActionStatus.EXPIRED
                    action.updated_at = now
                    action.status_code = "PENDING_ACTION_EXPIRED"
                    action.error_payload = {"decision_blocked_reason": "expired"}
                    action.decision_blocked_reason = "expired"
                    expired_list.append(action)
        return expired_list

    async def clear(self) -> None:
        async with self._lock:
            self._actions.clear()
            self._order.clear()

    def clear_sync(self) -> None:
        self._actions.clear()
        self._order.clear()

    def _trim_if_needed(self) -> None:
        if len(self._order) <= self._max_actions:
            return

        removable_count = len(self._order) - self._max_actions
        if removable_count <= 0:
            return

        removed = 0
        retained_order: list[str] = []
        for action_id in self._order:
            action = self._actions.get(action_id)
            if (
                removed < removable_count
                and action is not None
                and action.status
                in {
                    PendingActionStatus.REJECTED,
                    PendingActionStatus.EXECUTED,
                    PendingActionStatus.FAILED,
                    PendingActionStatus.EXPIRED,
                }
            ):
                self._actions.pop(action_id, None)
                removed += 1
                continue
            retained_order.append(action_id)

        # 如果仍然超限，强制移除最旧项。
        while len(retained_order) > self._max_actions:
            action_id = retained_order.pop(0)
            self._actions.pop(action_id, None)

        self._order = retained_order


class PostgresPendingActionStore:
    """PostgreSQL-backed pending action store with memory fallback."""

    _SELECT_COLUMNS = """
        action_id, tool_call_id, tool_name, arguments,
        operation_level, invocation_mode,
        requester_user_id, requester_user_roles,
        reason, status,
        approved_by, approved_at,
        rejected_by, rejected_reason, rejected_at,
        execution_result, execution_error,
        risk_level, entity_type, entity_id,
        before_snapshot, after_snapshot,
        json_patch, diff_summary,
        execution_receipt, status_code, error_payload,
        correlation_id, ui_hints, expires_at,
        created_at, updated_at
    """

    def __init__(
        self,
        db_pool: AsyncPooledDatabaseConnection,
        *,
        fallback_store: PendingActionStoreProtocol | None = None,
    ):
        self._db_pool = db_pool
        self._fallback = fallback_store or MemoryPendingActionStore()
        self._init_lock = asyncio.Lock()
        self._initialized = False
        self._init_failed = False

    async def _ensure_initialized(self) -> bool:
        if self._initialized:
            return True

        if self._init_failed:
            return False

        async with self._init_lock:
            if self._initialized:
                return True
            if self._init_failed:
                return False

            try:
                await self._init_tables()
                self._initialized = True
                return True
            except POSTGRES_EXCEPTIONS as exc:
                logger.warning(f"PendingActionStore postgres init failed, fallback to memory: {exc}")
                self._init_failed = True
                return False

    async def _init_tables(self) -> None:
        ddl_statements = [
            """
            CREATE TABLE IF NOT EXISTS ai_pending_actions (
                id SERIAL PRIMARY KEY,
                action_id VARCHAR(64) NOT NULL UNIQUE,
                tool_call_id VARCHAR(64) NOT NULL,
                tool_name VARCHAR(128) NOT NULL,
                arguments TEXT NOT NULL,
                operation_level VARCHAR(64) NOT NULL,
                invocation_mode VARCHAR(64) NOT NULL,
                requester_user_id VARCHAR(255),
                requester_user_roles JSONB NOT NULL DEFAULT '[]'::jsonb,
                reason TEXT NOT NULL,
                status VARCHAR(32) NOT NULL DEFAULT 'pending',
                approved_by VARCHAR(255),
                approved_at TIMESTAMP WITH TIME ZONE,
                rejected_by VARCHAR(255),
                rejected_reason TEXT,
                rejected_at TIMESTAMP WITH TIME ZONE,
                execution_result JSONB,
                execution_error TEXT,
                risk_level TEXT NOT NULL DEFAULT 'NORMAL',
                entity_type TEXT,
                entity_id TEXT,
                before_snapshot JSONB,
                after_snapshot JSONB,
                json_patch JSONB,
                diff_summary JSONB,
                execution_receipt JSONB,
                status_code TEXT,
                error_payload JSONB,
                correlation_id UUID,
                ui_hints JSONB NOT NULL DEFAULT '{}'::jsonb,
                expires_at TIMESTAMP WITH TIME ZONE,
                created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
                CONSTRAINT valid_ai_pending_action_status CHECK (
                    status IN ('pending', 'approved', 'rejected', 'executed', 'failed')
                )
            )
            """,
            "ALTER TABLE ai_pending_actions ADD COLUMN IF NOT EXISTS risk_level TEXT NOT NULL DEFAULT 'NORMAL'",
            "ALTER TABLE ai_pending_actions ADD COLUMN IF NOT EXISTS entity_type TEXT",
            "ALTER TABLE ai_pending_actions ADD COLUMN IF NOT EXISTS entity_id TEXT",
            "ALTER TABLE ai_pending_actions ADD COLUMN IF NOT EXISTS before_snapshot JSONB",
            "ALTER TABLE ai_pending_actions ADD COLUMN IF NOT EXISTS after_snapshot JSONB",
            "ALTER TABLE ai_pending_actions ADD COLUMN IF NOT EXISTS json_patch JSONB",
            "ALTER TABLE ai_pending_actions ADD COLUMN IF NOT EXISTS diff_summary JSONB",
            "ALTER TABLE ai_pending_actions ADD COLUMN IF NOT EXISTS execution_receipt JSONB",
            "ALTER TABLE ai_pending_actions ADD COLUMN IF NOT EXISTS status_code TEXT",
            "ALTER TABLE ai_pending_actions ADD COLUMN IF NOT EXISTS error_payload JSONB",
            "ALTER TABLE ai_pending_actions ADD COLUMN IF NOT EXISTS correlation_id UUID",
            "ALTER TABLE ai_pending_actions ADD COLUMN IF NOT EXISTS ui_hints JSONB NOT NULL DEFAULT '{}'::jsonb",
            "ALTER TABLE ai_pending_actions ADD COLUMN IF NOT EXISTS expires_at TIMESTAMP WITH TIME ZONE",
            # Update CHECK constraint to include 'expired'
            "ALTER TABLE ai_pending_actions DROP CONSTRAINT IF EXISTS valid_ai_pending_action_status",
            "ALTER TABLE ai_pending_actions ADD CONSTRAINT valid_ai_pending_action_status CHECK (status IN ('pending', 'approved', 'rejected', 'executed', 'failed', 'expired'))",
            "CREATE INDEX IF NOT EXISTS idx_ai_pending_actions_status ON ai_pending_actions(status)",
            "CREATE INDEX IF NOT EXISTS idx_ai_pending_actions_tool_name ON ai_pending_actions(tool_name)",
            "CREATE INDEX IF NOT EXISTS idx_ai_pending_actions_created_at ON ai_pending_actions(created_at DESC)",
            "CREATE INDEX IF NOT EXISTS idx_ai_pending_actions_status_risk_created ON ai_pending_actions(status, risk_level, created_at DESC)",
            "CREATE INDEX IF NOT EXISTS idx_ai_pending_actions_correlation_id ON ai_pending_actions(correlation_id)",
            "CREATE INDEX IF NOT EXISTS idx_ai_pending_actions_entity_created ON ai_pending_actions(entity_id, created_at DESC)",
            "CREATE INDEX IF NOT EXISTS idx_ai_pending_actions_json_patch_gin ON ai_pending_actions USING GIN (json_patch jsonb_path_ops)",
            "CREATE INDEX IF NOT EXISTS idx_ai_pending_actions_diff_summary_gin ON ai_pending_actions USING GIN (diff_summary jsonb_path_ops)",
            "CREATE INDEX IF NOT EXISTS idx_ai_pending_actions_expires_at ON ai_pending_actions(expires_at) WHERE status = 'pending' AND expires_at IS NOT NULL",
        ]

        async with self._db_pool.connection_context() as conn:
            async with conn.cursor() as cursor:
                for statement in ddl_statements:
                    await cursor.execute(statement)
            await conn.connection.commit()

    async def create_action(
        self,
        *,
        tool_call_id: str,
        tool_name: str,
        arguments: str,
        operation_level: str,
        invocation_mode: str,
        requester_user_id: str | None,
        requester_user_roles: list[str] | None,
        reason: str,
        risk_level: str = "NORMAL",
        entity_type: str | None = None,
        entity_id: str | None = None,
        before_snapshot: Any | None = None,
        after_snapshot: Any | None = None,
        json_patch: Any | None = None,
        diff_summary: Any | None = None,
        correlation_id: str | None = None,
        ui_hints: dict[str, Any] | None = None,
        ttl_seconds: int | None = None,
    ) -> PendingAction:
        if not await self._ensure_initialized():
            return await self._fallback.create_action(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                arguments=arguments,
                operation_level=operation_level,
                invocation_mode=invocation_mode,
                requester_user_id=requester_user_id,
                requester_user_roles=requester_user_roles,
                reason=reason,
                risk_level=risk_level,
                entity_type=entity_type,
                entity_id=entity_id,
                before_snapshot=before_snapshot,
                after_snapshot=after_snapshot,
                json_patch=json_patch,
                diff_summary=diff_summary,
                correlation_id=correlation_id,
                ui_hints=ui_hints,
                ttl_seconds=ttl_seconds,
            )

        action_id = generate_id("pending_action")
        now = utc_now()
        expires_at = now + timedelta(seconds=ttl_seconds) if ttl_seconds and ttl_seconds > 0 else None
        query = """
            INSERT INTO ai_pending_actions (
                action_id, tool_call_id, tool_name, arguments,
                operation_level, invocation_mode,
                requester_user_id, requester_user_roles,
                reason, status,
                risk_level, entity_type, entity_id,
                before_snapshot, after_snapshot,
                json_patch, diff_summary,
                correlation_id, ui_hints, expires_at,
                created_at, updated_at
            ) VALUES (
                %s, %s, %s, %s,
                %s, %s,
                %s, %s,
                %s, %s,
                %s, %s, %s,
                %s, %s,
                %s, %s,
                %s, %s, %s,
                %s, %s
            )
        """

        try:
            async with self._db_pool.connection_context() as conn:
                await conn.execute(
                    query,
                    (
                        action_id,
                        tool_call_id,
                        tool_name,
                        arguments,
                        operation_level,
                        invocation_mode,
                        requester_user_id,
                        json.dumps(list(requester_user_roles or []), ensure_ascii=False),
                        reason,
                        PendingActionStatus.PENDING.value,
                        str(risk_level or "NORMAL"),
                        entity_type,
                        entity_id,
                        self._dumps_json(before_snapshot),
                        self._dumps_json(after_snapshot),
                        self._dumps_json(json_patch),
                        self._dumps_json(diff_summary),
                        correlation_id,
                        self._dumps_json(ui_hints or {}),
                        expires_at,
                        now,
                        now,
                    ),
                )
                await conn.connection.commit()

            return PendingAction(
                action_id=action_id,
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                arguments=arguments,
                operation_level=operation_level,
                invocation_mode=invocation_mode,
                requester_user_id=requester_user_id,
                requester_user_roles=list(requester_user_roles or []),
                reason=reason,
                status=PendingActionStatus.PENDING,
                created_at=now,
                updated_at=now,
                risk_level=str(risk_level or "NORMAL"),
                entity_type=entity_type,
                entity_id=entity_id,
                before_snapshot=before_snapshot,
                after_snapshot=after_snapshot,
                json_patch=json_patch,
                diff_summary=diff_summary,
                correlation_id=correlation_id,
                ui_hints=dict(ui_hints or {}),
                expires_at=expires_at,
                diff_source=str((ui_hints or {}).get("diff_source") or "").strip() or None,
            )
        except POSTGRES_EXCEPTIONS as exc:
            logger.warning(f"Create pending action in postgres failed, fallback to memory: {exc}")
            return await self._fallback.create_action(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                arguments=arguments,
                operation_level=operation_level,
                invocation_mode=invocation_mode,
                requester_user_id=requester_user_id,
                requester_user_roles=requester_user_roles,
                reason=reason,
                risk_level=risk_level,
                entity_type=entity_type,
                entity_id=entity_id,
                before_snapshot=before_snapshot,
                after_snapshot=after_snapshot,
                json_patch=json_patch,
                diff_summary=diff_summary,
                correlation_id=correlation_id,
                ui_hints=ui_hints,
                ttl_seconds=ttl_seconds,
            )

    async def get_action(self, action_id: str) -> PendingAction | None:
        if not await self._ensure_initialized():
            return await self._fallback.get_action(action_id)

        query = f"""
            SELECT {self._SELECT_COLUMNS}
            FROM ai_pending_actions
            WHERE action_id = %s
        """

        try:
            async with self._db_pool.connection_context() as conn:
                row = await conn.fetchrow(query, (action_id,))
            return self._row_to_pending_action(row) if row else None
        except POSTGRES_EXCEPTIONS as exc:
            logger.warning(f"Get pending action from postgres failed, fallback to memory: {exc}")
            return await self._fallback.get_action(action_id)

    async def list_actions(
        self,
        *,
        status: str | None = None,
        tool_name: str | None = None,
        entity_id: str | None = None,
        requester_user_id: str | None = None,
        created_after: datetime | None = None,
        created_before: datetime | None = None,
        limit: int = 50,
        offset: int = 0,
    ) -> list[PendingAction]:
        if not await self._ensure_initialized():
            return await self._fallback.list_actions(
                status=status,
                tool_name=tool_name,
                entity_id=entity_id,
                requester_user_id=requester_user_id,
                created_after=created_after,
                created_before=created_before,
                limit=limit,
                offset=offset,
            )

        query_parts = [
            f"""
            SELECT {self._SELECT_COLUMNS}
            FROM ai_pending_actions
            WHERE 1=1
            """
        ]
        params: list[Any] = []

        normalized_status = str(status or "").strip().lower()
        if normalized_status:
            params.append(normalized_status)
            query_parts.append("AND status = %s")

        normalized_tool_name = str(tool_name or "").strip()
        if normalized_tool_name:
            params.append(normalized_tool_name)
            query_parts.append("AND tool_name = %s")

        normalized_entity_id = str(entity_id or "").strip()
        if normalized_entity_id:
            params.append(normalized_entity_id)
            query_parts.append("AND entity_id = %s")

        normalized_requester = str(requester_user_id or "").strip()
        if normalized_requester:
            params.append(normalized_requester)
            query_parts.append("AND requester_user_id = %s")

        if created_after is not None:
            params.append(created_after)
            query_parts.append("AND created_at >= %s")

        if created_before is not None:
            params.append(created_before)
            query_parts.append("AND created_at <= %s")

        bounded_limit = max(1, min(int(limit or 50), 200))
        bounded_offset = max(0, int(offset or 0))
        query_parts.append("ORDER BY created_at DESC")
        params.append(bounded_limit)
        query_parts.append("LIMIT %s")
        params.append(bounded_offset)
        query_parts.append("OFFSET %s")

        try:
            async with self._db_pool.connection_context() as conn:
                rows = await conn.fetchall(" ".join(query_parts), tuple(params))
            return [self._row_to_pending_action(row) for row in rows if row]
        except POSTGRES_EXCEPTIONS as exc:
            logger.warning(f"List pending actions from postgres failed, fallback to memory: {exc}")
            return await self._fallback.list_actions(
                status=status,
                tool_name=tool_name,
                entity_id=entity_id,
                created_after=created_after,
                created_before=created_before,
                limit=limit,
                offset=offset,
            )

    async def count_actions(
        self,
        *,
        status: str | None = None,
        tool_name: str | None = None,
        entity_id: str | None = None,
        created_after: datetime | None = None,
        created_before: datetime | None = None,
    ) -> int:
        if not await self._ensure_initialized():
            return await self._fallback.count_actions(
                status=status,
                tool_name=tool_name,
                entity_id=entity_id,
                created_after=created_after,
                created_before=created_before,
            )

        query_parts = ["SELECT COUNT(*) AS count FROM ai_pending_actions WHERE 1=1"]
        params: list[Any] = []

        normalized_status = str(status or "").strip().lower()
        if normalized_status:
            params.append(normalized_status)
            query_parts.append("AND status = %s")

        normalized_tool_name = str(tool_name or "").strip()
        if normalized_tool_name:
            params.append(normalized_tool_name)
            query_parts.append("AND tool_name = %s")

        normalized_entity_id = str(entity_id or "").strip()
        if normalized_entity_id:
            params.append(normalized_entity_id)
            query_parts.append("AND entity_id = %s")

        if created_after is not None:
            params.append(created_after)
            query_parts.append("AND created_at >= %s")

        if created_before is not None:
            params.append(created_before)
            query_parts.append("AND created_at <= %s")

        try:
            async with self._db_pool.connection_context() as conn:
                row = await conn.fetchrow(" ".join(query_parts), tuple(params))
            if not row:
                return 0

            if isinstance(row, dict):
                return int(row.get("count", 0) or 0)

            row_dict = dict(row)
            if "count" in row_dict:
                return int(row_dict.get("count", 0) or 0)

            return int(next(iter(row_dict.values()), 0) or 0)
        except POSTGRES_EXCEPTIONS as exc:
            logger.warning(f"Count pending actions from postgres failed, fallback to memory: {exc}")
            return await self._fallback.count_actions(
                status=status,
                tool_name=tool_name,
                entity_id=entity_id,
                created_after=created_after,
                created_before=created_before,
            )

    async def approve_action(self, action_id: str, approver_id: str) -> PendingAction:
        if not await self._ensure_initialized():
            return await self._fallback.approve_action(action_id, approver_id)

        now = utc_now()
        expired_payload = self._dumps_json({"decision_blocked_reason": "expired"})
        query = f"""
            WITH mark_expired AS (
                UPDATE ai_pending_actions
                SET status = %s,
                    status_code = %s,
                    error_payload = %s,
                    updated_at = %s
                WHERE action_id = %s
                  AND status = %s
                  AND expires_at IS NOT NULL
                  AND expires_at <= %s
                RETURNING {self._SELECT_COLUMNS}
            ),
            mark_approved AS (
                UPDATE ai_pending_actions
                SET status = %s,
                    approved_by = %s,
                    approved_at = %s,
                    updated_at = %s
                WHERE action_id = %s
                  AND status = %s
                  AND (expires_at IS NULL OR expires_at > %s)
                RETURNING {self._SELECT_COLUMNS}
            )
            SELECT * FROM mark_approved
            UNION ALL
            SELECT * FROM mark_expired
            LIMIT 1
        """

        try:
            async with self._db_pool.connection_context() as conn:
                row = await conn.fetchrow(
                    query,
                    (
                        PendingActionStatus.EXPIRED.value,
                        "PENDING_ACTION_EXPIRED",
                        expired_payload,
                        now,
                        action_id,
                        PendingActionStatus.PENDING.value,
                        now,
                        PendingActionStatus.APPROVED.value,
                        approver_id,
                        now,
                        now,
                        action_id,
                        PendingActionStatus.PENDING.value,
                        now,
                    ),
                )
                await conn.connection.commit()

            if row:
                action = self._row_to_pending_action(row)
                if action.status == PendingActionStatus.EXPIRED:
                    raise PendingActionConflictError(
                        "pending action has expired and can no longer be approved",
                        code="PENDING_ACTION_EXPIRED",
                        decision_blocked_reason="expired",
                    )
                return action

            return await self._raise_state_error(action_id)
        except (KeyError, PendingActionConflictError):
            raise
        except POSTGRES_EXCEPTIONS as exc:
            logger.warning(f"Approve pending action in postgres failed, fallback to memory: {exc}")
            return await self._fallback.approve_action(action_id, approver_id)

    async def reject_action(self, action_id: str, approver_id: str, reason: str | None = None) -> PendingAction:
        if not await self._ensure_initialized():
            return await self._fallback.reject_action(action_id, approver_id, reason)

        now = utc_now()
        normalized_reason = (reason or "").strip() or "rejected_by_human"
        expired_payload = self._dumps_json({"decision_blocked_reason": "expired"})
        query = f"""
            WITH mark_expired AS (
                UPDATE ai_pending_actions
                SET status = %s,
                    status_code = %s,
                    error_payload = %s,
                    updated_at = %s
                WHERE action_id = %s
                  AND status = %s
                  AND expires_at IS NOT NULL
                  AND expires_at <= %s
                RETURNING {self._SELECT_COLUMNS}
            ),
            mark_rejected AS (
                UPDATE ai_pending_actions
                SET status = %s,
                    rejected_by = %s,
                    rejected_reason = %s,
                    rejected_at = %s,
                    updated_at = %s
                WHERE action_id = %s
                  AND status = %s
                  AND (expires_at IS NULL OR expires_at > %s)
                RETURNING {self._SELECT_COLUMNS}
            )
            SELECT * FROM mark_rejected
            UNION ALL
            SELECT * FROM mark_expired
            LIMIT 1
        """

        try:
            async with self._db_pool.connection_context() as conn:
                row = await conn.fetchrow(
                    query,
                    (
                        PendingActionStatus.EXPIRED.value,
                        "PENDING_ACTION_EXPIRED",
                        expired_payload,
                        now,
                        action_id,
                        PendingActionStatus.PENDING.value,
                        now,
                        PendingActionStatus.REJECTED.value,
                        approver_id,
                        normalized_reason,
                        now,
                        now,
                        action_id,
                        PendingActionStatus.PENDING.value,
                        now,
                    ),
                )
                await conn.connection.commit()

            if row:
                action = self._row_to_pending_action(row)
                if action.status == PendingActionStatus.EXPIRED:
                    raise PendingActionConflictError(
                        "pending action has expired and can no longer be rejected",
                        code="PENDING_ACTION_EXPIRED",
                        decision_blocked_reason="expired",
                    )
                return action

            return await self._raise_state_error(action_id)
        except (KeyError, PendingActionConflictError):
            raise
        except POSTGRES_EXCEPTIONS as exc:
            logger.warning(f"Reject pending action in postgres failed, fallback to memory: {exc}")
            return await self._fallback.reject_action(action_id, approver_id, reason)

    async def mark_executed(
        self,
        action_id: str,
        execution_result: Any,
        status_code: str | None = None,
        execution_receipt: Any | None = None,
    ) -> PendingAction:
        if not await self._ensure_initialized():
            return await self._fallback.mark_executed(
                action_id,
                execution_result,
                status_code=status_code,
                execution_receipt=execution_receipt,
            )

        now = utc_now()
        query = f"""
            UPDATE ai_pending_actions
            SET status = %s,
                execution_result = %s,
                execution_error = NULL,
                status_code = %s,
                error_payload = NULL,
                execution_receipt = %s,
                updated_at = %s
            WHERE action_id = %s
            RETURNING {self._SELECT_COLUMNS}
        """

        try:
            async with self._db_pool.connection_context() as conn:
                row = await conn.fetchrow(
                    query,
                    (
                        PendingActionStatus.EXECUTED.value,
                        self._dumps_json(execution_result),
                        status_code,
                        self._dumps_json(execution_receipt),
                        now,
                        action_id,
                    ),
                )
                await conn.connection.commit()

            if not row:
                raise KeyError(action_id)
            return self._row_to_pending_action(row)
        except KeyError:
            raise
        except POSTGRES_EXCEPTIONS as exc:
            logger.warning(f"Mark pending action executed in postgres failed, fallback to memory: {exc}")
            return await self._fallback.mark_executed(
                action_id,
                execution_result,
                status_code=status_code,
                execution_receipt=execution_receipt,
            )

    async def mark_failed(
        self,
        action_id: str,
        error_message: str,
        status_code: str | None = None,
        error_payload: Any | None = None,
        execution_receipt: Any | None = None,
    ) -> PendingAction:
        if not await self._ensure_initialized():
            return await self._fallback.mark_failed(
                action_id,
                error_message,
                status_code=status_code,
                error_payload=error_payload,
                execution_receipt=execution_receipt,
            )

        now = utc_now()
        query = f"""
            UPDATE ai_pending_actions
            SET status = %s,
                execution_error = %s,
                status_code = %s,
                error_payload = %s,
                execution_receipt = %s,
                updated_at = %s
            WHERE action_id = %s
            RETURNING {self._SELECT_COLUMNS}
        """

        try:
            async with self._db_pool.connection_context() as conn:
                row = await conn.fetchrow(
                    query,
                    (
                        PendingActionStatus.FAILED.value,
                        str(error_message or "execution_failed"),
                        status_code,
                        self._dumps_json(error_payload),
                        self._dumps_json(execution_receipt),
                        now,
                        action_id,
                    ),
                )
                await conn.connection.commit()

            if not row:
                raise KeyError(action_id)
            return self._row_to_pending_action(row)
        except KeyError:
            raise
        except POSTGRES_EXCEPTIONS as exc:
            logger.warning(f"Mark pending action failed in postgres failed, fallback to memory: {exc}")
            return await self._fallback.mark_failed(
                action_id,
                error_message,
                status_code=status_code,
                error_payload=error_payload,
                execution_receipt=execution_receipt,
            )

    async def clear(self) -> None:
        fallback_error: Exception | None = None
        if not await self._ensure_initialized():
            await self._fallback.clear()
            return

        try:
            async with self._db_pool.connection_context() as conn:
                await conn.execute("DELETE FROM ai_pending_actions")
                await conn.connection.commit()
        except POSTGRES_EXCEPTIONS as exc:
            fallback_error = exc
            logger.warning(f"Clear pending actions in postgres failed, fallback to memory: {exc}")

        await self._fallback.clear()
        if fallback_error:
            return

    def clear_sync(self) -> None:
        # 同步上下文下仅清理内存回退存储，避免阻塞或意外删除生产数据。
        self._fallback.clear_sync()

    async def expire_stale_actions(self, before: Any) -> list[PendingAction]:
        """Mark all pending actions whose expires_at < before as expired (Postgres)."""
        if not await self._ensure_initialized():
            return await self._fallback.expire_stale_actions(before)

        now = utc_now()
        query = f"""
            UPDATE ai_pending_actions
            SET status = %s,
                status_code = %s,
                error_payload = %s,
                updated_at = %s
            WHERE status = %s AND expires_at IS NOT NULL AND expires_at < %s
            RETURNING {self._SELECT_COLUMNS}
        """

        try:
            async with self._db_pool.connection_context() as conn:
                rows = await conn.fetchall(
                    query,
                    (
                        PendingActionStatus.EXPIRED.value,
                        "PENDING_ACTION_EXPIRED",
                        self._dumps_json({"decision_blocked_reason": "expired"}),
                        now,
                        PendingActionStatus.PENDING.value,
                        before,
                    ),
                )
                await conn.connection.commit()
            return [self._row_to_pending_action(row) for row in rows if row]
        except POSTGRES_EXCEPTIONS as exc:
            logger.warning(f"Expire stale actions in postgres failed, fallback to memory: {exc}")
            return await self._fallback.expire_stale_actions(before)

    async def _raise_state_error(self, action_id: str) -> PendingAction:
        existing = await self.get_action(action_id)
        if existing is None:
            raise KeyError(action_id)
        if existing.status == PendingActionStatus.EXPIRED:
            raise PendingActionConflictError(
                "pending action has expired and can no longer be processed",
                code="PENDING_ACTION_EXPIRED",
                decision_blocked_reason="expired",
            )
        raise PendingActionConflictError(
            f"pending action state invalid: {existing.status.value}",
            code="PENDING_ACTION_STATE_CONFLICT",
        )

    def _row_to_pending_action(self, row: dict[str, Any]) -> PendingAction:
        if not isinstance(row, dict):
            row = dict(row)

        requester_roles = row.get("requester_user_roles")
        if isinstance(requester_roles, str):
            try:
                requester_roles = json.loads(requester_roles)
            except (json.JSONDecodeError, TypeError, ValueError):
                requester_roles = []

        if not isinstance(requester_roles, list):
            requester_roles = []

        execution_result = self._loads_json(row.get("execution_result"), row.get("execution_result"))
        before_snapshot = self._loads_json(row.get("before_snapshot"), row.get("before_snapshot"))
        after_snapshot = self._loads_json(row.get("after_snapshot"), row.get("after_snapshot"))
        json_patch = self._loads_json(row.get("json_patch"), row.get("json_patch"))
        diff_summary = self._loads_json(row.get("diff_summary"), row.get("diff_summary"))
        execution_receipt = self._loads_json(row.get("execution_receipt"), row.get("execution_receipt"))
        error_payload = self._loads_json(row.get("error_payload"), row.get("error_payload"))
        ui_hints = self._loads_json(row.get("ui_hints"), row.get("ui_hints") or {})
        if not isinstance(ui_hints, dict):
            ui_hints = {}

        diff_source = str(ui_hints.get("diff_source") or "").strip() or None
        decision_blocked_reason = None
        if isinstance(error_payload, dict):
            decision_blocked_reason = str(error_payload.get("decision_blocked_reason") or "").strip() or None
        if (
            not decision_blocked_reason
            and str(row.get("status") or "").strip().lower() == PendingActionStatus.EXPIRED.value
        ):
            decision_blocked_reason = "expired"

        return PendingAction(
            action_id=str(row["action_id"]),
            tool_call_id=str(row["tool_call_id"]),
            tool_name=str(row["tool_name"]),
            arguments=str(row.get("arguments") or "{}"),
            operation_level=str(row.get("operation_level") or ""),
            invocation_mode=str(row.get("invocation_mode") or ""),
            requester_user_id=row.get("requester_user_id"),
            requester_user_roles=list(requester_roles or []),
            reason=str(row.get("reason") or ""),
            status=PendingActionStatus(str(row.get("status") or PendingActionStatus.PENDING.value)),
            created_at=row.get("created_at"),
            updated_at=row.get("updated_at"),
            approved_by=row.get("approved_by"),
            approved_at=row.get("approved_at"),
            rejected_by=row.get("rejected_by"),
            rejected_reason=row.get("rejected_reason"),
            rejected_at=row.get("rejected_at"),
            execution_result=execution_result,
            execution_error=row.get("execution_error"),
            risk_level=str(row.get("risk_level") or "NORMAL"),
            entity_type=row.get("entity_type"),
            entity_id=row.get("entity_id"),
            before_snapshot=before_snapshot,
            after_snapshot=after_snapshot,
            json_patch=json_patch,
            diff_summary=diff_summary,
            execution_receipt=execution_receipt,
            status_code=row.get("status_code"),
            error_payload=error_payload,
            correlation_id=str(row.get("correlation_id")).strip() if row.get("correlation_id") else None,
            ui_hints=ui_hints,
            expires_at=row.get("expires_at"),
            diff_source=diff_source,
            decision_blocked_reason=decision_blocked_reason,
        )

    @staticmethod
    def _loads_json(value: Any, default: Any = None) -> Any:
        if value is None:
            return default
        if isinstance(value, (dict, list, bool, int, float)):
            return value
        if isinstance(value, (bytes, bytearray)):
            value = value.decode("utf-8", errors="ignore")
        if isinstance(value, str):
            text = value.strip()
            if not text:
                return default
            try:
                return json.loads(text)
            except (json.JSONDecodeError, TypeError, ValueError):
                return default
        return default

    @staticmethod
    def _dumps_json(value: Any) -> str | None:
        if value is None:
            return None
        return json.dumps(value, ensure_ascii=False, default=str)


PendingActionStore = MemoryPendingActionStore

_pending_action_store: PendingActionStoreProtocol | None = None


def _sync_pending_action_store(store: PendingActionStoreProtocol | None) -> PendingActionStoreProtocol | None:
    global _pending_action_store

    _pending_action_store = store
    runtime_holder = get_runtime_holder()
    if runtime_holder is not None:
        runtime_holder.pending_action_store = store
    return store


def get_pending_action_store() -> PendingActionStoreProtocol:
    runtime_holder = get_runtime_holder()
    if runtime_holder is not None:
        runtime_store = getattr(runtime_holder, "pending_action_store", None)
        if runtime_store is not None:
            return _sync_pending_action_store(runtime_store)

    if _pending_action_store is None:
        return _sync_pending_action_store(MemoryPendingActionStore())
    return _sync_pending_action_store(_pending_action_store)


def set_pending_action_store(store: PendingActionStoreProtocol) -> None:
    _sync_pending_action_store(store)
