"""
LangChain 事件打点与 SSE 广播适配 (Callbacks)。

在运行环境未安装 langchain-core 时提供轻量兜底定义，避免 graph 相关模块导入失败。
"""

import json
import time
from typing import Any

try:
    from langchain_core.callbacks import AsyncCallbackHandler
    from langchain_core.outputs import LLMResult
except ImportError:  # pragma: no cover - exercised in unit tests via fallback path

    class AsyncCallbackHandler:  # type: ignore[override]  # fallback when langchain-core is absent
        """Fallback callback base class used when langchain-core is unavailable."""

    LLMResult = Any  # type: ignore[assignment]  # fallback stub

from src.infrastructure.common.exceptions import JSON_EXCEPTIONS
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class SSEStreamingCallbackHandler(AsyncCallbackHandler):
    """将 LangChain/LangGraph 回调事件映射为内部 SSE 事件。"""

    TOOL_EVENT_ARGUMENT_MAX_CHARS = 4000
    TOOL_EVENT_RESULT_MAX_CHARS = 6000

    def __init__(self, notification_port: Any, run_id: str, todo_id: str):
        super().__init__()
        self._notification_port = notification_port
        self._run_id = run_id
        self._todo_id = todo_id
        self._current_phase = "planning"
        self._execution_started_at = time.time()
        self._tool_runs: dict[str, dict[str, Any]] = {}

    async def _emit_event(self, event_type: str, data: dict[str, Any]) -> None:
        if not self._notification_port:
            return

        real_payload = data.copy() if isinstance(data, dict) else {}
        status = str(real_payload.get("status") or self._default_status_for_event(event_type)).strip().lower()
        phase = str(real_payload.get("phase") or self._default_phase_for_event(event_type)).strip().lower()
        started_at = real_payload.get("started_at") or real_payload.get("execution_started_at")
        if not started_at:
            started_at = int(self._execution_started_at * 1000)
        tool_name = real_payload.get("tool_name")
        if tool_name is None and isinstance(real_payload.get("tool"), dict):
            tool_name = real_payload["tool"].get("name")

        payload = {
            "event": str(real_payload.get("event") or event_type),
            "execution_id": self._run_id,
            "conversation_id": self._run_id,
            "agent_id": self._run_id,
            "todo_id": self._todo_id,
            "phase": phase,
            "status": status,
            "tool_name": tool_name,
            "progress_pct": int(real_payload.get("progress_pct") or 0),
            "recoverable": bool(real_payload.get("recoverable", status in {"timeout", "error", "pending_approval"})),
            "retryable": bool(real_payload.get("retryable", status in {"timeout", "error"})),
            "meta": {
                **(real_payload.get("meta") or {}),
                "contract_version": "2.0",
                "runtime": "graph",
            },
            "execution_started_at": started_at,
            "timestamps": {
                "started_at": str(started_at),
                "ended_at": str(real_payload.get("ended_at")) if real_payload.get("ended_at") is not None else None,
            },
            **real_payload,
        }
        try:
            if hasattr(self._notification_port, "notify_ai_event"):
                await self._notification_port.notify_ai_event(event_type, payload)
            else:
                logger.debug("Unhandled SSE emit (port missing notify_ai_event): %s", payload)
        except (
            Exception  # noqa: BLE001
        ) as exc:  # pragma: no cover - defensive logging
            logger.warning("SSE 打点失败: %s", exc)

    @staticmethod
    def _default_phase_for_event(event_type: str) -> str:
        mapping = {
            "tool_start": "tool_execute",
            "tool_end": "tool_execute",
            "approval_required": "approval",
            "execution_end": "report",
            "progress": "planning",
        }
        return mapping.get(str(event_type or "").strip().lower(), "planning")

    @staticmethod
    def _default_status_for_event(event_type: str) -> str:
        mapping = {
            "tool_start": "in_progress",
            "tool_end": "success",
            "approval_required": "pending_approval",
            "execution_end": "success",
            "progress": "in_progress",
        }
        return mapping.get(str(event_type or "").strip().lower(), "in_progress")

    @staticmethod
    def _normalize_payload_value(value: Any) -> Any:
        if not isinstance(value, str):
            return value
        text = value.strip()
        if not text:
            return ""
        try:
            return json.loads(text)
        except JSON_EXCEPTIONS as exc:
            logger.warning("callback JSON parse failed; returning raw text: %s", exc)
            return value

    @classmethod
    def _truncate_payload(cls, value: Any, *, max_chars: int) -> tuple[Any, bool]:
        if value is None:
            return None, False
        if isinstance(value, str):
            if len(value) <= max_chars:
                return value, False
            return f"{value[:max_chars]}...(truncated)", True
        try:
            serialized = json.dumps(value, ensure_ascii=False, default=str)
        except JSON_EXCEPTIONS as exc:
            logger.warning("callback payload JSON serialization failed: %s", exc)
            fallback = str(value)
            if len(fallback) <= max_chars:
                return fallback, False
            return f"{fallback[:max_chars]}...(truncated)", True
        if len(serialized) <= max_chars:
            return value, False
        return f"{serialized[:max_chars]}...(truncated)", True

    @staticmethod
    def _resolve_tool_run_key(kwargs: dict[str, Any]) -> str | None:
        raw_key = kwargs.get("run_id") or kwargs.get("tool_call_id") or kwargs.get("name")
        if raw_key is None:
            return None
        key = str(raw_key).strip()
        return key or None

    async def on_llm_start(
        self,
        serialized: dict[str, Any],
        prompts: list[str],
        **kwargs: Any,
    ) -> None:
        self._current_phase = "planning"
        await self._emit_event(
            "progress",
            {
                "event": "progress",
                "phase": "planning",
                "status": "in_progress",
                "message": "Agent 正在思考...",
            },
        )

    async def on_llm_new_token(self, token: str, **kwargs: Any) -> None:
        return None

    async def on_llm_end(self, response: LLMResult, **kwargs: Any) -> None:
        self._current_phase = "acting"
        return None

    async def on_llm_error(
        self,
        error: Exception | KeyboardInterrupt,
        **kwargs: Any,
    ) -> None:
        self._current_phase = "planning"
        await self._emit_event(
            "execution_end",
            {
                "event": "execution_end",
                "phase": "report",
                "status": "error",
                "code": "GRAPH_LLM_ERROR",
                "message": str(error),
                "progress_pct": 100,
            },
        )

    async def on_tool_start(
        self,
        serialized: dict[str, Any],
        input_str: str,
        **kwargs: Any,
    ) -> None:
        self._current_phase = "tool_execute"
        tool_name = serialized.get("name", "unknown")
        normalized_arguments = self._normalize_payload_value(input_str)
        tool_arguments, arguments_truncated = self._truncate_payload(
            normalized_arguments,
            max_chars=self.TOOL_EVENT_ARGUMENT_MAX_CHARS,
        )
        run_key = self._resolve_tool_run_key(kwargs)
        if run_key:
            self._tool_runs[run_key] = {
                "tool_name": tool_name,
                "tool_call_id": str(kwargs.get("tool_call_id") or run_key),
                "started_at": time.time(),
            }
        await self._emit_event(
            "tool_start",
            {
                "event": "tool_start",
                "phase": "tool_execute",
                "status": "in_progress",
                "tool_name": tool_name,
                "tool_call_id": str(kwargs.get("tool_call_id") or run_key or ""),
                "tool": {"name": tool_name},
                "content": f"参数: {str(input_str)[:50]}...",
                "tool_arguments": tool_arguments,
                "tool_arguments_truncated": arguments_truncated,
            },
        )

    async def on_tool_end(self, output: str, **kwargs: Any) -> None:
        self._current_phase = "tool_execute"
        run_key = self._resolve_tool_run_key(kwargs)
        tool_run = self._tool_runs.pop(run_key, {}) if run_key else {}
        tool_name = str(kwargs.get("name") or tool_run.get("tool_name") or "unknown")
        tool_call_id = str(kwargs.get("tool_call_id") or tool_run.get("tool_call_id") or run_key or "")
        duration_ms = int((time.time() - float(tool_run.get("started_at") or time.time())) * 1000)
        normalized_result = self._normalize_payload_value(output)
        tool_result, result_truncated = self._truncate_payload(
            normalized_result,
            max_chars=self.TOOL_EVENT_RESULT_MAX_CHARS,
        )
        await self._emit_event(
            "tool_end",
            {
                "event": "tool_end",
                "phase": "tool_execute",
                "status": "success",
                "tool_name": tool_name,
                "tool_call_id": tool_call_id,
                "tool": {"name": tool_name},
                "content": str(output)[:50],
                "tool_result": tool_result,
                "tool_result_truncated": result_truncated,
                "tool_error": None,
                "duration_ms": max(duration_ms, 0),
            },
        )

    async def on_tool_error(
        self,
        error: Exception | KeyboardInterrupt,
        **kwargs: Any,
    ) -> None:
        self._current_phase = "tool_execute"
        run_key = self._resolve_tool_run_key(kwargs)
        tool_run = self._tool_runs.pop(run_key, {}) if run_key else {}
        tool_name = str(kwargs.get("name") or tool_run.get("tool_name") or "unknown")
        tool_call_id = str(kwargs.get("tool_call_id") or tool_run.get("tool_call_id") or run_key or "")
        duration_ms = int((time.time() - float(tool_run.get("started_at") or time.time())) * 1000)
        await self._emit_event(
            "tool_end",
            {
                "event": "tool_end",
                "phase": "tool_execute",
                "status": "error",
                "tool_name": tool_name,
                "tool_call_id": tool_call_id,
                "tool": {"name": tool_name},
                "tool_result": None,
                "tool_result_truncated": False,
                "tool_error": str(error),
                "duration_ms": max(duration_ms, 0),
            },
        )

    async def emit_approval_required(
        self,
        *,
        pending_action_id: str | None,
        pending_tool_call: dict[str, Any] | None = None,
        message: str = "等待人工审批中...",
    ) -> None:
        tool_name = str((pending_tool_call or {}).get("name") or "").strip() or None
        raw_arguments = (pending_tool_call or {}).get("args")
        normalized_arguments = self._normalize_payload_value(raw_arguments)
        tool_arguments, arguments_truncated = self._truncate_payload(
            normalized_arguments,
            max_chars=self.TOOL_EVENT_ARGUMENT_MAX_CHARS,
        )
        await self._emit_event(
            "approval_required",
            {
                "event": "approval_required",
                "phase": "approval",
                "status": "pending_approval",
                "tool_name": tool_name,
                "tool_call_id": str((pending_tool_call or {}).get("id") or ""),
                "tool": {"name": tool_name} if tool_name else None,
                "message": message,
                "tool_arguments": tool_arguments,
                "tool_arguments_truncated": arguments_truncated,
                "tool_result": {
                    "action_id": pending_action_id,
                    "tool_name": tool_name,
                },
                "tool_result_truncated": False,
                "meta": {
                    "pending_action_id": pending_action_id,
                },
            },
        )
