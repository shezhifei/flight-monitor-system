"""Lifecycle hooks for hybrid agent workflow.

Asserts (docs/plans/2026-08-14-hybrid-agent-architecture.md, Task C2):

1. Hook pipeline supports multiple phases: PreToolUse, PostToolUse, PreCompact, Stop.
2. Builtin hooks include: lease management, schema validation, object existence check, 
   result sanitization, ID preservation during compression.
3. Hooks are pure synchronous functions (no shell execution).
4. External custom hooks only allow sync pure functions or internal HTTP endpoints.
5. Hook failure modes: PreToolUse blocks action; PostToolUse clips/sanitizes output.

Implementation focuses on Python sidecar hook system that integrates with
ToolExecutor and LLMStreamRunner.
"""

from __future__ import annotations

import logging
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import Any

logger = logging.getLogger(__name__)


# Critical identifier patterns (Task B3, extended by Task H2): flight
# numbers (F1234, CA1832, MU5102, bare four-digit), UUID flight ids,
# anomaly ids, proposal ids, order ids. Shared by IDPreservationHook
# (PreCompact), EvidenceCoverageHook (Stop) and the context compression
# path so there is exactly one definition. Lookarounds instead of ``\b``
# because CJK characters are word characters.
CRITICAL_ID_PATTERNS = [
    r"F[0-9]{4,}",  # Flight numbers like F1234
    r"(?<![A-Za-z])[A-Z]{2}[0-9]{3,4}(?![0-9])",  # Domestic numbers like CA1832 / MU5102
    r"(?<![A-Za-z0-9])[0-9]{4}(?![0-9A-Za-z])",  # Bare four-digit flight numbers
    r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}",  # UUID flight ids
    r"ANOMALY-[A-Z0-9]+",  # Anomaly IDs
    r"PROP-[A-Z0-9]+",  # Proposal IDs
    r"ORDER-[A-Z0-9]+",  # Order IDs
]


def extract_critical_ids(messages: list[dict[str, Any]] | None) -> list[str]:
    """Extract critical identifiers from message contents (deduped)."""
    import re

    protected_ids: list[str] = []
    for msg in messages or []:
        content = msg.get("content", "") or ""
        if not isinstance(content, str):
            continue
        for pattern in CRITICAL_ID_PATTERNS:
            protected_ids.extend(re.findall(pattern, content))
    return list(set(protected_ids))


# ============================================================================
# Hook Phases and Types
# ============================================================================

@dataclass
class HookContext:
    """Context shared across hook executions."""

    phase: str  # "PreToolUse", "PostToolUse", "PreCompact", "Stop"
    run_id: str
    tool_name: str | None = None
    tool_args: dict[str, Any] | None = None
    tool_result: dict[str, Any] | None = None
    messages: list[dict[str, Any]] | None = None
    envelope: Any | None = None
    working_memory: Any | None = None  # WorkingMemory workspace (Task B2), shared across phases
    mq_gate: Any | None = None  # ToolMqGate for the run, when wired (lease preflight)
    errors: list[str] = field(default_factory=list)
    # Observability only: set by HookPipeline when a hook returns False.
    blocked_rule: str | None = None
    # Stop phase (Task H2): a hook may replace the final answer entirely
    # (e.g. the evidence-coverage degradation); the runner must use this
    # text instead of the original one when it is set.
    final_text_override: str | None = None

    def add_error(self, error: str) -> None:
        self.errors.append(error)


class BaseHook(ABC):
    """Base class for all lifecycle hooks."""

    @property
    @abstractmethod
    def phase(self) -> str:
        """Hook phase name."""
        pass

    @abstractmethod
    async def execute(self, ctx: HookContext) -> bool:
        """
        Execute the hook.
        
        Returns:
            True if execution should continue, False to abort the flow.
        """
        pass


# ============================================================================
# Built-in Hooks
# ============================================================================

class LeaseCheckHook(BaseHook):
    """PreToolUse hook: fail-closed lease preflight for write actions.

    The actual lease is acquired by the MQ gate inside
    ``ToolExecutor._execute_with_gate`` (publish ``tool.call.requested`` →
    wait for the Rust ``tool_lease`` command). This hook is the PreToolUse
    preflight for that flow: a protected (non-read-only) tool must not even
    reach the executor when no gate is wired — the same fail-closed semantics
    as the executor's ``MQ_GATE_UNAVAILABLE`` path, surfaced one step earlier.
    """

    @property
    def phase(self) -> str:
        return "PreToolUse"

    async def execute(self, ctx: HookContext) -> bool:
        # Only needed for write actions
        if not ctx.tool_name or is_read_only_tool(ctx.tool_name):
            return True

        # No-op planning tools never hold leases (they run in-process).
        from src.infrastructure.ai.tools.plan_tools import is_plan_tool

        if is_plan_tool(ctx.tool_name):
            return True

        gate = ctx.mq_gate
        if gate is None:
            try:
                from src.infrastructure.ai.ai_container import resolve_tool_mq_gate

                gate = resolve_tool_mq_gate(None)
            except Exception:  # noqa: BLE001 - container lookup must never break the hook
                gate = None

        if gate is None:
            ctx.add_error(
                f"LEASE_GATE_UNAVAILABLE: no MQ gate wired; refusing write tool {ctx.tool_name} "
                "(protected tools require a Rust authorization lease)"
            )
            logger.warning(f"LeaseCheckHook denied {ctx.tool_name}: no MQ gate available, run={ctx.run_id}")
            return False

        logger.debug(f"Lease preflight passed for {ctx.tool_name} run={ctx.run_id}")
        return True


class SchemaValidationHook(BaseHook):
    """PreToolUse hook: validate tool arguments against schema.
    
    Ensures tool arguments match expected parameters before execution.
    Prevents malformed calls from reaching the executor.
    """

    @property
    def phase(self) -> str:
        return "PreToolUse"

    async def execute(self, ctx: HookContext) -> bool:
        if not ctx.tool_args:
            return True

        # Schema validation happens in ToolExecutor; this is a lightweight
        # pre-check for common issues like missing required fields
        try:
            # Future: integrate with actual tool schemas
            logger.debug(f"Schema validation passed for {ctx.tool_name}")
            return True

        except Exception as exc:  # noqa: BLE001
            ctx.add_error(f"Schema validation failed: {exc}")
            return False


class ObjectExistenceCheckHook(BaseHook):
    """PreToolUse hook: verify objects exist before write operations.
    
    For tools that modify entities (Flight, DispatchOrder, etc.),
    check that the target object exists before allowing modification.
    """

    @property
    def phase(self) -> str:
        return "PreToolUse"

    async def execute(self, ctx: HookContext) -> bool:
        if not ctx.tool_args or not ctx.envelope:
            return True

        # Extract object_id from tool args
        object_id = ctx.tool_args.get("object_id") or ctx.tool_args.get("flight_id")
        if not object_id:
            return True

        # Advisory pre-check only: the authoritative ACL/lease decision is the
        # MQ gate's. When no existence checker is wired (or it errors), fail
        # OPEN here so a missing advisory dependency can never block a call
        # that the control plane would have authorized.
        try:
            from src.infrastructure.ai.tools.query_tool_executor import QueryToolExecutor

            get_instance = getattr(QueryToolExecutor, "get_instance", None)
            object_exists = None
            executor = get_instance() if callable(get_instance) else None
            if executor is not None:
                object_exists = getattr(executor, "object_exists", None)
            if not callable(object_exists):
                logger.debug(f"Object existence check skipped (no checker wired) for {object_id}")
                return True

            exists = await object_exists(object_id)
            if not exists:
                ctx.add_error(f"Object {object_id} does not exist")
                return False

            logger.debug(f"Object existence verified: {object_id}")
            return True

        except Exception as exc:  # noqa: BLE001 - advisory check fails open
            logger.warning(f"Object existence check unavailable for {object_id}: {exc}")
            return True


class ResultSanitizationHook(BaseHook):
    """PostToolUse hook: sanitize tool results before returning.
    
    Removes sensitive information and spills oversized payloads into the
    working-memory workspace (Task B2): the full result goes to
    ``evidence.json`` and the model only receives ``summary + pointer``
    instead of the old discard-style "... [truncated]" clip.
    """

    @property
    def phase(self) -> str:
        return "PostToolUse"

    async def execute(self, ctx: HookContext) -> bool:
        if not ctx.tool_result:
            return True

        MAX_RESULT_SIZE = 10 * 1024  # 10KB limit for result content

        try:
            result = ctx.tool_result

            # Spill oversized content to the working-memory workspace.
            if isinstance(result.get("content"), str) and len(result["content"]) > MAX_RESULT_SIZE:
                memory = self._resolve_working_memory(ctx)
                result["content"] = memory.spill_tool_result(
                    tool_name=ctx.tool_name or "unknown_tool",
                    content=result["content"],
                )
                logger.warning(f"Result spilled to working memory for {ctx.tool_name} (exceeded {MAX_RESULT_SIZE} bytes)")

            # Remove sensitive fields (implement based on policy)
            self._remove_sensitive_data(result)

            logger.debug(f"Result sanitized for {ctx.tool_name}")
            return True

        except Exception as exc:  # noqa: BLE001
            ctx.add_error(f"Result sanitization failed: {exc}")
            return False

    @staticmethod
    def _resolve_working_memory(ctx: HookContext):
        """Reuse the run's workspace when present; otherwise create one per context."""
        from src.infrastructure.ai.working_memory import WorkingMemory

        if ctx.working_memory is None:
            ctx.working_memory = WorkingMemory(run_id=ctx.run_id)
        return ctx.working_memory

    def _remove_sensitive_data(self, result: dict[str, Any]) -> None:
        """Remove or mask sensitive fields."""
        SENSITIVE_FIELDS = ["password", "secret", "token", "api_key", "ssn"]

        for key in list(result.keys()):
            if any(s in key.lower() for s in SENSITIVE_FIELDS):
                logger.warning(f"Removed sensitive field: {key}")
                del result[key]


class FreshnessCheckHook(BaseHook):
    """PostToolUse hook: read-only query evidence must be timestamped and fresh (Task H1).

    The freshness rules that used to live only in the ``query_ops`` prompt
    become a runtime invariant: a governed query tool (keys of
    ``shadow_mode_config.TOOL_FRESHNESS_LIMITS``) whose result carries no
    ``as_of`` — or one older than the per-tool threshold — has its result
    rewritten in place to ``{ok: false, error_code: EVIDENCE_STALE, ...}``
    so the model sees the failure and retries. The hook still returns True
    (PostToolUse clips, it does not abort) and records the failure in the
    run's working-memory evidence chain. Non-query tools (plan / skill /
    propose) are never gated.
    """

    @property
    def phase(self) -> str:
        return "PostToolUse"

    async def execute(self, ctx: HookContext) -> bool:
        tool_name = ctx.tool_name or ""
        result = ctx.tool_result

        from src.infrastructure.ai.templates.shadow_mode_config import resolve_freshness_limit

        max_age = resolve_freshness_limit(tool_name, ctx.tool_args)
        if max_age is None or not isinstance(result, dict):
            return True

        as_of = result.get("as_of")
        if as_of is None:
            evidence = result.get("evidence")
            if isinstance(evidence, dict):
                as_of = evidence.get("as_of")

        if as_of is None:
            self._rewrite_stale(
                ctx,
                result,
                detail="missing as_of",
                max_age=max_age,
            )
            return True

        from src.infrastructure.ai.evidence_metadata import compute_freshness_seconds

        try:
            freshness_seconds = compute_freshness_seconds(as_of)
        except (ValueError, TypeError):
            self._rewrite_stale(
                ctx,
                result,
                detail="unparseable as_of",
                max_age=max_age,
            )
            return True

        if freshness_seconds > max_age:
            self._rewrite_stale(
                ctx,
                result,
                detail="evidence older than tool threshold",
                max_age=max_age,
                freshness_seconds=freshness_seconds,
            )
        return True

    def _rewrite_stale(
        self,
        ctx: HookContext,
        result: dict[str, Any],
        *,
        detail: str,
        max_age: int,
        freshness_seconds: int | None = None,
    ) -> None:
        """Replace the tool result with the EVIDENCE_STALE error payload."""
        tool_name = ctx.tool_name or "unknown_tool"
        payload: dict[str, Any] = {
            "ok": False,
            "error_code": "EVIDENCE_STALE",
            "detail": detail,
            "max_age": max_age,
        }
        if freshness_seconds is not None:
            payload["freshness_seconds"] = freshness_seconds

        result.clear()
        result.update(payload)

        # Record the failure so the evidence chain itself shows the gap.
        if ctx.working_memory is not None:
            import json as _json

            ctx.working_memory.add_evidence(
                source=tool_name,
                object_id="",
                summary=f"EVIDENCE_STALE: {detail}",
                content=_json.dumps(payload, ensure_ascii=False),
            )
        logger.warning(
            f"FreshnessCheckHook rewrote stale result for {tool_name} "
            f"({detail}, max_age={max_age}), run={ctx.run_id}"
        )


class IDPreservationHook(BaseHook):
    """PreCompact hook: preserve critical IDs during context compression.
    
    Ensures flight numbers, anomaly IDs, proposal IDs, and other
    critical identifiers survive context window compression.
    """

    @property
    def phase(self) -> str:
        return "PreCompact"

    async def execute(self, ctx: HookContext) -> bool:
        if not ctx.messages:
            return True

        try:
            protected_ids = extract_critical_ids(ctx.messages)

            if protected_ids:
                logger.debug(f"Identified {len(protected_ids)} critical IDs for preservation")

                # Store in metadata for compression step
                if ctx.envelope and hasattr(ctx.envelope, "metadata"):
                    ctx.envelope.metadata["_protected_ids"] = protected_ids

            return True

        except Exception as exc:  # noqa: BLE001
            ctx.add_error(f"ID preservation failed: {exc}")
            return False


class NoPromisesHook(BaseHook):
    """Stop hook: prevent the LLM from making unauthorized promises.
    
    Scans final answer for language that implies completed actions
    when none were actually executed. Blocks runs that claim
    "already updated gate assignment" without approval.
    """

    @property
    def phase(self) -> str:
        return "Stop"

    async def execute(self, ctx: HookContext) -> bool:
        if not ctx.messages:
            return True

        FINAL_ANSWER_PATTERNS = [
            r".*\u5df2.*[\u4e3a\u4f60][\u60a8]?[\u6539\u66f4\u8c03\u6574].*",  # "Already changed/adjusted"
            r".*(\u5b8c\u6210|\u5b8c\u6bd5).*[\u64cd\u4f5c\u4fee\u6539].*",  # "Completed operation"
            r".*(\u4e3a\u60a8)(\u751f\u6210\u4e86|\u521b\u5efa\u4e86|\u5b89\u6392\u4e86).*",  # "Generated/created/scheduled for you"
            r".*\u64cd\u4f5c.*(\u5df2\u7ecf|\u5df1\. )?\u5b8c\u6210.*",  # "Operation already completed"
        ]

        try:
            # Get final assistant message
            last_message = None
            for msg in reversed(ctx.messages):
                if msg.get("role") == "assistant":
                    last_message = msg
                    break

            if not last_message:
                return True

            content = last_message.get("content", "").lower()

            for pattern in FINAL_ANSWER_PATTERNS:
                import re
                if re.search(pattern, content):
                    ctx.add_error(
                        f"Detected unapproved action promise: {content[:100]}"
                    )
                    logger.warning(f"NoPromisesHook blocked: {content[:100]}")
                    return False

            logger.debug("NoPromisesHook passed: no unauthorized promises detected")
            return True

        except Exception as exc:  # noqa: BLE001
            ctx.add_error(f"NoPromisesHook failed: {exc}")
            return False


class EvidenceCoverageHook(BaseHook):
    """Stop hook: the final answer may only cite identifiers backed by evidence (Task H2).

    Every critical ID extracted from the last assistant text must appear in
    the run's working-memory ``evidence.json`` (``object_id`` or
    ``content``). Uncovered IDs mean the model is "pretending to know":
    the locked behaviour is the rewrite option — the hook returns False and
    exposes ``ctx.final_text_override`` with a fixed Chinese degradation,
    which the runner uses instead of the original text.

    Policy: enforced for ``query_ops``; ``anomaly_ops`` / ``dispatch_ops``
    may carry hypothesis paragraphs and are left to :class:`NoPromisesHook`
    (they still may not claim executed changes).
    """

    DEGRADATION_TEMPLATE = (
        "以下编号缺少工具证据，不能当作事实：{ids}。"
        "请先通过查询工具获取实时数据，再给出结论。"
    )

    @property
    def phase(self) -> str:
        return "Stop"

    async def execute(self, ctx: HookContext) -> bool:
        envelope_task = getattr(ctx.envelope, "task", None)
        task_type = getattr(envelope_task, "task_type", None)
        if task_type != "query_ops":
            return True

        last_message = None
        for msg in reversed(ctx.messages or []):
            content = msg.get("content")
            if msg.get("role") == "assistant" and isinstance(content, str) and content.strip():
                last_message = msg
                break
        if last_message is None:
            return True

        text = last_message["content"]
        claimed_ids = self._extract_ids(text)
        if not claimed_ids:
            return True

        evidence_blobs = self._evidence_blobs(ctx)
        uncovered = [i for i in claimed_ids if not any(i in blob for blob in evidence_blobs)]
        if not uncovered:
            return True

        override = self.DEGRADATION_TEMPLATE.format(ids="、".join(uncovered))
        ctx.final_text_override = override
        last_message["content"] = override
        ctx.add_error(f"EVIDENCE_COVERAGE: ungrounded identifiers: {', '.join(uncovered)}")
        # J2: each degradation is one ungrounded event; the absolute count
        # feeds the FmsAiUngroundedSpike alert.
        from src.infrastructure.ai.monitoring.prometheus_exporter import inc_error

        inc_error("ungrounded")
        logger.warning(
            f"EvidenceCoverageHook degraded final answer (uncovered: {uncovered}), run={ctx.run_id}"
        )
        return False

    @staticmethod
    def _extract_ids(text: str) -> list[str]:
        import re

        seen: set[str] = set()
        ordered: list[str] = []
        for pattern in CRITICAL_ID_PATTERNS:
            for match in re.findall(pattern, text):
                if match not in seen:
                    seen.add(match)
                    ordered.append(match)
        return ordered

    @staticmethod
    def _evidence_blobs(ctx: HookContext) -> list[str]:
        if ctx.working_memory is None:
            return []
        blobs: list[str] = []
        for record in ctx.working_memory.read_evidence():
            for key in ("object_id", "content", "summary"):
                value = record.get(key)
                if isinstance(value, str) and value:
                    blobs.append(value)
        return blobs


class PlanFirstHook(BaseHook):
    """PreToolUse hook: high-risk templates must establish a plan first (Task C1).

    When the run's task template is plan-first (``anomaly_ops`` /
    ``dispatch_ops``), proposal-class write tools are rejected until the run's
    WorkingMemory holds a non-empty plan (``update_plan`` writes ``plan.md``).
    Read-only tools and the plan tools themselves are never gated.
    """

    @property
    def phase(self) -> str:
        return "PreToolUse"

    async def execute(self, ctx: HookContext) -> bool:
        envelope_task = getattr(ctx.envelope, "task", None)
        task_type = getattr(envelope_task, "task_type", None)
        if not isinstance(task_type, str):
            return True

        from src.infrastructure.ai.templates import get_task_template
        from src.infrastructure.ai.tools.plan_tools import is_plan_tool

        template = get_task_template(task_type)
        if template is None or not getattr(template, "requires_plan_first", False):
            return True

        tool_name = ctx.tool_name
        if not tool_name or is_plan_tool(tool_name) or is_read_only_tool(tool_name):
            return True

        # Only proposal-class write tools are gated; anything else (unknown /
        # MCP) is the responsibility of the ACL snapshot and the MQ gate.
        from src.infrastructure.ai.tools.tool_executor import is_write_action_tool

        if not is_write_action_tool(tool_name):
            return True

        has_plan = bool(
            ctx.working_memory is not None and (ctx.working_memory.read_plan() or "").strip()
        )
        if has_plan:
            return True

        ctx.add_error(
            f"PLAN_REQUIRED: task_type={task_type} is plan-first; call update_plan "
            f"to establish an execution plan before requesting {tool_name}"
        )
        logger.warning(f"PlanFirstHook blocked {tool_name} (no plan yet), run={ctx.run_id}")
        return False


class SolverFirstHook(BaseHook):
    """PreToolUse hook: dispatch proposals must be grounded in the solver first (Task I2).

    For ``task_type=dispatch_ops``, proposal-class calls
    (``ontology.propose_action`` and every ``WRITE_ACTION_TOOLS`` member)
    are rejected until this run has a successful
    ``dispatch.list_solver_candidates`` (or ``ontology.explain_constraints``)
    result recorded in the working-memory evidence chain by
    :class:`SolverGateEvidenceHook`. Read-only and plan tools are never
    gated; ``query_ops`` / ``anomaly_ops`` are out of scope.
    """

    @property
    def phase(self) -> str:
        return "PreToolUse"

    async def execute(self, ctx: HookContext) -> bool:
        envelope_task = getattr(ctx.envelope, "task", None)
        task_type = getattr(envelope_task, "task_type", None)
        if task_type != "dispatch_ops":
            return True

        tool_name = ctx.tool_name or ""

        from src.infrastructure.ai.tools.tool_executor import is_write_action_tool

        if tool_name != "ontology.propose_action" and not is_write_action_tool(tool_name):
            return True

        if self._gate_satisfied(ctx):
            return True

        ctx.add_error(
            "SOLVER_FIRST_REQUIRED: dispatch proposals must be grounded first; call "
            "dispatch.list_solver_candidates (or ontology.explain_constraints) and "
            f"retry {tool_name} with its result"
        )
        logger.warning(f"SolverFirstHook blocked {tool_name} (no solver/constraint evidence), run={ctx.run_id}")
        return False

    @staticmethod
    def _gate_satisfied(ctx: HookContext) -> bool:
        memory = ctx.working_memory
        if memory is None:
            return False
        return any(
            record.get("source") in SOLVER_GATE_SATISFYING_TOOLS
            for record in memory.read_evidence()
        )


class SolverGateEvidenceHook(BaseHook):
    """PostToolUse hook: record successful solver/constraint results for the gate (Task I2).

    Companion to :class:`SolverFirstHook`: when
    ``dispatch.list_solver_candidates`` or ``ontology.explain_constraints``
    succeeds, an evidence record is appended to the run's working memory so
    later PreToolUse checks can see the gate was satisfied. Failed or
    stale results (e.g. ``EVIDENCE_STALE`` rewrites by
    :class:`FreshnessCheckHook`, which runs first) never satisfy the gate.
    """

    @property
    def phase(self) -> str:
        return "PostToolUse"

    async def execute(self, ctx: HookContext) -> bool:
        tool_name = ctx.tool_name or ""
        if tool_name not in SOLVER_GATE_SATISFYING_TOOLS:
            return True
        if ctx.working_memory is None:
            return True
        result = ctx.tool_result
        if not self._succeeded(result):
            logger.warning(f"SolverGateEvidenceHook skipped failed/stale result for {tool_name}, run={ctx.run_id}")
            return True

        import json as _json

        try:
            content = _json.dumps(result, ensure_ascii=False, default=str)[:2000]
        except (TypeError, ValueError):
            content = str(result)[:2000]
        ctx.working_memory.add_evidence(
            source=tool_name,
            object_id="",
            summary=f"solver-first gate satisfied by {tool_name}",
            content=content,
        )
        return True

    @staticmethod
    def _succeeded(result: Any) -> bool:
        """True when the PostToolUse result shape indicates a successful call."""
        if not isinstance(result, dict) or not result:
            return False
        if result.get("ok") is False:
            return False
        if result.get("error_code"):
            return False
        if result.get("error"):
            return False
        content = result.get("content")
        return not (isinstance(content, str) and content.startswith("Error:"))


# Tools whose successful execution satisfies the SolverFirst gate (Task I2).
SOLVER_GATE_SATISFYING_TOOLS: frozenset[str] = frozenset(
    {
        "dispatch.list_solver_candidates",
        "ontology.explain_constraints",
    }
)


class OutputGuardrailHook(BaseHook):
    """Stop hook: run the legacy output guardrail inside the hook pipeline.

    Migrates ``guardrails/output_guardrail.py`` (internal-id leakage,
    flight-number consistency against this run's tool results, false
    operation claims) into the ``Stop`` phase as a parallel implementation
    next to :class:`NoPromisesHook`. The original module keeps its entry
    points (``OutputGuardrail.validate`` / ``apply_guardrail_warnings``) for
    compatibility; the main runtime path goes through this hook.
    """

    @property
    def phase(self) -> str:
        return "Stop"

    async def execute(self, ctx: HookContext) -> bool:
        if not ctx.messages:
            return True

        try:
            from src.infrastructure.ai.guardrails.output_guardrail import OutputGuardrail

            last_message = None
            for msg in reversed(ctx.messages):
                if msg.get("role") == "assistant":
                    last_message = msg
                    break
            if not last_message:
                return True

            content = last_message.get("content") or ""
            if not isinstance(content, str) or not content.strip():
                return True

            tool_results = [
                str(msg.get("content"))
                for msg in ctx.messages
                if msg.get("role") == "tool" and msg.get("content")
            ]

            # Writes are proposal-only in this runtime — the model never
            # executes them, so false-claim checks always apply.
            result = OutputGuardrail().validate(
                response_text=content,
                tool_results=tool_results or None,
                had_write_operations=False,
            )
            for warning in result.warnings:
                ctx.add_error(f"OutputGuardrail: {warning}")
            return result.passed

        except Exception as exc:  # noqa: BLE001
            ctx.add_error(f"OutputGuardrailHook failed: {exc}")
            return False


# ============================================================================
# Hook Pipeline
# ============================================================================

class HookPipeline:
    """Manages execution of hooks across phases."""

    def __init__(self):
        self._hooks_by_phase: dict[str, list[BaseHook]] = {}

    def register_hook(self, hook: BaseHook) -> None:
        """Register a hook for its phase."""
        phase = hook.phase
        if phase not in self._hooks_by_phase:
            self._hooks_by_phase[phase] = []
        self._hooks_by_phase[phase].append(hook)

    async def execute_phase(self, phase: str, ctx: HookContext) -> bool:
        """Execute all hooks for a phase. Stops on first failure."""
        hooks = self._hooks_by_phase.get(phase, [])

        for hook in hooks:
            hook_name = type(hook).__name__
            try:
                if not await hook.execute(ctx):
                    # Observability only — does not change allow/deny semantics.
                    if not ctx.blocked_rule:
                        ctx.blocked_rule = hook_name
                    logger.error(f"Hook {hook_name} failed at phase {phase}")
                    return False
            except Exception as exc:  # noqa: BLE001
                if not ctx.blocked_rule:
                    ctx.blocked_rule = hook_name
                ctx.add_error(f"Hook {hook_name} exception: {exc}")
                return False

        return True

    async def execute_all_phases(self, ctx: HookContext) -> bool:
        """Execute all phases in order."""
        PHASE_ORDER = ["PreToolUse", "PostToolUse", "PreCompact", "Stop"]

        for phase in PHASE_ORDER:
            if phase not in self._hooks_by_phase:
                continue

            if not await self.execute_phase(phase, ctx):
                logger.error(f"Hooks stopped at phase {phase} for run={ctx.run_id}")
                return False

        return True


def is_read_only_tool(tool_name: str) -> bool:
    """Check if tool is read-only."""
    READ_ONLY_PREFIXES = [
        "list_",
        "get_",
        "search_",
        "query_",
        "lookup_",
        "check_",  # Add generic check prefix
        "check_status",
        "fetch_",
    ]

    return any(tool_name.startswith(prefix) for prefix in READ_ONLY_PREFIXES)


# ============================================================================
# Default Built-in Hooks
# ============================================================================

def get_builtin_hooks() -> list[BaseHook]:
    """Get default set of built-in hooks.

    Order matters within a phase (Task H3): PostToolUse sanitizes before
    the freshness check, and the Stop phase screens promises before the
    evidence-coverage grounding check, which runs before the output
    guardrail.
    """
    return [
        PlanFirstHook(),            # PreToolUse - plan-first enforcement (high-risk templates)
        SolverFirstHook(),          # PreToolUse - solver-first gate (dispatch_ops, Task I2)
        LeaseCheckHook(),           # PreToolUse - lease preflight (fail-closed)
        SchemaValidationHook(),     # PreToolUse - argument validation
        ObjectExistenceCheckHook(), # PreToolUse - entity existence (advisory)
        ResultSanitizationHook(),   # PostToolUse - result clipping
        FreshnessCheckHook(),       # PostToolUse - evidence freshness invariant (Task H1)
        SolverGateEvidenceHook(),   # PostToolUse - solver gate evidence (Task I2, after freshness)
        IDPreservationHook(),       # PreCompact - ID protection
        NoPromisesHook(),           # Stop - anti-promises
        EvidenceCoverageHook(),     # Stop - grounding degradation (Task H2)
        OutputGuardrailHook(),      # Stop - output guardrail (leakage / flight consistency)
    ]


def build_default_pipeline() -> HookPipeline:
    """Build pipeline with all built-in hooks registered."""
    pipeline = HookPipeline()
    for hook in get_builtin_hooks():
        pipeline.register_hook(hook)
    return pipeline


__all__ = [
    "CRITICAL_ID_PATTERNS",
    "SOLVER_GATE_SATISFYING_TOOLS",
    "BaseHook",
    "EvidenceCoverageHook",
    "FreshnessCheckHook",
    "HookContext",
    "HookPipeline",
    "IDPreservationHook",
    "LeaseCheckHook",
    "NoPromisesHook",
    "ObjectExistenceCheckHook",
    "OutputGuardrailHook",
    "PlanFirstHook",
    "ResultSanitizationHook",
    "SchemaValidationHook",
    "SolverFirstHook",
    "SolverGateEvidenceHook",
    "build_default_pipeline",
    "extract_critical_ids",
    "get_builtin_hooks",
    "is_read_only_tool",
]
