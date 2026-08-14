"""AI tool registry service: ToolRegistry class and registry lifecycle functions."""

import asyncio
import json
from contextlib import contextmanager
from datetime import datetime
from typing import Any

from src.domain.utils.time_utils import utc_now
from src.infrastructure.ai.monitoring.metrics import record_query_tool_selection
from src.infrastructure.common.runtime_utils import get_runtime_holder
from src.infrastructure.logging.core import get_logger

from ..base import (
    BaseToolDefinition,
    BaseToolExecutor,
    InvocationMode,
    OperationLevel,
    ToolCategory,
    ToolExecutionResult,
    ToolExecutionStatus,
)
from ..diff_utils import build_json_patch
from ..pending_actions import PendingActionConflictError, get_pending_action_store
from ..permissions import ToolPermissionManager
from . import models

logger = get_logger(__name__)


def _try_get_notification_service() -> Any | None:
    """Safely resolve notification service from runtime providers."""
    try:
        import importlib

        deps = importlib.import_module("src.application.api.dependencies")
        return deps.get_notification_service()
    except Exception as exc:  # noqa: BLE001 - bootstrap must catch all init failures
        logger.debug("notification_service_resolve_failed", exc_info=exc)
        return None


async def _send_pending_action_notification(
    *,
    action_id: str,
    tool_name: str,
    requester_user_id: str | None,
    status: str = "pending",
) -> None:
    """Non-blocking notification for pending action lifecycle events."""
    if not requester_user_id:
        return
    notification_service = _try_get_notification_service()
    if notification_service is None:
        return

    severity_map = {
        "pending": "warning",
        "approved": "info",
        "rejected": "warning",
    }
    title_map = {
        "pending": f"AI 工具 '{tool_name}' 已进入审批队列",
        "approved": f"AI 工具 '{tool_name}' 审批已通过",
        "rejected": f"AI 工具 '{tool_name}' 审批已被拒绝",
    }

    try:
        await notification_service.send(
            user_id=requester_user_id,
            title=title_map.get(status, title_map["pending"]),
            body=f"动作 {action_id} 状态: {status}",
            category="ai_approval",
            severity=severity_map.get(status, "warning"),
            related_entity_type="pending_action",
            related_entity_id=action_id,
        )
    except Exception as exc:  # noqa: BLE001 - best-effort side effect must not abort main flow
        logger.debug(
            "non-blocking pending action notification failed action_id=%s status=%s: %s",
            action_id,
            status,
            exc,
        )


class ToolRegistry:
    """Registry for tool definitions, executors, and permissions."""

    def __init__(self):
        self._tools: dict[str, tuple[BaseToolDefinition, ToolCategory]] = {}

        self._executors: dict[ToolCategory, BaseToolExecutor] = {}

        self.permission_manager = ToolPermissionManager()
        self._require_user_context = False

        logger.info("ToolRegistry initialized")

    def set_require_user_context(self, required: bool) -> None:
        """设置是否要求工具调用必须携带用户上下文。"""
        self._require_user_context = bool(required)

    def requires_user_context(self) -> bool:
        """返回当前是否要求用户上下文。"""
        return self._require_user_context

    def register_tool(self, definition: BaseToolDefinition, category: ToolCategory | None = None) -> None:
        """
        注册单个工具

        Args:
            definition: 工具定义
            category: 工具类别（如果定义中未指定）
        """
        tool_category = category or definition.category or ToolCategory.CUSTOM
        tool_name = definition.name

        previous = self._tools.get(tool_name)
        self._tools[tool_name] = (definition, tool_category)

        if previous is None:
            logger.debug(f"Registered tool: {tool_name} (category: {tool_category.value})")
        else:
            logger.debug(f"Updated tool: {tool_name} (version {previous[0].version} -> {definition.version})")

    def register_tools(self, definitions: list[BaseToolDefinition], category: ToolCategory | None = None) -> None:
        """批量注册工具"""
        for definition in definitions:
            self.register_tool(definition, category)

    def unregister_tool(self, tool_name: str) -> None:
        """注销工具"""
        if tool_name in self._tools:
            del self._tools[tool_name]
            logger.info(f"Unregistered tool: {tool_name}")

    def register_executor(self, executor: BaseToolExecutor) -> None:
        """注册工具执行器"""
        category = executor.get_category()
        self._executors[category] = executor
        logger.debug(f"Registered executor for category: {category.value}")

    def _is_allowed_by_user(
        self,
        tool_name: str,
        user_id: str | None,
        user_roles: list[str] | None,
    ) -> bool:
        normalized_roles = [str(role).strip() for role in (user_roles or []) if str(role).strip()]
        lowered_roles = {role.lower() for role in normalized_roles}

        # 管理员兜底
        if "admin" in lowered_roles:
            return True

        if not user_id and not normalized_roles:
            if self._require_user_context:
                return False
            return self.permission_manager.check_permission(tool_name, "", [])

        return self.permission_manager.check_permission(tool_name, user_id or "", normalized_roles)

    def _normalize_invocation_mode(
        self,
        invocation_mode: str | InvocationMode | None,
    ) -> InvocationMode:
        if isinstance(invocation_mode, InvocationMode):
            return invocation_mode

        if isinstance(invocation_mode, str):
            mode_value = invocation_mode.strip()
            if mode_value:
                try:
                    return InvocationMode(mode_value)
                except ValueError:
                    logger.warning(f"Unknown invocation mode '{invocation_mode}', fallback to user_requested")

        return InvocationMode.USER_REQUESTED

    def _get_tool_operation_level(self, tool_name: str) -> OperationLevel:
        if tool_name not in self._tools:
            return OperationLevel.READ

        definition, _ = self._tools[tool_name]
        raw_level = getattr(definition, "operation_level", OperationLevel.READ)
        if isinstance(raw_level, OperationLevel):
            return raw_level

        try:
            return OperationLevel(str(raw_level))
        except ValueError:
            logger.warning(f"Unknown operation level '{raw_level}' for tool '{tool_name}', fallback to READ")
            return OperationLevel.READ

    def _is_allowed_by_invocation_mode(
        self,
        tool_name: str,
        invocation_mode: str | InvocationMode | None,
    ) -> bool:
        mode = self._normalize_invocation_mode(invocation_mode)
        operation_level = self._get_tool_operation_level(tool_name)
        return mode.allows(operation_level)

    @staticmethod
    def _safe_decode_arguments(arguments: str) -> dict[str, Any]:
        if not isinstance(arguments, str):
            return {}
        raw = arguments.strip()
        if not raw:
            return {}
        try:
            parsed = json.loads(raw)
            return parsed if isinstance(parsed, dict) else {}
        except (json.JSONDecodeError, TypeError, ValueError):
            return {}

    @staticmethod
    def _approval_result_payload(execution_result: ToolExecutionResult) -> dict[str, Any]:
        return {
            "status": execution_result.status.value,
            "code": execution_result.code,
            "message": execution_result.message,
            "recoverable": execution_result.recoverable,
            "retryable": execution_result.retryable,
            "severity": execution_result.severity,
            "result": execution_result.result,
            "error": execution_result.error_message,
        }

    @staticmethod
    def _derive_entity_type(tool_name: str, args: dict[str, Any]) -> str:
        explicit = str(args.get("entity_type") or "").strip()
        if explicit:
            return explicit

        lowered = str(tool_name or "").lower()
        if "rule" in lowered:
            return "rule"
        if "task" in lowered or "todo" in lowered:
            return "task"
        if "alert" in lowered or "anomaly" in lowered:
            return "alert"
        return "config"

    @staticmethod
    def _derive_entity_id(args: dict[str, Any]) -> str | None:
        for key in ("entity_id", "id", "rule_id", "task_id", "todo_id", "alert_id", "flight_id"):
            value = args.get(key)
            if value is None:
                continue
            normalized = str(value).strip()
            if normalized:
                return normalized
        return None

    @staticmethod
    def _derive_risk_level(level: OperationLevel) -> str:
        if level == OperationLevel.CRITICAL_WRITE:
            return "CRITICAL_WRITE"
        if level == OperationLevel.ASSISTED_WRITE:
            return "ASSISTED_WRITE"
        return "NORMAL"

    @staticmethod
    def _is_audited_write_level(level: OperationLevel) -> bool:
        return level in {
            OperationLevel.ASSISTED_WRITE,
            OperationLevel.CRITICAL_WRITE,
        }

    @staticmethod
    def _summarize_payload(payload: Any, *, max_items: int = 12, max_chars: int = 200) -> Any:
        if isinstance(payload, dict):
            summary: dict[str, Any] = {}
            for index, (key, value) in enumerate(payload.items()):
                if index >= max_items:
                    summary["..."] = f"{len(payload) - max_items} more keys"
                    break
                summary[str(key)] = ToolRegistry._summarize_payload(value, max_items=4, max_chars=max_chars)
            return summary

        if isinstance(payload, list):
            subset = payload[:max_items]
            summarized = [ToolRegistry._summarize_payload(item, max_items=4, max_chars=max_chars) for item in subset]
            if len(payload) > max_items:
                summarized.append(f"... {len(payload) - max_items} more items")
            return summarized

        text = str(payload)
        if len(text) <= max_chars:
            return payload
        return text[:max_chars] + "...(truncated)"

    def _emit_write_audit(
        self,
        *,
        event_status: str,
        tool_name: str,
        operation_level: OperationLevel,
        user_id: str | None,
        arguments: str | None = None,
        result: Any = None,
        error_message: str | None = None,
        pending_action: dict[str, Any] | None = None,
        approver_id: str | None = None,
        invocation_mode: str | None = None,
    ) -> None:
        if not self._is_audited_write_level(operation_level):
            return

        pending_payload = pending_action if isinstance(pending_action, dict) else {}
        parsed_args = self._safe_decode_arguments(arguments or "")
        if not parsed_args and pending_payload.get("arguments"):
            parsed_args = self._safe_decode_arguments(str(pending_payload.get("arguments")))

        entity_type = (
            str(pending_payload.get("entity_type") or "").strip()
            or self._derive_entity_type(tool_name, parsed_args)
            or "ai_tool"
        )
        entity_id = str(pending_payload.get("entity_id") or "").strip() or self._derive_entity_id(parsed_args) or ""
        actor_user_id = (
            str(approver_id or "").strip()
            or str(user_id or "").strip()
            or str(pending_payload.get("approved_by") or "").strip()
            or str(pending_payload.get("requester_user_id") or "").strip()
        )

        changes: dict[str, Any] = {
            "status": event_status,
            "tool_name": tool_name,
            "operation_level": operation_level.value,
            "invocation_mode": invocation_mode or pending_payload.get("invocation_mode"),
            "arguments_summary": self._summarize_payload(parsed_args),
            "result_summary": self._summarize_payload(result),
            "error": error_message,
            "approval_id": pending_payload.get("action_id"),
            "pending_status": pending_payload.get("status"),
            "decision_blocked_reason": pending_payload.get("decision_blocked_reason"),
            "occurred_at": utc_now().isoformat(),
        }

        logger.info(
            "tool_write_audit",
            audit=True,
            entity_type=entity_type,
            entity_id=entity_id,
            action=event_status,
            user_id=actor_user_id,
            changes=changes,
        )

    def _build_pending_metadata(
        self,
        *,
        tool_name: str,
        arguments: str,
        operation_level: OperationLevel,
    ) -> dict[str, Any]:
        parsed_args = self._safe_decode_arguments(arguments)
        has_explicit_before = "before" in parsed_args and parsed_args.get("before") is not None
        has_explicit_after = "after" in parsed_args and parsed_args.get("after") is not None
        before_snapshot = parsed_args.get("before")
        after_snapshot = parsed_args.get("after")

        if before_snapshot is None:
            before_snapshot = {}
        if after_snapshot is None:
            # 缺少显式 after 时回退到全部参数预览
            after_snapshot = parsed_args

        json_patch, diff_summary = build_json_patch(before_snapshot, after_snapshot)
        diff_source = "snapshot_based" if (has_explicit_before and has_explicit_after) else "argument_inferred"

        # Default TTL: 10 minutes (600s), can be overridden per-tool via tool definition ui_hints
        default_ttl = 600

        return {
            "risk_level": self._derive_risk_level(operation_level),
            "entity_type": self._derive_entity_type(tool_name, parsed_args),
            "entity_id": self._derive_entity_id(parsed_args),
            "before_snapshot": before_snapshot,
            "after_snapshot": after_snapshot,
            "json_patch": json_patch,
            "diff_summary": diff_summary,
            "diff_source": diff_source,
            "ui_hints": {
                "show_diff": operation_level in {OperationLevel.ASSISTED_WRITE, OperationLevel.CRITICAL_WRITE},
                "show_receipt": True,
                "ttl_seconds": default_ttl,
                "diff_source": diff_source,
            },
            "correlation_id": None,
        }

    @staticmethod
    def _log_approval_audit(
        *,
        approver_id: str | None,
        pending_action: dict[str, Any],
    ) -> None:
        if not isinstance(pending_action, dict):
            return
        logger.info(
            "approval_audit approver_id=%s action_id=%s correlation_id=%s status=%s status_code=%s execution_receipt=%s",
            approver_id,
            pending_action.get("action_id"),
            pending_action.get("correlation_id"),
            pending_action.get("status"),
            pending_action.get("status_code"),
            pending_action.get("execution_receipt"),
        )

    async def _create_pending_action(
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
        metadata: dict[str, Any],
    ):
        pending_store = get_pending_action_store()
        try:
            return await pending_store.create_action(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                arguments=arguments,
                operation_level=operation_level,
                invocation_mode=invocation_mode,
                requester_user_id=requester_user_id,
                requester_user_roles=requester_user_roles,
                reason=reason,
                risk_level=metadata.get("risk_level"),
                entity_type=metadata.get("entity_type"),
                entity_id=metadata.get("entity_id"),
                before_snapshot=metadata.get("before_snapshot"),
                after_snapshot=metadata.get("after_snapshot"),
                json_patch=metadata.get("json_patch"),
                diff_summary=metadata.get("diff_summary"),
                correlation_id=metadata.get("correlation_id"),
                ui_hints=metadata.get("ui_hints"),
                ttl_seconds=(metadata.get("ui_hints") or {}).get("ttl_seconds"),
            )
        except TypeError:
            # 兼容旧存储实现签名
            return await pending_store.create_action(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                arguments=arguments,
                operation_level=operation_level,
                invocation_mode=invocation_mode,
                requester_user_id=requester_user_id,
                requester_user_roles=requester_user_roles,
                reason=reason,
            )

    def get_tools(
        self,
        categories: list[ToolCategory] | None = None,
        user_id: str | None = None,
        user_roles: list[str] | None = None,
        invocation_mode: str | InvocationMode | None = InvocationMode.USER_REQUESTED,
    ) -> list[dict[str, Any]]:
        """
        获取工具列表（OpenAI格式）

        Args:
            categories: 筛选特定类别
            user_id: 用户ID（用于新版权限检查）
            user_roles: 用户角色（用于新版权限检查）

        Returns:
            OpenAI格式的工具定义列表
        """
        tools = []

        for tool_name, (definition, tool_category) in self._tools.items():
            # 类别筛选
            if categories is not None and tool_category not in categories:
                continue

            # 新版权限筛选
            if not self._is_allowed_by_user(tool_name, user_id, user_roles):
                continue

            if not self._is_allowed_by_invocation_mode(tool_name, invocation_mode):
                continue

            tools.append(definition.to_openai_schema())

        return tools

    def get_executor(self, category: ToolCategory) -> BaseToolExecutor | None:
        """获取指定类别的执行器"""
        return self._executors.get(category)

    def get_executor_for_tool(self, tool_name: str) -> BaseToolExecutor | None:
        """根据工具名称获取对应的执行器"""
        if tool_name not in self._tools:
            return None

        _, category = self._tools[tool_name]
        return self._executors.get(category)

    async def execute_tool_call(
        self,
        tool_call_id: str,
        tool_name: str,
        arguments: str,
        user_id: str | None = None,
        user_roles: list[str] | None = None,
        invocation_mode: str | InvocationMode | None = InvocationMode.USER_REQUESTED,
    ) -> ToolExecutionResult:
        """
        执行工具调用
        """
        # 检查工具是否存在
        if tool_name not in self._tools:
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                status=ToolExecutionStatus.VALIDATION_ERROR,
                error_message=f"未知的工具: {tool_name}",
                code="TOOL_NOT_REGISTERED",
            )

        _, category = self._tools[tool_name]
        mode = self._normalize_invocation_mode(invocation_mode)
        level = self._get_tool_operation_level(tool_name)

        if not self._is_allowed_by_invocation_mode(tool_name, invocation_mode):
            if level in {OperationLevel.ASSISTED_WRITE, OperationLevel.CRITICAL_WRITE}:
                metadata = self._build_pending_metadata(
                    tool_name=tool_name,
                    arguments=arguments,
                    operation_level=level,
                )
                pending_action = await self._create_pending_action(
                    tool_call_id=tool_call_id,
                    tool_name=tool_name,
                    arguments=arguments,
                    operation_level=level.value,
                    invocation_mode=mode.value,
                    requester_user_id=user_id,
                    requester_user_roles=user_roles,
                    reason=(f"tool '{tool_name}' requires human approval in mode '{mode.value}'"),
                    metadata=metadata,
                )
                self._emit_write_audit(
                    event_status="pending_approval",
                    tool_name=tool_name,
                    operation_level=level,
                    user_id=user_id,
                    arguments=arguments,
                    pending_action=pending_action.to_dict(),
                    invocation_mode=mode.value,
                )
                try:
                    await _send_pending_action_notification(
                        action_id=pending_action.action_id,
                        tool_name=tool_name,
                        requester_user_id=user_id,
                        status="pending",
                    )
                except Exception as exc:  # noqa: BLE001 - best-effort side effect must not abort main flow
                    logger.debug(
                        "pending action notification hook failed action_id=%s tool=%s: %s",
                        pending_action.action_id,
                        tool_name,
                        exc,
                    )
                return ToolExecutionResult(
                    tool_call_id=tool_call_id,
                    tool_name=tool_name,
                    status=ToolExecutionStatus.PENDING_APPROVAL,
                    result=pending_action.to_dict(),
                    error_message=(f"工具 '{tool_name}' 已进入人工审批队列 (operation_level={level.value})"),
                    code="TOOL_PENDING_APPROVAL",
                    message=f"tool '{tool_name}' is queued for human approval",
                    recoverable=True,
                    retryable=False,
                    severity="warning",
                )

            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                status=ToolExecutionStatus.PERMISSION_DENIED,
                error_message=(f"调用模式 '{mode.value}' 不允许执行工具 '{tool_name}' (operation_level={level.value})"),
                code="TOOL_INVOCATION_MODE_BLOCKED",
            )

        # 检查权限 (新版)
        if not self._is_allowed_by_user(tool_name, user_id, user_roles):
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                status=ToolExecutionStatus.PERMISSION_DENIED,
                error_message=f"用户没有权限执行工具: {tool_name}",
                code="TOOL_PERMISSION_DENIED",
            )

        # 获取执行器
        executor = self._executors.get(category)
        if not executor:
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                status=ToolExecutionStatus.ERROR,
                error_message=f"未找到类别 {category.value} 的执行器",
                code="TOOL_EXECUTOR_MISSING",
            )

        # 执行工具
        result = await executor.execute_tool_call(tool_call_id, tool_name, arguments)
        if self._is_audited_write_level(level):
            audit_status = "executed" if result.status == ToolExecutionStatus.SUCCESS else "failed"
            self._emit_write_audit(
                event_status=audit_status,
                tool_name=tool_name,
                operation_level=level,
                user_id=user_id,
                arguments=arguments,
                result=result.result,
                error_message=result.error_message,
                invocation_mode=mode.value,
            )
        if tool_name == "QUERY":
            status_value = result.status.value
            mismatch = status_value in {
                ToolExecutionStatus.VALIDATION_ERROR.value,
                ToolExecutionStatus.NOT_FOUND.value,
                ToolExecutionStatus.ERROR.value,
                ToolExecutionStatus.TIMEOUT.value,
            }
            reason = (
                "query_validation_error"
                if status_value == ToolExecutionStatus.VALIDATION_ERROR.value
                else "query_not_found"
                if status_value == ToolExecutionStatus.NOT_FOUND.value
                else "query_runtime_error"
                if status_value
                in {
                    ToolExecutionStatus.ERROR.value,
                    ToolExecutionStatus.TIMEOUT.value,
                }
                else "none"
            )
            record_query_tool_selection(
                status=status_value,
                mismatch=mismatch,
                tool_name=tool_name,
                reason=reason,
            )
        return result

    async def execute_tool_calls(
        self,
        tool_calls: list[dict[str, Any]],
        user_id: str | None = None,
        user_roles: list[str] | None = None,
        invocation_mode: str | InvocationMode | None = InvocationMode.USER_REQUESTED,
        max_concurrent: int | None = None,
    ) -> list[ToolExecutionResult]:
        """批量执行工具调用（默认并行执行无依赖调用）。"""
        if not tool_calls:
            return []

        concurrency = max_concurrent if max_concurrent is not None else len(tool_calls)
        concurrency = max(1, min(int(concurrency), len(tool_calls), 16))
        semaphore = asyncio.Semaphore(concurrency)

        async def _run(index: int, tool_call: dict[str, Any]):
            tool_call_id = tool_call.get("id", "unknown")
            function = tool_call.get("function", {})
            tool_name = function.get("name", "unknown")
            arguments = function.get("arguments", "{}")

            async with semaphore:
                result = await self.execute_tool_call(
                    tool_call_id,
                    tool_name,
                    arguments,
                    user_id,
                    user_roles,
                    invocation_mode,
                )
            return index, result

        indexed_results = await asyncio.gather(*(_run(index, tool_call) for index, tool_call in enumerate(tool_calls)))
        indexed_results.sort(key=lambda item: item[0])
        return [result for _, result in indexed_results]

    def get_all_tool_names(self) -> list[str]:
        """获取所有已注册的工具名称"""
        return list(self._tools.keys())

    def get_tools_by_category(self, category: ToolCategory) -> list[str]:
        """获取指定类别的所有工具名称"""
        return [name for name, (_, cat) in self._tools.items() if cat == category]

    def get_tool_category(self, tool_name: str) -> str:
        """获取工具的类别名称"""
        if tool_name not in self._tools:
            return "unknown"
        _, category = self._tools[tool_name]
        return category.value

    def get_tool_operation_level(self, tool_name: str) -> str:
        """获取工具的操作级别。"""
        if tool_name not in self._tools:
            return "unknown"
        return self._get_tool_operation_level(tool_name).value

    def get_tool_side_effect(self, tool_name: str) -> bool:
        """返回工具是否有副作用。"""
        if tool_name not in self._tools:
            return False

        definition, _ = self._tools[tool_name]
        return bool(getattr(definition, "side_effect", False))

    async def list_pending_actions(
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
    ) -> list[dict[str, Any]]:
        """列出待审批动作。"""
        store = get_pending_action_store()
        actions = await store.list_actions(
            status=status,
            tool_name=tool_name,
            entity_id=entity_id,
            requester_user_id=requester_user_id,
            created_after=created_after,
            created_before=created_before,
            limit=limit,
            offset=offset,
        )
        return [action.to_dict() for action in actions]

    async def count_pending_actions(
        self,
        *,
        status: str | None = None,
        tool_name: str | None = None,
        entity_id: str | None = None,
        created_after: datetime | None = None,
        created_before: datetime | None = None,
    ) -> int:
        """统计待审批动作数量。"""
        store = get_pending_action_store()
        return await store.count_actions(
            status=status,
            tool_name=tool_name,
            entity_id=entity_id,
            created_after=created_after,
            created_before=created_before,
        )

    async def get_pending_action(self, action_id: str) -> dict[str, Any]:
        store = get_pending_action_store()
        action = await store.get_action(action_id)
        if action is None:
            raise KeyError(action_id)
        return action.to_dict()

    async def get_pending_action_diff(self, action_id: str) -> dict[str, Any]:
        action = await self.get_pending_action(action_id)
        return {
            "approval_id": action.get("action_id"),
            "risk_level": action.get("risk_level") or "NORMAL",
            "entity_type": action.get("entity_type"),
            "entity_id": action.get("entity_id"),
            "diff_source": action.get("diff_source"),
            "before": action.get("before_snapshot") or {},
            "after": action.get("after_snapshot") or {},
            "json_patch": action.get("json_patch") or [],
            "summary": action.get("diff_summary") or {"adds": 0, "updates": 0, "deletes": 0},
        }

    async def get_pending_action_result(self, action_id: str) -> dict[str, Any]:
        action = await self.get_pending_action(action_id)
        receipt = action.get("execution_receipt") or {}
        return {
            "approval_id": action.get("action_id"),
            "status": receipt.get("status") or ("applied" if action.get("status") == "executed" else "failed"),
            "applied_at": receipt.get("applied_at") or action.get("updated_at"),
            "affected_rows": receipt.get("affected_rows", 0),
            "side_effects": receipt.get("side_effects") or [],
            "error": receipt.get("error"),
        }

    async def approve_pending_action(
        self,
        action_id: str,
        approver_id: str,
        approver_roles: list[str] | None = None,
    ) -> dict[str, Any]:
        """批准待审批动作并尝试执行原工具。"""
        pending_store = get_pending_action_store()
        action = await pending_store.approve_action(action_id, approver_id)
        level = self._get_tool_operation_level(action.tool_name)
        self._emit_write_audit(
            event_status="approved",
            tool_name=action.tool_name,
            operation_level=level,
            user_id=action.requester_user_id,
            arguments=action.arguments,
            pending_action=action.to_dict(),
            approver_id=approver_id,
            invocation_mode=action.invocation_mode,
        )

        execution_result = await self.execute_tool_call(
            tool_call_id=action.tool_call_id,
            tool_name=action.tool_name,
            arguments=action.arguments,
            user_id=approver_id,
            user_roles=approver_roles,
            invocation_mode=InvocationMode.USER_REQUESTED,
        )

        receipt = {
            "status": "applied" if execution_result.status == ToolExecutionStatus.SUCCESS else "failed",
            "applied_at": utc_now().isoformat(),
            "affected_rows": 0,
            "side_effects": [],
            "error": execution_result.error_message
            if execution_result.status != ToolExecutionStatus.SUCCESS
            else None,
        }
        if isinstance(execution_result.result, dict):
            side_effects = execution_result.result.get("side_effects")
            affected_rows = execution_result.result.get("affected_rows")
            if isinstance(side_effects, list):
                receipt["side_effects"] = side_effects
            if isinstance(affected_rows, int):
                receipt["affected_rows"] = affected_rows

        if execution_result.status == ToolExecutionStatus.SUCCESS:
            try:
                latest = await pending_store.mark_executed(
                    action_id,
                    execution_result.result,
                    status_code=execution_result.code,
                    execution_receipt=receipt,
                )
            except TypeError:
                latest = await pending_store.mark_executed(action_id, execution_result.result)
        else:
            try:
                latest = await pending_store.mark_failed(
                    action_id,
                    execution_result.error_message or execution_result.status.value,
                    status_code=execution_result.code,
                    error_payload={
                        "status": execution_result.status.value,
                        "code": execution_result.code,
                        "message": execution_result.message or execution_result.error_message,
                    },
                    execution_receipt=receipt,
                )
            except TypeError:
                latest = await pending_store.mark_failed(
                    action_id,
                    execution_result.error_message or execution_result.status.value,
                )

        self._log_approval_audit(
            approver_id=approver_id,
            pending_action=latest.to_dict(),
        )

        return {
            "pending_action": latest.to_dict(),
            "execution_result": self._approval_result_payload(execution_result),
        }

    async def reject_pending_action(
        self,
        action_id: str,
        approver_id: str,
        reason: str | None = None,
    ) -> dict[str, Any]:
        """拒绝待审批动作。"""
        pending_store = get_pending_action_store()
        action = await pending_store.reject_action(action_id, approver_id, reason)
        level = self._get_tool_operation_level(action.tool_name)
        self._emit_write_audit(
            event_status="rejected",
            tool_name=action.tool_name,
            operation_level=level,
            user_id=action.requester_user_id,
            arguments=action.arguments,
            pending_action=action.to_dict(),
            approver_id=approver_id,
            invocation_mode=action.invocation_mode,
        )
        self._log_approval_audit(
            approver_id=approver_id,
            pending_action=action.to_dict(),
        )

        payload = {"pending_action": action.to_dict()}

        return payload

    async def approve_pending_action_with_modification(
        self,
        action_id: str,
        approver_id: str,
        modified_arguments: dict[str, Any],
        approver_roles: list[str] | None = None,
    ) -> dict[str, Any]:
        """批准待审批动作，使用人工修改后的参数执行。

        与 approve_pending_action 的区别：本方法允许审批人员在批准时
        微调 AI 提出的参数（例如修改建议的机位分配）。系统会同时保留
        原始参数和修改后参数，用于后续偏好数据收集。
        """
        pending_store = get_pending_action_store()
        action = await pending_store.approve_action(action_id, approver_id)
        level = self._get_tool_operation_level(action.tool_name)
        self._emit_write_audit(
            event_status="approved",
            tool_name=action.tool_name,
            operation_level=level,
            user_id=action.requester_user_id,
            arguments=action.arguments,
            pending_action=action.to_dict(),
            approver_id=approver_id,
            invocation_mode=action.invocation_mode,
        )

        # 构建修改后的参数 JSON，合并原始参数与用户修改
        original_args = self._safe_decode_arguments(action.arguments)
        merged_args = {**original_args, **modified_arguments}
        merged_arguments_json = json.dumps(merged_args, ensure_ascii=False)

        execution_result = await self.execute_tool_call(
            tool_call_id=action.tool_call_id,
            tool_name=action.tool_name,
            arguments=merged_arguments_json,
            user_id=approver_id,
            user_roles=approver_roles,
            invocation_mode=InvocationMode.USER_REQUESTED,
        )

        receipt = {
            "status": "applied" if execution_result.status == ToolExecutionStatus.SUCCESS else "failed",
            "applied_at": utc_now().isoformat(),
            "affected_rows": 0,
            "side_effects": [],
            "error": execution_result.error_message
            if execution_result.status != ToolExecutionStatus.SUCCESS
            else None,
            "modification": {
                "original_arguments": original_args,
                "modified_arguments": merged_args,
                "modifier_id": approver_id,
            },
        }
        if isinstance(execution_result.result, dict):
            side_effects = execution_result.result.get("side_effects")
            affected_rows = execution_result.result.get("affected_rows")
            if isinstance(side_effects, list):
                receipt["side_effects"] = side_effects
            if isinstance(affected_rows, int):
                receipt["affected_rows"] = affected_rows

        if execution_result.status == ToolExecutionStatus.SUCCESS:
            try:
                latest = await pending_store.mark_executed(
                    action_id,
                    execution_result.result,
                    status_code=execution_result.code,
                    execution_receipt=receipt,
                )
            except TypeError:
                latest = await pending_store.mark_executed(action_id, execution_result.result)
        else:
            try:
                latest = await pending_store.mark_failed(
                    action_id,
                    execution_result.error_message or execution_result.status.value,
                    status_code=execution_result.code,
                    error_payload={
                        "status": execution_result.status.value,
                        "code": execution_result.code,
                        "message": execution_result.message or execution_result.error_message,
                    },
                    execution_receipt=receipt,
                )
            except TypeError:
                latest = await pending_store.mark_failed(
                    action_id,
                    execution_result.error_message or execution_result.status.value,
                )

        self._log_approval_audit(
            approver_id=approver_id,
            pending_action=latest.to_dict(),
        )

        try:
            await _send_pending_action_notification(
                action_id=action_id,
                tool_name=action.tool_name,
                requester_user_id=action.requester_user_id,
                status="approved",
            )
        except Exception as exc:  # noqa: BLE001 - best-effort side effect must not abort main flow
            logger.debug("Failed to send approval notification for action %s: %s", action_id, exc)

        return {
            "pending_action": latest.to_dict(),
            "execution_result": self._approval_result_payload(execution_result),
            "modification": {
                "original_arguments": original_args,
                "modified_arguments": merged_args,
            },
        }

    async def batch_approve_pending_actions(
        self,
        action_ids: list[str],
        approver_id: str,
        approver_roles: list[str] | None = None,
    ) -> dict[str, Any]:
        """批量批准多个待审批动作。"""
        results = []
        for action_id in action_ids:
            try:
                data = await self.approve_pending_action(
                    action_id=action_id,
                    approver_id=approver_id,
                    approver_roles=approver_roles,
                )
                pending_action = data.get("pending_action", {}) if isinstance(data, dict) else {}
                exec_result = data.get("execution_result", {})
                pending_status = str(pending_action.get("status") or "").strip().lower()
                exec_status = str(exec_result.get("status") or "").strip().lower()
                normalized_status = pending_status or exec_status or "unknown"
                success = normalized_status in {"approved", "executed", "success"} and exec_status in {
                    "success",
                    "executed",
                    "",
                }
                results.append(
                    {
                        "action_id": action_id,
                        "success": success,
                        "status": normalized_status,
                        "code": exec_result.get("code") or pending_action.get("status_code"),
                        "message": exec_result.get("message") or pending_action.get("execution_error"),
                    }
                )
            except KeyError as exc:
                results.append(
                    {
                        "action_id": action_id,
                        "success": False,
                        "status": "error",
                        "code": "PENDING_ACTION_NOT_FOUND",
                        "message": str(exc),
                    }
                )
            except PendingActionConflictError as exc:
                results.append(
                    {
                        "action_id": action_id,
                        "success": False,
                        "status": "expired" if exc.code == "PENDING_ACTION_EXPIRED" else "conflict",
                        "code": exc.code,
                        "message": str(exc),
                    }
                )
            except RuntimeError as exc:
                results.append(
                    {
                        "action_id": action_id,
                        "success": False,
                        "status": "conflict",
                        "code": "PENDING_ACTION_STATE_CONFLICT",
                        "message": str(exc),
                    }
                )
            except Exception as exc:  # noqa: BLE001 - recovery handler must catch all errors
                results.append(
                    {
                        "action_id": action_id,
                        "success": False,
                        "status": "error",
                        "code": "PENDING_ACTION_BATCH_ERROR",
                        "message": str(exc),
                    }
                )

        succeeded = sum(1 for r in results if r["success"])
        return {
            "results": results,
            "total": len(results),
            "succeeded": succeeded,
            "failed": len(results) - succeeded,
        }

    async def batch_reject_pending_actions(
        self,
        action_ids: list[str],
        approver_id: str,
        reason: str | None = None,
    ) -> dict[str, Any]:
        """批量拒绝多个待审批动作。"""
        results = []
        for action_id in action_ids:
            try:
                data = await self.reject_pending_action(
                    action_id=action_id,
                    approver_id=approver_id,
                    reason=reason,
                )
                pending_action = data.get("pending_action", {}) if isinstance(data, dict) else {}
                results.append(
                    {
                        "action_id": action_id,
                        "success": True,
                        "status": "rejected",
                        "code": pending_action.get("status_code") or "APPROVAL_REJECTED",
                        "message": "approval request rejected by human reviewer",
                    }
                )
            except KeyError as exc:
                results.append(
                    {
                        "action_id": action_id,
                        "success": False,
                        "status": "error",
                        "code": "PENDING_ACTION_NOT_FOUND",
                        "message": str(exc),
                    }
                )
            except PendingActionConflictError as exc:
                results.append(
                    {
                        "action_id": action_id,
                        "success": False,
                        "status": "expired" if exc.code == "PENDING_ACTION_EXPIRED" else "conflict",
                        "code": exc.code,
                        "message": str(exc),
                    }
                )
            except RuntimeError as exc:
                results.append(
                    {
                        "action_id": action_id,
                        "success": False,
                        "status": "conflict",
                        "code": "PENDING_ACTION_STATE_CONFLICT",
                        "message": str(exc),
                    }
                )
            except Exception as exc:  # noqa: BLE001 - recovery handler must catch all errors
                results.append(
                    {
                        "action_id": action_id,
                        "success": False,
                        "status": "error",
                        "code": "PENDING_ACTION_BATCH_ERROR",
                        "message": str(exc),
                    }
                )

        succeeded = sum(1 for r in results if r["success"])
        return {
            "results": results,
            "total": len(results),
            "succeeded": succeeded,
            "failed": len(results) - succeeded,
        }

    def clear(self) -> None:
        """清空注册表（主要用于测试）"""
        self._tools.clear()
        self._executors.clear()
        self.permission_manager = ToolPermissionManager()
        self._require_user_context = False
        logger.info("ToolRegistry cleared")


def _sync_tool_registry(registry: ToolRegistry | None) -> ToolRegistry | None:
    models._registry = registry
    runtime_holder = get_runtime_holder()
    if runtime_holder is not None:
        runtime_holder.tool_registry = registry
    return registry


def create_tool_registry() -> ToolRegistry:
    """创建新的工具注册表实例。"""
    return ToolRegistry()


def set_tool_registry(registry: ToolRegistry) -> None:
    """设置全局默认注册表实例。"""
    _sync_tool_registry(registry)


@contextmanager
def use_tool_registry(registry: ToolRegistry):
    """在上下文中临时覆盖当前注册表实例（用于测试隔离）。"""
    token = models._registry_context.set(registry)
    try:
        yield registry
    finally:
        models._registry_context.reset(token)


def get_tool_registry() -> ToolRegistry:
    """获取全局工具注册表实例"""
    scoped_registry = models._registry_context.get()
    if scoped_registry is not None:
        return scoped_registry

    runtime_holder = get_runtime_holder()
    if runtime_holder is not None:
        runtime_registry = getattr(runtime_holder, "tool_registry", None)
        if runtime_registry is not None:
            return _sync_tool_registry(runtime_registry)

    if models._registry is None:
        return _sync_tool_registry(ToolRegistry())
    return _sync_tool_registry(models._registry)


DynamicToolRegistry = ToolRegistry


__all__ = [
    "DynamicToolRegistry",
    "ToolRegistry",
    "create_tool_registry",
    "get_tool_registry",
    "set_tool_registry",
    "use_tool_registry",
]
