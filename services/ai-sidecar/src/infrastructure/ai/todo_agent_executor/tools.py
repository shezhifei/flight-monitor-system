"""Tool execution helpers for the Todo Agent Executor."""

from __future__ import annotations

import json
from typing import Any


def normalize_tool_payload_value(value: Any) -> Any:
    """Normalize a single tool payload value for SSE serialization."""
    if isinstance(value, str):
        return value
    if isinstance(value, (int, float, bool)):
        return value
    if value is None:
        return ""
    if isinstance(value, (dict, list)):
        try:
            text = json.dumps(value, ensure_ascii=False, default=str)
            if len(text) > 2000:
                return text[:2000] + "..."
            return text
        except (json.JSONDecodeError, TypeError, ValueError):
            return str(value)
    return str(value)


def truncate_tool_payload(payload: dict[str, Any], max_chars: int = 1500) -> dict[str, Any]:
    """Truncate tool payload fields to fit SSE size limits."""
    truncated = {}
    for key, value in payload.items():
        if isinstance(value, str):
            if len(value) > max_chars:
                truncated[key] = f"{value[:max_chars]}...(truncated)"
            else:
                truncated[key] = value
        elif isinstance(value, (dict, list)):
            try:
                serialized = json.dumps(value, ensure_ascii=False, default=str)
            except (json.JSONDecodeError, TypeError, ValueError):
                serialized = str(value)
            if len(serialized) > max_chars:
                truncated[key] = f"{serialized[:max_chars]}...(truncated)"
            else:
                truncated[key] = value
        else:
            truncated[key] = value
    return truncated


def convert_tools_for_responses(tools: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Convert tool definitions to Responses API format."""
    responses_tools = []
    for tool in tools:
        func = tool.get("function", {})
        responses_tools.append(
            {
                "type": "function",
                "name": func.get("name", ""),
                "description": func.get("description", ""),
                "parameters": func.get("parameters", {}),
                "strict": True,
            }
        )
    return responses_tools


def collect_graph_tool_names(tools: list[dict[str, Any]]) -> list[str]:
    """Collect tool names from OpenAI-style function definitions."""
    return [t.get("function", {}).get("name", "") for t in tools if t.get("function", {}).get("name")]


def inject_error_coaching(
    tool_name: str,
    tool_status: str,
    tool_result: str,
    available_tools: list[dict[str, Any]],
) -> str:
    """Inject error coaching into tool result when tool execution fails."""
    try:
        result_dict = json.loads(tool_result)
    except (json.JSONDecodeError, TypeError):
        result_dict = {"error": tool_result, "status": tool_status}

    code = result_dict.get("code", "")
    hint = _build_error_coaching(tool_name, tool_status, code, available_tools)
    if hint:
        result_dict["_system_hint"] = hint

    return json.dumps(result_dict, ensure_ascii=False)


def _build_error_coaching(
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
