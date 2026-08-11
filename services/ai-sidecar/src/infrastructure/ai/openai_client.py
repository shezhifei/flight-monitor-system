"""OpenAI-compatible gateway implemented with official OpenAI SDK.

This module keeps existing public types and method signatures stable for the
rest of the codebase while delegating network calls to ``openai.AsyncOpenAI``.
"""

from __future__ import annotations

import asyncio
import inspect
import json
import time
from abc import ABC, abstractmethod
from collections.abc import AsyncIterator
from dataclasses import dataclass
from dataclasses import fields as dataclass_fields
from enum import Enum, StrEnum
from pathlib import Path
from typing import Any, TypeVar

import httpx
from openai import AsyncOpenAI
from pydantic import BaseModel, Field

from src.infrastructure.ai.security.url_guard import validate_external_http_url
from src.infrastructure.common.exceptions import LLM_EXCEPTIONS
from src.infrastructure.config.config_manager import ConfigManager
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)

T = TypeVar("T")


class MessageRole(StrEnum):
    """Message role enum."""

    SYSTEM = "system"
    USER = "user"
    ASSISTANT = "assistant"
    FUNCTION = "function"
    TOOL = "tool"


class ContentPartType(StrEnum):
    """Multimodal content part enum."""

    TEXT = "text"
    IMAGE_URL = "image_url"
    INPUT_AUDIO = "input_audio"


@dataclass
class ContentPart:
    """Multimodal content part.

    ``image_url`` format example:
    ``{"url": "data:image/jpeg;base64,...", "detail": "auto|low|high"}``

    ``input_audio`` format example:
    ``{"data": "<base64>", "format": "wav|mp3"}``
    """

    type: ContentPartType
    text: str | None = None
    image_url: dict[str, str] | None = None
    input_audio: dict[str, str] | None = None

    def to_dict(self) -> dict[str, Any]:
        result: dict[str, Any] = {"type": self.type.value}
        if self.type == ContentPartType.TEXT and self.text is not None:
            result["text"] = self.text
        elif self.type == ContentPartType.IMAGE_URL and self.image_url is not None:
            result["image_url"] = self.image_url
        elif self.type == ContentPartType.INPUT_AUDIO and self.input_audio is not None:
            result["input_audio"] = self.input_audio
        return result

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ContentPart:
        part_type = ContentPartType(data.get("type", "text"))
        return cls(
            type=part_type, text=data.get("text"), image_url=data.get("image_url"), input_audio=data.get("input_audio")
        )


def build_audio_content_part(data_b64: str, fmt: str) -> dict[str, Any]:
    """Build an ``input_audio`` multimodal content block.

    ``data_b64`` is the base64-encoded audio payload and ``fmt`` is the audio
    container format (``"wav"`` or ``"mp3"``). Returns the dict shape expected
    by the OpenAI chat completions API for audio inputs.
    """
    return {
        "type": ContentPartType.INPUT_AUDIO.value,
        "input_audio": {"data": data_b64, "format": fmt},
    }


MultimodalContent = str | list[dict[str, Any] | ContentPart]


@dataclass
class Message:
    """Chat message that supports plain text and multimodal list content."""

    role: MessageRole
    content: MultimodalContent
    name: str | None = None
    tool_calls: list[dict[str, Any]] | None = None
    tool_call_id: str | None = None
    metadata: dict[str, Any] | None = None

    def to_dict(self) -> dict[str, Any]:
        result: dict[str, Any] = {
            "role": self.role.value if isinstance(self.role, Enum) else str(self.role),
        }

        if isinstance(self.content, str):
            result["content"] = self.content
        elif isinstance(self.content, list):
            content_list: list[dict[str, Any]] = []
            for part in self.content:
                if isinstance(part, ContentPart):
                    content_list.append(part.to_dict())
                elif isinstance(part, dict):
                    content_list.append(part)
                else:
                    content_list.append({"type": "text", "text": str(part)})
            result["content"] = content_list
        else:
            result["content"] = str(self.content)

        if self.name is not None:
            result["name"] = self.name
        if self.tool_calls is not None:
            result["tool_calls"] = self.tool_calls
        if self.tool_call_id is not None:
            result["tool_call_id"] = self.tool_call_id
        if self.metadata is not None:
            result["metadata"] = self.metadata

        return result

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> Message:
        role_str = data.get("role", "user")
        if isinstance(role_str, str):
            role = MessageRole(role_str)
        else:
            role = role_str

        content_data = data.get("content", "")
        content: MultimodalContent = content_data
        if isinstance(content_data, list):
            parsed_content: list[dict[str, Any] | ContentPart] = []
            for part in content_data:
                if isinstance(part, dict):
                    try:
                        parsed_content.append(ContentPart.from_dict(part))
                    except (ValueError, KeyError):
                        parsed_content.append(part)
                else:
                    parsed_content.append(part)
            content = parsed_content

        return cls(
            role=role,
            content=content,
            name=data.get("name"),
            tool_calls=data.get("tool_calls"),
            tool_call_id=data.get("tool_call_id"),
            metadata=data.get("metadata"),
        )


@dataclass
class ChatCompletionRequest:
    """Chat completion request DTO (compat export)."""

    messages: list[Message]
    model: str = "gpt-3.5-turbo"
    temperature: float | None = None
    top_p: float | None = None
    n: int | None = 1
    stream: bool | None = False
    max_tokens: int | None = None
    presence_penalty: float | None = None
    frequency_penalty: float | None = None
    logit_bias: dict[int, float] | None = None
    user: str | None = None
    tools: list[dict[str, Any]] | None = None
    tool_choice: str | dict[str, Any] | None = None


@dataclass
class ChatCompletionResponse:
    """Chat completion response DTO used across the current codebase."""

    id: str
    object: str
    created: int
    model: str
    choices: list[dict[str, Any]]
    usage: dict[str, Any] | None = None
    system_fingerprint: str | None = None


@dataclass
class ChatCompletionChunk:
    """Streaming chat completion chunk DTO."""

    id: str
    object: str
    created: int
    model: str
    choices: list[dict[str, Any]]
    system_fingerprint: str | None = None


@dataclass
class ResponsesAPIResponse:
    """Responses API response DTO."""

    id: str
    object: str
    status: str
    model: str
    output: list[dict[str, Any]]
    usage: dict[str, Any] | None = None
    created_at: float | None = None
    instructions: str | None = None
    incomplete_details: dict[str, Any] | None = None
    metadata: dict[str, Any] | None = None
    error: dict[str, Any] | None = None
    temperature: float | None = None
    top_p: float | None = None
    max_output_tokens: int | None = None
    previous_response_id: str | None = None
    reasoning: dict[str, Any] | None = None
    text: dict[str, Any] | None = None
    tools: list[dict[str, Any]] | None = None
    tool_choice: Any | None = None
    truncation: str | None = None
    user: str | None = None

    @property
    def output_text(self) -> str:
        parts: list[str] = []
        for item in self.output or []:
            if item.get("type") != "message":
                continue
            for content_part in item.get("content") or []:
                if content_part.get("type") == "output_text":
                    parts.append(content_part.get("text") or "")
        return "".join(parts)


@dataclass
class ResponsesAPIStreamEvent:
    """Typed stream event DTO for the Responses API."""

    type: str
    response: dict[str, Any] | None = None
    output_index: int | None = None
    content_index: int | None = None
    item: dict[str, Any] | None = None
    delta: str | None = None
    text: str | None = None
    part: dict[str, Any] | None = None
    sequence_number: int | None = None


class AiGateway(ABC):
    """AI gateway interface (port)."""

    @abstractmethod
    async def chat_completion(
        self,
        messages: list[Message],
        model: str,
        temperature: float | None = None,
        max_tokens: int | None = None,
        stream: bool | None = False,
        prompt_cache_key: str | None = None,
        prompt_cache_retention: str | None = None,
        **kwargs: Any,
    ) -> ChatCompletionResponse | AsyncIterator[ChatCompletionChunk]:
        pass

    @abstractmethod
    async def completion(
        self,
        prompt: str,
        model: str,
        temperature: float | None = None,
        max_tokens: int | None = None,
        stream: bool | None = False,
        **kwargs: Any,
    ) -> dict[str, Any] | AsyncIterator[dict[str, Any]]:
        pass

    @abstractmethod
    async def embeddings(
        self,
        input: str | list[str],
        model: str,
        **kwargs: Any,
    ) -> dict[str, Any]:
        pass

    @abstractmethod
    async def audio_transcriptions(
        self,
        file: str | bytes,
        model: str,
        language: str | None = None,
        prompt: str | None = None,
        response_format: str | None = "json",
        temperature: float | None = None,
        **kwargs: Any,
    ) -> dict[str, Any]:
        pass

    @abstractmethod
    async def audio_speech(
        self,
        input: str,
        model: str,
        voice: str,
        response_format: str | None = "mp3",
        speed: float | None = 1.0,
        **kwargs: Any,
    ) -> bytes:
        pass

    @abstractmethod
    async def health_check(self) -> bool:
        pass

    @abstractmethod
    async def responses_create(
        self,
        model: str,
        input: str | list[dict[str, Any]],
        *,
        instructions: str | None = None,
        tools: list[dict[str, Any]] | None = None,
        tool_choice: str | dict[str, Any] | None = None,
        max_output_tokens: int | None = None,
        reasoning: dict[str, Any] | None = None,
        previous_response_id: str | None = None,
        store: bool | None = None,
        truncation: str | None = None,
        stream: bool | None = False,
        include: list[str] | None = None,
        prompt_cache_key: str | None = None,
        prompt_cache_retention: str | None = None,
        **kwargs: Any,
    ) -> ResponsesAPIResponse | AsyncIterator[ResponsesAPIStreamEvent]:
        pass

    @abstractmethod
    async def responses_retrieve(self, response_id: str) -> ResponsesAPIResponse:
        pass

    @abstractmethod
    async def responses_cancel(self, response_id: str) -> ResponsesAPIResponse:
        pass

    @abstractmethod
    async def responses_delete(self, response_id: str) -> bool:
        pass


class OpenAIClientConfig(BaseModel):
    """OpenAI client configuration."""

    api_key: str = Field(..., description="API key")
    base_url: str = Field("https://api.openai.com/v1", description="API base URL")
    default_model: str = Field("gpt-3.5-turbo", description="Default model")
    timeout: float = Field(30.0, description="Request timeout in seconds")
    max_retries: int = Field(3, description="SDK max retries")
    retry_delay: float = Field(0.5, description="Compatibility field, not actively used")
    max_connections: int = Field(100, description="Max HTTP connections")
    max_keepalive_connections: int = Field(20, description="Max keep-alive HTTP connections")
    enable_logging: bool = Field(True, description="Enable request logging")
    log_prompts: bool = Field(False, description="Log prompt payloads (be careful)")


class OpenAIClient(AiGateway):
    """Thin compatibility wrapper over official OpenAI Async SDK."""

    def __init__(
        self,
        config: OpenAIClientConfig | None = None,
        config_manager: ConfigManager | None = None,
        http_client: httpx.AsyncClient | None = None,
    ):
        if config is None and config_manager is not None:
            config = self._load_config_from_manager(config_manager)
        elif config is None:
            raise ValueError("must provide config or config_manager")

        config.base_url = validate_external_http_url(config.base_url, purpose="OpenAI base_url")
        self.config = config
        self._http_client = http_client or self._create_http_client()
        self._sdk_client = AsyncOpenAI(
            api_key=config.api_key,
            base_url=config.base_url,
            timeout=config.timeout,
            max_retries=max(0, int(config.max_retries)),
            http_client=self._http_client,
            default_headers={"User-Agent": "FlightMonitorAI/1.0"},
        )
        self._closed = False

        logger.info(
            "OpenAIClient initialized with SDK, base_url=%s, default_model=%s, timeout=%s",
            config.base_url,
            config.default_model,
            config.timeout,
        )

    @staticmethod
    def _merge_non_none(target: dict[str, Any], source: dict[str, Any]) -> dict[str, Any]:
        for key, value in source.items():
            if value is not None:
                target[key] = value
        return target

    @staticmethod
    def _to_data(payload: Any) -> Any:
        if payload is None:
            return None
        if isinstance(payload, (str, int, float, bool, bytes)):
            return payload
        if isinstance(payload, dict):
            return payload
        if isinstance(payload, list):
            return [OpenAIClient._to_data(item) for item in payload]
        model_dump = getattr(payload, "model_dump", None)
        if callable(model_dump):
            return model_dump(mode="python")
        to_dict = getattr(payload, "to_dict", None)
        if callable(to_dict):
            return to_dict()
        return payload

    def _build_dataclass_from_payload(
        self,
        payload: dict[str, Any],
        dataclass_type: type[T],
        default_values: dict[str, Any] | None = None,
    ) -> T:
        if not isinstance(payload, dict):
            raise ValueError(f"Expected dict payload for {dataclass_type.__name__}, got {type(payload).__name__}")

        allowed_fields = {field.name for field in dataclass_fields(dataclass_type)}
        normalized_payload: dict[str, Any] = {key: value for key, value in payload.items() if key in allowed_fields}

        unknown_fields = sorted(set(payload.keys()) - allowed_fields)
        if unknown_fields:
            logger.debug("Ignore unknown %s fields: %s", dataclass_type.__name__, unknown_fields)

        if default_values:
            for key, value in default_values.items():
                normalized_payload.setdefault(key, value)

        return dataclass_type(**normalized_payload)

    def _parse_chat_completion_response(
        self,
        payload: dict[str, Any],
        requested_model: str,
    ) -> ChatCompletionResponse:
        return self._build_dataclass_from_payload(
            payload,
            ChatCompletionResponse,
            {
                "id": "chatcmpl_compat",
                "object": "chat.completion",
                "created": int(time.time()),
                "model": requested_model,
                "choices": [],
            },
        )

    def _parse_chat_completion_chunk(self, payload: dict[str, Any]) -> ChatCompletionChunk:
        return self._build_dataclass_from_payload(
            payload,
            ChatCompletionChunk,
            {
                "id": "chatcmpl_chunk_compat",
                "object": "chat.completion.chunk",
                "created": int(time.time()),
                "model": self.config.default_model,
                "choices": [],
            },
        )

    def _parse_responses_api_response(
        self,
        payload: dict[str, Any],
        requested_model: str,
    ) -> ResponsesAPIResponse:
        return self._build_dataclass_from_payload(
            payload,
            ResponsesAPIResponse,
            {
                "id": "resp_compat",
                "object": "response",
                "status": "completed",
                "model": requested_model,
                "output": [],
            },
        )

    def _parse_responses_api_stream_event(self, payload: dict[str, Any]) -> ResponsesAPIStreamEvent:
        event_type = str(payload.get("type") or "unknown").strip() or "unknown"
        return self._build_dataclass_from_payload(
            payload,
            ResponsesAPIStreamEvent,
            {"type": event_type},
        )

    def _load_config_from_manager(self, config_manager: ConfigManager) -> OpenAIClientConfig:
        ai_config = config_manager.get_dict("ai.providers.openai", {})
        return OpenAIClientConfig(
            api_key=ai_config.get("api_key", ""),
            base_url=ai_config.get("base_url", "https://api.openai.com/v1"),
            default_model=ai_config.get("default_model", "gpt-3.5-turbo"),
            timeout=ai_config.get("timeout", 30.0),
            max_retries=ai_config.get("max_retries", 3),
            retry_delay=ai_config.get("retry_delay", 0.5),
            max_connections=ai_config.get("max_connections", 100),
            max_keepalive_connections=ai_config.get("max_keepalive_connections", 20),
            enable_logging=ai_config.get("enable_logging", True),
            log_prompts=ai_config.get("log_prompts", False),
        )

    def _create_http_client(self) -> httpx.AsyncClient:
        connect_timeout = min(10.0, float(self.config.timeout))
        timeout = httpx.Timeout(
            timeout=float(self.config.timeout),
            connect=connect_timeout,
            read=float(self.config.timeout),
            write=float(self.config.timeout),
        )
        limits = httpx.Limits(
            max_connections=int(self.config.max_connections),
            max_keepalive_connections=int(self.config.max_keepalive_connections),
        )
        return httpx.AsyncClient(timeout=timeout, limits=limits, follow_redirects=False)

    async def _close_stream_object(self, stream_obj: Any) -> None:
        for close_method_name in ("aclose", "close"):
            close_method = getattr(stream_obj, close_method_name, None)
            if not callable(close_method):
                continue
            try:
                result = close_method()
                if inspect.isawaitable(result):
                    await result
                return
            except Exception as exc:  # noqa: BLE001 - stream close must not break iteration
                logger.debug("failed to close stream object via %s: %s", close_method_name, exc)

    async def _iter_chat_completion_stream(self, stream_obj: Any) -> AsyncIterator[ChatCompletionChunk]:
        try:
            async for chunk in stream_obj:
                payload = self._to_data(chunk)
                if isinstance(payload, dict):
                    yield self._parse_chat_completion_chunk(payload)
        finally:
            await self._close_stream_object(stream_obj)

    async def _iter_dict_stream(self, stream_obj: Any) -> AsyncIterator[dict[str, Any]]:
        try:
            async for item in stream_obj:
                payload = self._to_data(item)
                if isinstance(payload, dict):
                    yield payload
        finally:
            await self._close_stream_object(stream_obj)

    async def _iter_responses_stream(self, stream_obj: Any) -> AsyncIterator[ResponsesAPIStreamEvent]:
        try:
            async for event in stream_obj:
                payload = self._to_data(event)
                if isinstance(payload, dict):
                    yield self._parse_responses_api_stream_event(payload)
        finally:
            await self._close_stream_object(stream_obj)

    async def chat_completion(
        self,
        messages: list[Message],
        model: str,
        temperature: float | None = None,
        max_tokens: int | None = None,
        stream: bool | None = False,
        prompt_cache_key: str | None = None,
        prompt_cache_retention: str | None = None,
        **kwargs: Any,
    ) -> ChatCompletionResponse | AsyncIterator[ChatCompletionChunk]:
        request_data: dict[str, Any] = {
            "messages": [msg.to_dict() for msg in messages],
            "model": model,
            "stream": bool(stream),
        }
        self._merge_non_none(
            request_data,
            {
                "temperature": temperature,
                "max_tokens": max_tokens,
                "prompt_cache_key": prompt_cache_key,
                "prompt_cache_retention": prompt_cache_retention,
                **kwargs,
            },
        )

        if self.config.log_prompts:
            sanitized = self._sanitize_for_logging(request_data)
            logger.debug("Chat completion request: %s", json.dumps(sanitized, ensure_ascii=False))

        start_time = time.time()
        result = await self._sdk_client.chat.completions.create(**request_data)

        if stream:
            return self._iter_chat_completion_stream(result)

        payload = self._to_data(result)
        if not isinstance(payload, dict):
            raise TypeError("Unexpected SDK payload type for chat completion")

        response = self._parse_chat_completion_response(payload, model)
        latency_ms = int((time.time() - start_time) * 1000)
        usage = response.usage or {}
        logger.info(
            "Chat completion successful: model=%s, prompt_tokens=%s, completion_tokens=%s, total_tokens=%s, latency_ms=%s",
            response.model,
            usage.get("prompt_tokens", 0),
            usage.get("completion_tokens", 0),
            usage.get("total_tokens", 0),
            latency_ms,
        )
        return response

    async def completion(
        self,
        prompt: str,
        model: str,
        temperature: float | None = None,
        max_tokens: int | None = None,
        stream: bool | None = False,
        **kwargs: Any,
    ) -> dict[str, Any] | AsyncIterator[dict[str, Any]]:
        request_data: dict[str, Any] = {
            "prompt": prompt,
            "model": model,
            "stream": bool(stream),
        }
        self._merge_non_none(
            request_data,
            {
                "temperature": temperature,
                "max_tokens": max_tokens,
                **kwargs,
            },
        )

        result = await self._sdk_client.completions.create(**request_data)
        if stream:
            return self._iter_dict_stream(result)

        payload = self._to_data(result)
        return payload if isinstance(payload, dict) else {"data": payload}

    async def embeddings(
        self,
        input: str | list[str],
        model: str,
        **kwargs: Any,
    ) -> dict[str, Any]:
        request_data: dict[str, Any] = {"input": input, "model": model}
        self._merge_non_none(request_data, kwargs)

        result = await self._sdk_client.embeddings.create(**request_data)
        payload = self._to_data(result)
        return payload if isinstance(payload, dict) else {"data": payload}

    async def audio_transcriptions(
        self,
        file: str | bytes,
        model: str,
        language: str | None = None,
        prompt: str | None = None,
        response_format: str | None = "json",
        temperature: float | None = None,
        **kwargs: Any,
    ) -> dict[str, Any]:
        request_data: dict[str, Any] = {"model": model}
        self._merge_non_none(
            request_data,
            {
                "language": language,
                "prompt": prompt,
                "response_format": response_format,
                "temperature": temperature,
                **kwargs,
            },
        )

        if isinstance(file, str):
            path = Path(file)
            with path.open("rb") as file_handle:
                result = await self._sdk_client.audio.transcriptions.create(
                    file=file_handle,
                    **request_data,
                )
        else:
            result = await self._sdk_client.audio.transcriptions.create(
                file=("audio.wav", file),
                **request_data,
            )

        if isinstance(result, str):
            return {"text": result}

        payload = self._to_data(result)
        if isinstance(payload, dict):
            return payload
        return {"data": payload}

    async def audio_speech(
        self,
        input: str,
        model: str,
        voice: str,
        response_format: str | None = "mp3",
        speed: float | None = 1.0,
        **kwargs: Any,
    ) -> bytes:
        request_data: dict[str, Any] = {
            "model": model,
            "input": input,
            "voice": voice,
        }
        self._merge_non_none(
            request_data,
            {
                "response_format": response_format,
                "speed": speed,
                **kwargs,
            },
        )

        result = await self._sdk_client.audio.speech.create(**request_data)
        return await self._binary_to_bytes(result)

    async def _binary_to_bytes(self, payload: Any) -> bytes:
        if payload is None:
            return b""

        if isinstance(payload, (bytes, bytearray)):
            return bytes(payload)

        content = getattr(payload, "content", None)
        if isinstance(content, (bytes, bytearray)):
            return bytes(content)

        aread = getattr(payload, "aread", None)
        if callable(aread):
            data = aread()
            if inspect.isawaitable(data):
                return bytes(await data)
            if isinstance(data, (bytes, bytearray)):
                return bytes(data)

        read = getattr(payload, "read", None)
        if callable(read):
            data = read()
            if inspect.isawaitable(data):
                return bytes(await data)
            if isinstance(data, (bytes, bytearray)):
                return bytes(data)

        return bytes(str(payload), encoding="utf-8")

    async def health_check(self) -> bool:
        try:
            await self.list_models()
            return True
        except LLM_EXCEPTIONS as exc:
            logger.warning("OpenAI health check failed: %s", exc)
            return False

    async def list_models(self) -> list[dict[str, Any]]:
        models: list[dict[str, Any]] = []
        paginator = self._sdk_client.models.list()
        if inspect.isawaitable(paginator):
            paginator = await paginator

        async for item in paginator:
            payload = self._to_data(item)
            if isinstance(payload, dict):
                models.append(payload)
        return models

    async def responses_create(
        self,
        model: str,
        input: str | list[dict[str, Any]],
        *,
        instructions: str | None = None,
        tools: list[dict[str, Any]] | None = None,
        tool_choice: str | dict[str, Any] | None = None,
        max_output_tokens: int | None = None,
        reasoning: dict[str, Any] | None = None,
        previous_response_id: str | None = None,
        store: bool | None = None,
        truncation: str | None = None,
        stream: bool | None = False,
        include: list[str] | None = None,
        prompt_cache_key: str | None = None,
        prompt_cache_retention: str | None = None,
        **kwargs: Any,
    ) -> ResponsesAPIResponse | AsyncIterator[ResponsesAPIStreamEvent]:
        request_data: dict[str, Any] = {
            "model": model,
            "input": input,
            "stream": bool(stream),
        }
        self._merge_non_none(
            request_data,
            {
                "instructions": instructions,
                "tools": tools,
                "tool_choice": tool_choice,
                "max_output_tokens": max_output_tokens,
                "reasoning": reasoning,
                "previous_response_id": previous_response_id,
                "store": store,
                "truncation": truncation,
                "include": include,
                "prompt_cache_key": prompt_cache_key,
                "prompt_cache_retention": prompt_cache_retention,
                **kwargs,
            },
        )

        if self.config.log_prompts:
            sanitized = self._sanitize_for_logging(request_data)
            logger.debug("Responses API request: %s", json.dumps(sanitized, ensure_ascii=False))

        start_time = time.time()
        result = await self._sdk_client.responses.create(**request_data)

        if stream:
            return self._iter_responses_stream(result)

        payload = self._to_data(result)
        if not isinstance(payload, dict):
            raise TypeError("Unexpected SDK payload type for responses API")

        response = self._parse_responses_api_response(payload, model)
        latency_ms = int((time.time() - start_time) * 1000)
        usage = response.usage or {}
        logger.info(
            "Responses API successful: model=%s, status=%s, input_tokens=%s, output_tokens=%s, latency_ms=%s",
            response.model,
            response.status,
            usage.get("input_tokens", 0),
            usage.get("output_tokens", 0),
            latency_ms,
        )
        return response

    async def responses_retrieve(self, response_id: str) -> ResponsesAPIResponse:
        result = await self._sdk_client.responses.retrieve(response_id=response_id)
        payload = self._to_data(result)
        if not isinstance(payload, dict):
            raise TypeError("Unexpected SDK payload type for responses retrieve")
        requested_model = str(payload.get("model") or self.config.default_model)
        return self._parse_responses_api_response(payload, requested_model)

    async def responses_cancel(self, response_id: str) -> ResponsesAPIResponse:
        result = await self._sdk_client.responses.cancel(response_id=response_id)
        payload = self._to_data(result)
        if not isinstance(payload, dict):
            raise TypeError("Unexpected SDK payload type for responses cancel")
        requested_model = str(payload.get("model") or self.config.default_model)
        return self._parse_responses_api_response(payload, requested_model)

    async def responses_delete(self, response_id: str) -> bool:
        await self._sdk_client.responses.delete(response_id=response_id)
        return True

    async def responses_input_items(self, response_id: str) -> list[dict[str, Any]]:
        items: list[dict[str, Any]] = []
        paginator = self._sdk_client.responses.input_items.list(response_id=response_id)
        if inspect.isawaitable(paginator):
            paginator = await paginator

        async for item in paginator:
            payload = self._to_data(item)
            if isinstance(payload, dict):
                items.append(payload)
        return items

    async def close(self):
        if self._closed:
            return
        self._closed = True
        await self._sdk_client.close()

    def _sanitize_for_logging(self, data: dict[str, Any] | list[Any] | str) -> dict[str, Any] | list[Any] | str:
        if isinstance(data, dict):
            new_data: dict[str, Any] = {}
            for key, value in data.items():
                if key == "url" and isinstance(value, str) and value.startswith("data:image"):
                    new_data[key] = "[BASE64_IMAGE_DATA_TRUNCATED]"
                elif key == "image_url" and isinstance(value, dict):
                    new_data[key] = self._sanitize_for_logging(value)
                else:
                    new_data[key] = self._sanitize_for_logging(value)
            return new_data
        if isinstance(data, list):
            return [self._sanitize_for_logging(item) for item in data]
        return data

    async def __aenter__(self):
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb):
        await self.close()


class SyncOpenAIClient:
    """Minimal sync wrapper around ``OpenAIClient``."""

    _shared_loop: asyncio.AbstractEventLoop | None = None

    def __init__(self, config: OpenAIClientConfig | None = None):
        self._async_client = OpenAIClient(config=config)
        self._loop: asyncio.AbstractEventLoop | None = None

    @classmethod
    def _get_loop(cls) -> asyncio.AbstractEventLoop:
        """Get or create a shared event loop."""
        if cls._shared_loop is None or cls._shared_loop.is_closed():
            cls._shared_loop = asyncio.new_event_loop()
        return cls._shared_loop

    def __enter__(self):
        self._loop = self._get_loop()
        self._loop.run_until_complete(self._async_client.__aenter__())
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        if self._loop is None:
            return
        # Don't close the loop - keep it alive for reuse
        self._loop.run_until_complete(self._async_client.__aexit__(exc_type, exc_val, exc_tb))

    def _run(self, coro: Any) -> Any:
        if self._loop is None:
            raise RuntimeError("SyncOpenAIClient must be used inside a context manager")
        return self._loop.run_until_complete(coro)

    def chat_completion_sync(self, *args: Any, **kwargs: Any):
        """Synchronous helper for ``chat_completion``."""

        return self._run(self._async_client.chat_completion(*args, **kwargs))

    def close(self) -> None:
        if self._loop is None:
            return
        self._run(self._async_client.close())

    @classmethod
    def cleanup_loop(cls) -> None:
        """Clean up the shared event loop. Call at process exit."""
        if cls._shared_loop is not None and not cls._shared_loop.is_closed():
            cls._shared_loop.close()
            cls._shared_loop = None
