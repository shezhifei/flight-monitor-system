"""
AI实体主类 - 重构版
精简核心逻辑，移除过度设计。
"""

import asyncio
from collections.abc import AsyncIterator
from dataclasses import dataclass
from typing import Any

from src.infrastructure.logging.core import get_logger
from src.shared.id_generator import generate_short_id

from .context_manager import ContextManager, MemoryContextManager
from .conversation_manager import ConversationManager, MemoryConversationManager
from .llm_stream_runner import LLMStreamRunner
from .openai_client import AiGateway, Message, MessageRole, OpenAIClient, OpenAIClientConfig
from .services.tool_execution_service import ToolExecutionService

logger = get_logger(__name__)


class AIEntity:
    """
    AI实体主类 (Facade)

    作为AI系统的统一入口，协调 Client, Context, Conversation 和 Tool Service。
    """

    def __init__(
        self,
        config: Any = None,
        ai_client: AiGateway | None = None,
        context_manager: ContextManager | None = None,
        conversation_manager: ConversationManager | None = None,
        entity_id: str | None = None,
    ):
        from .ai_entity import AIEntityConfig  # Local import to avoid circular issues if any

        self.config = config or AIEntityConfig()
        self.entity_id = entity_id or f"ai_entity_{generate_short_id(8)}"

        self._ai_client = ai_client
        self._context_manager = context_manager
        self._conversation_manager = conversation_manager

        # Tool Service
        self._tool_service = None

        # Metrics
        self.metrics = {"requests": 0, "errors": 0, "total_tokens": 0, "total_cost": 0.0}

        self._init_task: asyncio.Task | None = None

        self._init_task = asyncio.create_task(self._ensure_initialized())

        logger.info(f"AIEntity '{self.entity_id}' created")

    async def _ensure_initialized(self):
        """Lazy initialization of components"""
        if self._ai_client and self._conversation_manager:
            return

        try:
            if not self._ai_client:
                self._create_ai_client()

            if not self._context_manager:
                self._context_manager = MemoryContextManager(default_model=self.config.default_model)

            if not self._conversation_manager:
                self._conversation_manager = MemoryConversationManager(
                    context_manager=self._context_manager,
                    ai_client=self._ai_client,
                    default_model=self.config.default_model,
                )

            self._tool_service = ToolExecutionService(ai_client=self._ai_client, metrics_callback=self._update_metrics)
            logger.info(f"AIEntity '{self.entity_id}' fully initialized")
        except Exception as e:
            logger.error(f"AIEntity initialization failed: {e}")
            raise

    def _create_ai_client(self):
        """Simple client creation"""
        if self.config.api_key:
            self._ai_client = OpenAIClient(
                config=OpenAIClientConfig(
                    api_key=self.config.api_key,
                    base_url=self.config.base_url,
                    default_model=self.config.default_model,
                    timeout=self.config.timeout,
                    max_retries=self.config.max_retries,
                    retry_delay=self.config.retry_delay,
                )
            )
        else:
            raise ValueError("API key required")

    def _update_metrics(self, latency_ms, tokens, cost, error):
        """Simple metrics callback"""
        self.metrics["requests"] += 1
        if error:
            self.metrics["errors"] += 1
        else:
            self.metrics["total_tokens"] += tokens
            self.metrics["total_cost"] += cost

    def _get_stream_runner(self) -> LLMStreamRunner:
        """Return an LLMStreamRunner backed by this entity's AI client."""
        return LLMStreamRunner(self._ai_client)

    async def send_message(self, message: str, conversation_id: str | None = None, stream: bool = False, **kwargs):
        """
        发送消息 - 核心入口
        """
        await self._ensure_initialized()
        api_format = str(getattr(self.config, "api_format", "chat_completions") or "").strip().lower()

        # Delegate to ConversationManager
        if not conversation_id:
            conversation = await self._conversation_manager.create_conversation(
                user_id=f"user_{self.entity_id}", model=self.config.default_model
            )
            conversation_id = conversation.id

        if stream:
            if api_format == "responses":
                return await self._stream_responses_message(
                    conversation_id=conversation_id,
                    user_message=message,
                    **kwargs,
                )
            return self._conversation_manager.stream_chat_completion(
                conversation_id=conversation_id, user_message=message, **kwargs
            )

        if api_format != "responses":
            return await self._conversation_manager.chat_completion(
                conversation_id=conversation_id, user_message=message, **kwargs
            )

        # responses: keep conversation persistence but call /responses endpoint
        await self._conversation_manager.add_message(
            conversation_id,
            MessageRole.USER,
            message,
        )
        context_messages = await self._conversation_manager.get_conversation_context(conversation_id)
        instructions, input_items = self._messages_to_responses_input(
            messages=context_messages,
            fallback_instructions=self.config.system_prompt or "",
        )
        runner = self._get_stream_runner()
        result = await runner.run_responses(
            model=self.config.default_model,
            instructions=instructions or None,
            input=input_items,
            **kwargs,
        )
        if result.text:
            await self._conversation_manager.add_message(
                conversation_id,
                MessageRole.ASSISTANT,
                result.text,
            )
        return result.raw_response or result

    async def execute_task(self, task_description: str, **kwargs):
        """执行单次任务 (No context)"""
        await self._ensure_initialized()
        api_format = str(getattr(self.config, "api_format", "chat_completions") or "").strip().lower()
        runner = self._get_stream_runner()
        if api_format == "responses":
            result = await runner.run_responses(
                model=self.config.default_model,
                instructions=self.config.system_prompt,
                input=[{"role": "user", "content": task_description}],
                **kwargs,
            )
            return result.raw_response or result

        messages = [Message(role=MessageRole.USER, content=task_description)]
        result = await runner.run_chat(
            messages=messages,
            model=self.config.default_model,
            **kwargs,
        )
        return result

    async def execute_with_tools(
        self, message: str, tools: list[dict[str, Any]] | None = None, tool_executor: Any | None = None, **kwargs
    ):
        """使用工具执行任务"""
        await self._ensure_initialized()
        return await self._tool_service.execute_with_tools(
            message=message, tools=tools or [], tool_executor=tool_executor, config=self.config, **kwargs
        )

    async def _stream_responses_message(
        self,
        *,
        conversation_id: str,
        user_message: str,
        **kwargs,
    ) -> AsyncIterator[Any]:
        await self._conversation_manager.add_message(
            conversation_id,
            MessageRole.USER,
            user_message,
        )

        context_messages = await self._conversation_manager.get_conversation_context(conversation_id)
        instructions, input_items = self._messages_to_responses_input(
            messages=context_messages,
            fallback_instructions=self.config.system_prompt or "",
        )

        request_kwargs = dict(kwargs)
        if "max_tokens" in request_kwargs and "max_output_tokens" not in request_kwargs:
            request_kwargs["max_output_tokens"] = request_kwargs.pop("max_tokens")
        request_kwargs["stream"] = True

        event_stream = await self._ai_client.responses_create(
            model=self.config.default_model,
            instructions=instructions or None,
            input=input_items,
            **request_kwargs,
        )

        async def _generator():
            text_parts: list[str] = []
            completed_response_payload: dict[str, Any] | None = None

            async for event in event_stream:
                delta_text = self._extract_stream_delta_text(event)
                if delta_text:
                    text_parts.append(delta_text)

                event_type = str(getattr(event, "type", "") or "").strip().lower()
                if event_type in {"response.completed", "response.done"}:
                    payload = getattr(event, "response", None)
                    if isinstance(payload, dict):
                        completed_response_payload = payload

                yield event

            final_text = "".join(text_parts).strip()
            if not final_text and completed_response_payload:
                final_text = self._extract_response_text(completed_response_payload).strip()

            if final_text:
                await self._conversation_manager.add_message(
                    conversation_id,
                    MessageRole.ASSISTANT,
                    final_text,
                )

        return _generator()

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
        messages: list[Message],
        fallback_instructions: str,
    ) -> tuple[str, list[dict[str, Any]]]:
        instructions_lines: list[str] = []
        input_items: list[dict[str, Any]] = []

        for message in messages:
            payload = message.to_dict() if hasattr(message, "to_dict") else dict(message)
            role = str(payload.get("role") or "user").strip().lower()
            text_content = cls._message_content_to_text(payload.get("content"))

            if role == MessageRole.SYSTEM.value:
                if text_content:
                    instructions_lines.append(text_content)
                continue

            normalized_role = "assistant" if role == MessageRole.ASSISTANT.value else "user"
            input_items.append({"role": normalized_role, "content": text_content})

        instructions = "\n\n".join([line for line in instructions_lines if line]).strip() or fallback_instructions
        if not input_items:
            input_items = [{"role": "user", "content": ""}]
        return instructions, input_items

    @staticmethod
    def _extract_response_text(response: Any) -> str:
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

    @staticmethod
    def _extract_stream_delta_text(event: Any) -> str:
        event_type = str(getattr(event, "type", "") or "").strip().lower()
        if event_type in {"response.text.delta", "response.output_text.delta"}:
            delta = getattr(event, "delta", None)
            if delta is not None:
                return str(delta)
        if event_type in {"response.text.done", "response.output_text.done"}:
            text = getattr(event, "text", None)
            if text is not None:
                return str(text)
        return ""

    def get_allowed_tools(
        self,
        user_id: str | None = None,
        user_roles: list[str] | None = None,
        invocation_mode: str | None = None,
    ) -> list[dict[str, Any]]:
        """Return tool schemas filtered by entity config and user permission."""
        from .tools import ToolCategory, get_tool_registry

        registry = get_tool_registry()

        category_filter: list[ToolCategory] | None = None
        configured_categories = self.config.allowed_tool_categories or []
        if configured_categories:
            category_filter = []
            for raw_category in configured_categories:
                if isinstance(raw_category, ToolCategory):
                    category_filter.append(raw_category)
                    continue

                try:
                    category_filter.append(ToolCategory(str(raw_category)))
                except ValueError:
                    logger.warning(f"Unknown tool category '{raw_category}' for entity '{self.entity_id}'")

        tools = registry.get_tools(
            categories=category_filter,
            user_id=user_id,
            user_roles=user_roles,
            invocation_mode=invocation_mode,
        )

        allowed_tools = set(self.config.allowed_tools or [])
        denied_tools = set(self.config.denied_tools or [])

        filtered_tools: list[dict[str, Any]] = []
        for tool in tools:
            function_block = tool.get("function", {}) if isinstance(tool, dict) else {}
            tool_name = str(function_block.get("name", "")).strip()
            if not tool_name:
                continue

            if allowed_tools and tool_name not in allowed_tools:
                continue

            if tool_name in denied_tools:
                continue

            filtered_tools.append(tool)

        return filtered_tools


# Keep Config class but expanded
@dataclass
class AIEntityConfig:
    # 基础配置
    api_key: str | None = None
    base_url: str = "https://api.openai.com/v1"
    default_model: str = "gpt-3.5-turbo"

    # 运行参数 (原 Providers 配置)
    timeout: float = 30.0
    max_retries: int = 3
    retry_delay: float = 0.5
    temperature: float = 0.7
    max_tokens: int = 1000

    # 模型元数据 (原静态配置)
    cost_per_1k_input: float = 0.0
    cost_per_1k_output: float = 0.0
    context_window: int = 128000

    # API 格式选择: "chat_completions" | "responses"
    api_format: str = "chat_completions"

    # 提示词模板
    system_prompt: str | None = None
    task_template: str | None = None

    # 权限与工具
    allowed_tool_categories: list[str] = None
    allowed_tools: list[str] = None
    denied_tools: list[str] = None

    # Prompt Cache 配置
    enable_prompt_cache: bool = False
    prompt_cache_retention: str | None = None  # "in_memory" | "24h"

    # Responses Session Chaining 配置
    enable_responses_session_chain: bool = False
