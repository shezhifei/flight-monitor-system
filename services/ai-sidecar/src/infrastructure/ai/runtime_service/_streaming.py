"""Mixin for RuntimeService — stream_run, _stream_with_provider, _stream_with_graph, _stream_graph_with_provider, _stream_graph_heuristic, _stream_heuristic."""

from __future__ import annotations

import logging
import time
import uuid
from collections.abc import AsyncIterator, Iterator
from typing import Any

from src.infrastructure.ai.context_envelope import ContextEnvelope
from src.infrastructure.ai.intent_router import classify_intent
from src.infrastructure.ai.runtime_graph import (
    StreamingGraphContext,
    graph_result_to_output,
)
from src.infrastructure.ai.runtime_llm import (
    LlmStreamError,
    LlmUnavailableError,
    OpenAiStreamingLlmClient,
    StreamingLlmClient,
    sanitize_provider_error,
)
from src.infrastructure.ai.structured_output import (
    ReasoningStep,
    TokenUsage,
)

from ._streaming_tools import (
    _publish_run_fail_mq,
    _resolve_mq_publisher,
)
from .helpers import (
    _iter_answer_chunks,
    _sse_event,
    build_system_prompt,
    heuristic_answer,
    structured_output_to_response_dict,
    validate_envelope,
)
from .models import _RunContext

logger = logging.getLogger(__name__)


class _StreamingMixin:
    async def stream_run(self, envelope: ContextEnvelope) -> AsyncIterator[dict[str, Any]]:
        started = time.monotonic()
        run_id = (envelope.run_id or "").strip() or f"run_{uuid.uuid4().hex[:12]}"

        validation_errors = validate_envelope(envelope)
        if validation_errors:
            output = self._failed_output(
                run_id=run_id,
                answer="; ".join(validation_errors),
                duration_ms=self._elapsed_ms(started),
            )
            yield _sse_event("run.complete", structured_output_to_response_dict(output))
            await _publish_run_fail_mq(
                _resolve_mq_publisher(),
                run_id=run_id,
                job_id=getattr(envelope, "job_id", "") or "",
                round_index=0,
                event_sequence=1,
                error_code="VALIDATION_FAILED",
                error_message="; ".join(validation_errors),
            )
            return

        # Shared preamble: resolve capabilities + enforce the attachment security gate
        # before streaming any tokens. Fail-closed; no-op without a resolver.
        prep = await self._prepare_capabilities(envelope, run_id, started, read_context_cache=False)
        for evt in prep.progress_events:
            yield evt
        if prep.rejection_event is not None:
            yield prep.rejection_event
            return
        resolved_config = prep.resolved_config

        try:
            if self._graph_runner.is_enabled():
                for evt in self._stream_with_graph(
                    envelope,
                    run_id,
                    started,
                    resolved_config,
                ):
                    yield evt
                return

            intent = classify_intent(envelope.task.user_message)
            duration_ms = self._elapsed_ms(started)
            yield _sse_event(
                "progress",
                {
                    "run_id": run_id,
                    "step": "classify_intent",
                    "summary": f"Classified intent as {intent}",
                    "duration_ms": duration_ms,
                },
            )

            duration_ms = self._elapsed_ms(started)
            yield _sse_event(
                "progress",
                {
                    "run_id": run_id,
                    "step": "assemble_context",
                    "summary": (
                        f"Loaded {len(envelope.context.objects)} object(s), "
                        f"{len(envelope.ontology.allowed_actions)} allowed action(s)"
                    ),
                    "duration_ms": duration_ms,
                },
            )

            ctx = self._prepare_run_context(envelope, intent)
            streaming_llm = self._resolve_streaming_llm(resolved_config)

            if streaming_llm is not None:
                for evt in self._stream_with_provider(envelope, run_id, started, intent, ctx, streaming_llm):
                    yield evt
            else:
                for evt in self._stream_heuristic(envelope, run_id, started, intent, ctx, limitations_extra=[]):
                    yield evt
        except Exception as exc:  # noqa: BLE001 - top-level route handler must catch all errors
            output = self._failed_output(
                run_id=run_id,
                answer=f"AI runtime processing error: {sanitize_provider_error(exc)}",
                duration_ms=self._elapsed_ms(started),
            )
            yield _sse_event("run.fail", structured_output_to_response_dict(output))
            await _publish_run_fail_mq(
                _resolve_mq_publisher(),
                run_id=run_id,
                job_id=getattr(envelope, "job_id", "") or "",
                round_index=0,
                event_sequence=1,
                error_code="AI_RUNTIME_PROCESSING_ERROR",
                error_message=sanitize_provider_error(exc),
            )

    def _stream_with_provider(
        self,
        envelope: ContextEnvelope,
        run_id: str,
        started: float,
        intent: str,
        ctx: _RunContext,
        streaming_llm: StreamingLlmClient,
    ) -> Iterator[dict[str, Any]]:
        system_prompt = build_system_prompt(envelope)
        user_message = envelope.task.user_message
        accumulated: list[str] = []
        model_name = getattr(streaming_llm, "model", None) or "openai-stream"

        try:
            for delta in streaming_llm.stream_complete(system_prompt, user_message):
                accumulated.append(delta)
                yield _sse_event("token", {"run_id": run_id, "delta": delta})
        except (LlmStreamError, LlmUnavailableError) as exc:
            if accumulated:
                yield _sse_event(
                    "transport.abort",
                    {"run_id": run_id, "message": sanitize_provider_error(exc)},
                )
                return
            yield from self._stream_heuristic(
                envelope,
                run_id,
                started,
                intent,
                ctx,
                limitations_extra=[sanitize_provider_error(exc)],
            )
            return

        answer = "".join(accumulated).strip()
        if not answer:
            yield from self._stream_heuristic(
                envelope,
                run_id,
                started,
                intent,
                ctx,
                limitations_extra=["LLM returned an empty streamed response"],
            )
            return

        usage = TokenUsage()
        if isinstance(streaming_llm, OpenAiStreamingLlmClient):
            raw_usage = streaming_llm.last_usage
            pt = int(raw_usage.get("prompt_tokens", 0) or 0)
            ct = int(raw_usage.get("completion_tokens", 0) or 0)
            usage = TokenUsage(
                prompt_tokens=pt,
                completion_tokens=ct,
                total_tokens=int(raw_usage.get("total_tokens", 0) or 0) or pt + ct,
            )
        elif hasattr(streaming_llm, "model"):
            usage = TokenUsage(
                prompt_tokens=max(1, len(system_prompt) // 4),
                completion_tokens=max(1, len(answer) // 4),
                total_tokens=max(2, (len(system_prompt) + len(answer)) // 4),
            )

        ctx.reasoning_steps.append(
            ReasoningStep(
                step="llm_stream",
                summary=f"Streamed answer via {model_name}",
            )
        )
        output = self._build_output(
            run_id=run_id,
            started=started,
            answer=answer,
            intent=intent,
            ctx=ctx,
            model_name=str(model_name),
            usage=usage,
            limitations=[],
            degraded=False,
        )
        yield _sse_event("run.complete", structured_output_to_response_dict(output))

    def _stream_with_graph(
        self,
        envelope: ContextEnvelope,
        run_id: str,
        started: float,
        resolved_config: Any = None,
    ) -> Iterator[dict[str, Any]]:
        """Graph-enabled streaming: pre-steps eagerly, then true provider deltas.

        ``resolved_config`` (from the shared preamble) carries the entity's provider
        credentials into the streaming client; ``None`` keeps the legacy env-only path.
        """
        try:
            graph_ctx, setup_error = self._graph_runner.run_streaming(envelope)
        except Exception as exc:  # noqa: BLE001 - recovery handler must catch all errors
            yield _sse_event(
                "progress",
                {
                    "run_id": run_id,
                    "step": "graph_fallback",
                    "summary": "Graph runner failed; falling back to heuristic",
                },
            )
            intent = classify_intent(envelope.task.user_message)
            ctx = self._prepare_run_context(envelope, intent)
            ctx.limitations.append(f"graph runner failed ({sanitize_provider_error(exc)}); fell back to heuristic")
            streaming_llm = self._resolve_streaming_llm(resolved_config)
            if streaming_llm is not None:
                yield from self._stream_with_provider(
                    envelope,
                    run_id,
                    started,
                    intent,
                    ctx,
                    streaming_llm,
                )
            else:
                yield from self._stream_heuristic(
                    envelope,
                    run_id,
                    started,
                    intent,
                    ctx,
                    limitations_extra=[],
                )
            return

        # Emit progress immediately (true TTFT: sidecar emits before LLM)
        yield _sse_event(
            "progress",
            {
                "run_id": run_id,
                "step": "graph_orchestrate",
                "summary": "Running read-only LangGraph orchestration",
            },
        )

        if setup_error:
            yield _sse_event(
                "progress",
                {
                    "run_id": run_id,
                    "step": "graph_fallback",
                    "summary": f"Graph setup fallback: {setup_error}",
                },
            )

        # Try streaming LLM for true token-by-token streaming
        streaming_llm = self._resolve_streaming_llm(resolved_config)
        if streaming_llm is not None:
            yield from self._stream_graph_with_provider(
                envelope,
                run_id,
                started,
                graph_ctx,
                streaming_llm,
            )
        else:
            # No streaming LLM: use heuristic answer with chunked pseudo-tokens
            yield from self._stream_graph_heuristic(
                envelope,
                run_id,
                started,
                graph_ctx,
            )

    def _stream_graph_with_provider(
        self,
        envelope: ContextEnvelope,
        run_id: str,
        started: float,
        graph_ctx: StreamingGraphContext,
        streaming_llm: StreamingLlmClient,
    ) -> Iterator[dict[str, Any]]:
        """Stream real tokens from provider using graph-prepared context.

        Note: This uses the sync streaming path (no tool execution).
        For tool-enabled streaming, use stream_run_with_tools() instead.
        """
        system_prompt = graph_ctx.system_prompt or build_system_prompt(envelope)
        user_message = graph_ctx.user_message
        accumulated: list[str] = []
        model_name = getattr(streaming_llm, "model", None) or "openai-stream"

        try:
            for delta in streaming_llm.stream_complete(system_prompt, user_message):
                accumulated.append(delta)
                yield _sse_event("token", {"run_id": run_id, "delta": delta})
        except (LlmStreamError, LlmUnavailableError) as exc:
            if accumulated:
                yield _sse_event(
                    "transport.abort",
                    {"run_id": run_id, "message": sanitize_provider_error(exc)},
                )
                return
            graph_ctx.limitations.append(sanitize_provider_error(exc))
            yield from self._stream_graph_heuristic(
                envelope,
                run_id,
                started,
                graph_ctx,
            )
            return

        answer = "".join(accumulated).strip()
        if not answer:
            yield from self._stream_graph_heuristic(
                envelope,
                run_id,
                started,
                graph_ctx,
                extra_limitations=["LLM returned an empty streamed response"],
            )
            return

        usage = TokenUsage()
        if isinstance(streaming_llm, OpenAiStreamingLlmClient):
            raw_usage = streaming_llm.last_usage
            pt = int(raw_usage.get("prompt_tokens", 0) or 0)
            ct = int(raw_usage.get("completion_tokens", 0) or 0)
            usage = TokenUsage(
                prompt_tokens=pt,
                completion_tokens=ct,
                total_tokens=int(raw_usage.get("total_tokens", 0) or 0) or pt + ct,
            )
        elif hasattr(streaming_llm, "model"):
            usage = TokenUsage(
                prompt_tokens=max(1, len(system_prompt) // 4),
                completion_tokens=max(1, len(answer) // 4),
                total_tokens=max(2, (len(system_prompt) + len(answer)) // 4),
            )

        result = graph_ctx.build_streamed_result(
            answer=answer,
            model_name=str(model_name),
            token_usage=usage,
            extra_steps=[
                ReasoningStep(
                    step="llm_stream",
                    summary=f"Streamed answer via {model_name}",
                )
            ],
        )
        output = graph_result_to_output(result, envelope)
        yield _sse_event("run.complete", structured_output_to_response_dict(output))

    def _stream_graph_heuristic(
        self,
        envelope: ContextEnvelope,
        run_id: str,
        started: float,
        graph_ctx: StreamingGraphContext,
        *,
        extra_limitations: list[str] | None = None,
    ) -> Iterator[dict[str, Any]]:
        """Emit heuristic answer as chunked pseudo-tokens using graph context."""
        limitations = list(graph_ctx.limitations)
        if extra_limitations:
            limitations.extend(extra_limitations)
        if not any("LLM" in lim or "provider" in lim.lower() for lim in limitations):
            limitations.append("LLM not configured (set OPENAI_API_KEY for full model-backed answers)")

        answer = heuristic_answer(envelope, graph_ctx.intent)
        graph_ctx.reasoning_steps.append(
            ReasoningStep(
                step="heuristic_only",
                summary="No LLM configured or provider unavailable; produced heuristic answer",
            )
        )

        for delta in _iter_answer_chunks(answer):
            yield _sse_event("token", {"run_id": run_id, "delta": delta})

        result = graph_ctx.build_heuristic_result(answer)
        result.limitations = limitations
        output = graph_result_to_output(result, envelope)
        yield _sse_event("run.complete", structured_output_to_response_dict(output))

    def _stream_heuristic(
        self,
        envelope: ContextEnvelope,
        run_id: str,
        started: float,
        intent: str,
        ctx: _RunContext,
        *,
        limitations_extra: list[str],
    ) -> Iterator[dict[str, Any]]:
        limitations = list(ctx.limitations)
        limitations.extend(limitations_extra)
        if not limitations_extra:
            limitations.append("LLM not configured (set OPENAI_API_KEY for full model-backed answers)")
        answer = heuristic_answer(envelope, intent)
        ctx.reasoning_steps.append(
            ReasoningStep(
                step="heuristic_only",
                summary="No LLM configured or provider unavailable; produced heuristic answer",
            )
        )
        for delta in _iter_answer_chunks(answer):
            yield _sse_event("token", {"run_id": run_id, "delta": delta})

        output = self._build_output(
            run_id=run_id,
            started=started,
            answer=answer,
            intent=intent,
            ctx=ctx,
            model_name="heuristic-runtime-v1",
            usage=TokenUsage(),
            limitations=limitations,
            degraded=True,
        )
        yield _sse_event("run.complete", structured_output_to_response_dict(output))
