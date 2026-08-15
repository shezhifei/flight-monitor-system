"""Subagent dispatcher - controlled recursive delegation to child entities.

Security constraints:
- entity_id must be in resolved_config.subagents.allowed_entity_ids
- empty allowed_entity_ids means NO delegation allowed (fail closed)
- max_depth defaults to 1; exceeded returns SUBAGENT_MAX_DEPTH_EXCEEDED
- cycle detection via subagent_trace returns SUBAGENT_CYCLE_DETECTED
- child run inherits tool/MCP/skills/cache policy from child entity config
- write actions remain proposal_only in child runs
- max_concurrency is per-parent-entity: dispatches from the same parent share a semaphore
"""

from __future__ import annotations

import asyncio
import logging
import threading
from collections.abc import Awaitable, Callable
from dataclasses import dataclass, field
from typing import (
    TYPE_CHECKING,
    Any,
)

if TYPE_CHECKING:  # pragma: no cover - typing only
    from src.infrastructure.ai.llm_stream_runner import StreamEvent

logger = logging.getLogger(__name__)

# Callback invoked (best-effort, out of band) for every child StreamEvent so the
# parent run can bubble sub-agent events up to its own SSE stream. The blocking
# SubagentResult semantics are unaffected: forwarding is a pure side channel.
OnChildEvent = Callable[["StreamEvent"], Awaitable[None]]

SUBAGENT_TOOL_SCHEMA: dict[str, Any] = {
    "type": "function",
    "function": {
        "name": "delegate_to_subagent",
        "description": (
            "Delegate a task to a sub-entity agent. "
            "Only entity_ids listed in the allowed_entity_ids may be used. "
            "The sub-agent will resolve its own capabilities and execute independently."
        ),
        "parameters": {
            "type": "object",
            "properties": {
                "entity_id": {
                    "type": "string",
                    "description": "Target entity_id to delegate to",
                },
                "task": {
                    "type": "string",
                    "description": "Task description for the sub-agent",
                },
                "context_summary": {
                    "type": "string",
                    "description": "Optional summary of relevant parent context",
                },
                "expected_output": {
                    "type": "string",
                    "description": "Optional description of expected output format",
                },
            },
            "required": ["entity_id", "task"],
        },
    },
}


@dataclass
class SubagentResult:
    """Structured result returned to the parent LLM."""

    entity_id: str
    status: str  # "succeeded" or "failed"
    answer: str
    limitations: list[str] = field(default_factory=list)
    proposal_count: int = 0


class SubagentDispatcher:
    """Dispatches tasks to child entity agents via RuntimeService.

    This is a same-process controlled recursive call, not a multi-process agent pool.
    Concurrency is controlled by per-parent-entity semaphores: dispatches from the
    same parent_entity_id with the same max_concurrency share a semaphore, so the
    per-call max_concurrency from resolved_config is respected.
    """

    def __init__(
        self,
        runtime_service_factory: Callable | None = None,
        capability_resolver: Any = None,
        max_concurrency: int = 2,
    ):
        self._runtime_service_factory = runtime_service_factory
        self._capability_resolver = capability_resolver
        # Default concurrency for backward compat (tests that don't pass per-call value)
        self._default_max_concurrency = max(1, max_concurrency)
        # Semaphore pool: (parent_entity_id, max_concurrency) -> asyncio.Semaphore
        self._semaphore_pool: dict[tuple[str, int], asyncio.Semaphore] = {}
        self._semaphore_pool_guard = threading.Lock()

    def _get_semaphore(self, parent_entity_id: str, max_concurrency: int) -> asyncio.Semaphore:
        """Get or create a semaphore for the given parent entity and concurrency limit."""
        effective = max(1, max_concurrency)
        key = (parent_entity_id, effective)
        with self._semaphore_pool_guard:
            if key not in self._semaphore_pool:
                self._semaphore_pool[key] = asyncio.Semaphore(effective)
            return self._semaphore_pool[key]

    async def dispatch(
        self,
        parent_entity_id: str,
        target_entity_id: str,
        task: str,
        context_summary: str | None = None,
        expected_output: str | None = None,
        subagent_depth: int = 0,
        subagent_trace: list[str] | None = None,
        max_depth: int = 1,
        max_concurrency: int = 2,
        allowed_entity_ids: list[str] | None = None,
        inherit_parent_context: bool = True,
        parent_envelope: Any = None,
        on_child_event: OnChildEvent | None = None,
    ) -> SubagentResult:
        """Dispatch a task to a child entity.

        Args:
            parent_entity_id: The entity that initiated the delegation
            target_entity_id: The entity to delegate to
            task: Task description
            context_summary: Optional parent context summary
            expected_output: Optional expected output description
            subagent_depth: Current depth in the subagent chain
            subagent_trace: List of entity_ids in the current call chain
            max_depth: Maximum allowed depth
            max_concurrency: Maximum concurrent subagent calls for this parent entity
            allowed_entity_ids: Entity IDs allowed by parent config; empty = none allowed
            inherit_parent_context: Whether to copy parent envelope context
            parent_envelope: The parent ContextEnvelope
            on_child_event: Optional async callback. When provided, the child run is
                streamed and every child StreamEvent is forwarded (out of band) after
                stamping subagent_depth/parent_run_id into its metadata. This does NOT
                change the blocking SubagentResult returned to the caller.

        Returns:
            SubagentResult with status, answer, limitations, proposal_count
        """
        # Check depth
        if subagent_depth >= max_depth:
            return SubagentResult(
                entity_id=target_entity_id,
                status="failed",
                answer="SUBAGENT_MAX_DEPTH_EXCEEDED: Maximum subagent delegation depth reached",
                limitations=["SUBAGENT_MAX_DEPTH_EXCEEDED"],
            )

        # Check allowlist - FAIL CLOSED: empty list means no entities allowed
        if allowed_entity_ids is None or target_entity_id not in allowed_entity_ids:
            return SubagentResult(
                entity_id=target_entity_id,
                status="failed",
                answer=f"SUBAGENT_ENTITY_NOT_ALLOWED: entity '{target_entity_id}' is not in allowed_entity_ids",
                limitations=["SUBAGENT_ENTITY_NOT_ALLOWED"],
            )

        # Check cycle
        trace = list(subagent_trace or [])
        if target_entity_id in trace:
            return SubagentResult(
                entity_id=target_entity_id,
                status="failed",
                answer=f"SUBAGENT_CYCLE_DETECTED: entity '{target_entity_id}' is already in the call chain",
                limitations=["SUBAGENT_CYCLE_DETECTED"],
            )
        trace.append(target_entity_id)

        # Concurrency control via per-parent-entity semaphore
        sem = self._get_semaphore(parent_entity_id, max_concurrency)
        async with sem:
            return await self._execute_child_run(
                target_entity_id=target_entity_id,
                task=task,
                context_summary=context_summary,
                expected_output=expected_output,
                subagent_depth=subagent_depth + 1,
                subagent_trace=trace,
                inherit_parent_context=inherit_parent_context,
                parent_envelope=parent_envelope,
                on_child_event=on_child_event,
            )

    async def _execute_child_run(
        self,
        target_entity_id: str,
        task: str,
        context_summary: str | None,
        expected_output: str | None,
        subagent_depth: int,
        subagent_trace: list[str],
        inherit_parent_context: bool,
        parent_envelope: Any,
        on_child_event: OnChildEvent | None = None,
    ) -> SubagentResult:
        """Execute a child run using RuntimeService.

        When ``on_child_event`` is supplied and the child RuntimeService exposes a
        StreamEvent-yielding streaming method, the child run is streamed so every
        event can be bubbled up to the parent. Otherwise we fall back to the
        non-streaming execute_run path (no forwarding). Either way the returned
        SubagentResult is the same synchronous, fully-resolved result.
        """
        if not self._runtime_service_factory:
            return SubagentResult(
                entity_id=target_entity_id,
                status="failed",
                answer="SUBAGENT_DISPATCHER_NOT_CONFIGURED: runtime_service_factory not set",
                limitations=["SUBAGENT_DISPATCHER_NOT_CONFIGURED"],
            )

        try:
            from src.infrastructure.ai.context_envelope import (
                ContextEnvelope,
                EnvelopeContext,
                EnvelopeLimits,
                EnvelopeOntology,
                EnvelopeRequester,
                EnvelopeTask,
            )

            # Build child envelope
            parent_requester = getattr(parent_envelope, "requester", None)
            user_id = getattr(parent_requester, "user_id", "subagent") if parent_requester else "subagent"
            parent_permissions = list(getattr(parent_requester, "permissions", []) or []) if parent_requester else []
            parent_run_id = getattr(parent_envelope, "run_id", None)

            # Build task message
            task_message = task
            if context_summary:
                task_message = f"[Parent context summary]\n{context_summary}\n\n[Task]\n{task}"
            if expected_output:
                task_message += f"\n\n[Expected output]\n{expected_output}"

            child_envelope = ContextEnvelope(
                job_id=f"subagent-{target_entity_id}",
                run_id=f"subagent-run-{target_entity_id}",
                entity_id=target_entity_id,
                requester=EnvelopeRequester(user_id=user_id, permissions=parent_permissions),
                ontology=EnvelopeOntology(),
                context=EnvelopeContext(limits=EnvelopeLimits(redaction="standard")),
                task=EnvelopeTask(task_type="chat", user_message=task_message),
            )

            # Inject subagent metadata for depth/trace propagation
            # Use object.__setattr__ to bypass Pydantic model validation
            if not hasattr(child_envelope, "metadata"):
                object.__setattr__(child_envelope, "metadata", {})
            elif child_envelope.metadata is None:
                child_envelope.metadata = {}
            child_envelope.metadata["subagent_depth"] = subagent_depth
            child_envelope.metadata["subagent_trace"] = subagent_trace
            # ContextEnvelope drops unknown kwargs (extra=ignore), so entity_id must be
            # attached explicitly for the runtime to resolve the CHILD entity config
            # (getattr(envelope, "entity_id", ...) in RuntimeService), per docstring:
            # "child run inherits tool/MCP/skills/cache policy from child entity config".
            object.__setattr__(child_envelope, "entity_id", target_entity_id)

            # Get runtime service for child entity (factory injects dispatcher into child)
            runtime_service = self._runtime_service_factory(target_entity_id)
            if not runtime_service:
                return SubagentResult(
                    entity_id=target_entity_id,
                    status="failed",
                    answer=f"SUBAGENT_RUNTIME_SERVICE_UNAVAILABLE: could not create RuntimeService for '{target_entity_id}'",
                    limitations=["SUBAGENT_RUNTIME_SERVICE_UNAVAILABLE"],
                )

            # Execute child run. When the caller wants event bubbling AND the child
            # service can stream raw StreamEvents, take the streaming path so each
            # event can be forwarded; otherwise fall back to the blocking run.
            stream_events = getattr(runtime_service, "stream_run_events", None)
            if on_child_event is not None and callable(stream_events):
                return await self._stream_child_run(
                    runtime_service=runtime_service,
                    child_envelope=child_envelope,
                    target_entity_id=target_entity_id,
                    subagent_depth=subagent_depth,
                    parent_run_id=parent_run_id,
                    on_child_event=on_child_event,
                )

            from src.infrastructure.ai.runtime_service import structured_output_to_response_dict

            output = await runtime_service.execute_run(child_envelope)

            result_dict = structured_output_to_response_dict(output)
            proposal_count = len(result_dict.get("proposals", []))

            return SubagentResult(
                entity_id=target_entity_id,
                status=result_dict.get("status", "failed"),
                answer=result_dict.get("answer", ""),
                limitations=result_dict.get("limitations", []),
                proposal_count=proposal_count,
            )

        except Exception as exc:  # noqa: BLE001 - subagent dispatch fallback must catch all errors to avoid parent run failure
            logger.error(f"Subagent dispatch failed for {target_entity_id}: {exc}")
            return SubagentResult(
                entity_id=target_entity_id,
                status="failed",
                answer=f"SUBAGENT_EXECUTION_ERROR: {exc}",
                limitations=["SUBAGENT_EXECUTION_ERROR"],
            )

    async def _stream_child_run(
        self,
        runtime_service: Any,
        child_envelope: Any,
        target_entity_id: str,
        subagent_depth: int,
        parent_run_id: str | None,
        on_child_event: OnChildEvent,
    ) -> SubagentResult:
        """Stream a child run, bubbling each StreamEvent to ``on_child_event``.

        The dispatch contract stays synchronous: we fully drain the child stream and
        return a resolved SubagentResult assembled from the terminal event. Each
        forwarded event is stamped (in its ``metadata``) with the child's
        subagent_depth and the parent run id so the parent can attribute it.
        Forwarding is best-effort: a failing callback must not abort the child run.
        """
        answer_text = ""
        proposals: list[Any] = []
        limitations: list[str] = []
        status = "failed"

        async for event in runtime_service.stream_run_events(child_envelope):
            # Stamp attribution metadata onto the event. StreamEvent is a plain
            # dataclass without a metadata field, so attach/merge one dynamically.
            meta = getattr(event, "metadata", None)
            if not isinstance(meta, dict):
                meta = {}
            meta["subagent_depth"] = subagent_depth
            meta["parent_run_id"] = parent_run_id
            try:
                event.metadata = meta
            except Exception as exc:  # pragma: no cover - exotic immutable event types  # noqa: BLE001 - defensive fallback for exotic immutable event types
                logger.debug("subagent_event_metadata_setattr_failed", exc_info=exc)
                object.__setattr__(event, "metadata", meta)

            # Out-of-band forwarding; never let a sink error abort the child run.
            try:
                await on_child_event(event)
            except Exception as exc:  # pragma: no cover - defensive  # noqa: BLE001 - defensive: callback failure must not abort child run
                logger.warning(f"on_child_event callback failed for subagent {target_entity_id}: {exc}")

            # Assemble the blocking result from the terminal completion event.
            if getattr(event, "type", None) == "completed":
                result = getattr(event, "result", None)
                if result is not None and getattr(result, "text", None):
                    answer_text = result.text
                status = "succeeded"
            elif getattr(event, "type", None) == "error":
                limitations.append("SUBAGENT_STREAM_ERROR")
                status = "failed"

        return SubagentResult(
            entity_id=target_entity_id,
            status=status,
            answer=answer_text,
            limitations=limitations,
            proposal_count=len(proposals),
        )


__all__ = [
    "SUBAGENT_TOOL_SCHEMA",
    "SubagentDispatcher",
    "SubagentResult",
]
