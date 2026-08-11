import asyncio
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any

from src.infrastructure.logging.core import get_logger

from ..ai_entity import AIEntity

if TYPE_CHECKING:
    from .ai_entity_manager import AIEntityManager

logger = get_logger(__name__)


@dataclass
class BatchRequest:
    """批量请求项"""

    request_id: str
    content: str
    metadata: dict[str, Any] = None


@dataclass
class BatchResult:
    """批量结果项"""

    request_id: str
    success: bool
    response: str | None
    error: str | None
    metadata: dict[str, Any] = None


class BatchOperationService:
    """
    AI 批量操作服务

    用于处理大量并行的 AI 任务（如批量分析评论、日志分类等）。
    支持并发控制和错误处理。
    """

    def __init__(self, entity_manager: "AIEntityManager", max_concurrency: int = 5):
        self.max_concurrency = max_concurrency
        self._semaphore = asyncio.Semaphore(max_concurrency)
        self.entity_manager = entity_manager

    async def process_batch(
        self, entity_id: str, requests: list[BatchRequest], system_prompt_override: str | None = None
    ) -> list[BatchResult]:
        """
        处理批量请求

        Args:
            entity_id: 用于处理请求的 AI 实体 ID
            requests: 请求列表
            system_prompt_override: 覆盖系统提示词

        Returns:
            结果列表
        """
        entity = await self.entity_manager.get_entity(entity_id)
        if not entity:
            raise ValueError(f"Entity not found: {entity_id}")

        logger.info(f"Starting batch process: {len(requests)} items with entity {entity_id}")

        results: list[BatchResult] = await asyncio.gather(
            *(self._process_single_item(entity, req, system_prompt_override) for req in requests)
        )

        logger.info(f"Batch process completed. Success: {sum(1 for r in results if r.success)}/{len(results)}")
        return results

    async def _process_single_item(
        self, entity: AIEntity, request: BatchRequest, system_prompt: str | None
    ) -> BatchResult:
        """处理单个请求"""
        async with self._semaphore:
            try:
                # 构造消息
                # 这里简化处理，直接发送 content
                # 实际可以复用 context_manager 或 conversation_manager
                from ..openai_client import Message, MessageRole

                messages = []
                if system_prompt:
                    messages.append(Message(role=MessageRole.SYSTEM, content=system_prompt))
                messages.append(Message(role=MessageRole.USER, content=request.content))

                api_format = self._normalize_api_format(getattr(entity.config, "api_format", "chat_completions"))
                if api_format == "responses":
                    instructions, input_items = self._messages_to_responses_input(
                        messages=messages,
                        fallback_instructions=system_prompt or "",
                    )
                    response = await entity._ai_client.responses_create(
                        model=entity.config.default_model,
                        instructions=instructions or None,
                        input=input_items,
                        temperature=getattr(entity.config, "temperature", None),
                        max_output_tokens=getattr(entity.config, "max_tokens", None),
                        stream=False,
                    )
                else:
                    response = await entity._ai_client.chat_completion(
                        messages=messages, model=entity.config.default_model, temperature=entity.config.temperature
                    )

                content = self._extract_response_content(response)

                return BatchResult(
                    request_id=request.request_id, success=True, response=content, error=None, metadata=request.metadata
                )

            except Exception as e:  # noqa: BLE001 - batch item failures captured as failed results, not propagated
                logger.error(f"Batch item failed {request.request_id}: {e}")
                return BatchResult(
                    request_id=request.request_id, success=False, response=None, error=str(e), metadata=request.metadata
                )

    @staticmethod
    def _normalize_api_format(api_format: Any) -> str:
        value = str(api_format or "").strip().lower()
        return "responses" if value == "responses" else "chat_completions"

    @staticmethod
    def _message_content_to_text(content: Any) -> str:
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

    @classmethod
    def _messages_to_responses_input(
        cls,
        *,
        messages: list[Any],
        fallback_instructions: str,
    ) -> tuple[str, list[dict[str, Any]]]:
        instructions_lines: list[str] = []
        input_items: list[dict[str, Any]] = []

        for message in messages:
            payload = message.to_dict() if hasattr(message, "to_dict") else dict(message)
            role = str(payload.get("role") or "user").strip().lower()
            text_content = cls._message_content_to_text(payload.get("content"))

            if role == "system":
                if text_content:
                    instructions_lines.append(text_content)
                continue

            normalized_role = "assistant" if role == "assistant" else "user"
            input_items.append({"role": normalized_role, "content": text_content})

        instructions = "\n\n".join([line for line in instructions_lines if line]).strip() or fallback_instructions
        if not input_items:
            input_items = [{"role": "user", "content": ""}]
        return instructions, input_items

    @staticmethod
    def _extract_response_content(response: Any) -> str:
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
        if not choices:
            return ""
        first_choice = choices[0]
        if isinstance(first_choice, dict):
            return str((first_choice.get("message") or {}).get("content") or "")
        message = getattr(first_choice, "message", None)
        return str(getattr(message, "content", "") or "")
