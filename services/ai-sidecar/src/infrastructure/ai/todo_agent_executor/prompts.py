"""Prompt building helpers for the Todo Agent Executor."""

from __future__ import annotations

import re
from typing import Any

from src.infrastructure.ai.prompts import (
    DISTILLED_CONCLUSION_HINT,
    TASK_DESCRIPTION_TEMPLATE,
    UPSTREAM_CONTEXT_TEMPLATE,
)
from src.infrastructure.ai.shared_context_pool import ContextEntry


def summarize_available_capabilities(tools: list[dict[str, Any]]) -> str:
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
    for t in tools:
        name = t.get("function", {}).get("name", "")
        desc = capability_map.get(name)
        if desc:
            summaries.append(desc)
    if not summaries:
        return "查询航班信息、管理待办事项等基础操作"
    return "、".join(summaries[:10])


def build_error_coaching(
    tool_name: str,
    tool_status: str,
    error_code: str,
    available_tools: list[dict[str, Any]],
) -> str:
    """根据错误类型生成针对性的纠偏提示文本。"""
    tool_names = [t.get("function", {}).get("name", "") for t in available_tools if t.get("function", {}).get("name")]
    tool_list_str = "、".join(tool_names[:15])

    if error_code == "TOOL_NOT_REGISTERED" or "未知的工具" in str(error_code):
        return (
            f"工具 '{tool_name}' 不存在。不要编造工具名称。"
            f"当前可用的工具有：{tool_list_str}。"
            "请根据用户意图从以上工具中重新选择，或直接用文字回答用户。"
        )

    if tool_status == "validation_error" or error_code == "TOOL_VALIDATION_ERROR":
        required = []
        for t in available_tools:
            func = t.get("function", {})
            if func.get("name") == tool_name:
                required = func.get("parameters", {}).get("required", [])
                break
        if required:
            return f"参数格式错误。工具 '{tool_name}' 的必填参数为：{', '.join(required)}。请检查参数后重试。"
        return f"工具 '{tool_name}' 参数验证失败，请检查参数格式和类型后重试。"

    if tool_status == "not_found" or error_code == "TOOL_NOT_FOUND":
        return (
            "未找到该资源。请确认ID是否正确。"
            "如果不确定ID，请先使用搜索工具（如 search_flights_by_number、list_anomalies）检索。"
        )

    if tool_status == "permission_denied":
        return f"没有权限执行工具 '{tool_name}'。该操作可能需要更高级别的授权。请告知用户需要人工处理。"

    if tool_status == "timeout":
        return f"工具 '{tool_name}' 执行超时。请告知用户系统暂时繁忙，稍后再试。"

    return "工具执行失败。请尝试使用其他工具，或直接用文字告知用户当前无法完成该操作。"


def build_task_description(
    todo: Any,
    entity_config: dict[str, Any] | None = None,
    has_downstream: bool = False,
) -> str:
    """构建任务描述，支持实体自定义模板。"""
    template = TASK_DESCRIPTION_TEMPLATE
    if entity_config and entity_config.get("task_template"):
        template = entity_config["task_template"]

    desc = template.format(
        title=todo.title,
        description=todo.description or "无",
    )
    if has_downstream:
        desc += DISTILLED_CONCLUSION_HINT
    return desc


def format_upstream_context(entries: list[ContextEntry]) -> str:
    """将上游 Agent 的结论格式化为黑板注入段。"""
    formatted_entries = "\n\n".join(f"▼ 任务 [{e.source_todo_title}] 的执行结论:\n{e.content}" for e in entries)
    return UPSTREAM_CONTEXT_TEMPLATE.format(entries=formatted_entries)


def distill_conclusion(response_text: str | None) -> str | None:
    """从 Agent 回答中提取精炼结论。"""
    if not response_text or not response_text.strip():
        return None

    text = response_text.strip()

    match = re.search(r"【结论】\s*(.*?)(?=\n\n|\n##|\n---|\'\'\'|$)", text, re.DOTALL)
    if match:
        conclusion = match.group(1).strip()
        if conclusion:
            return conclusion[:1000]

    if len(text) > 500:
        return text[:500] + "…"
    return text


def format_runtime_fallback_reason(exc: Exception) -> str:
    raw_message = str(exc or "").strip()
    if not raw_message:
        return exc.__class__.__name__
    if len(raw_message) > 240:
        raw_message = f"{raw_message[:237]}..."
    return raw_message
