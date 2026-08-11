"""LLM adapters for the AI runtime sidecar (sync + streaming).

Used by runtime_service.py only — routes must not import provider SDKs directly.
"""

from __future__ import annotations

import asyncio
import os
import re
import time
from collections.abc import Iterator
from dataclasses import dataclass
from typing import TYPE_CHECKING, Protocol, runtime_checkable

from src.infrastructure.ai.security.url_guard import validate_external_http_url
from src.infrastructure.logging.core import get_logger

from .circuit_breaker import CircuitBreaker, CircuitState

if TYPE_CHECKING:
    from openai import AsyncOpenAI

_SENSITIVE_PATTERNS = (
    (re.compile(r"sk-[a-zA-Z0-9_-]{8,}", re.IGNORECASE), "sk-[REDACTED]"),
    (re.compile(r"Bearer\s+\S+", re.IGNORECASE), "Bearer [REDACTED]"),
    (re.compile(r"OPENAI_API_KEY[=:\s]+\S+", re.IGNORECASE), "OPENAI_API_KEY=[REDACTED]"),
    (re.compile(r"api[_-]?key[=:\s]+\S+", re.IGNORECASE), "api_key=[REDACTED]"),
)

logger = get_logger(__name__)


class LlmUnavailableError(Exception):
    """Raised when LLM is configured but cannot complete a request."""


class LlmStreamError(LlmUnavailableError):
    """Raised when a streaming LLM request fails (including mid-stream)."""


@dataclass(frozen=True)
class LlmCompletion:
    content: str
    model: str
    prompt_tokens: int = 0
    completion_tokens: int = 0


@runtime_checkable
class LlmClient(Protocol):
    def is_configured(self) -> bool: ...

    def complete(self, system_prompt: str, user_message: str) -> LlmCompletion: ...


@runtime_checkable
class StreamingLlmClient(Protocol):
    def is_configured(self) -> bool: ...

    def stream_complete(self, system_prompt: str, user_message: str) -> Iterator[str]: ...


def sanitize_provider_error(exc: BaseException, *, max_len: int = 200) -> str:
    """Return a short error summary safe for SSE / logs (no keys or full prompts)."""
    msg = str(exc).strip() or exc.__class__.__name__
    for pattern, replacement in _SENSITIVE_PATTERNS:
        msg = pattern.sub(replacement, msg)
    if len(msg) > max_len:
        msg = msg[: max_len - 3] + "..."
    return msg


class FakeLlmClient:
    """Deterministic LLM for unit tests — no network, no API keys."""

    def __init__(
        self,
        response: str = "Mock LLM answer for the flight operations request.",
        *,
        configured: bool = True,
        raise_on_complete: Exception | None = None,
        model: str = "fake-llm-v1",
    ) -> None:
        self._response = response
        self._configured = configured
        self._raise_on_complete = raise_on_complete
        self._model = model

    def is_configured(self) -> bool:
        return self._configured

    def complete(self, system_prompt: str, user_message: str) -> LlmCompletion:
        if self._raise_on_complete is not None:
            raise self._raise_on_complete
        return LlmCompletion(
            content=self._response,
            model=self._model,
            prompt_tokens=max(1, len(system_prompt) // 4),
            completion_tokens=max(1, len(self._response) // 4),
        )


class FakeStreamingLlmClient:
    """Yields predetermined token deltas; optional mid-stream failure for tests."""

    def __init__(
        self,
        tokens: list[str] | None = None,
        *,
        configured: bool = True,
        model: str = "fake-llm-v1",
        raise_after_tokens: int | None = None,
        raise_on_start: Exception | None = None,
        error_message: str = "simulated mid-stream provider failure",
    ) -> None:
        self._tokens = tokens if tokens is not None else ["Mock ", "LLM ", "stream ", "answer."]
        self._configured = configured
        self._model = model
        self._raise_after_tokens = raise_after_tokens
        self._raise_on_start = raise_on_start
        self._error_message = error_message

    @property
    def model(self) -> str:
        return self._model

    def is_configured(self) -> bool:
        return self._configured

    def stream_complete(self, system_prompt: str, user_message: str) -> Iterator[str]:
        if self._raise_on_start is not None:
            raise self._raise_on_start
        for emitted, token in enumerate(self._tokens):
            if self._raise_after_tokens is not None and emitted >= self._raise_after_tokens:
                raise LlmStreamError(self._error_message)
            yield token


class OpenAiLlmClient:
    """Lazy OpenAI adapter — only loads openai_client when an API key is present.

    Credentials default to the OPENAI_* env vars (byte-for-byte the legacy
    behavior for single-provider deployments). When the caller passes explicit
    ``api_key`` / ``base_url`` / ``model`` (derived from the resolved provider),
    those take precedence so per-entity provider credentials are honored.
    """

    def __init__(
        self,
        api_key: str | None = None,
        base_url: str | None = None,
        model: str | None = None,
    ) -> None:
        self._api_key = (api_key or "").strip() or os.getenv("OPENAI_API_KEY", "").strip()
        self._base_url = (base_url or "").strip() or os.getenv("OPENAI_BASE_URL", "").strip()
        if self._base_url:
            self._base_url = validate_external_http_url(self._base_url, purpose="OpenAI base_url")
        self._model = (model or "").strip() or os.getenv("OPENAI_MODEL", "gpt-4o-mini").strip() or "gpt-4o-mini"
        self._circuit_breaker = CircuitBreaker()

    def is_configured(self) -> bool:
        return bool(self._api_key)

    def complete(self, system_prompt: str, user_message: str) -> LlmCompletion:
        if not self.is_configured():
            raise LlmUnavailableError("OPENAI_API_KEY is not configured")

        try:
            from openai import AsyncOpenAI
        except ImportError as exc:
            raise LlmUnavailableError(f"OpenAI SDK unavailable: {exc}") from exc

        messages = [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_message},
        ]

        async def _complete() -> object:
            kwargs = {"api_key": self._api_key, "timeout": 120}
            if self._base_url:
                kwargs["base_url"] = self._base_url
            client = AsyncOpenAI(**kwargs)
            return await client.chat.completions.create(
                model=self._model,
                messages=messages,
                temperature=0.2,
            )

        try:
            response = asyncio.run(self._circuit_breaker.execute(_complete))
        except Exception as exc:
            logger.warning("llm_completion_failed", exc_info=exc, extra={"error": str(exc)})
            raise LlmUnavailableError(sanitize_provider_error(exc)) from exc

        content = _extract_chat_content(response)
        if not content.strip():
            raise LlmUnavailableError("LLM returned an empty response")

        usage = getattr(response, "usage", None)
        if isinstance(usage, dict):
            prompt_tokens = int(usage.get("prompt_tokens", 0) or 0)
            completion_tokens = int(usage.get("completion_tokens", 0) or 0)
        elif usage is not None:
            prompt_tokens = int(getattr(usage, "prompt_tokens", 0) or 0)
            completion_tokens = int(getattr(usage, "completion_tokens", 0) or 0)
        else:
            prompt_tokens = 0
            completion_tokens = 0

        return LlmCompletion(
            content=content.strip(),
            model=self._model,
            prompt_tokens=prompt_tokens,
            completion_tokens=completion_tokens,
        )


class OpenAiStreamingLlmClient:
    """OpenAI Chat Completions streaming via AsyncOpenAI SDK (real token deltas).

    Streams natively via ``async for`` internally and bridges to a sync
    ``Iterator[str]`` via an event-loop thread — no ``asyncio.to_thread()``
    wrapping needed when callers eventually migrate to ``AsyncIterator``.
    """

    def __init__(
        self,
        api_key: str | None = None,
        base_url: str | None = None,
        model: str | None = None,
    ) -> None:
        self._api_key = (api_key or "").strip() or os.getenv("OPENAI_API_KEY", "").strip()
        self._base_url = (base_url or "").strip() or os.getenv("OPENAI_BASE_URL", "").strip()
        if self._base_url:
            self._base_url = validate_external_http_url(self._base_url, purpose="OpenAI base_url")
        self._model = (model or "").strip() or os.getenv("OPENAI_MODEL", "gpt-4o-mini").strip() or "gpt-4o-mini"
        self._last_usage: dict = {}
        self._client: AsyncOpenAI | None = None
        self._circuit_breaker = CircuitBreaker()

    @property
    def model(self) -> str:
        return self._model

    @property
    def last_usage(self) -> dict:
        return dict(self._last_usage)

    def is_configured(self) -> bool:
        return bool(self._api_key)

    def _get_client(self) -> AsyncOpenAI:
        if self._client is not None:
            return self._client
        try:
            from openai import AsyncOpenAI
        except ImportError as exc:
            raise LlmUnavailableError(f"OpenAI SDK unavailable: {exc}") from exc
        kwargs = {"api_key": self._api_key, "timeout": 120}
        if self._base_url:
            kwargs["base_url"] = self._base_url
        self._client = AsyncOpenAI(**kwargs)
        return self._client

    def stream_complete(self, system_prompt: str, user_message: str) -> Iterator[str]:
        if not self.is_configured():
            raise LlmUnavailableError("OPENAI_API_KEY is not configured")

        if self._circuit_breaker.state == CircuitState.OPEN:
            if time.time() - self._circuit_breaker.last_failure_time > self._circuit_breaker.recovery_timeout:
                self._circuit_breaker.state = CircuitState.HALF_OPEN
            else:
                raise LlmStreamError("Circuit breaker is open")

        client = self._get_client()
        self._last_usage = {}

        import queue as _queue
        from threading import Thread

        q: _queue.Queue = _queue.Queue(maxsize=32)
        _exc_info: list[BaseException] = []

        async def _producer() -> None:
            """Run the async stream and push chunks into the sync queue."""
            try:
                stream = await client.chat.completions.create(
                    model=self._model,
                    messages=[
                        {"role": "system", "content": system_prompt},
                        {"role": "user", "content": user_message},
                    ],
                    temperature=0.2,
                    stream=True,
                    stream_options={"include_usage": True},
                )
                async for chunk in stream:
                    q.put(chunk)
            except BaseException as exc:  # noqa: BLE001 - thread producer must capture all exceptions including CancelledError for propagation
                _exc_info.append(exc)
            finally:
                q.put(None)  # sentinel

        Thread(target=lambda: asyncio.run(_producer()), daemon=True).start()

        try:
            while True:
                chunk = q.get()
                if chunk is None:
                    break
                if _exc_info:
                    raise _exc_info[0]
                usage = getattr(chunk, "usage", None)
                if usage is not None:
                    self._last_usage = {
                        "prompt_tokens": int(getattr(usage, "prompt_tokens", 0) or 0),
                        "completion_tokens": int(getattr(usage, "completion_tokens", 0) or 0),
                        "total_tokens": int(getattr(usage, "total_tokens", 0) or 0),
                    }
                if not chunk.choices:
                    continue
                delta = chunk.choices[0].delta
                content = getattr(delta, "content", None) if delta else None
                if content:
                    yield content
        except LlmStreamError:
            self._circuit_breaker.failure_count += 1
            self._circuit_breaker.last_failure_time = time.time()
            if self._circuit_breaker.failure_count >= self._circuit_breaker.failure_threshold:
                self._circuit_breaker.state = CircuitState.OPEN
            raise
        except BaseException as exc:
            self._circuit_breaker.failure_count += 1
            self._circuit_breaker.last_failure_time = time.time()
            if self._circuit_breaker.failure_count >= self._circuit_breaker.failure_threshold:
                self._circuit_breaker.state = CircuitState.OPEN
            if _exc_info:
                raise LlmStreamError(sanitize_provider_error(_exc_info[0])) from _exc_info[0]
            raise LlmStreamError(sanitize_provider_error(exc)) from exc


def _extract_chat_content(response: object) -> str:
    if hasattr(response, "content") and response.content:
        return str(response.content)
    if hasattr(response, "choices") and response.choices:
        choice = response.choices[0]
        message = getattr(choice, "message", None) or choice.get("message", {})
        return getattr(message, "content", None) or message.get("content", "") or ""
    return ""
