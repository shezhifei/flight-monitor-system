"""TODO Agent executor — main class."""

from __future__ import annotations

import json
import re
from typing import Any

from src.domain.ai.agent_execution import (
    AgentExecution,
    AgentExecutionStatus,
)
from src.domain.ai.todo_graph_pilot import (
    resolve_todo_graph_pilot_rollout_override,
)
from src.domain.ports.agent_runtime_port import (
    AgentExecutionResultDTO,
)
from src.domain.utils.time_utils import utc_now
from src.infrastructure.ai.openai_client import Message
from src.infrastructure.ai.prompts import (
    UPSTREAM_CONTEXT_TEMPLATE,
)
from src.infrastructure.ai.responses_adapter import (
    extract_message_content,
    message_content_to_text,
    messages_to_responses_input,
    normalize_api_format,
)
from src.infrastructure.ai.responses_adapter import (
    extract_tool_calls as _adapter_extract_tool_calls,
)
from src.infrastructure.ai.shared_context_pool import (
    ContextEntry,
)
from src.infrastructure.ai.todo_agent_executor.models import PENDING_APPROVAL_STATUS
from src.infrastructure.ai.todo_agent_executor.prompts import (
    format_runtime_fallback_reason,
)
from src.infrastructure.ai.todo_agent_executor.tools import (
    convert_tools_for_responses,
)
from src.infrastructure.ai.tools.base import InvocationMode


class _PolicyMixin:
    """Mixin for TodoAgentExecutor."""

    def _default_phase_for_event(self: str) -> str:
        mapping = {
            "tool_start": "tool_execute",
            "tool_progress": "tool_execute",
            "tool_end": "tool_execute",
            "approval_required": "approval",
            "approval_result": "approval",
            "execution_end": "report",
            "progress": "planning",
        }
        return mapping.get(str(self or "").strip().lower(), "tool_execute")

    def _default_status_for_event(self: str) -> str:
        mapping = {
            "tool_start": "in_progress",
            "tool_progress": "in_progress",
            "tool_end": "success",
            "approval_required": "pending_approval",
            "approval_result": "success",
            "execution_end": "success",
            "progress": "in_progress",
        }
        return mapping.get(str(self or "").strip().lower(), "in_progress")

    def _normalize_tool_payload_value(self: Any) -> Any:
        if isinstance(self, str):
            text = self.strip()
            if not text:
                return ""
            try:
                return json.loads(text)
            except (json.JSONDecodeError, TypeError, ValueError):
                return self
        return self

    def _truncate_tool_payload(
        self,
        value: Any,
        *,
        max_chars: int,
    ) -> tuple[Any, bool]:
        if value is None:
            return None, False
        if isinstance(value, str):
            if len(value) <= max_chars:
                return value, False
            return f"{value[:max_chars]}...(truncated)", True
        try:
            serialized = json.dumps(value, ensure_ascii=False, default=str)
        except (json.JSONDecodeError, TypeError, ValueError):
            fallback = str(value)
            if len(fallback) <= max_chars:
                return fallback, False
            return f"{fallback[:max_chars]}...(truncated)", True

        if len(serialized) <= max_chars:
            return value, False
        return f"{serialized[:max_chars]}...(truncated)", True

    def _resolve_pending_approval_status() -> Any:
        return PENDING_APPROVAL_STATUS

    def _extract_graph_response_text(self: dict[str, Any]) -> str:
        messages = self.get("messages") if isinstance(self, dict) else None
        if not messages:
            return ""

        last_message = messages[-1]
        content = getattr(last_message, "content", None)
        if content is None and isinstance(last_message, dict):
            content = last_message.get("content")
        if hasattr(content, "content"):
            nested_content = getattr(content, "content", None)
            if nested_content is not None:
                content = nested_content
        text = message_content_to_text(content)
        if text:
            return text
        if isinstance(content, list):
            return "\n".join(str(item) for item in content if item is not None).strip()
        return _PolicyMixin._normalize_graph_response_text(str(content or last_message or "").strip())

    def _normalize_graph_response_text(self: Any) -> str:
        text = str(self or "").strip()
        if not text:
            return ""
        matched = re.search(r"content='([^']*)'", text)
        if matched:
            return matched.group(1)
        return text

    def _normalize_graph_guardrail_state(self: Any) -> dict[str, Any]:
        source = dict(self) if isinstance(self, dict) else {}

        def _normalize_id_list(value: Any) -> list[str]:
            seen: set[str] = set()
            normalized: list[str] = []
            for item in value or []:
                item_id = str(item or "").strip()
                if not item_id or item_id in seen:
                    continue
                seen.add(item_id)
                normalized.append(item_id)
            return normalized

        return {
            "executed_tool_call_ids": _normalize_id_list(source.get("executed_tool_call_ids")),
            "inflight_tool_call_ids": _normalize_id_list(source.get("inflight_tool_call_ids")),
            "duplicate_tool_execution_total": max(0, int(source.get("duplicate_tool_execution_total") or 0)),
            "duplicate_tool_execution_blocked_total": max(
                0, int(source.get("duplicate_tool_execution_blocked_total") or 0)
            ),
            "duplicate_tool_execution_events": list(source.get("duplicate_tool_execution_events") or []),
            "graph_local_abort_total": max(0, int(source.get("graph_local_abort_total") or 0)),
            "graph_tool_failure_streak_max": max(0, int(source.get("graph_tool_failure_streak_max") or 0)),
            "last_graph_abort_reason": str(source.get("last_graph_abort_reason") or "").strip(),
        }

    def _append_graph_guardrail_event(
        self: dict[str, Any],
        *,
        tool_call_id: str,
        tool_name: str | None,
        status: str,
        reason: str | None = None,
    ) -> None:
        events = list(self.get("duplicate_tool_execution_events") or [])
        events.append(
            {
                "tool_call_id": str(tool_call_id or "").strip(),
                "tool_name": str(tool_name or "").strip() or None,
                "status": str(status or "").strip() or None,
                "reason": str(reason or "").strip() or None,
                "observed_at": utc_now().isoformat(),
            }
        )
        self["duplicate_tool_execution_events"] = events[-20:]

    def _extract_graph_pending_tool_call(self: AgentExecution) -> dict[str, Any] | None:
        metadata = self.metadata or {}
        checkpoint_payload = metadata.get("langgraph_checkpoint")
        if isinstance(checkpoint_payload, str):
            try:
                checkpoint_payload = json.loads(checkpoint_payload)
            except (json.JSONDecodeError, TypeError, ValueError):
                checkpoint_payload = None
        if not isinstance(checkpoint_payload, dict):
            return None
        pending_tool_call = checkpoint_payload.get("pending_tool_call")
        if not isinstance(pending_tool_call, dict):
            state_payload = checkpoint_payload.get("state")
            if isinstance(state_payload, dict):
                pending_tool_call = state_payload.get("pending_tool_call")
        if not isinstance(pending_tool_call, dict):
            return None
        tool_call_id = str(pending_tool_call.get("id") or "").strip()
        tool_name = str(pending_tool_call.get("name") or "").strip()
        if not tool_call_id and not tool_name:
            return None
        return {
            "id": tool_call_id,
            "name": tool_name,
            "args": pending_tool_call.get("args"),
        }

    def _build_resume_guardrail_response(
        self: AgentExecution,
        *,
        reason: str,
    ) -> AgentExecutionResultDTO:
        runtime_status = str((self.metadata or {}).get("runtime_status") or self.status.value)
        response_text = _PolicyMixin._normalize_graph_response_text(
            (self.metadata or {}).get("graph_runtime_last_response")
        )
        if self.status == AgentExecutionStatus.COMPLETED:
            response_text = response_text or "graph resume already applied"
            status = AgentExecutionStatus.COMPLETED
        elif runtime_status == PENDING_APPROVAL_STATUS.value:
            response_text = response_text or "等待人工审批中..."
            status = PENDING_APPROVAL_STATUS
        elif self.status == AgentExecutionStatus.FAILED:
            response_text = response_text or (self.error_message or "graph resume previously failed")
            status = AgentExecutionStatus.FAILED
        else:
            response_text = response_text or "graph resume already in progress"
            status = self.status

        return AgentExecutionResultDTO(
            run_id=self.run_id,
            todo_id=self.todo_id,
            status=status,
            response=response_text,
            total_steps=self.total_steps,
            total_tokens=self.total_tokens,
            total_tool_calls=self.total_tool_calls,
            duration_ms=0,
            error_message=reason if status == AgentExecutionStatus.FAILED else None,
            child_todos=[],
        )

    def _resolve_invocation_mode(self: Any) -> InvocationMode:
        if isinstance(self, InvocationMode):
            return self
        try:
            return InvocationMode(str(self or InvocationMode.AGENT_AUTONOMOUS.value))
        except ValueError:
            return InvocationMode.AGENT_AUTONOMOUS

    def _resolve_entity_graph_runtime_override(self: dict[str, Any]) -> bool | None:
        return resolve_todo_graph_pilot_rollout_override(self)

    def _collect_graph_tool_names(self: list[dict[str, Any]]) -> list[str]:
        tool_names: list[str] = []
        seen: set[str] = set()
        for tool in self or []:
            function_payload = tool.get("function") if isinstance(tool, dict) else None
            tool_name = str((function_payload or {}).get("name") or "").strip()
            if not tool_name or tool_name in seen:
                continue
            seen.add(tool_name)
            tool_names.append(tool_name)
        return tool_names

    def _format_runtime_fallback_reason(self: Exception) -> str:
        return format_runtime_fallback_reason(self)

    def _build_initial_graph_state(
        *,
        todo_id: str,
        entity_id: str,
        current_plan: str,
        messages: list[Any],
        requires_approval: bool,
        pending_action_id: str | None,
        pending_tool_call: dict[str, Any] | None,
        metrics: dict[str, Any],
        blackboard_facts: list[str],
        error_message: str | None,
    ) -> dict[str, Any]:
        return {
            "todo_id": todo_id,
            "entity_id": entity_id,
            "current_plan": current_plan,
            "messages": messages,
            "requires_approval": requires_approval,
            "pending_action_id": pending_action_id,
            "pending_tool_call": pending_tool_call,
            "metrics": metrics,
            "blackboard_facts": blackboard_facts,
            "error_message": error_message,
            "consecutive_tool_failures": 0,
            "max_consecutive_tool_failures": 0,
            "last_tool_error_code": "",
            "last_tool_error_message": "",
        }

    def _normalize_api_format(self: Any) -> str:
        return normalize_api_format(self)

    def _message_content_to_text(self: Any) -> str:
        return message_content_to_text(self)

    def _messages_to_responses_input(
        self,
        *,
        messages: list[Message],
        fallback_instructions: str,
    ) -> tuple[str, list[dict[str, Any]]]:
        return messages_to_responses_input(
            messages=messages,
            fallback_instructions=fallback_instructions,
        )

    def _convert_tools_for_responses(self: list[dict[str, Any]]) -> list[dict[str, Any]]:
        return convert_tools_for_responses(self)

    def _parse_thinking_content(self, raw_content: str) -> tuple[str | None, str, str | None]:
        """解析响应内容"""
        import re

        if not raw_content:
            return None, "", None

        match = re.search(r"<thinking>(.*?)</thinking>", raw_content, re.DOTALL)
        if match:
            thinking = match.group(1).strip()
            content = raw_content.replace(match.group(0), "").strip()
            decision_summary = thinking.split("\n")[0][:100]
            return thinking, content, decision_summary

        return None, raw_content, None

    def _extract_response_content(self, response: Any) -> str | None:
        """提取响应内容"""
        if not response:
            return None
        result = extract_message_content(response)
        return result or None

    def _extract_tool_calls(self, response: Any) -> list[dict]:
        """提取工具调用"""
        return _adapter_extract_tool_calls(response)

    def _summarize_available_capabilities(self: list[dict[str, Any]]) -> str:
        """将可用工具列表转化为中文能力摘要，用于优雅降级时告知用户。"""
        capability_map = {
            "search_flights_by_number": "按航班号查询航班",
            "get_flight_details": "查看航班详情",
            "search_flights_advanced": "多条件搜索航班",
            "get_delayed_flights": "查询延误航班",
            "get_abnormal_flights": "查询异常航班",
            "count_flights_by_status": "航班状态统计",
            "get_flights_by_time_range": "按时间范围查询航班",
            "get_turnaround_stats": "过站统计",
            "list_anomalies": "查看系统异常告警",
            "get_anomaly_detail": "查看异常详情",
            "get_anomaly_stats": "异常统计",
            "change_stand": "变更机位（需审批）",
            "notify_teams": "通知保障班组（需审批）",
            "filter_flights": "筛选航班",
            "get_handling_recommendation": "获取处置建议",
            "generate_incident_report": "生成事件报告",
            "create_todo": "创建待办事项",
            "list_todos": "查看待办事项列表",
        }
        summaries = []
        for t in self:
            name = t.get("function", {}).get("name", "")
            desc = capability_map.get(name)
            if desc:
                summaries.append(desc)
        if not summaries:
            return "查询航班信息、管理待办事项等基础操作"
        return "、".join(summaries[:10])

    def _format_upstream_context(self: list[ContextEntry]) -> str:
        """将上游 Agent 的结论格式化为黑板注入段。"""
        formatted_entries = "\n\n".join(f"▼ 任务 [{e.source_todo_title}] 的执行结论:\n{e.content}" for e in self)
        return UPSTREAM_CONTEXT_TEMPLATE.format(entries=formatted_entries)

    def _distill_conclusion(self: str | None) -> str | None:
        """从 Agent 回答中提取精炼结论。

        优先提取以「【结论】」开头的段落；
        如果没有标记，则取前 500 字符作为摘要。
        """
        if not self or not self.strip():
            return None

        text = self.strip()

        # 尝试提取 【结论】 标记后的内容
        import re

        match = re.search(r"【结论】\s*(.*?)(?=\n\n|\n##|\n---|\'\'\'|$)", text, re.DOTALL)
        if match:
            conclusion = match.group(1).strip()
            if conclusion:
                return conclusion[:1000]  # 硬限制 1000 字符

        # 回退：取前 500 字符
        if len(text) > 500:
            return text[:500] + "…"
        return text
