"""Models for the pending action queue (dataclasses, enums, protocols)."""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime
from enum import StrEnum
from typing import Any, Protocol


class PendingActionStatus(StrEnum):
    """Status of a human-approval action."""

    PENDING = "pending"
    APPROVED = "approved"
    REJECTED = "rejected"
    EXECUTED = "executed"
    FAILED = "failed"
    EXPIRED = "expired"


class PendingActionConflictError(RuntimeError):
    """Stable conflict error for pending action state transitions."""

    def __init__(
        self,
        message: str,
        *,
        code: str = "PENDING_ACTION_STATE_CONFLICT",
        decision_blocked_reason: str | None = None,
    ):
        super().__init__(message)
        self.code = str(code or "PENDING_ACTION_STATE_CONFLICT")
        self.decision_blocked_reason = (
            str(decision_blocked_reason).strip() if decision_blocked_reason is not None else None
        ) or None


@dataclass
class PendingAction:
    """Represents one approval-required tool invocation."""

    action_id: str
    tool_call_id: str
    tool_name: str
    arguments: str
    operation_level: str
    invocation_mode: str
    requester_user_id: str | None
    requester_user_roles: list[str]
    reason: str
    status: PendingActionStatus
    created_at: Any
    updated_at: Any
    approved_by: str | None = None
    approved_at: Any | None = None
    rejected_by: str | None = None
    rejected_reason: str | None = None
    rejected_at: Any | None = None
    execution_result: Any | None = None
    execution_error: str | None = None
    risk_level: str = "NORMAL"
    entity_type: str | None = None
    entity_id: str | None = None
    before_snapshot: Any | None = None
    after_snapshot: Any | None = None
    json_patch: Any | None = None
    diff_summary: Any | None = None
    execution_receipt: Any | None = None
    status_code: str | None = None
    error_payload: Any | None = None
    correlation_id: str | None = None
    ui_hints: dict[str, Any] = field(default_factory=dict)
    expires_at: Any | None = None
    diff_source: str | None = None
    decision_blocked_reason: str | None = None

    def to_dict(self) -> dict[str, Any]:
        resolved_diff_source = self.diff_source
        if not resolved_diff_source and isinstance(self.ui_hints, dict):
            ui_diff_source = self.ui_hints.get("diff_source")
            if ui_diff_source is not None:
                resolved_diff_source = str(ui_diff_source).strip() or None

        resolved_blocked_reason = self.decision_blocked_reason
        if not resolved_blocked_reason and isinstance(self.error_payload, dict):
            payload_reason = self.error_payload.get("decision_blocked_reason")
            if payload_reason is not None:
                resolved_blocked_reason = str(payload_reason).strip() or None

        if not resolved_blocked_reason and self.status == PendingActionStatus.EXPIRED:
            resolved_blocked_reason = "expired"

        return {
            "action_id": self.action_id,
            "tool_call_id": self.tool_call_id,
            "tool_name": self.tool_name,
            "arguments": self.arguments,
            "operation_level": self.operation_level,
            "invocation_mode": self.invocation_mode,
            "requester_user_id": self.requester_user_id,
            "requester_user_roles": list(self.requester_user_roles or []),
            "reason": self.reason,
            "status": self.status.value,
            "created_at": self.created_at.isoformat()
            if hasattr(self.created_at, "isoformat")
            else str(self.created_at),
            "updated_at": self.updated_at.isoformat()
            if hasattr(self.updated_at, "isoformat")
            else str(self.updated_at),
            "approved_by": self.approved_by,
            "approved_at": self.approved_at.isoformat() if hasattr(self.approved_at, "isoformat") else None,
            "rejected_by": self.rejected_by,
            "rejected_reason": self.rejected_reason,
            "rejected_at": self.rejected_at.isoformat() if hasattr(self.rejected_at, "isoformat") else None,
            "execution_result": self.execution_result,
            "execution_error": self.execution_error,
            "risk_level": self.risk_level,
            "entity_type": self.entity_type,
            "entity_id": self.entity_id,
            "before_snapshot": self.before_snapshot,
            "after_snapshot": self.after_snapshot,
            "json_patch": self.json_patch,
            "diff_summary": self.diff_summary,
            "execution_receipt": self.execution_receipt,
            "status_code": self.status_code,
            "error_payload": self.error_payload,
            "correlation_id": self.correlation_id,
            "ui_hints": dict(self.ui_hints or {}),
            "expires_at": self.expires_at.isoformat() if hasattr(self.expires_at, "isoformat") else None,
            "diff_source": resolved_diff_source,
            "decision_blocked_reason": resolved_blocked_reason,
        }


class PendingActionStoreProtocol(Protocol):
    """Minimal protocol required by ToolRegistry."""

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
    ) -> PendingAction: ...

    async def get_action(self, action_id: str) -> PendingAction | None: ...

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
    ) -> list[PendingAction]: ...

    async def count_actions(
        self,
        *,
        status: str | None = None,
        tool_name: str | None = None,
        entity_id: str | None = None,
        created_after: datetime | None = None,
        created_before: datetime | None = None,
    ) -> int: ...

    async def approve_action(self, action_id: str, approver_id: str) -> PendingAction: ...

    async def reject_action(self, action_id: str, approver_id: str, reason: str | None = None) -> PendingAction: ...

    async def mark_executed(
        self,
        action_id: str,
        execution_result: Any,
        status_code: str | None = None,
        execution_receipt: Any | None = None,
    ) -> PendingAction: ...

    async def mark_failed(
        self,
        action_id: str,
        error_message: str,
        status_code: str | None = None,
        error_payload: Any | None = None,
        execution_receipt: Any | None = None,
    ) -> PendingAction: ...

    async def expire_stale_actions(self, before: Any) -> list[PendingAction]: ...

    async def clear(self) -> None: ...

    def clear_sync(self) -> None: ...
