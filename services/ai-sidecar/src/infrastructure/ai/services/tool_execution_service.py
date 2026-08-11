import time
from typing import Any

from src.infrastructure.ai.llm_stream_runner import LLMStreamRunner
from src.infrastructure.logging.core import get_logger

from ..openai_client import ChatCompletionResponse, Message, MessageRole
from ..responses_adapter import (
    convert_tools_for_responses as _convert_tools_fn,
)
from ..responses_adapter import (
    extract_message_content as _extract_content_fn,
)
from ..responses_adapter import (
    extract_tool_calls as _extract_tool_calls_fn,
)
from ..responses_adapter import (
    message_content_to_text as _content_to_text_fn,
)
from ..responses_adapter import (
    messages_to_responses_input as _messages_to_responses_fn,
)
from ..responses_adapter import (
    normalize_api_format as _normalize_fn,
)

logger = get_logger(__name__)


class ToolExecutionService:
    """工具执行服务 - 负责处理AI工具调用逻辑"""

    def __init__(self, ai_client, metrics_callback=None):
        self.ai_client = ai_client
        self.metrics_callback = metrics_callback

    async def execute_with_tools(
        self,
        message: str,
        tools: list[dict[str, Any]],
        tool_executor: Any,
        config: Any,
        system_prompt: str | None = None,
        max_iterations: int = 3,
        **kwargs,
    ) -> Any:
        """执行带工具调用的任务"""
        start_time = time.time()

        # 准备初始消息
        messages = []
        if system_prompt:
            messages.append(Message(role=MessageRole.SYSTEM, content=system_prompt))
        messages.append(Message(role=MessageRole.USER, content=message))

        current_response = None
        iteration = 0

        try:
            while iteration < max_iterations:
                iteration += 1

                # 调用AI
                response = await self._request_ai(
                    messages=messages,
                    config=config,
                    tools=tools,
                    kwargs=kwargs,
                )

                current_response = response

                # 检查是否有工具调用
                tool_calls = self._extract_tool_calls_from_response(response)

                if not tool_calls:
                    break

                if not tool_executor:
                    logger.warning("收到工具调用但没有提供执行器")
                    break

                logger.info(f"执行 {len(tool_calls)} 个工具调用 (迭代 {iteration}/{max_iterations})")

                # 添加AI的工具调用消息
                assistant_content = self._extract_response_content(response)
                messages.append(Message(role=MessageRole.ASSISTANT, content=assistant_content, tool_calls=tool_calls))

                # 执行工具并添加结果
                results = await tool_executor.execute_tool_calls(tool_calls)

                for result in results:
                    tool_message = result.to_message()
                    messages.append(
                        Message(
                            role=MessageRole.TOOL,
                            content=tool_message["content"],
                            tool_call_id=tool_message["tool_call_id"],
                        )
                    )

            # 更新指标
            if self.metrics_callback:
                latency_ms = int((time.time() - start_time) * 1000)
                tokens = 0
                cost = 0.0
                if isinstance(current_response, ChatCompletionResponse) and current_response.usage:
                    usage = current_response.usage
                    tokens = usage.get("total_tokens", 0)
                elif getattr(current_response, "usage", None):
                    usage = current_response.usage or {}
                    input_tokens = int(usage.get("input_tokens", 0) or 0)
                    output_tokens = int(usage.get("output_tokens", 0) or 0)
                    tokens = int(usage.get("total_tokens", 0) or (input_tokens + output_tokens))

                # 使用配置中的费率计算成本
                if getattr(current_response, "usage", None):
                    usage = current_response.usage or {}
                    input_tokens = int(usage.get("prompt_tokens", 0) or usage.get("input_tokens", 0) or 0)
                    output_tokens = int(usage.get("completion_tokens", 0) or usage.get("output_tokens", 0) or 0)
                    cost = (input_tokens / 1000 * getattr(config, "cost_per_1k_input", 0)) + (
                        output_tokens / 1000 * getattr(config, "cost_per_1k_output", 0)
                    )

                self.metrics_callback(latency_ms, tokens, cost, error=False)

            return current_response

        except Exception:
            latency_ms = int((time.time() - start_time) * 1000)
            if self.metrics_callback:
                self.metrics_callback(latency_ms, 0, 0.0, error=True)
            logger.error(f"tool execution failed after {latency_ms}ms", exc_info=True)
            raise

    async def _request_ai(
        self,
        *,
        messages: list[Message],
        config: Any,
        tools: list[dict[str, Any]],
        kwargs: dict[str, Any],
    ) -> Any:
        runner = LLMStreamRunner(self.ai_client)
        api_format = self._normalize_api_format(getattr(config, "api_format", "chat_completions"))
        temperature = kwargs.get("temperature", config.temperature)
        max_tokens = kwargs.get("max_tokens", config.max_tokens)

        extra_kwargs = dict(kwargs)
        extra_kwargs.pop("temperature", None)
        extra_kwargs.pop("max_tokens", None)
        extra_kwargs.pop("tools", None)
        extra_kwargs.pop("tool_choice", None)
        extra_kwargs.pop("stream", None)

        if api_format == "responses":
            instructions, input_items = self._messages_to_responses_input(
                messages=messages,
                fallback_instructions=system_prompt_from_messages(messages),
            )
            result = await runner.run_responses(
                model=config.default_model,
                instructions=instructions or None,
                input=input_items,
                tools=self._convert_tools_for_responses(tools),
                tool_choice="auto" if tools else None,
                temperature=temperature,
                max_output_tokens=max_tokens,
                **extra_kwargs,
            )
            return result.raw_response or result

        result = await runner.run_chat(
            messages=messages,
            model=config.default_model,
            temperature=temperature,
            max_tokens=max_tokens,
            tools=tools,
            **extra_kwargs,
        )
        return result.raw_response or result

    def _extract_tool_calls_from_response(self, response: Any) -> list[dict[str, Any]]:
        """从响应中提取工具调用"""
        return _extract_tool_calls_fn(response)

    @staticmethod
    def _is_responses_payload(response: Any) -> bool:
        return hasattr(response, "output") and hasattr(response, "output_text")

    @staticmethod
    def _normalize_api_format(api_format: Any) -> str:
        return _normalize_fn(api_format)

    @staticmethod
    def _message_content_to_text(content: Any) -> str:
        return _content_to_text_fn(content)

    @classmethod
    def _messages_to_responses_input(
        cls,
        *,
        messages: list[Message],
        fallback_instructions: str,
    ) -> tuple[str, list[dict[str, Any]]]:
        return _messages_to_responses_fn(
            messages=messages,
            fallback_instructions=fallback_instructions,
        )

    @staticmethod
    def _convert_tools_for_responses(tools: list[dict[str, Any]]) -> list[dict[str, Any]]:
        return _convert_tools_fn(tools)

    @classmethod
    def _extract_response_content(cls, response: Any) -> str:
        return _extract_content_fn(response)


def system_prompt_from_messages(messages: list[Message]) -> str:
    for message in messages:
        role = message.role.value if hasattr(message.role, "value") else message.role
        if role == MessageRole.SYSTEM.value:
            content = message.content
            if isinstance(content, str) and content.strip():
                return content
    return ""
