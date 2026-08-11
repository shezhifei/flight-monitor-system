"""
AIP Action Executor - 集成 HITL 的 Action 执行器

负责执行 Ontology Actions，集成现有的 PendingAction 审批机制，
支持对象级权限检查和变更差异计算。
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from enum import StrEnum
from typing import TYPE_CHECKING, Any

from src.infrastructure.logging.core import get_logger

if TYPE_CHECKING:
    from ..ontology.schema import OntologyRegistry
    from ..ontology.security.object_acl import ObjectACL, Permission, PermissionCheckResult

logger = get_logger(__name__)


class ExecutionStatus(StrEnum):
    """执行状态"""

    PENDING = "pending"
    APPROVED = "approved"
    REJECTED = "rejected"
    EXECUTING = "executing"
    COMPLETED = "completed"
    FAILED = "failed"
    PENDING_APPROVAL = "pending_approval"


@dataclass
class ExecutionResult:
    """执行结果"""

    status: ExecutionStatus
    object_type: str
    object_id: str
    action: str
    parameters: dict[str, Any]
    result: dict[str, Any] | None = None
    error: str | None = None
    pending_action_id: str | None = None
    change_preview: dict[str, Any] | None = None
    execution_time_ms: float = 0.0
    metadata: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        return {
            "status": self.status.value,
            "object_type": self.object_type,
            "object_id": self.object_id,
            "action": self.action,
            "parameters": self.parameters,
            "result": self.result,
            "error": self.error,
            "pending_action_id": self.pending_action_id,
            "change_preview": self.change_preview,
            "execution_time_ms": self.execution_time_ms,
            "metadata": self.metadata,
        }


@dataclass
class ChangePreview:
    """变更预览"""

    object_type: str
    object_id: str
    action: str
    before_state: dict[str, Any]
    after_state: dict[str, Any]
    property_changes: list[dict[str, Any]] = field(default_factory=list)
    relationship_changes: list[dict[str, Any]] = field(default_factory=list)
    risk_level: str = "NORMAL"
    summary: str = ""

    def to_dict(self) -> dict[str, Any]:
        return {
            "object_type": self.object_type,
            "object_id": self.object_id,
            "action": self.action,
            "before_state": self.before_state,
            "after_state": self.after_state,
            "property_changes": self.property_changes,
            "relationship_changes": self.relationship_changes,
            "risk_level": self.risk_level,
            "summary": self.summary,
        }


class AIPActionExecutor:
    """
    AIP Action 执行器

    核心职责：
    1. 权限检查（ObjectACL）
    2. 审批决策（基于风险等级）
    3. 变更差异计算
    4. 集成现有 PendingAction 审批机制
    5. Action 执行与结果封装
    """

    def __init__(
        self,
        ontology_registry: OntologyRegistry,
        object_acl: ObjectACL,
        pending_action_store: Any,
        action_handlers: dict[str, Any] | None = None,
    ):
        self._ontology = ontology_registry
        self._acl = object_acl
        self._pending_store = pending_action_store
        self._action_handlers = action_handlers or {}
        self._default_ttl = 600

    def register_handler(self, object_type: str, action: str, handler: Any) -> None:
        """注册 Action 处理器"""
        key = f"{object_type}.{action}"
        self._action_handlers[key] = handler

    async def execute(
        self,
        *,
        principal: str,
        object_type: str,
        object_id: str,
        action: str,
        parameters: dict[str, Any],
        invocation_mode: str = "user_requested",
        skip_approval: bool = False,
    ) -> ExecutionResult:
        """
        执行 Action

        Args:
            principal: 执行主体，格式为 "user:xxx" 或 "role:xxx"
            object_type: 对象类型
            object_id: 对象ID
            action: Action 名称
            parameters: Action 参数
            invocation_mode: 调用模式
            skip_approval: 是否跳过审批（用于测试或特殊场景）

        Returns:
            ExecutionResult
        """
        import time

        start_time = time.time()

        action_def = self._ontology.get_action(action, "default")
        if not action_def or action_def.object_type != object_type:
            return ExecutionResult(
                status=ExecutionStatus.FAILED,
                object_type=object_type,
                object_id=object_id,
                action=action,
                parameters=parameters,
                error=f"Action '{action}' not found for object type '{object_type}'",
                execution_time_ms=(time.time() - start_time) * 1000,
            )

        permission_check = self._check_permission(principal, object_type, object_id, action_def, parameters)

        if not permission_check.allowed:
            return ExecutionResult(
                status=ExecutionStatus.FAILED,
                object_type=object_type,
                object_id=object_id,
                action=action,
                parameters=parameters,
                error=f"Permission denied: {permission_check.reason}",
                execution_time_ms=(time.time() - start_time) * 1000,
                metadata={"permission_check": permission_check.__dict__},
            )

        requires_approval = self._should_require_approval(action_def, parameters, permission_check)

        if requires_approval and not skip_approval:
            pending_action = await self._create_pending_action(
                principal=principal,
                object_type=object_type,
                object_id=object_id,
                action=action,
                parameters=parameters,
                action_def=action_def,
            )

            change_preview = await self._compute_change_preview(object_type, object_id, action, parameters)

            return ExecutionResult(
                status=ExecutionStatus.PENDING_APPROVAL,
                object_type=object_type,
                object_id=object_id,
                action=action,
                parameters=parameters,
                pending_action_id=pending_action.action_id,
                change_preview=change_preview.to_dict() if change_preview else None,
                execution_time_ms=(time.time() - start_time) * 1000,
                metadata={"approval_reason": "risk_level_threshold"},
            )

        result = await self._execute_action(object_type, object_id, action, parameters)

        return ExecutionResult(
            status=ExecutionStatus.COMPLETED,
            object_type=object_type,
            object_id=object_id,
            action=action,
            parameters=parameters,
            result=result,
            execution_time_ms=(time.time() - start_time) * 1000,
        )

    def _check_permission(
        self, principal: str, object_type: str, object_id: str, action_def: Any, parameters: dict[str, Any]
    ) -> PermissionCheckResult:
        """检查执行权限"""
        permission = self._map_action_to_permission(action_def)

        check_result = self._acl.check_permission(
            principal=principal, object_type=object_type, object_id=object_id, permission=permission, context=parameters
        )

        if not check_result.allowed:
            return check_result

        return check_result

    def _should_require_approval(
        self, action_def: Any, parameters: dict[str, Any], permission_result: PermissionCheckResult
    ) -> bool:
        """判断是否需要审批"""
        if action_def.requires_approval:
            return True

        risk_level = getattr(action_def, "risk_level", "NORMAL")
        if risk_level in ("HIGH", "CRITICAL"):
            return True

        return bool(permission_result.requires_approval)

    async def _create_pending_action(
        self,
        *,
        principal: str,
        object_type: str,
        object_id: str,
        action: str,
        parameters: dict[str, Any],
        action_def: Any,
    ) -> Any:
        """创建待审批 Action"""

        operation_level = self._map_risk_to_operation_level(getattr(action_def, "risk_level", "NORMAL"))

        change_preview = await self._compute_change_preview(object_type, object_id, action, parameters)

        principal_parts = principal.split(":", 1)
        user_id = principal_parts[1] if len(principal_parts) > 1 else principal
        user_roles = []

        pending_action = await self._pending_store.create_action(
            tool_call_id=f"{object_type}.{action}",
            tool_name=f"{object_type}:{action}",
            arguments=json.dumps(parameters),
            operation_level=operation_level.value,
            invocation_mode="user_requested",
            requester_user_id=user_id,
            requester_user_roles=user_roles,
            reason=f"Action '{action}' requires approval",
            entity_type=object_type,
            entity_id=object_id,
            before_snapshot=change_preview.before_state if change_preview else {},
            after_snapshot=change_preview.after_state if change_preview else {},
            json_patch=self._compute_json_patch(
                change_preview.before_state if change_preview else {},
                change_preview.after_state if change_preview else {},
            ),
            diff_summary={
                "property_changes": len(change_preview.property_changes) if change_preview else 0,
                "relationship_changes": len(change_preview.relationship_changes) if change_preview else 0,
            },
            ui_hints={
                "show_diff": True,
                "show_receipt": True,
                "ttl_seconds": self._default_ttl,
                "diff_source": "ontology_aware",
            },
        )

        return pending_action

    async def _compute_change_preview(
        self, object_type: str, object_id: str, action: str, parameters: dict[str, Any]
    ) -> ChangePreview | None:
        """计算变更预览"""
        current_state = await self._get_object_state(object_type, object_id)

        if not current_state:
            return None

        after_state = self._simulate_action(current_state, action, parameters)

        property_changes = self._compute_property_changes(current_state, after_state)

        return ChangePreview(
            object_type=object_type,
            object_id=object_id,
            action=action,
            before_state=current_state,
            after_state=after_state,
            property_changes=property_changes,
            risk_level=getattr(self._ontology.get_action(action), "risk_level", "NORMAL")
            if self._ontology.get_action(action)
            else "NORMAL",
        )

    async def _get_object_state(self, object_type: str, object_id: str) -> dict[str, Any]:
        """获取对象当前状态"""
        try:
            from .data_access import get_object_accessor

            accessor = get_object_accessor()
            state = await accessor.get_object_state(object_type, object_id)
            return state if state else {}

        except Exception as exc:  # noqa: BLE001 - accessor call may fail in various ways
            logger.warning(f"Failed to get object state from accessor: {exc}")
            return {}

    def _simulate_action(
        self, current_state: dict[str, Any], action: str, parameters: dict[str, Any]
    ) -> dict[str, Any]:
        """模拟 Action 执行后的状态"""
        after_state = current_state.copy()

        if "stand" in parameters:
            after_state["stand"] = parameters["stand"]
        if "status" in parameters:
            after_state["status"] = parameters["status"]
        if "delay_minutes" in parameters:
            after_state["delay_minutes"] = parameters["delay_minutes"]
        if "location" in parameters:
            after_state["location"] = parameters["location"]

        return after_state

    def _compute_property_changes(self, before: dict[str, Any], after: dict[str, Any]) -> list[dict[str, Any]]:
        """计算属性变更"""
        changes = []

        all_keys = set(before.keys()) | set(after.keys())

        for key in all_keys:
            before_val = before.get(key)
            after_val = after.get(key)

            if before_val != after_val:
                changes.append({"property": key, "before": before_val, "after": after_val})

        return changes

    @staticmethod
    def _compute_json_patch(before: dict[str, Any], after: dict[str, Any]) -> list[dict[str, Any]]:
        """计算 JSON Patch"""
        patch = []
        all_keys = set(before.keys()) | set(after.keys())

        for key in all_keys:
            before_val = before.get(key)
            after_val = after.get(key)

            if before_val is None and after_val is not None:
                patch.append({"op": "add", "path": f"/{key}", "value": after_val})
            elif before_val is not None and after_val is None:
                patch.append({"op": "remove", "path": f"/{key}"})
            elif before_val != after_val:
                patch.append({"op": "replace", "path": f"/{key}", "value": after_val})

        return patch

    async def _execute_action(
        self, object_type: str, object_id: str, action: str, parameters: dict[str, Any]
    ) -> dict[str, Any]:
        """执行 Action"""
        handler_key = f"{object_type}.{action}"
        handler = self._action_handlers.get(handler_key)

        if handler and callable(handler):
            result = handler(object_id, parameters)
            if hasattr(result, "__await__"):
                result = await result
            return result if isinstance(result, dict) else {"result": result}

        return {
            "executed": True,
            "object_type": object_type,
            "object_id": object_id,
            "action": action,
            "parameters": parameters,
        }

    def _map_action_to_permission(self, action_def: Any) -> Permission:
        """将 Action 映射到权限类型"""
        from ..ontology.security.object_acl import Permission

        category = getattr(action_def, "category", "mutation")

        return {
            "mutation": Permission.WRITE,
            "query": Permission.READ,
            "delete": Permission.DELETE,
        }.get(category, Permission.EXECUTE)

    @staticmethod
    def _map_risk_to_operation_level(risk_level: str) -> Any:
        """将风险等级映射到操作级别"""
        from src.infrastructure.ai.tools.base import OperationLevel

        mapping = {
            "LOW": OperationLevel.WORKSPACE_WRITE,
            "NORMAL": OperationLevel.WORKSPACE_WRITE,
            "MEDIUM": OperationLevel.ASSISTED_WRITE,
            "HIGH": OperationLevel.ASSISTED_WRITE,
            "CRITICAL": OperationLevel.CRITICAL_WRITE,
        }

        return mapping.get(risk_level.upper(), OperationLevel.WORKSPACE_WRITE)


__all__ = [
    "AIPActionExecutor",
    "ChangePreview",
    "ExecutionResult",
    "ExecutionStatus",
]
