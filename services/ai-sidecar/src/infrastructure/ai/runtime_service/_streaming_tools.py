"""Mixin for RuntimeService — stream_run_with_tools, _prepare_run_context, _build_output, _failed_output."""

from __future__ import annotations

import asyncio
import logging
import time
import uuid
from collections.abc import AsyncIterator
from typing import Any

from src.infrastructure.ai.context_envelope import ContextEnvelope
from src.infrastructure.ai.openai_client import Message
from src.infrastructure.ai.runtime_context import enhance_context
from src.infrastructure.ai.runtime_graph import graph_result_to_output
from src.infrastructure.ai.runtime_llm import (
    sanitize_provider_error,
)
from src.infrastructure.ai.structured_output import (
    AiStructuredOutput,
    OutputEvidence,
    OutputMetrics,
    OutputProposal,
    ReasoningStep,
    TokenUsage,
)
from src.infrastructure.ai.templates import (
    get_task_template,
    resolve_budget_with_hard_cap,
    template_allows_tool,
)
from src.infrastructure.common.exceptions import REDIS_EXCEPTIONS

from ._constants import CONTRACT_VERSION, STATUS_FAILED, STATUS_SUCCEEDED
from ._mq_publish import (
    _publish_run_complete_mq,
    _publish_run_fail_mq,
    _resolve_mq_gate,
    _resolve_mq_publisher,
)
from .helpers import (
    _iter_answer_chunks,
    _sanitize_tool_call_event,
    _sanitize_tool_result_event,
    _sse_event,
    build_evidence,
    build_proposals_from_envelope,
    build_reasoning_steps,
    build_system_prompt,
    heuristic_answer,
    structured_output_to_response_dict,
    validate_envelope,
)
from .models import _RunContext

logger = logging.getLogger(__name__)


class _StreamingToolsMixin:
    async def stream_run_with_tools(
        self,
        envelope: ContextEnvelope,
    ) -> AsyncIterator[dict[str, Any]]:
        """Async streaming run with full tool execution loop.

        Uses LLMStreamRunner.stream_chat_with_tools() for multi-turn tool execution.
        Read-only tools execute locally; write actions generate OutputProposals.
        """
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
            success = await _publish_run_fail_mq(
                _resolve_mq_publisher(),
                run_id=run_id,
                job_id=getattr(envelope, "job_id", "") or "",
                round_index=0,
                event_sequence=1,
                error_code="VALIDATION_FAILED",
                error_message="; ".join(validation_errors),
                require_durable_ack=True,
            )
            if not success:
                logger.error(f"Failed to publish durable run.fail for validation error, run={run_id}")
            return

        # Resolve entity capabilities (shared preamble; tool path reuses cache).
        entity_id = getattr(envelope, "entity_id", None) or "default"
        prep = await self._prepare_capabilities(envelope, run_id, started, read_context_cache=True)
        for evt in prep.progress_events:
            yield evt
        if prep.rejection_event is not None:
            yield prep.rejection_event
            return
        resolved_config = prep.resolved_config
        # Prior turns reused from the context cache on a hit (non-system only).
        # Falls back to [] when the cache is disabled, misses, or holds no transcript.
        cached_prior_messages: list[Message] = prep.cached_prior_messages

        try:
            graph_ctx, setup_error = self._graph_runner.run_streaming(envelope)
        except Exception as exc:  # noqa: BLE001 - recovery handler must catch all errors
            yield _sse_event(
                "progress",
                {
                    "run_id": run_id,
                    "step": "graph_fallback",
                    "summary": "Graph runner failed; falling back",
                },
            )
            graph_ctx = self._graph_runner._build_streaming_context(envelope, "general", started)
            graph_ctx.limitations.append(f"graph runner failed ({sanitize_provider_error(exc)}); fell back")

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

        gateway = self._resolve_gateway(resolved_config)
        if gateway is None:
            graph_ctx.limitations.append("LLM not configured (set OPENAI_API_KEY for full model-backed answers)")
            answer = heuristic_answer(envelope, graph_ctx.intent)
            for delta in _iter_answer_chunks(answer):
                yield _sse_event("token", {"run_id": run_id, "delta": delta})
            result = graph_ctx.build_heuristic_result(answer)
            result.limitations = graph_ctx.limitations
            output = graph_result_to_output(result, envelope)
            yield _sse_event("run.complete", structured_output_to_response_dict(output))
            await _publish_run_complete_mq(
                _resolve_mq_publisher(),
                _resolve_mq_gate(),
                run_id=run_id,
                job_id=getattr(envelope, "job_id", "") or "",
                round_index=0,
                event_sequence=1,
                output=structured_output_to_response_dict(output),
            )
            return

        last_round_index = 0
        try:
            # Import through the package so tests can patch
            # ``src.infrastructure.ai.runtime_service.LLMStreamRunner``.
            from src.infrastructure.ai.runtime_service import LLMStreamRunner

            # B2: per-run working memory workspace (plan.md / notes.md /
            # evidence.json). Large tool results spill here; the model only
            # receives summary + pointer.
            from src.infrastructure.ai.working_memory import WorkingMemory

            working_memory = WorkingMemory(run_id=run_id)

            # C2: per-run lifecycle hook pipeline (PreToolUse / PostToolUse /
            # PreCompact / Stop). Hooks are synchronous pure functions over the
            # run context — no shell, no external calls.
            from src.infrastructure.ai.hooks.pipeline import build_default_pipeline

            hook_pipeline = build_default_pipeline()

            runner = LLMStreamRunner(
                client=gateway,
                tool_executor=self._tool_executor,
            )

            # Build system prompt with skill injection
            base_system_prompt = graph_ctx.system_prompt or build_system_prompt(envelope)
            system_prompt_parts = [base_system_prompt]
            skill_hash = None
            skill_instruction_tokens = 0

            if resolved_config and resolved_config.skills.enabled and self._skill_instruction_composer:
                try:
                    task_type = getattr(envelope.task, "task_type", None) if hasattr(envelope, "task") else None
                    composed = await self._skill_instruction_composer.compose(
                        entity_id=entity_id,
                        task_type=task_type,
                        max_total_tokens=3000,
                    )
                    if composed and composed.combined_text:
                        skill_block = (
                            "\n\n# Enabled Agent Skills\n"
                            f"<!-- skills_hash={composed.hash} -->\n"
                            f"{composed.combined_text}\n"
                            "<!-- End Agent Skills -->"
                        )
                        system_prompt_parts.append(skill_block)
                        skill_hash = composed.hash
                        skill_instruction_tokens = composed.total_tokens or 0
                        yield _sse_event(
                            "progress",
                            {
                                "run_id": run_id,
                                "step": "skills.injected",
                                "summary": (
                                    f"Injected {len(composed.fragments)} skill instructions, "
                                    f"hash={composed.hash}, tokens={composed.total_tokens}"
                                ),
                            },
                        )
                except Exception as exc:  # noqa: BLE001 - recovery handler must catch all errors
                    if resolved_config.skills.fail_closed:
                        output = self._failed_output(
                            run_id=run_id,
                            answer=f"SKILL_INSTRUCTION_LOAD_FAILED: {exc}",
                            duration_ms=self._elapsed_ms(started),
                        )
                        yield _sse_event("run.fail", structured_output_to_response_dict(output))
                        await _publish_run_fail_mq(
                            _resolve_mq_publisher(),
                            run_id=run_id,
                            job_id=getattr(envelope, "job_id", "") or "",
                            round_index=0,
                            event_sequence=1,
                            error_code="SKILL_INSTRUCTION_LOAD_FAILED",
                            error_message=str(exc),
                        )
                        return
                    yield _sse_event(
                        "progress",
                        {
                            "run_id": run_id,
                            "step": "skills.skipped",
                            "summary": f"Skill injection failed (fail_closed=false): {exc}",
                        },
                    )

            system_prompt_text = "\n".join(system_prompt_parts)

            # Prior turns: explicit caller-supplied history wins over cache reuse.
            # Caller-supplied "system" entries are ignored (system is built above).
            explicit_history = [
                Message.from_dict(m)
                for m in (getattr(envelope, "conversation_history", None) or [])
                if isinstance(m, dict) and m.get("role") != "system"
            ]
            prior_messages = explicit_history or cached_prior_messages
            # P1a: assemble the user turn as multimodal content when attachments are
            # present (audio fed directly when the model supports it, else ASR-routed;
            # images as image_url). Falls back to a plain string when there is no media.
            user_content = await self._build_user_content(envelope, resolved_config, graph_ctx.user_message)
            messages = [
                Message(role="system", content=system_prompt_text),
                *prior_messages,
                Message(role="user", content=user_content),
            ]
            tool_proposals: list[OutputProposal] = []

            # Hybrid agent Task A2: fail closed without a resolved tool snapshot.
            # The resolved entity snapshot is the ONLY tool source for a
            # production run — there is no mock/default fallback anymore
            # (docs/architecture/AGENT_RUNTIME_LOOP.md).
            if resolved_config is None or not resolved_config.tools:
                output = self._failed_output(
                    run_id=run_id,
                    answer=(
                        "AI_TOOL_SNAPSHOT_MISSING: no resolved tool snapshot for this run; "
                        "refusing to fall back to mock/default tools"
                    ),
                    duration_ms=self._elapsed_ms(started),
                )
                yield _sse_event("run.fail", structured_output_to_response_dict(output))
                await _publish_run_fail_mq(
                    _resolve_mq_publisher(),
                    run_id=run_id,
                    job_id=getattr(envelope, "job_id", "") or "",
                    round_index=0,
                    event_sequence=1,
                    error_code="AI_TOOL_SNAPSHOT_MISSING",
                    error_message="no resolved tool snapshot for this run",
                    terminal_event_id=None,
                )
                return

            # Hybrid agent Task A4–A6: task templates narrow the visible tool
            # face (read-only categories for query_ops, write actions denied).
            # Intersection only — a template can never widen the resolved
            # entity snapshot (non-negotiable constraint #2).
            task_template = get_task_template(getattr(envelope.task, "task_type", None))
            tools = [
                t.to_schema()
                for t in resolved_config.tools
                if template_allows_tool(
                    task_template,
                    tool_name=getattr(t, "name", ""),
                    tool_category=getattr(t, "category", None),
                )
            ]

            # C1: plan-first templates (anomaly_ops / dispatch_ops) additionally
            # expose the no-op plan-board tools. This is additive policy from the
            # task template, not part of the entity snapshot — the tools execute
            # in-process against the run's WorkingMemory and never reach the
            # executor's ACL / MQ gate path.
            if task_template is not None and getattr(task_template, "requires_plan_first", False):
                from src.infrastructure.ai.tools.plan_tools import plan_schemas_for_task_type

                existing_tool_names = {
                    (t.get("function") or {}).get("name") for t in tools if isinstance(t, dict)
                }
                for schema in plan_schemas_for_task_type(task_template.task_type):
                    if (schema.get("function") or {}).get("name") not in existing_tool_names:
                        tools.append(schema)

            # Add subagent tool if enabled
            if resolved_config and resolved_config.subagents.enabled:
                from src.infrastructure.ai.subagents.dispatcher import SUBAGENT_TOOL_SCHEMA

                tools.append(SUBAGENT_TOOL_SCHEMA)
                # Inject subagent config into envelope metadata for ToolExecutor
                if not hasattr(envelope, "metadata") or envelope.metadata is None:
                    envelope.metadata = {}
                envelope.metadata["subagent_allowed_entity_ids"] = resolved_config.subagents.allowed_entity_ids
                envelope.metadata["subagent_max_depth"] = resolved_config.subagents.max_depth
                envelope.metadata["subagent_max_concurrency"] = resolved_config.subagents.max_concurrency
                envelope.metadata["subagent_inherit_parent_context"] = resolved_config.subagents.inherit_parent_context
                # Inherit parent depth/trace if present
                if "subagent_depth" not in envelope.metadata:
                    envelope.metadata["subagent_depth"] = 0
                if "subagent_trace" not in envelope.metadata:
                    envelope.metadata["subagent_trace"] = []

            # Budget-driven context compression (before the LLM request).
            messages, compression_payload = await self._apply_context_budget(
                messages=messages,
                system_prompt_text=system_prompt_text,
                tools=tools,
                resolved_config=resolved_config,
                skill_instruction_tokens=skill_instruction_tokens,
                envelope=envelope,
            )
            if compression_payload is not None:
                yield _sse_event(
                    "context.compressed",
                    {
                        "run_id": run_id,
                        **compression_payload,
                    },
                )

            # B1: inject round budget from capability snapshot's tool_policy and task template.
            # Resolution priority (Task B1):
            #   1. Retrieve entity's tooling.max_rounds from the snapshot
            #   2. Apply template default + hard cap
            #   3. Final clamp with production default (20)
            entity_max_rounds = resolved_config.tool_policy.get("max_rounds", 5)
            task_template = get_task_template(getattr(envelope.task, "task_type", None))
            effective_max_tool_rounds = resolve_budget_with_hard_cap(entity_max_rounds, task_template, production_default_hard_cap=20)

            # Note: the context cache write-through happens AFTER the model responds
            # (see the "completed" branch below) so the assistant turn is captured in
            # the stored transcript; writing here would persist a stale, reply-less
            # snapshot that is useless as reusable history.

            # Build tool result cache policy from resolved entity config (per-run, not mutable state)
            tool_cache_policy = None
            if (
                resolved_config
                and resolved_config.cache_policy.enabled
                and resolved_config.cache_policy.tool_result_cache_enabled
            ):
                from src.infrastructure.ai.tools.tool_executor import ToolResultCachePolicy

                tool_cache_policy = ToolResultCachePolicy(
                    enabled=True,
                    cacheable_tools=resolved_config.cache_policy.tool_result_cacheable_tools,
                    ttl=resolved_config.cache_policy.tool_result_cache_ttl,
                )

            # Build prompt cache params via AiCacheManager
            prompt_cache_params = {}
            if resolved_config and resolved_config.cache_policy.enabled:
                import hashlib as _hashlib
                import json as _json

                from src.infrastructure.ai.capability_resolver import generate_prompt_cache_key

                system_prompt_hash = _hashlib.sha256(system_prompt_text.encode()).hexdigest()[:16]
                tools_canonical = _json.dumps(tools, sort_keys=True, separators=(",", ":"), default=str)
                tool_schema_hash = _hashlib.sha256(tools_canonical.encode()).hexdigest()[:16]
                cache_key = generate_prompt_cache_key(
                    namespace=resolved_config.cache_policy.provider_prompt_cache_namespace or "flight_monitor",
                    entity_id=entity_id,
                    api_format=resolved_config.api_format,
                    model_id=resolved_config.model_id,
                    system_prompt_hash=system_prompt_hash,
                    tool_schema_hash=tool_schema_hash,
                    skill_hash=skill_hash,
                )
                if self._cache_manager:
                    prompt_cache_params = self._cache_manager.build_prompt_cache_params(
                        enabled=resolved_config.cache_policy.provider_prompt_cache_enabled,
                        cache_key=cache_key,
                        retention=resolved_config.cache_policy.provider_prompt_cache_retention,
                    )
                elif resolved_config.cache_policy.provider_prompt_cache_enabled:
                    prompt_cache_params = {
                        "prompt_cache_key": cache_key,
                        "prompt_cache_retention": resolved_config.cache_policy.provider_prompt_cache_retention,
                    }

            # P2b: sub-agent event bubbling. The dispatcher invokes on_child_event
            # for every child StreamEvent (out of band, depth/parent_run_id stamped).
            # We cannot yield from inside that callback, so it pushes onto a queue we
            # drain between the parent runner's own events and re-emit as SSE
            # type='subagent_event' (sanitized like the parent's tool.result frames).
            subagent_queue: asyncio.Queue[dict[str, Any]] = asyncio.Queue()
            run_completed_emitted = False

            async def _on_child_event(child_event: Any) -> None:
                subagent_queue.put_nowait(self._sanitize_subagent_event(child_event, run_id))

            def _drain_subagent_queue() -> list[dict[str, Any]]:
                drained: list[dict[str, Any]] = []
                while not subagent_queue.empty():
                    drained.append(subagent_queue.get_nowait())
                return drained

            async for event in runner.stream_chat_with_tools(
                messages=messages,
                model=(
                    (
                        (getattr(getattr(resolved_config, "model", None), "provider_model", "") or "").strip()
                        or (getattr(resolved_config, "model_id", "") or "").strip()
                    )
                    if resolved_config
                    else (getattr(getattr(gateway, "config", None), "default_model", None) or "gpt-4o")
                ),
                tools=tools,
                run_id=run_id,
                envelope=envelope,
                tool_cache_policy=tool_cache_policy,
                on_child_event=_on_child_event,
                entity_id=entity_id,
                max_tool_rounds=effective_max_tool_rounds,
                working_memory=working_memory,
                hook_pipeline=hook_pipeline,
                **prompt_cache_params,
            ):
                # Bubble any sub-agent events accumulated since the last parent event.
                for sub_sse in _drain_subagent_queue():
                    yield _sse_event("subagent_event", sub_sse)

                if event.type == "text_delta":
                    yield _sse_event("token", {"run_id": run_id, "delta": event.text_delta})
                elif event.type == "tool_call":
                    yield _sse_event("tool.call", _sanitize_tool_call_event(event.tool_call or {}))
                    if event.tool_call:
                        prop_data = event.tool_call.get("result", {}).get("proposal")
                        if prop_data:
                            tool_proposals.append(OutputProposal(**prop_data))
                elif event.type == "tool_result":
                    yield _sse_event("tool.result", _sanitize_tool_result_event(event.tool_call or {}))
                elif event.type == "completed":
                    if run_completed_emitted:
                        continue
                    last_round_index = getattr(event, "round_index", last_round_index) or last_round_index
                    # Flush any sub-agent events still queued from the final tool round
                    # before the terminal run.complete frame.
                    for sub_sse in _drain_subagent_queue():
                        yield _sse_event("subagent_event", sub_sse)

                    final_result = event.result
                    if final_result and final_result.text:
                        answer_text = final_result.text
                    else:
                        answer_text = heuristic_answer(envelope, graph_ctx.intent)

                    extra_steps = []
                    if final_result and final_result.model:
                        extra_steps.append(
                            ReasoningStep(
                                step="llm_stream",
                                summary=f"Streamed answer via {final_result.model}",
                            )
                        )

                    token_usage = TokenUsage()
                    if final_result and final_result.usage:
                        raw = final_result.usage
                        token_usage = TokenUsage(
                            prompt_tokens=int(raw.get("prompt_tokens", 0) or 0),
                            completion_tokens=int(raw.get("completion_tokens", 0) or 0),
                            total_tokens=int(raw.get("total_tokens", 0) or 0),
                        )

                        # Record provider prompt cache metrics
                        cached_tokens = int(
                            raw.get("cached_tokens", 0)
                            or raw.get("prompt_tokens_details", {}).get("cached_tokens", 0)
                            or 0
                        )
                        if self._cache_manager and resolved_config and cached_tokens > 0:
                            try:
                                import hashlib as _hashlib

                                from src.infrastructure.ai.cache_manager import CacheEvent

                                cache_key_str = prompt_cache_params.get("prompt_cache_key", "")
                                key_hash = (
                                    _hashlib.sha256(cache_key_str.encode()).hexdigest()[:16] if cache_key_str else ""
                                )
                                self._cache_manager._record_event(
                                    CacheEvent(
                                        cache_type="provider_prompt",
                                        key_hash=key_hash,
                                        hit=True,
                                        cached_tokens=cached_tokens,
                                    )
                                )
                            except REDIS_EXCEPTIONS as e:
                                logger.warning("Failed to record provider prompt cache metrics: %s", e)

                    all_proposals = list(graph_ctx.proposals) + tool_proposals
                    result = graph_ctx.build_streamed_result(
                        answer=answer_text,
                        model_name=str(getattr(final_result, "model", "gpt-4o")) if final_result else "gpt-4o",
                        token_usage=token_usage,
                        extra_steps=extra_steps,
                    )
                    result.proposals = all_proposals
                    output = graph_result_to_output(result, envelope)
                    output_dict = structured_output_to_response_dict(output)
                    yield _sse_event("run.complete", output_dict)
                    run_completed_emitted = True
                    terminal_event_id = str(output_dict.get("event_id") or "") or None
                    publisher = _resolve_mq_publisher()
                    gate = _resolve_mq_gate()
                    terminal_sequence = 1
                    if gate is not None:
                        try:
                            terminal_sequence = await gate.next_event_sequence(run_id)
                        except Exception:  # noqa: BLE001
                            terminal_sequence = 1

                    mq_success = await _publish_run_complete_mq(
                        publisher,
                        gate,
                        run_id=run_id,
                        job_id=getattr(envelope, "job_id", "") or "",
                        round_index=last_round_index,
                        event_sequence=terminal_sequence,
                        output=output_dict,
                        proposal_ids=[p.proposal_id for p in all_proposals if getattr(p, "proposal_id", None)],
                        terminal_event_id=terminal_event_id,
                        require_durable_ack=True,
                    )

                    if not mq_success:
                        logger.error(
                            "Critical: Failed to publish durable run.complete after 3 attempts. "
                            "Run may appear stuck in running state. "
                            f"run_id={run_id}"
                        )
                        # Mark as failed without durable event
                        output = self._failed_output(
                            run_id=run_id,
                            answer="DURABLE_EVENT_PUBLISH_FAILED: Terminal event could not be durably published",
                            duration_ms=self._elapsed_ms(started),
                        )
                        yield _sse_event("run.fail", structured_output_to_response_dict(output))

                    # Context cache write-through: persist the running transcript so the
                    # next turn can reuse it as prior history. Excludes the system prompt
                    # (rebuilt per run from config) and is written here, post-response, so
                    # the assistant turn is included. `messages` is the post-compression
                    # list, so reuse inherits compression too.
                    if self._context_cache_enabled(resolved_config):
                        conversation_id = self._context_conversation_id(envelope, run_id)
                        transcript = [m for m in messages if getattr(m.role, "value", m.role) != "system"]
                        transcript.append(Message(role="assistant", content=answer_text))
                        wrote = await self._write_context_cache(resolved_config, entity_id, conversation_id, transcript)
                        if wrote:
                            yield _sse_event(
                                "progress",
                                {
                                    "run_id": run_id,
                                    "step": "context.cache_write",
                                    "summary": "context transcript cached",
                                },
                            )

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
                round_index=last_round_index,
                event_sequence=1,
                error_code="AI_RUNTIME_PROCESSING_ERROR",
                error_message=sanitize_provider_error(exc),
            )

    def _prepare_run_context(self, envelope: ContextEnvelope, intent: str) -> _RunContext:
        ctx = _RunContext(
            reasoning_steps=build_reasoning_steps(envelope, intent),
            evidence=build_evidence(envelope),
            proposals=build_proposals_from_envelope(envelope, intent),
        )
        enrichment = enhance_context(envelope)
        ctx.reasoning_steps.extend(enrichment.reasoning_steps)
        ctx.evidence.extend(enrichment.evidence)
        ctx.limitations.extend(enrichment.limitations)
        return ctx

    def _build_output(
        self,
        *,
        run_id: str,
        started: float,
        answer: str,
        intent: str,
        ctx: _RunContext,
        model_name: str,
        usage: TokenUsage,
        limitations: list[str],
        degraded: bool,
    ) -> AiStructuredOutput:
        all_limitations = list(ctx.limitations) + list(limitations)
        if (
            degraded
            and not any("LLM" in lim or "provider" in lim.lower() for lim in all_limitations)
            and not any("not configured" in lim for lim in all_limitations)
        ):
            all_limitations.append("LLM not configured (set OPENAI_API_KEY for full model-backed answers)")
        return AiStructuredOutput(
            contract_version=CONTRACT_VERSION,
            run_id=run_id,
            status=STATUS_SUCCEEDED,
            answer=answer,
            reasoning_steps=ctx.reasoning_steps,
            evidence=ctx.evidence,
            proposals=ctx.proposals,
            limitations=all_limitations,
            metrics=OutputMetrics(model=model_name, duration_ms=self._elapsed_ms(started)),
            token_usage=usage,
        )

    def _failed_output(self, run_id: str, answer: str, duration_ms: int) -> AiStructuredOutput:
        return AiStructuredOutput(
            contract_version=CONTRACT_VERSION,
            run_id=run_id,
            status=STATUS_FAILED,
            answer=answer,
            reasoning_steps=[
                ReasoningStep(step="validate_envelope", summary="Envelope validation failed"),
            ],
            evidence=[
                OutputEvidence(
                    object_type="system",
                    object_id="runtime",
                    source="ai_runtime.validation",
                    field=None,
                )
            ],
            proposals=[],
            limitations=[],
            metrics=OutputMetrics(model="runtime-validator", duration_ms=duration_ms),
            token_usage=TokenUsage(),
        )
