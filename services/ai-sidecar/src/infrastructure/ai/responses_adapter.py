"""
Responses API adapter utilities.

Shared helpers for dual-format (Chat Completions / Responses API) support.
Eliminates duplication across TodoAgentExecutor, NLQueryService,
ToolExecutionService, FlightInsightService, FlowableProcessDraftService.
"""

from __future__ import annotations

import json
from typing import Any

from src.infrastructure.ai.openai_client import Message, MessageRole, ResponsesAPIResponse


def normalize_api_format(api_format: Any) -> str:
    """Normalize api_format value to 'responses' or 'chat_completions'."""
    value = str(api_format or "").strip().lower()
    return "responses" if value == "responses" else "chat_completions"


def message_content_to_text(content: Any) -> str:
    """Extract plain text from str / list-of-parts content."""
    if content is None:
        return ""
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        parts: list[str] = []
        for item in content:
            if isinstance(item, dict):
                text_value = item.get("text")
                if text_value is not None and str(text_value).strip():
                    parts.append(str(text_value).strip())
            elif str(item).strip():
                parts.append(str(item).strip())
        return "\n".join(parts)
    return str(content)


def extract_message_content(response: Any, *, fallback: str = "") -> str:
    """Extract text content from either ChatCompletionResponse or ResponsesAPIResponse."""
    # Handle ResponsesAPIResponse dataclass
    if isinstance(response, ResponsesAPIResponse):
        output_text = (response.output_text or "").strip()
        if output_text:
            return output_text

        parts: list[str] = []
        for item in response.output or []:
            if not isinstance(item, dict) or str(item.get("type") or "").lower() != "message":
                continue
            content = item.get("content")
            if isinstance(content, str):
                if content.strip():
                    parts.append(content.strip())
                continue
            if not isinstance(content, list):
                continue
            for content_part in content:
                if isinstance(content_part, dict):
                    text_value = content_part.get("text")
                    if text_value is not None and str(text_value).strip():
                        parts.append(str(text_value).strip())
                elif str(content_part).strip():
                    parts.append(str(content_part).strip())
        merged = "\n".join(parts).strip()
        return merged if merged else fallback

    # Handle dict from LLMStreamRunner raw_response (Responses API)
    if isinstance(response, dict) and "output" in response:
        parts_d: list[str] = []
        for item in response.get("output") or []:
            if not isinstance(item, dict) or str(item.get("type") or "").lower() != "message":
                continue
            content = item.get("content")
            if isinstance(content, str):
                if content.strip():
                    parts_d.append(content.strip())
                continue
            if not isinstance(content, list):
                continue
            for content_part in content:
                if isinstance(content_part, dict):
                    text_value = content_part.get("text")
                    if text_value is not None and str(text_value).strip():
                        parts_d.append(str(text_value).strip())
                elif str(content_part).strip():
                    parts_d.append(str(content_part).strip())
        merged_d = "\n".join(parts_d).strip()
        return merged_d if merged_d else fallback

    choices = getattr(response, "choices", None) or []
    if not choices:
        return fallback
    first = choices[0]
    if isinstance(first, dict):
        message = first.get("message", {}) or {}
        content = message.get("content")
    else:
        message = getattr(first, "message", None)
        content = getattr(message, "content", None) if message else None
    return str(content or fallback)


def extract_tool_calls(response: Any) -> list[dict[str, Any]]:
    """Extract normalized tool calls from either ChatCompletionResponse or ResponsesAPIResponse."""
    if not response:
        return []

    if isinstance(response, ResponsesAPIResponse):
        normalized: list[dict[str, Any]] = []
        for index, item in enumerate(response.output or []):
            if not isinstance(item, dict) or str(item.get("type") or "").lower() != "function_call":
                continue
            function_name = str(item.get("name") or "").strip()
            if not function_name:
                continue
            raw_arguments = item.get("arguments")
            if isinstance(raw_arguments, str):
                function_arguments = raw_arguments
            elif raw_arguments is None:
                function_arguments = "{}"
            else:
                function_arguments = json.dumps(raw_arguments, ensure_ascii=False)
            normalized.append(
                {
                    "id": str(item.get("call_id") or item.get("id") or f"tool_call_{index}"),
                    "type": "function",
                    "function": {
                        "name": function_name,
                        "arguments": function_arguments,
                    },
                }
            )
        return normalized

    # Handle dict from LLMStreamRunner raw_response (Responses API)
    if isinstance(response, dict) and "output" in response:
        normalized_d: list[dict[str, Any]] = []
        for index, item in enumerate(response.get("output") or []):
            if not isinstance(item, dict) or str(item.get("type") or "").lower() != "function_call":
                continue
            function_name = str(item.get("name") or "").strip()
            if not function_name:
                continue
            raw_arguments = item.get("arguments")
            if isinstance(raw_arguments, str):
                function_arguments = raw_arguments
            elif raw_arguments is None:
                function_arguments = "{}"
            else:
                function_arguments = json.dumps(raw_arguments, ensure_ascii=False)
            normalized_d.append(
                {
                    "id": str(item.get("call_id") or item.get("id") or f"tool_call_{index}"),
                    "type": "function",
                    "function": {
                        "name": function_name,
                        "arguments": function_arguments,
                    },
                }
            )
        return normalized_d

    choices = getattr(response, "choices", None) or []
    if not choices:
        return []
    first_choice = choices[0]
    if isinstance(first_choice, dict):
        message = first_choice.get("message", {})
        raw_tool_calls = message.get("tool_calls", []) if isinstance(message, dict) else []
    else:
        message = getattr(first_choice, "message", None)
        raw_tool_calls = getattr(message, "tool_calls", []) if message else []

    if not raw_tool_calls:
        return []

    normalized_calls: list[dict[str, Any]] = []
    for index, call in enumerate(raw_tool_calls):
        if isinstance(call, dict):
            normalized_calls.append(call)
            continue
        function_payload = getattr(call, "function", None)
        function_name = None
        function_arguments = "{}"
        if function_payload is not None:
            function_name = getattr(function_payload, "name", None)
            function_arguments = getattr(function_payload, "arguments", "{}")
        normalized_calls.append(
            {
                "id": getattr(call, "id", None) or f"tool_call_{index}",
                "type": getattr(call, "type", "function"),
                "function": {
                    "name": function_name,
                    "arguments": function_arguments,
                },
            }
        )
    return normalized_calls


def convert_tools_for_responses(tools: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Convert Chat Completions tool schema to Responses API schema."""
    converted: list[dict[str, Any]] = []
    for tool in tools or []:
        if not isinstance(tool, dict):
            continue
        if str(tool.get("type") or "") != "function":
            converted.append(tool)
            continue
        function_block = tool.get("function") or {}
        converted.append(
            {
                "type": "function",
                "name": function_block.get("name", ""),
                "description": function_block.get("description", ""),
                "parameters": function_block.get("parameters", {}),
            }
        )
    return converted


def messages_to_responses_input(
    *,
    messages: list[Message],
    fallback_instructions: str,
) -> tuple[str, list[dict[str, Any]]]:
    """Convert Chat Completions message list to Responses API instructions + input."""
    instructions_lines: list[str] = []
    input_items: list[dict[str, Any]] = []

    for message in messages:
        payload = message.to_dict() if hasattr(message, "to_dict") else dict(message)
        role = str(payload.get("role") or "user").strip().lower()
        text_content = message_content_to_text(payload.get("content"))

        if role == MessageRole.SYSTEM.value:
            if text_content:
                instructions_lines.append(text_content)
            continue

        if role == MessageRole.TOOL.value:
            call_id = payload.get("tool_call_id")
            if call_id:
                input_items.append(
                    {
                        "type": "function_call_output",
                        "call_id": str(call_id),
                        "output": text_content,
                    }
                )
            else:
                input_items.append(
                    {
                        "role": "user",
                        "content": f"[TOOL OUTPUT]\n{text_content}" if text_content else "[TOOL OUTPUT]",
                    }
                )
            continue

        if role == MessageRole.ASSISTANT.value:
            tool_calls = payload.get("tool_calls")
            if isinstance(tool_calls, list):
                for index, tool_call in enumerate(tool_calls):
                    if not isinstance(tool_call, dict):
                        continue
                    function_block = tool_call.get("function") or {}
                    function_name = str(function_block.get("name") or "").strip()
                    if not function_name:
                        continue
                    raw_arguments = function_block.get("arguments")
                    if isinstance(raw_arguments, str):
                        arguments_text = raw_arguments
                    elif raw_arguments is None:
                        arguments_text = "{}"
                    else:
                        arguments_text = json.dumps(raw_arguments, ensure_ascii=False)
                    input_items.append(
                        {
                            "type": "function_call",
                            "call_id": str(tool_call.get("id") or f"tool_call_{index}"),
                            "name": function_name,
                            "arguments": arguments_text,
                        }
                    )

            if text_content:
                input_items.append({"role": "assistant", "content": text_content})
            continue

        normalized_role = "assistant" if role == MessageRole.ASSISTANT.value else "user"
        input_items.append({"role": normalized_role, "content": text_content})

    instructions = "\n\n".join([line for line in instructions_lines if line]).strip() or fallback_instructions
    if not input_items:
        input_items = [{"role": "user", "content": ""}]
    return instructions, input_items


__all__ = [
    "convert_tools_for_responses",
    "extract_message_content",
    "extract_tool_calls",
    "message_content_to_text",
    "messages_to_responses_input",
    "normalize_api_format",
]
