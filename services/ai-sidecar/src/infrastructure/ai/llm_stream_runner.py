"""Unified streaming orchestration layer for all LLM requests.

Every production-path LLM call flows through :class:`LLMStreamRunner`, which:

1. Always issues ``stream=True`` requests to the underlying gateway.
2. Aggregates text deltas, tool-call deltas, and usage into a single
   :class:`StreamCompletionResult`.
3. Injects Prompt Cache and Responses-API session-chain parameters.
4. Yields raw events to callers who need incremental delivery.

Business services should **never** call ``chat_completion`` or
``responses_create`` directly — they consume either the aggregated
result or the event iterator from this module.
"""

from __future__ import annotations

import json
import time
from collections.abc import AsyncIterator, Awaitable, Callable
from dataclasses import dataclass, field
from typing import Any

from src.infrastructure.ai.openai_client import (
    AiGateway,
    ChatCompletionChunk,
    ChatCompletionResponse,
    Message,
    ResponsesAPIStreamEvent,
)
from src.infrastructure.ai.prompt_cache import (
    parse_cached_tokens,
)
from src.infrastructure.ai.responses_session_state import (
    ResponsesSessionStateManager,
)
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


# ---------------------------------------------------------------------------
# Result type
# ---------------------------------------------------------------------------


@dataclass
class StreamCompletionResult:
    """Normalised result produced after fully consuming an LLM stream.

    Both Chat Completions and Responses API streams are collapsed into
    this single type so business code never has to branch on API format.
    """

    text: str = ""
    tool_calls: list[dict[str, Any]] = field(default_factory=list)
    usage: dict[str, Any] = field(default_factory=dict)
    model: str = ""
    raw_response: Any | None = None
    response_id: str | None = None
    cached_tokens: int = 0
    latency_ms: int = 0

    # Responses-API specific
    output_items: list[dict[str, Any]] = field(default_factory=list)

    @property
    def has_tool_calls(self) -> bool:
        return bool(self.tool_calls)


# ---------------------------------------------------------------------------
# Stream event wrapper (for callers that iterate)
# ---------------------------------------------------------------------------


@dataclass
class StreamEvent:
    """Thin wrapper yielded to callers during streaming."""

    type: str  # "text_delta" | "tool_call_delta" | "completed" | "error"
    text_delta: str | None = None
    tool_call: dict[str, Any] | None = None
    result: StreamCompletionResult | None = None
    raw: Any | None = None
    round_index: int = 0


# ---------------------------------------------------------------------------
# LLMStreamRunner
# ---------------------------------------------------------------------------


class LLMStreamRunner:
    """Unified streaming orchestrator.

    Usage::

        runner = LLMStreamRunner(client)
        result = await runner.run_chat(messages=..., model=..., tools=...)
        # or iterate:
        async for event in runner.stream_chat(messages=..., model=...):
            ...
        # with tool execution:
        async for event in runner.stream_chat_with_tools(messages=..., tools=..., run_id=...):
            ...
    """

    def __init__(
        self,
        client: AiGateway,
        *,
        session_manager: ResponsesSessionStateManager | None = None,
        tool_executor=None,
    ):
        self._client = client
        self._session_manager = session_manager
        self._tool_executor = tool_executor

    # ---- Chat Completions: aggregated result -----------------------------

    async def run_chat(
        self,
        *,
        messages: list[Message],
        model: str,
        temperature: float | None = None,
        max_tokens: int | None = None,
        tools: list[dict[str, Any]] | None = None,
        prompt_cache_key: str | None = None,
        prompt_cache_retention: str | None = None,
        **kwargs: Any,
    ) -> StreamCompletionResult:
        """Issue a streaming Chat Completion and return the aggregated result."""
        result = StreamCompletionResult(model=model)
        async for _event in self._stream_chat_impl(
            messages=messages,
            model=model,
            temperature=temperature,
            max_tokens=max_tokens,
            tools=tools,
            prompt_cache_key=prompt_cache_key,
            prompt_cache_retention=prompt_cache_retention,
            result=result,
            **kwargs,
        ):
            pass  # consume to completion
        return result

    # ---- Chat Completions: event iterator --------------------------------

    async def stream_chat(
        self,
        *,
        messages: list[Message],
        model: str,
        temperature: float | None = None,
        max_tokens: int | None = None,
        tools: list[dict[str, Any]] | None = None,
        prompt_cache_key: str | None = None,
        prompt_cache_retention: str | None = None,
        **kwargs: Any,
    ) -> AsyncIterator[StreamEvent]:
        """Yield incremental :class:`StreamEvent` objects from a Chat Completion."""
        result = StreamCompletionResult(model=model)
        async for event in self._stream_chat_impl(
            messages=messages,
            model=model,
            temperature=temperature,
            max_tokens=max_tokens,
            tools=tools,
            prompt_cache_key=prompt_cache_key,
            prompt_cache_retention=prompt_cache_retention,
            result=result,
            **kwargs,
        ):
            yield event

    # ---- Responses API: aggregated result --------------------------------

    async def run_responses(
        self,
        *,
        model: str,
        instructions: str | None = None,
        input: Any = None,
        tools: list[dict[str, Any]] | None = None,
        conversation_id: str | None = None,
        prompt_cache_key: str | None = None,
        prompt_cache_retention: str | None = None,
        **kwargs: Any,
    ) -> StreamCompletionResult:
        """Issue a streaming Responses API call and return the aggregated result."""
        result = StreamCompletionResult(model=model)
        async for _event in self._stream_responses_impl(
            model=model,
            instructions=instructions,
            input=input,
            tools=tools,
            conversation_id=conversation_id,
            prompt_cache_key=prompt_cache_key,
            prompt_cache_retention=prompt_cache_retention,
            result=result,
            **kwargs,
        ):
            pass
        return result

    # ---- Responses API: event iterator -----------------------------------

    async def stream_responses(
        self,
        *,
        model: str,
        instructions: str | None = None,
        input: Any = None,
        tools: list[dict[str, Any]] | None = None,
        conversation_id: str | None = None,
        prompt_cache_key: str | None = None,
        prompt_cache_retention: str | None = None,
        **kwargs: Any,
    ) -> AsyncIterator[StreamEvent]:
        """Yield incremental events from a Responses API stream."""
        result = StreamCompletionResult(model=model)
        async for event in self._stream_responses_impl(
            model=model,
            instructions=instructions,
            input=input,
            tools=tools,
            conversation_id=conversation_id,
            prompt_cache_key=prompt_cache_key,
            prompt_cache_retention=prompt_cache_retention,
            result=result,
            **kwargs,
        ):
            yield event

    # =====================================================================
    # Tool-enabled streaming
    # =====================================================================

    async def stream_chat_with_tools(
        self,
        *,
        messages: list[Message],
        model: str,
        tools: list[dict[str, Any]] | None = None,
        temperature: float | None = None,
        max_tokens: int | None = None,
        run_id: str = "",
        envelope=None,
        prompt_cache_key: str | None = None,
        prompt_cache_retention: str | None = None,
        tool_cache_policy=None,
        on_child_event: Callable[[StreamEvent], Awaitable[None]] | None = None,
        entity_id: str | None = None,
        max_tool_rounds: int | None = None,
        consecutive_failure_threshold: int = 3,
        **kwargs: Any,
    ) -> AsyncIterator[StreamEvent]:
        """Stream chat with full tool execution loop.

        Handles: LLM stream → detect tool calls → execute → feed results back → LLM continues.
        Yields text_delta, tool_call_delta, tool_call, tool_result, completed events.
        Stop conditions: no tool_calls, budget exhausted (emit StreamEvent(type="budget_exhausted")),
        consecutive tool failures >= threshold, user cancellation.

        ``max_tool_rounds`` overrides the class default. If None, callers should inject a per-run budget via
        wrapper logic (runtime_graph/_streaming_tools sets it from capability snapshot's tool_policy).

        ``consecutive_failure_threshold`` defaults to 3: stop if that many tool rounds in a row return
        failures/errors without progress.
        """
        from src.infrastructure.ai.tools.tool_executor import ToolExecutor

        executor = self._tool_executor or ToolExecutor()

        # Extract the set of tool names that were presented to the LLM.
        allowed_tool_names: set[str] | None = None
        if tools:
            allowed_tool_names = set()
            for t in tools:
                fn = t.get("function", {}) if isinstance(t, dict) else {}
                name = fn.get("name") if isinstance(fn, dict) else None
                if name:
                    allowed_tool_names.add(name)

        current_messages = list(messages)
        last_round_index = 0
        consecutive_failures = 0
        effective_max_rounds = max_tool_rounds if max_tool_rounds is not None else 999

        for round_index in range(effective_max_rounds):
            last_round_index = round_index
            result = StreamCompletionResult(model=model)

            async for event in self._stream_chat_impl(
                messages=current_messages,
                model=model,
                temperature=temperature,
                max_tokens=max_tokens,
                tools=tools,
                prompt_cache_key=prompt_cache_key,
                prompt_cache_retention=prompt_cache_retention,
                result=result,
                round_index=round_index,
                **kwargs,
            ):
                yield event

            if not result.tool_calls:
                break

            parsed_calls = []
            for tc in result.tool_calls:
                fn = tc.get("function")
                raw_args = fn.get("arguments", "{}") if fn else "{}"
                parsed_calls.append(
                    {
                        "tool_call_id": tc.get("id", ""),
                        "tool_name": fn.get("name", "") if fn else "",
                        "arguments": raw_args,
                    }
                )

            for pc in parsed_calls:
                # D1: Emit checkpoint before tool execution (via MQ)
                checkpoint_event = {
                    "type": "checkpoint",
                    "checkpoint_type": "before_tool",
                    "run_id": run_id,
                    "round_index": round_index,
                    "tool_calls": parsed_calls,
                    "timestamp": time.time(),
                }
                if on_child_event:
                    await on_child_event(checkpoint_event)
                
                yield StreamEvent(
                    type="tool_call",
                    tool_call=pc,
                )

            execution_results = await executor.execute_batch(
                parsed_calls,
                run_id=run_id,
                envelope=envelope,
                cache_policy=tool_cache_policy,
                on_child_event=on_child_event,
                entity_id=entity_id,
                allowed_tool_names=allowed_tool_names,
                round_index=round_index,
                job_id=run_id,
            )

            # D1: Emit checkpoint after tool execution (via MQ)
            checkpoint_after_tool = {
                "type": "checkpoint",
                "checkpoint_type": "after_tool",
                "run_id": run_id,
                "round_index": round_index,
                "tool_calls_executed": len(parsed_calls),
                "results": [er.to_sse_payload() for er in execution_results],
                "timestamp": time.time(),
            }
            if on_child_event:
                await on_child_event(checkpoint_after_tool)

            # Track success/failure based on execution results
            any_success = any(er.error is None and er.result is not None for er in execution_results)
            if not any_success:
                consecutive_failures += 1
                if consecutive_failures >= consecutive_failure_threshold:
                    logger.warning(
                        f"Consecutive tool failures ({consecutive_failures}) exceeded threshold, stopping."
                    )
                    yield StreamEvent(type="budget_exhausted", round_index=last_round_index)
                    break

            tool_results_for_llm: list[dict[str, Any]] = []
            for exec_result in execution_results:
                payload = exec_result.to_sse_payload()
                payload["run_id"] = run_id
                yield StreamEvent(
                    type="tool_result",
                    tool_call=payload,
                )
                content = (
                    json.dumps(exec_result.result) if exec_result.result is not None else f"Error: {exec_result.error}"
                )
                tool_results_for_llm.append(
                    {
                        "role": "tool",
                        "tool_call_id": exec_result.tool_call_id,
                        "content": content,
                    }
                )

            # D1: Emit checkpoint before proposal if applicable
            # (proposal generation happens after this loop iteration when LLM requests)
            checkpoint_before_proposal = {
                "type": "checkpoint",
                "checkpoint_type": "before_proposal",
                "run_id": run_id,
                "round_index": round_index,
                "context_snapshot": {
                    "messages_count": len(current_messages),
                    "has_tool_calls": bool(result.tool_calls),
                    "timestamp": time.time(),
                },
                # Include pending tool results for resume
                "pending_results": [er.to_sse_payload() for er in execution_results] if execution_results else [],
            }
            if on_child_event:
                await on_child_event(checkpoint_before_proposal)

            tool_calls_list = []
            for tc in result.tool_calls:
                fn = tc.get("function")
                tool_calls_list.append(
                    {
                        "id": tc.get("id", ""),
                        "type": "function",
                        "function": {
                            "name": fn.get("name", "") if fn else "",
                            "arguments": fn.get("arguments", "") if fn else "",
                        },
                    }
                )
            assistant_message: dict[str, Any] = {
                "role": "assistant",
                "content": result.text,
                "tool_calls": tool_calls_list,
            }
            current_messages.append(assistant_message)
            current_messages.extend(tool_results_for_llm)

        # D1: Emit final checkpoint after completion
        final_checkpoint = {
            "type": "checkpoint",
            "checkpoint_type": "after_completion",
            "run_id": run_id,
            "round_index": last_round_index,
            "final_result": {"text": result.text, "tool_calls_count": len(result.tool_calls)} if result else {},
            "messages_count": len(current_messages),
            "timestamp": time.time(),
        }
        if on_child_event:
            await on_child_event(final_checkpoint)

        yield StreamEvent(type="completed", result=result, round_index=last_round_index)

    # =====================================================================
    # Internals — Chat Completions
    # =====================================================================

    async def _stream_chat_impl(
        self,
        *,
        messages: list[Message],
        model: str,
        temperature: float | None,
        max_tokens: int | None,
        tools: list[dict[str, Any]] | None,
        prompt_cache_key: str | None,
        prompt_cache_retention: str | None,
        result: StreamCompletionResult,
        round_index: int = 0,
        **kwargs: Any,
    ) -> AsyncIterator[StreamEvent]:
        start = time.monotonic()
        text_parts: list[str] = []
        # tool-call delta accumulators: index -> {id, type, function: {name, arguments}}
        tc_accum: dict[int, dict[str, Any]] = {}

        from src.infrastructure.ai.monitoring.prometheus_exporter import inc_llm_call

        inc_llm_call(model)

        stream = await self._client.chat_completion(
            messages=messages,
            model=model,
            temperature=temperature,
            max_tokens=max_tokens,
            stream=True,
            tools=tools if tools else None,
            prompt_cache_key=prompt_cache_key,
            prompt_cache_retention=prompt_cache_retention,
            **kwargs,
        )

        try:
            async for chunk in stream:
                if not isinstance(chunk, ChatCompletionChunk):
                    continue

                for choice in chunk.choices or []:
                    delta = choice.get("delta") or {}

                    # Text delta
                    content = delta.get("content")
                    if content:
                        text_parts.append(content)
                        yield StreamEvent(type="text_delta", text_delta=content)

                    # Tool-call deltas
                    for tc_delta in delta.get("tool_calls") or []:
                        idx = tc_delta.get("index", 0)
                        if idx not in tc_accum:
                            tc_accum[idx] = {
                                "id": tc_delta.get("id", ""),
                                "type": "function",
                                "function": {"name": "", "arguments": ""},
                            }
                        acc = tc_accum[idx]
                        if tc_delta.get("id"):
                            acc["id"] = tc_delta["id"]
                        fn = tc_delta.get("function") or {}
                        if fn.get("name"):
                            acc["function"]["name"] += fn["name"]
                        if fn.get("arguments"):
                            acc["function"]["arguments"] += fn["arguments"]
                        yield StreamEvent(type="tool_call_delta", tool_call=acc)

                    # Finish reason / usage (last chunk)
                    finish = choice.get("finish_reason")
                    if finish:
                        result.model = chunk.model or model

                chunk_usage = getattr(chunk, "usage", None) or {}
                if chunk_usage:
                    result.usage = chunk_usage
        except Exception as exc:
            logger.error("Chat completion stream error: %s", exc)
            yield StreamEvent(type="error", raw=exc)
            raise

        result.text = "".join(text_parts)
        result.tool_calls = [tc_accum[i] for i in sorted(tc_accum)]
        result.cached_tokens = parse_cached_tokens(result.usage)
        result.latency_ms = int((time.monotonic() - start) * 1000)

        # Synthesize raw_response for downstream compatibility
        message_payload: dict[str, Any] = {"role": "assistant", "content": result.text}
        if result.tool_calls:
            message_payload["tool_calls"] = result.tool_calls
        result.raw_response = ChatCompletionResponse(
            id="stream-agg",
            object="chat.completion",
            created=int(time.time()),
            model=result.model,
            choices=[{"index": 0, "message": message_payload, "finish_reason": "stop"}],
            usage=result.usage,
        )
        yield StreamEvent(type="completed", result=result, round_index=round_index)

    # =====================================================================
    # Internals — Responses API
    # =====================================================================

    async def _stream_responses_impl(
        self,
        *,
        model: str,
        instructions: str | None,
        input: Any,
        tools: list[dict[str, Any]] | None,
        conversation_id: str | None,
        prompt_cache_key: str | None,
        prompt_cache_retention: str | None,
        result: StreamCompletionResult,
        **kwargs: Any,
    ) -> AsyncIterator[StreamEvent]:
        start = time.monotonic()
        text_parts: list[str] = []

        from src.infrastructure.ai.monitoring.prometheus_exporter import inc_llm_call

        inc_llm_call(model)

        # Session chain injection
        session_kwargs: dict[str, Any] = {}
        if self._session_manager and conversation_id:
            state = self._session_manager.get_state(conversation_id)
            if state.previous_response_id:
                session_kwargs["previous_response_id"] = state.previous_response_id

        merged_kwargs = {**kwargs, **session_kwargs}

        stream = await self._client.responses_create(
            model=model,
            instructions=instructions,
            input=input,
            tools=tools,
            stream=True,
            prompt_cache_key=prompt_cache_key,
            prompt_cache_retention=prompt_cache_retention,
            **merged_kwargs,
        )

        completed_payload: dict[str, Any] | None = None
        try:
            async for event in stream:
                if not isinstance(event, ResponsesAPIStreamEvent):
                    continue

                event_type = (event.type or "").strip().lower()

                # Text deltas
                if event_type in {
                    "response.output_text.delta",
                    "response.content_part.delta",
                }:
                    delta_text = event.delta or event.text or ""
                    if delta_text:
                        text_parts.append(delta_text)
                        yield StreamEvent(type="text_delta", text_delta=delta_text)

                # Response completed — full payload
                elif event_type in {"response.completed", "response.done"}:
                    if event.response and isinstance(event.response, dict):
                        completed_payload = event.response

                # Function call items
                elif event_type == "response.output_item.done":
                    item = event.item or {}
                    if item.get("type") == "function_call":
                        result.tool_calls.append(item)
                        yield StreamEvent(type="tool_call_delta", tool_call=item)

                yield StreamEvent(type="raw_event", raw=event)

        except Exception as exc:
            logger.error("Responses stream error: %s", exc)
            yield StreamEvent(type="error", raw=exc)
            raise

        # Reconstruct from completed payload
        if completed_payload:
            result.response_id = completed_payload.get("id")
            result.model = completed_payload.get("model", model)
            result.usage = completed_payload.get("usage") or {}
            result.output_items = completed_payload.get("output") or []
            result.raw_response = completed_payload

            # Extract text from output if not collected via deltas
            if not text_parts:
                for item in result.output_items:
                    if item.get("type") != "message":
                        continue
                    for part in item.get("content") or []:
                        if part.get("type") == "output_text":
                            text_parts.append(part.get("text") or "")

            # Extract tool calls from output items if not already collected
            if not result.tool_calls:
                for item in result.output_items:
                    if item.get("type") == "function_call":
                        result.tool_calls.append(item)

        result.text = "".join(text_parts)
        result.cached_tokens = parse_cached_tokens(result.usage)
        result.latency_ms = int((time.monotonic() - start) * 1000)

        # Advance session chain on success
        if self._session_manager and conversation_id and result.response_id:
            # We don't have fingerprints here; callers are responsible
            # for calling advance_state if they want session chaining.
            pass

        yield StreamEvent(type="completed", result=result)
