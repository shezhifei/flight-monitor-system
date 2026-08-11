"""Mixin for RuntimeService — execute_run, _execute_validated, _provider_creds_from_config, _resolve_llm, _resolve_gateway, _resolve_streaming_llm, _sanitize_subagent_event."""

from __future__ import annotations

import asyncio
import logging
import os
import time
import uuid
from typing import Any, cast

from src.infrastructure.ai.context_envelope import ContextEnvelope
from src.infrastructure.ai.intent_router import classify_intent
from src.infrastructure.ai.runtime_graph import graph_result_to_output
from src.infrastructure.ai.runtime_llm import (
    LlmClient,
    LlmUnavailableError,
    OpenAiLlmClient,
    OpenAiStreamingLlmClient,
    StreamingLlmClient,
    sanitize_provider_error,
)
from src.infrastructure.ai.security.url_guard import UnsafeUrlError, redact_url_for_log
from src.infrastructure.ai.structured_output import (
    AiStructuredOutput,
    ReasoningStep,
    TokenUsage,
)
from src.infrastructure.ai.tools.read_only_tools import is_read_only_tool
from src.infrastructure.common.exceptions import LLM_EXCEPTIONS

from .helpers import (
    build_system_prompt,
    heuristic_answer,
    validate_envelope,
)

logger = logging.getLogger(__name__)


class _ResolveMixin:
    async def execute_run(self, envelope: ContextEnvelope) -> AiStructuredOutput:
        started = time.monotonic()
        run_id = (envelope.run_id or "").strip() or f"run_{uuid.uuid4().hex[:12]}"

        validation_errors = validate_envelope(envelope)
        if validation_errors:
            return self._failed_output(
                run_id=run_id,
                answer="; ".join(validation_errors),
                duration_ms=self._elapsed_ms(started),
            )

        try:
            return await self._execute_validated(envelope, run_id, started)
        except Exception as exc:  # noqa: BLE001 - top-level route handler must catch all errors
            return self._failed_output(
                run_id=run_id,
                answer=f"AI runtime processing error: {exc}",
                duration_ms=self._elapsed_ms(started),
            )

    async def _execute_validated(
        self,
        envelope: ContextEnvelope,
        run_id: str,
        started: float,
    ) -> AiStructuredOutput:
        # Shared preamble: resolve capabilities + enforce the attachment security gate
        # before any model call. Fail-closed (mirrors the streaming paths). No-op when
        # no resolver is wired (resolved_config stays None → env-only behavior).
        prep = await self._prepare_capabilities(envelope, run_id, started, read_context_cache=False)
        if prep.rejection_answer is not None:
            return self._failed_output(
                run_id=run_id,
                answer=prep.rejection_answer,
                duration_ms=self._elapsed_ms(started),
            )
        resolved_config = prep.resolved_config

        if self._graph_runner.is_enabled():
            try:
                llm = self._resolve_llm(resolved_config)
                result, graph_error = self._graph_runner.run(envelope, llm=llm)
                return graph_result_to_output(
                    result,
                    envelope,
                    graph_error=graph_error,
                )
            except Exception as exc:  # noqa: BLE001 - recovery handler must catch all errors
                intent = classify_intent(envelope.task.user_message)
                ctx = self._prepare_run_context(envelope, intent)
                limitations = [f"graph runner failed ({sanitize_provider_error(exc)}); fell back to heuristic"]
                llm = self._resolve_llm(resolved_config)
                if llm is not None:
                    try:
                        completion = await asyncio.to_thread(
                            llm.complete,
                            build_system_prompt(envelope),
                            envelope.task.user_message,
                        )
                        ctx.reasoning_steps.append(
                            ReasoningStep(
                                step="llm_complete",
                                summary=f"Generated answer via {completion.model} (graph fallback)",
                            )
                        )
                        return self._build_output(
                            run_id=run_id,
                            started=started,
                            answer=completion.content,
                            intent=intent,
                            ctx=ctx,
                            model_name=completion.model,
                            usage=TokenUsage(
                                prompt_tokens=completion.prompt_tokens,
                                completion_tokens=completion.completion_tokens,
                                total_tokens=completion.prompt_tokens + completion.completion_tokens,
                            ),
                            limitations=limitations,
                            degraded=True,
                        )
                    except LLM_EXCEPTIONS as fallback_exc:
                        logger.debug("LLM fallback in graph path: %s", fallback_exc)
                        limitations.append("LLM unavailable in graph fallback")
                return self._build_output(
                    run_id=run_id,
                    started=started,
                    answer=heuristic_answer(envelope, intent),
                    intent=intent,
                    ctx=ctx,
                    model_name="heuristic-runtime-v1",
                    usage=TokenUsage(),
                    limitations=limitations,
                    degraded=True,
                )

        intent = classify_intent(envelope.task.user_message)
        ctx = self._prepare_run_context(envelope, intent)

        llm = self._resolve_llm(resolved_config)
        if llm is not None:
            try:
                completion = await asyncio.to_thread(
                    llm.complete,
                    build_system_prompt(envelope),
                    envelope.task.user_message,
                )
                ctx.reasoning_steps.append(
                    ReasoningStep(
                        step="llm_complete",
                        summary=f"Generated answer via {completion.model}",
                    )
                )
                return self._build_output(
                    run_id=run_id,
                    started=started,
                    answer=completion.content,
                    intent=intent,
                    ctx=ctx,
                    model_name=completion.model,
                    usage=TokenUsage(
                        prompt_tokens=completion.prompt_tokens,
                        completion_tokens=completion.completion_tokens,
                        total_tokens=completion.prompt_tokens + completion.completion_tokens,
                    ),
                    limitations=[],
                    degraded=False,
                )
            except LlmUnavailableError as exc:
                ctx.reasoning_steps.append(
                    ReasoningStep(
                        step="llm_fallback",
                        summary="LLM unavailable; fell back to heuristic answer",
                    )
                )
                return self._build_output(
                    run_id=run_id,
                    started=started,
                    answer=heuristic_answer(envelope, intent),
                    intent=intent,
                    ctx=ctx,
                    model_name="heuristic-runtime-v1",
                    usage=TokenUsage(),
                    limitations=[sanitize_provider_error(exc)],
                    degraded=True,
                )

        ctx.reasoning_steps.append(
            ReasoningStep(
                step="heuristic_only",
                summary="No LLM configured; produced heuristic answer",
            )
        )
        return self._build_output(
            run_id=run_id,
            started=started,
            answer=heuristic_answer(envelope, intent),
            intent=intent,
            ctx=ctx,
            model_name="heuristic-runtime-v1",
            usage=TokenUsage(),
            limitations=[],
            degraded=True,
        )

    @staticmethod
    def _provider_creds_from_config(resolved_config: Any = None) -> tuple[str, str, str]:
        """Derive (api_key, base_url, model) from the resolved provider.

        Mirrors :meth:`_resolve_gateway`: api_key/base_url come straight from the
        resolved config; model prefers the provider-side ``provider_model`` and
        falls back to ``model_id``. Each value is left empty here when absent so
        the env-only client can self-configure (the legacy fallback lives in the
        client constructors), keeping single-provider env deployments unchanged.
        """
        api_key = base_url = model = ""
        if resolved_config is not None:
            api_key = (getattr(resolved_config, "api_key", "") or "").strip()
            base_url = (getattr(resolved_config, "base_url", "") or "").strip()
            model = (getattr(getattr(resolved_config, "model", None), "provider_model", "") or "").strip() or (
                getattr(resolved_config, "model_id", "") or ""
            ).strip()
        return api_key, base_url, model

    def _resolve_llm(self, resolved_config: Any = None) -> LlmClient | None:
        if self._llm_override is not None:
            return self._llm_override if self._llm_override.is_configured() else None
        api_key, base_url, model = self._provider_creds_from_config(resolved_config)
        try:
            openai_client = OpenAiLlmClient(api_key=api_key, base_url=base_url, model=model)
        except UnsafeUrlError as exc:
            logger.warning(
                "llm_url_blocked: base_url=%s error=%s",
                redact_url_for_log(base_url),
                exc,
            )
            return None
        return openai_client if openai_client.is_configured() else None

    def _resolve_gateway(self, resolved_config: Any = None) -> Any | None:
        """Build the AiGateway used by the tool-calling runner from the resolved provider.

        This is what wires per-entity provider credentials into the *real* model
        call: base_url / api_key / model come from ``resolved_config`` (the
        snapshot's provider, selected via ``provider_ref``), each falling back to
        the same env vars the legacy env-only client used so existing single-
        provider deployments keep working unchanged.

        Precedence: an injected ``_llm_override`` (tests / explicit wiring) wins so
        fakes flow straight through. Returns ``None`` when no usable API key is
        available, so the caller degrades to the heuristic answer.
        """
        if self._llm_override is not None:
            is_conf = getattr(self._llm_override, "is_configured", None)
            if callable(is_conf) and not is_conf():
                return None
            return self._llm_override

        api_key = base_url = model = ""
        if resolved_config is not None:
            api_key = (getattr(resolved_config, "api_key", "") or "").strip()
            base_url = (getattr(resolved_config, "base_url", "") or "").strip()
            model = (getattr(getattr(resolved_config, "model", None), "provider_model", "") or "").strip() or (
                getattr(resolved_config, "model_id", "") or ""
            ).strip()

        api_key = api_key or os.getenv("OPENAI_API_KEY", "").strip()
        if not api_key:
            return None
        base_url = base_url or os.getenv("OPENAI_BASE_URL", "").strip() or "https://api.openai.com/v1"
        model = model or os.getenv("OPENAI_MODEL", "gpt-4o-mini").strip() or "gpt-4o-mini"
        timeout = float(getattr(resolved_config, "timeout", 120.0) or 120.0) if resolved_config is not None else 120.0
        max_retries = int(getattr(resolved_config, "max_retries", 3) or 3) if resolved_config is not None else 3

        from src.infrastructure.ai.openai_client import OpenAIClient, OpenAIClientConfig

        try:
            return OpenAIClient(
                config=OpenAIClientConfig(
                    api_key=api_key,
                    base_url=base_url,
                    default_model=model,
                    timeout=timeout,
                    max_retries=max(0, max_retries),
                )
            )
        except (
            Exception  # noqa: BLE001
        ) as exc:  # pragma: no cover - defensive
            logger.warning("Gateway construction failed; falling back to heuristic: %s", exc)
            return None

    def _resolve_streaming_llm(self, resolved_config: Any = None) -> StreamingLlmClient | None:
        if self._streaming_override is not None:
            return self._streaming_override if self._streaming_override.is_configured() else None
        if (
            self._llm_override is not None
            and hasattr(self._llm_override, "stream_complete")
            and self._llm_override.is_configured()
        ):
            return cast(StreamingLlmClient, self._llm_override)
        api_key, base_url, model = self._provider_creds_from_config(resolved_config)
        try:
            client = OpenAiStreamingLlmClient(api_key=api_key, base_url=base_url, model=model)
        except UnsafeUrlError as exc:
            logger.warning(
                "streaming_llm_url_blocked: base_url=%s error=%s",
                redact_url_for_log(base_url),
                exc,
            )
            return None
        return client if client.is_configured() else None

    @staticmethod
    def _sanitize_subagent_event(child_event: Any, parent_run_id: str) -> dict[str, Any]:
        """Sanitize a bubbled child StreamEvent into an SSE-safe payload (P2b).

        Mirrors the redaction discipline of :func:`_sanitize_tool_result_event`:
        only event type, attribution metadata (subagent_depth / parent_run_id), and
        non-sensitive shape are exposed — never raw tool args or model output beyond
        the streamed text delta the child already emits.
        """
        meta = getattr(child_event, "metadata", None) or {}
        event_type = getattr(child_event, "type", "") or ""
        payload: dict[str, Any] = {
            "run_id": parent_run_id,
            "event_type": event_type,
            "subagent_depth": meta.get("subagent_depth"),
            "parent_run_id": meta.get("parent_run_id") or parent_run_id,
        }
        if event_type == "text_delta":
            payload["delta"] = getattr(child_event, "text_delta", "") or ""
        elif event_type in ("tool_call", "tool_result"):
            tool_call = getattr(child_event, "tool_call", None) or {}
            payload["tool_name"] = tool_call.get("tool_name", "")
            payload["tool_type"] = "read_only" if is_read_only_tool(tool_call.get("tool_name", "")) else "write_action"
        return payload
