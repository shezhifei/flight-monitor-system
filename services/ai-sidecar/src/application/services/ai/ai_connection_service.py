"""Application-layer service for AI connection testing and plan generation.

Encapsulates infrastructure-level ``OpenAIClient`` / ``AIEntity`` instantiation
so that route handlers never directly touch ``src.infrastructure.ai.*`` classes.
"""

from __future__ import annotations

from typing import Any

from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


def _normalize_optional_string(value: str | None) -> str | None:
    if value is None:
        return None
    normalized = str(value).strip()
    return normalized or None


def _normalize_api_format(api_format: str | None) -> str:
    value = str(api_format or "").strip().lower()
    return "responses" if value == "responses" else "chat_completions"


async def test_ai_connection(
    *,
    base_url: str,
    api_key: str,
    timeout: float = 10.0,
) -> list[dict[str, Any]]:
    """Create a short-lived OpenAI client, probe connectivity, and return raw model list.

    Raises the same openai / httpx exceptions as the underlying client so
    the caller (route handler) can map them to HTTP status codes.
    """
    from src.infrastructure.ai.openai_client import OpenAIClient, OpenAIClientConfig

    client = OpenAIClient(
        config=OpenAIClientConfig(
            api_key=api_key,
            base_url=base_url,
            default_model="gpt-4o-mini",
            timeout=timeout,
            max_retries=0,
            retry_delay=0.2,
            enable_logging=False,
            log_prompts=False,
        )
    )

    try:
        return await client.list_models()
    finally:
        await client.close()


async def generate_plan(
    *,
    prompt: str,
    config: dict[str, Any],
    system_prompt_override: str | None = None,
) -> str:
    """Generate a structured task plan using AI.

    Returns the raw text content from the AI response.
    """
    from src.infrastructure.ai.ai_entity import AIEntity, AIEntityConfig
    from src.infrastructure.ai.llm_stream_runner import LLMStreamRunner
    from src.infrastructure.ai.openai_client import Message, MessageRole
    from src.infrastructure.ai.prompts import PLANNER_SYSTEM_PROMPT

    entity_config = AIEntityConfig.from_document(config, temperature=0.7)
    entity_config.api_format = _normalize_api_format(entity_config.api_format)
    entity = AIEntity(config=entity_config)
    await entity._ensure_initialized()

    system_prompt = system_prompt_override or config.get("system_prompt") or PLANNER_SYSTEM_PROMPT
    runner = LLMStreamRunner(entity._ai_client)

    if _normalize_api_format(entity_config.api_format) == "responses":
        result = await runner.run_responses(
            model=entity_config.default_model,
            instructions=system_prompt,
            input=[{"role": "user", "content": prompt}],
            temperature=0.7,
        )
        response = result.raw_response or result
    else:
        messages = [
            Message(role=MessageRole.SYSTEM, content=system_prompt),
            Message(role=MessageRole.USER, content=prompt),
        ]
        result = await runner.run_chat(
            messages=messages,
            model=entity_config.default_model,
            temperature=0.7,
            response_format={"type": "json_object"},
        )
        response = result.raw_response or result

    # Re-use the same text extraction logic
    if isinstance(response, dict):
        output_text = response.get("output_text")
        if isinstance(output_text, str) and output_text.strip():
            return output_text.strip()

        output_items = response.get("output")
        if isinstance(output_items, list):
            parts: list[str] = []
            for item in output_items:
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
            if merged:
                return merged

        choices = response.get("choices") or []
        if choices:
            first_choice = choices[0]
            if isinstance(first_choice, dict):
                return str((first_choice.get("message") or {}).get("content") or "")

    output_text = getattr(response, "output_text", None)
    if isinstance(output_text, str) and output_text.strip():
        return output_text.strip()

    output_items = getattr(response, "output", None)
    if isinstance(output_items, list):
        parts: list[str] = []
        for item in output_items:
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
        if merged:
            return merged

    choices = getattr(response, "choices", None) or []
    if choices:
        first_choice = choices[0]
        if isinstance(first_choice, dict):
            return str((first_choice.get("message") or {}).get("content") or "")
        message = getattr(first_choice, "message", None)
        return str(getattr(message, "content", "") or "")

    return ""
