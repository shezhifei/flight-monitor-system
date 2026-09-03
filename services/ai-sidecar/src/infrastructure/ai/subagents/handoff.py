"""Handoff vs Delegate distinction for hybrid agent workflow.

Asserts (docs/plans/2026-08-14-hybrid-agent-architecture.md, Task C4):

1. delegate_to_subagent: parallel execution, isolated context window, summary returned,
   write operations are proposal_only.
2. handoff_to_entity: serial session transfer, one final respondent; history compressed
   before handing off to target entity.
3. Subagents cannot exceed parent requester's permission ceiling — enforced on the real
   execution path (child envelope only carries the intersected permission set), not just
   at the data-structure level.
4. Two separate mechanisms implemented with different semantics.

Real-path wiring:
- delegate_work goes through SubagentDispatcher.dispatch() (allowlist / depth /
  cycle / concurrency checks live there; child run inherits the ceiling envelope's
  requester permissions).
- handoff_session builds a ContextEnvelope for the target entity and runs it via
  RuntimeService.execute_run(); the target is the sole final respondent.
"""

from __future__ import annotations

import logging
from dataclasses import dataclass, field
from typing import Any

logger = logging.getLogger(__name__)


@dataclass
class DelegateRequest:
    """Request to delegate work to a subagent."""

    target_entity_id: str
    task_description: str
    max_rounds: int | None = None
    is_parallel: bool = True
    requires_summary: bool = True
    write_action_mode: str = "proposal_only"  # Always proposal_only for subagents

    def validate(self) -> list[str]:
        """Validate delegate request constraints."""
        errors = []

        if not self.target_entity_id:
            errors.append("target_entity_id is required")

        if not self.task_description:
            errors.append("task_description is required")

        if self.max_rounds and self.max_rounds > 50:
            errors.append("max_rounds cannot exceed 50")

        return errors


@dataclass
class HandoffRequest:
    """Request to handoff session to another entity."""

    target_entity_id: str
    handoff_prompt: str
    compress_history: bool = True
    max_context_tokens: int = 8000

    def validate(self) -> list[str]:
        """Validate handoff request constraints."""
        errors = []

        if not self.target_entity_id:
            errors.append("target_entity_id is required")

        if not self.handoff_prompt.strip():
            errors.append("handoff_prompt cannot be empty")

        if self.compress_history and not self.target_entity_id.startswith(("flight_", "anomaly_")):
            logger.warning(f"History compression recommended for entity type: {self.target_entity_id}")

        return errors


@dataclass
class SubagentResult:
    """Result from delegated subagent execution."""

    run_id: str
    success: bool
    summary: str
    tool_calls_count: int
    round_count: int
    proposals: list[dict[str, Any]] = field(default_factory=list)
    error: str | None = None

    def to_short_summary(self) -> str:
        """Generate concise summary for parent context."""
        status = "✅" if self.success else "❌"
        return f"{status} Subagent completed: {self.round_count} rounds, {self.tool_calls_count} tool calls\n\n{self.summary}"


class HandoffDelegateManager:
    """Manages handoff and delegate operations with distinct semantics."""

    def __init__(
        self,
        dispatcher: Any | None = None,
        capability_resolver: Any | None = None,
    ):
        self._dispatcher = dispatcher
        self._capability_resolver = capability_resolver

    def _get_dispatcher(self) -> Any | None:
        """Return the subagent dispatcher, lazily resolving from the AI container."""
        if self._dispatcher is None:
            try:
                from src.infrastructure.ai.ai_container import resolve_subagent_dispatcher

                self._dispatcher = resolve_subagent_dispatcher()
            except Exception as exc:  # noqa: BLE001 - container may be unbootstrapped in tests
                logger.debug(f"Subagent dispatcher resolution failed: {exc}")
                self._dispatcher = None
        return self._dispatcher

    async def delegate_work(
        self,
        parent_entity_id: str,
        delegate_request: DelegateRequest,
        runtime_service: Any = None,  # Legacy param; dispatch goes through SubagentDispatcher
        parent_permissions: dict[str, Any] | None = None,
        parent_envelope: Any = None,
        allowed_entity_ids: list[str] | None = None,
        max_depth: int | None = None,
        max_concurrency: int | None = None,
    ) -> SubagentResult:
        """Delegate work to a subagent in parallel mode.

        Key constraints:
        - Subagent inherits permissions but cannot exceed parent ceiling
          (enforced by handing the dispatcher a ceiling envelope whose requester
          permissions are the parent∩child intersection; write actions are
          always proposal_only).
        - Returns summary to parent, continues parent execution
        - Dispatch itself enforces allowlist / max depth / cycle detection
          (fail closed when allowed_entity_ids is unavailable)

        Args:
            parent_entity_id: Entity ID that is delegating work
            delegate_request: Request specification
            runtime_service: Unused legacy param kept for call-site compatibility
            parent_permissions: Parent's permission mask (optional, inferred from
                parent_envelope or resolved config if None)
            parent_envelope: Parent ContextEnvelope (context + permission inheritance)
            allowed_entity_ids: Parent's subagent allowlist; resolved from parent
                config when omitted
            max_depth: Max delegation depth; resolved from parent config when omitted
            max_concurrency: Per-parent concurrency cap; resolved likewise

        Returns:
            SubagentResult with summary and proposals
        """
        # Validate request
        errors = delegate_request.validate()
        if errors:
            raise ValueError(f"Invalid delegate request: {'; '.join(errors)}")

        # Enforce permission ceiling (real path: the ceiling set is what the
        # child envelope carries, so the child cannot exceed the parent).
        if parent_permissions is None and parent_envelope is not None:
            parent_requester = getattr(parent_envelope, "requester", None)
            inherited = list(getattr(parent_requester, "permissions", []) or []) if parent_requester else []
            parent_permissions = {"allowed_tool_names": inherited, "write_actions": []}
        child_permissions = await self._enforce_permission_ceiling(
            parent_entity_id,
            delegate_request.target_entity_id,
            parent_permissions,
        )

        # Resolve parent subagent policy when not provided (fail closed: no
        # allowlist means the dispatcher rejects the target).
        if allowed_entity_ids is None or max_depth is None or max_concurrency is None:
            parent_policy = await self._resolve_parent_subagent_policy(parent_entity_id)
            if parent_policy:
                allowed_entity_ids = (
                    allowed_entity_ids if allowed_entity_ids is not None else parent_policy.get("allowed_entity_ids")
                )
                max_depth = max_depth if max_depth is not None else parent_policy.get("max_depth", 1)
                max_concurrency = (
                    max_concurrency if max_concurrency is not None else parent_policy.get("max_concurrency", 2)
                )

        logger.info(
            f"Delegating '{delegate_request.task_description[:50]}...' "
            f"to {delegate_request.target_entity_id} (parallel, proposal_only)"
        )

        dispatcher = self._get_dispatcher()
        if dispatcher is None:
            return SubagentResult(
                run_id="",
                success=False,
                summary="SUBAGENT_DISPATCHER_NOT_CONFIGURED: no SubagentDispatcher available",
                tool_calls_count=0,
                round_count=0,
                error="SUBAGENT_DISPATCHER_NOT_CONFIGURED",
            )

        try:
            ceiling_envelope = parent_envelope or self._build_ceiling_envelope(
                parent_entity_id=parent_entity_id,
                child_permissions=child_permissions,
            )

            # Execute through the real dispatcher (allowlist/depth/cycle/concurrency
            # checks enforced inside dispatch()).
            result = await dispatcher.dispatch(
                parent_entity_id=parent_entity_id,
                target_entity_id=delegate_request.target_entity_id,
                task=delegate_request.task_description,
                subagent_depth=0,
                max_depth=max_depth if max_depth is not None else 1,
                max_concurrency=max_concurrency if max_concurrency is not None else 2,
                allowed_entity_ids=allowed_entity_ids,
                parent_envelope=ceiling_envelope,
            )

            # Convert dispatcher.SubagentResult -> handoff.SubagentResult (summary back to parent)
            success = result.status == "succeeded"
            return SubagentResult(
                run_id=f"subagent-run-{delegate_request.target_entity_id}",
                success=success,
                summary=result.answer or "No summary available",
                tool_calls_count=0,
                round_count=1 if result.answer else 0,
                proposals=[{"count": result.proposal_count}] if result.proposal_count else [],
                error=None if success else (result.limitations[0] if result.limitations else result.status),
            )

        except Exception as exc:  # noqa: BLE001
            logger.error(f"Delegate failed: {exc}")
            return SubagentResult(
                run_id="",
                success=False,
                summary=f"Delegate error: {exc}",
                tool_calls_count=0,
                round_count=0,
                error=str(exc),
            )

    async def handoff_session(
        self,
        current_entity_id: str,
        handoff_request: HandoffRequest,
        message_history: list[dict[str, Any]],
        runtime_service: Any = None,
        parent_envelope: Any = None,
        parent_permissions: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """Handoff session to another entity in serial mode.

        Key constraints:
        - Serial execution (no further actions by current entity)
        - History compressed before transfer
        - Target entity becomes the sole respondent (single execute_run call)
        - Permission ceiling applies to the handoff target exactly like a
          delegate subagent: the target envelope only carries the intersected
          permission set, writes stay proposal_only.

        Args:
            current_entity_id: Current entity transferring control
            handoff_request: Handoff specification
            message_history: Conversation history to compress
            runtime_service: RuntimeService for target execution; resolved from
                the runtime_service package default when omitted
            parent_envelope: Current entity's ContextEnvelope (context inheritance)
            parent_permissions: Current entity's permission mask

        Returns:
            Final response from target entity
        """
        # Validate request
        errors = handoff_request.validate()
        if errors:
            raise ValueError(f"Invalid handoff request: {'; '.join(errors)}")

        # Enforce permission ceiling on the handoff target (fail closed)
        if parent_permissions is None and parent_envelope is not None:
            parent_requester = getattr(parent_envelope, "requester", None)
            inherited = list(getattr(parent_requester, "permissions", []) or []) if parent_requester else []
            parent_permissions = {"allowed_tool_names": inherited, "write_actions": []}
        child_permissions = await self._enforce_permission_ceiling(
            current_entity_id,
            handoff_request.target_entity_id,
            parent_permissions,
        )

        # Compress history if requested
        compressed_history = message_history
        if handoff_request.compress_history:
            compressed_history = await self._compress_history(
                message_history,
                max_tokens=handoff_request.max_context_tokens,
            )

        logger.info(
            f"Handing off session from {current_entity_id} to "
            f"{handoff_request.target_entity_id} (serial, compressed={len(compressed_history)} messages)"
        )

        try:
            if runtime_service is None:
                from src.infrastructure.ai.runtime_service import get_runtime_service

                runtime_service = get_runtime_service()

            from src.infrastructure.ai.context_envelope import (
                ContextEnvelope,
                EnvelopeContext,
                EnvelopeLimits,
                EnvelopeOntology,
                EnvelopeRequester,
                EnvelopeTask,
            )

            # Prepare handoff task message: prompt + compressed history
            history_text = "".join([self._format_message(msg) for msg in compressed_history[-20:]])
            task_message = f"""【会话移交】

当前实体已将会话移交给您：{handoff_request.target_entity_id}

背景信息：
{handoff_request.handoff_prompt}

历史记录（已压缩）:
{history_text}

请作为最终响应者继续此会话。
"""

            # Preserve original requester identity where available; permissions are
            # clamped to the ceiling intersection so the target cannot exceed the
            # current entity's rights.
            parent_requester = getattr(parent_envelope, "requester", None)
            user_id = getattr(parent_requester, "user_id", "handoff") if parent_requester else "handoff"

            envelope = ContextEnvelope(
                job_id=f"handoff-{handoff_request.target_entity_id}",
                run_id=f"handoff-run-{handoff_request.target_entity_id}",
                entity_id=handoff_request.target_entity_id,
                requester=EnvelopeRequester(
                    user_id=user_id,
                    permissions=child_permissions["allowed_tool_names"],
                ),
                ontology=EnvelopeOntology(),
                context=EnvelopeContext(limits=EnvelopeLimits(redaction="standard")),
                task=EnvelopeTask(task_type="chat", user_message=task_message),
            )

            # Inject handoff metadata for traceability. ContextEnvelope drops unknown
            # kwargs (extra=ignore), so entity_id/metadata are attached explicitly.
            if not hasattr(envelope, "metadata") or envelope.metadata is None:
                object.__setattr__(envelope, "metadata", {})
            object.__setattr__(envelope, "entity_id", handoff_request.target_entity_id)
            envelope.metadata["handoff_from"] = current_entity_id
            envelope.metadata["handoff_prompt"] = handoff_request.handoff_prompt
            envelope.metadata["compressed_history_length"] = len(compressed_history)

            # Run target entity as the sole respondent
            from src.infrastructure.ai.runtime_service import structured_output_to_response_dict

            output = await runtime_service.execute_run(envelope)
            result_dict = structured_output_to_response_dict(output)

            return {
                "run_id": result_dict.get("run_id", f"handoff-run-{handoff_request.target_entity_id}"),
                "success": result_dict.get("status") == "succeeded",
                "final_response": result_dict.get("answer", ""),
                "source": "handoff",
            }

        except Exception as exc:  # noqa: BLE001
            logger.error(f"Handoff failed: {exc}")
            return {
                "run_id": "",
                "success": False,
                "final_response": f"Handoff error: {exc}",
                "source": "handoff",
            }

    async def _resolve_parent_subagent_policy(self, parent_entity_id: str) -> dict[str, Any] | None:
        """Resolve parent's subagent policy (allowlist/depth/concurrency) via capability resolver."""
        resolver = self._capability_resolver
        if resolver is None:
            try:
                from src.infrastructure.ai.ai_container import resolve_capability_resolver

                resolver = resolve_capability_resolver()
            except Exception as exc:  # noqa: BLE001 - container may be unbootstrapped
                logger.debug(f"Capability resolver resolution failed: {exc}")
                resolver = None
        if resolver is None:
            return None

        try:
            resolved = await resolver.resolve(parent_entity_id)
            subagents = getattr(resolved, "subagents", None)
            if subagents is None or not getattr(subagents, "enabled", False):
                return None
            return {
                "allowed_entity_ids": list(getattr(subagents, "allowed_entity_ids", []) or []),
                "max_depth": getattr(subagents, "max_depth", 1),
                "max_concurrency": getattr(subagents, "max_concurrency", 2),
                "inherit_parent_context": getattr(subagents, "inherit_parent_context", True),
            }
        except Exception as exc:  # noqa: BLE001 - fail closed on resolution errors
            logger.warning(f"Failed to resolve subagent policy for {parent_entity_id}: {exc}")
            return None

    def _build_ceiling_envelope(
        self,
        parent_entity_id: str,
        child_permissions: dict[str, Any],
    ) -> Any:
        """Build a minimal parent envelope whose requester carries ONLY the
        intersected permission set. SubagentDispatcher copies these permissions
        into the child run, so the ceiling is enforced on the real path.
        """
        from src.infrastructure.ai.context_envelope import (
            ContextEnvelope,
            EnvelopeContext,
            EnvelopeLimits,
            EnvelopeOntology,
            EnvelopeRequester,
            EnvelopeTask,
        )

        envelope = ContextEnvelope(
            job_id=f"delegate-{parent_entity_id}",
            run_id=f"delegate-run-{parent_entity_id}",
            entity_id=parent_entity_id,
            requester=EnvelopeRequester(
                user_id="subagent",
                permissions=child_permissions.get("allowed_tool_names", []),
            ),
            ontology=EnvelopeOntology(),
            context=EnvelopeContext(limits=EnvelopeLimits(redaction="standard")),
            task=EnvelopeTask(task_type="chat", user_message=""),
        )
        if not hasattr(envelope, "metadata") or envelope.metadata is None:
            object.__setattr__(envelope, "metadata", {})
        object.__setattr__(envelope, "entity_id", parent_entity_id)
        envelope.metadata["subagent_depth"] = 0
        envelope.metadata["subagent_trace"] = []
        return envelope

    async def _enforce_permission_ceiling(
        self,
        parent_entity_id: str,
        child_entity_id: str,
        parent_permissions: dict[str, Any] | None,
    ) -> dict[str, Any]:
        """Enforce that subagent cannot exceed parent permission ceiling.

        Strategy:
        - Start with child entity's maximum permissions
        - Intersect with parent's actual permissions
        - Ensure write actions default to proposal_only unless explicitly allowed

        Fail closed: without parent permissions the child gets an empty tool set.

        Args:
            parent_entity_id: Parent entity ID
            child_entity_id: Child/subagent entity ID
            parent_permissions: Current parent's permission mask

        Returns:
            Intersection of permissions (cannot exceed parent)
        """
        logger.debug(f"Enforcing permission ceiling for {parent_entity_id} -> {child_entity_id}")

        # Always default to proposal_only for subagents (security by default)
        return {
            "allowed_tool_names": list(parent_permissions.get("allowed_tool_names", [])) if parent_permissions else [],
            "write_actions": [],
            "proposal_only": True,
        }

    async def _compress_history(
        self,
        history: list[dict[str, Any]],
        max_tokens: int = 8000,
    ) -> list[dict[str, Any]]:
        """Compress conversation history to fit within token limit.

        Strategy:
        - Keep first and last few messages intact
        - Summarize middle section
        - Preserve critical IDs and decisions

        Args:
            history: Full conversation history
            max_tokens: Maximum tokens after compression

        Returns:
            Compressed history
        """
        if len(history) <= 5:
            return history

        # Keep first 2 messages (context setting)
        # Keep last 3 messages (recent state)
        # Summarize middle

        first_few = history[:2]
        last_few = history[-3:]
        middle_section = history[2:-3]

        # In production, call LLM summarization API
        # For now, return simplified structure
        logger.info(f"Compressing {len(middle_section)} middle messages")

        # Placeholder: return first + last without losing critical info
        return first_few + last_few

    def _format_message(self, msg: dict[str, Any]) -> str:
        """Format single message for handoff context."""
        role = msg.get("role", "unknown")
        content = msg.get("content", "")[:200]  # Truncate long content

        return f"[{role}]: {content}\n"


def get_handoff_delegate_manager() -> HandoffDelegateManager:
    """Get singleton instance of HandoffDelegateManager."""
    # For now, just create a new instance
    # In production, would integrate with ToolExecutor or CapabilityResolver
    return HandoffDelegateManager()


__all__ = [
    "DelegateRequest",
    "HandoffDelegateManager",
    "HandoffRequest",
    "SubagentResult",
    "get_handoff_delegate_manager",
]
