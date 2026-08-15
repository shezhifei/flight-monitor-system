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


def _extract_tool_object_id(result: Any) -> str:
    """Best-effort object_id extraction from a tool result for evidence provenance."""
    if isinstance(result, dict):
        for key in ("object_id", "flight_id", "anomaly_id", "proposal_id", "order_id", "id"):
            value = result.get(key)
            if value:
                return str(value)
    return ""


async def _publish_stream_checkpoint(
    *,
    run_id: str,
    envelope: Any,
    checkpoint_type: str,
    round_index: int,
    snapshot: dict[str, Any],
) -> None:
    """D1: publish a durable ``checkpoint`` event over RocketMQ.

    Best-effort: when the MQ control plane is not wired (or the publish
    fails after retries) the run continues — resume then falls back to an
    earlier checkpoint. Never raises into the streaming loop; the SSE-only
    ``on_child_event`` path remains the UI channel.
    """
    try:
        from src.infrastructure.ai.runtime_service._mq_publish import (
            _publish_checkpoint_mq,
            _resolve_mq_gate,
            _resolve_mq_publisher,
        )

        publisher = _resolve_mq_publisher()
        if publisher is None:
            return
        job_id = getattr(envelope, "job_id", "") if envelope is not None else ""
        if not isinstance(job_id, str):
            job_id = ""
        await _publish_checkpoint_mq(
            publisher,
            _resolve_mq_gate(),
            run_id=run_id,
            job_id=job_id,
            round_index=round_index,
            checkpoint_type=checkpoint_type,
            snapshot=snapshot,
        )
    except Exception:  # noqa: BLE001 - checkpoint publish must never break the stream
        logger.debug("checkpoint MQ publish skipped", exc_info=True)


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
        working_memory=None,
        hook_pipeline=None,
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

        ``working_memory`` (Task B2): optional :class:`WorkingMemory` workspace. Tool results
        above its spill threshold are written to ``evidence.json`` and the model receives only
        ``summary + pointer`` instead of the raw payload.

        ``hook_pipeline`` (Task C2): optional :class:`HookPipeline`. When provided,
        ``PreToolUse`` hooks gate every tool call (a denied call becomes an error
        result fed back to the model and never reaches the executor),
        ``PostToolUse`` hooks sanitize/spill each tool result, and ``Stop``
        hooks screen the final answer (a flagged answer is annotated with a
        warning marker, since its tokens were already streamed).
        """
        from src.infrastructure.ai.tools.tool_executor import ToolExecutionResult, ToolExecutor

        executor = self._tool_executor or ToolExecutor()

        # C2: lifecycle hook context type, imported once when a pipeline is wired.
        HookContext = None
        if hook_pipeline is not None:
            from src.infrastructure.ai.hooks.pipeline import HookContext as _HookContext

            HookContext = _HookContext

        # Extract the set of tool names that were presented to the LLM.
        allowed_tool_names: set[str] | None = None
        if tools:
            allowed_tool_names = set()
            for t in tools:
                fn = t.get("function", {}) if isinstance(t, dict) else {}
                name = fn.get("name") if isinstance(fn, dict) else None
                if name:
                    allowed_tool_names.add(name)

        # P0-6-A: Enforce global hard cap (never exceed 50 rounds regardless of template)
        GLOBAL_MAX_TOOL_ROUNDS = 50
        
        current_messages = list(messages)
        last_round_index = 0
        consecutive_failures = 0
        run_cancelled = False
        effective_max_rounds = min(max_tool_rounds or GLOBAL_MAX_TOOL_ROUNDS, GLOBAL_MAX_TOOL_ROUNDS)

        for round_index in range(effective_max_rounds):
            last_round_index = round_index
            result = StreamCompletionResult(model=model)

            # P0-6-D: Check cancellation before LLM API call.
            # ``is True`` (not truthiness) so mock/stub envelopes that
            # auto-materialize attributes never read as cancelled.
            if envelope and getattr(envelope, "cancelled", False) is True:
                logger.info(f"[P0-6-D] Request cancelled before LLM API call, stopping round {round_index}")
                run_cancelled = True
                yield StreamEvent(
                    type="cancelled",
                    round_index=last_round_index,
                )
                break

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

            # D1: durable before_tool checkpoint over MQ (once per round),
            # carrying the working-memory snapshot for resume.
            await _publish_stream_checkpoint(
                run_id=run_id,
                envelope=envelope,
                checkpoint_type="before_tool",
                round_index=round_index,
                snapshot={
                    "round_index": round_index,
                    "tool_calls": parsed_calls,
                    "messages_count": len(current_messages),
                    "working_memory": working_memory.to_dict() if working_memory is not None else {},
                    "timestamp": time.time(),
                },
            )

            # C2: PreToolUse lifecycle hooks (schema / object existence / lease
            # preflight). A denied call never reaches the executor; it becomes
            # an error result fed back to the model.
            hook_blocked_results: list[ToolExecutionResult] = []
            calls_for_execution = parsed_calls
            if hook_pipeline is not None:
                calls_for_execution = []
                for pc in parsed_calls:
                    raw_args = pc.get("arguments")
                    try:
                        hook_args = json.loads(raw_args) if isinstance(raw_args, str) else dict(raw_args or {})
                    except (ValueError, TypeError):
                        hook_args = {}
                    hook_ctx = HookContext(
                        phase="PreToolUse",
                        run_id=run_id,
                        tool_name=pc.get("tool_name") or "",
                        tool_args=hook_args,
                        envelope=envelope,
                        working_memory=working_memory,
                        mq_gate=getattr(executor, "mq_gate", None),
                    )
                    if await hook_pipeline.execute_phase("PreToolUse", hook_ctx):
                        calls_for_execution.append(pc)
                    else:
                        reason = "; ".join(hook_ctx.errors) or "blocked by PreToolUse hook"
                        logger.warning(
                            f"Tool call {pc.get('tool_name')} blocked by PreToolUse hooks, run={run_id}: {reason}"
                        )
                        hook_blocked_results.append(
                            ToolExecutionResult(
                                tool_call_id=pc.get("tool_call_id", ""),
                                tool_name=pc.get("tool_name", ""),
                                success=False,
                                error=f"HOOK_BLOCKED: {reason}",
                            )
                        )

            # C1: plan-board tools are no-op planning primitives executed
            # in-process against the run's WorkingMemory (plan.md) — they never
            # go through the executor or the MQ gate.
            from src.infrastructure.ai.tools.plan_tools import execute_plan_tool, is_plan_tool
            from src.infrastructure.ai.tools.skill_tools import SKILL_TOOL_NAMES

            plan_calls = [pc for pc in calls_for_execution if is_plan_tool(pc.get("tool_name") or "")]
            skill_calls = [
                pc
                for pc in calls_for_execution
                if (pc.get("tool_name") or "") in SKILL_TOOL_NAMES
            ]
            executor_calls = [
                pc
                for pc in calls_for_execution
                if not is_plan_tool(pc.get("tool_name") or "")
                and (pc.get("tool_name") or "") not in SKILL_TOOL_NAMES
            ]

            # C3: load_skill / read_skill_reference are read-only local file
            # reads against allowlisted skill roots — executed in-process like
            # the plan tools, never through the executor or the MQ gate.
            skill_results: list[ToolExecutionResult] = []
            if skill_calls:
                from src.infrastructure.ai.tools.skill_tools import SkillProgressiveDiscloser

                discloser = SkillProgressiveDiscloser()
                for pc in skill_calls:
                    raw_args = pc.get("arguments")
                    try:
                        skill_args = json.loads(raw_args) if isinstance(raw_args, str) else dict(raw_args or {})
                    except (ValueError, TypeError):
                        skill_args = {}
                    result_dict = await discloser.execute_tool(pc.get("tool_name") or "", skill_args)
                    succeeded = bool(result_dict.get("success"))
                    skill_results.append(
                        ToolExecutionResult(
                            tool_call_id=pc.get("tool_call_id", ""),
                            tool_name=pc.get("tool_name", ""),
                            success=succeeded,
                            result=result_dict if succeeded else None,
                            error=None if succeeded else result_dict.get("error", "skill tool failed"),
                        )
                    )

            plan_results: list[ToolExecutionResult] = []
            for pc in plan_calls:
                raw_args = pc.get("arguments")
                try:
                    plan_args = json.loads(raw_args) if isinstance(raw_args, str) else dict(raw_args or {})
                except (ValueError, TypeError):
                    plan_args = {}
                plan_envelope = envelope
                if plan_envelope is None:
                    from types import SimpleNamespace

                    plan_envelope = SimpleNamespace(run_id=run_id, task=SimpleNamespace(task_type="general"))
                result_dict = await execute_plan_tool(
                    pc.get("tool_name") or "",
                    plan_args,
                    envelope=plan_envelope,
                    working_memory=working_memory,
                )
                succeeded = str(result_dict.get("status", "")) in ("succeeded", "success")
                plan_results.append(
                    ToolExecutionResult(
                        tool_call_id=pc.get("tool_call_id", ""),
                        tool_name=pc.get("tool_name", ""),
                        success=succeeded,
                        result=result_dict if succeeded else None,
                        error=None if succeeded else result_dict.get("answer", "plan tool failed"),
                    )
                )

            execution_results = []
            if executor_calls:
                execution_results = await executor.execute_batch(
                    executor_calls,
                    run_id=run_id,
                    envelope=envelope,
                    cache_policy=tool_cache_policy,
                    on_child_event=on_child_event,
                    entity_id=entity_id,
                    allowed_tool_names=allowed_tool_names,
                    round_index=round_index,
                    # P0-6-C: Pass run_id as run_id, not mixed with job_id
                    # Job = long-running experiment (days); Run = single execution (minutes)
                    job_id=getattr(envelope, "job_id", None) or run_id,  # Use envelope's job_id if available
                )
            execution_results = plan_results + skill_results + list(execution_results) + hook_blocked_results

            # P0-6-D: Check cancellation after tool response (identity check,
            # see the pre-call guard above for the rationale).
            if envelope and getattr(envelope, "cancelled", False) is True:
                logger.info(f"[P0-6-D] Request cancelled after tool response at round {round_index}, stopping")
                run_cancelled = True
                yield StreamEvent(
                    type="cancelled",
                    round_index=last_round_index,
                )
                break

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

            # D1: durable after_tool checkpoint over MQ. This is the primary
            # resume point, so the snapshot carries the working memory and
            # the tool results needed to rebuild the transcript summary.
            await _publish_stream_checkpoint(
                run_id=run_id,
                envelope=envelope,
                checkpoint_type="after_tool",
                round_index=round_index,
                snapshot={
                    "round_index": round_index,
                    "tool_calls_executed": len(parsed_calls),
                    "results": [er.to_sse_payload() for er in execution_results],
                    "messages_count": len(current_messages),
                    "working_memory": working_memory.to_dict() if working_memory is not None else {},
                    "timestamp": time.time(),
                },
            )

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
            else:
                # P0-6-B: Reset failure counter after successful round
                consecutive_failures = 0

            tool_results_for_llm: list[dict[str, Any]] = []
            for exec_result in execution_results:
                payload = exec_result.to_sse_payload()
                payload["run_id"] = run_id
                yield StreamEvent(
                    type="tool_result",
                    tool_call=payload,
                )
                # C2: PostToolUse hooks (result sanitization, sensitive-field
                # stripping, oversized-content spill into working memory).
                hook_content_override: str | None = None
                if hook_pipeline is not None:
                    if isinstance(exec_result.result, dict):
                        hook_result = exec_result.result  # mutated in place
                    elif exec_result.result is not None:
                        hook_result = {"content": json.dumps(exec_result.result, default=str)}
                    else:
                        hook_result = {"content": f"Error: {exec_result.error}"}
                    post_ctx = HookContext(
                        phase="PostToolUse",
                        run_id=run_id,
                        tool_name=exec_result.tool_name,
                        tool_result=hook_result,
                        envelope=envelope,
                        working_memory=working_memory,
                    )
                    await hook_pipeline.execute_phase("PostToolUse", post_ctx)
                    if not isinstance(exec_result.result, dict):
                        overridden = hook_result.get("content")
                        if isinstance(overridden, str):
                            hook_content_override = overridden
                if hook_content_override is not None:
                    content = hook_content_override
                else:
                    content = (
                        json.dumps(exec_result.result) if exec_result.result is not None else f"Error: {exec_result.error}"
                    )
                # B2: large results spill into the working-memory workspace; the
                # model only receives summary + pointer (full text in evidence.json).
                if (
                    working_memory is not None
                    and exec_result.result is not None
                    and working_memory.should_spill(content)
                ):
                    content = working_memory.spill_tool_result(
                        tool_name=exec_result.tool_name,
                        content=content,
                        object_id=_extract_tool_object_id(exec_result.result),
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
                    # B2: working memory snapshot rides the existing checkpoint
                    # payload (ai_run_checkpoints JSONB); resume restores it via
                    # ResumeContext.working_memory.
                    "working_memory": working_memory.to_dict() if working_memory is not None else {},
                },
                # Include pending tool results for resume
                "pending_results": [er.to_sse_payload() for er in execution_results] if execution_results else [],
            }
            if on_child_event:
                await on_child_event(checkpoint_before_proposal)

            # D1: durable before_proposal checkpoint over MQ (mapped to the
            # Rust ``before_proposal_ingest`` checkpoint type).
            await _publish_stream_checkpoint(
                run_id=run_id,
                envelope=envelope,
                checkpoint_type="before_proposal",
                round_index=round_index,
                snapshot={
                    **checkpoint_before_proposal["context_snapshot"],
                    "round_index": round_index,
                    "pending_results": checkpoint_before_proposal["pending_results"],
                },
            )

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

        # D4: a cancelled run stops at the round boundary — no Stop-hook
        # screening, no after_completion checkpoint, no "completed" event.
        # The caller (_streaming_tools) emits the terminal run.fail /
        # RUN_CANCELLED frame instead.
        if run_cancelled:
            return

        # C2: Stop hooks (NoPromises / OutputGuardrail) screen the final answer.
        # Tokens were already streamed, so a flagged answer is annotated with a
        # warning marker rather than suppressed.
        if hook_pipeline is not None:
            stop_messages = [
                m if isinstance(m, dict) else m.to_dict() for m in current_messages
            ]
            if result is not None and result.text:
                stop_messages.append({"role": "assistant", "content": result.text})
            stop_ctx = HookContext(
                phase="Stop",
                run_id=run_id,
                messages=stop_messages,
                envelope=envelope,
                working_memory=working_memory,
            )
            if not await hook_pipeline.execute_phase("Stop", stop_ctx):
                reason = "; ".join(stop_ctx.errors) or "blocked by Stop hook"
                logger.warning(f"Stop hooks flagged final answer for run={run_id}: {reason}")
                result.text = (result.text or "") + f"\n\n---\n⚠️ 输出安全钩子拦截：{reason}"

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

        # D1: durable after_completion checkpoint over MQ.
        await _publish_stream_checkpoint(
            run_id=run_id,
            envelope=envelope,
            checkpoint_type="after_completion",
            round_index=last_round_index,
            snapshot={
                "round_index": last_round_index,
                "final_result": {"text": result.text, "tool_calls_count": len(result.tool_calls)} if result else {},
                "messages_count": len(current_messages),
                "working_memory": working_memory.to_dict() if working_memory is not None else {},
                "timestamp": time.time(),
            },
        )

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
