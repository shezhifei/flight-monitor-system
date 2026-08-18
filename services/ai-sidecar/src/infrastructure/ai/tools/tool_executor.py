"""Tool execution router for streaming inference.

This module provides a ToolExecutor that:
1. Receives a tool call request (name + arguments) from the LLM
2. Checks if it's read-only -> executes locally in Python
3. Checks if it's a write action -> builds an OutputProposal for Rust to execute
4. Checks if it's an MCP tool (mcp.{server_id}.{tool_name}) -> routes to MCP client manager
5. Returns the result or proposal

When a ``ToolMqGate`` is wired in, every tool call goes through the
gate first: a
``tool.call.requested`` event is published to the RocketMQ control
channel and protected tools block on a Rust authorization decision
(``tool_lease`` / ``tool_denied`` / ``tool_proposal_only``). Public L0
tools short-circuit the lease wait. See :mod:`.mq_gate` for the gate
implementation and the failure semantics.
"""

import asyncio
import json
import time
from collections.abc import Awaitable, Callable
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any

from src.infrastructure.ai.capability_resolver import is_tool_allowed, normalized_mcp_binding_tool_acl
from src.infrastructure.ai.governance.governance_resolver import is_public_l0_tool
from src.infrastructure.ai.mcp.annotations import normalize_mcp_tool_annotations
from src.infrastructure.ai.ontology.action_client import OntologyActionClientError
from src.infrastructure.ai.ontology_tools import (
    ADVISORY_ACTIONS,
    CONTROLLED_WRITE_ACTIONS,
    UnregisteredActionError,
)
from src.infrastructure.ai.tools.ontology_tool_definitions import (
    ONTOLOGY_TOOL_NAMES,
    is_ontology_tool,
)
from src.infrastructure.ai.tools.read_only_tools import (
    READ_ONLY_TOOLS,
    execute_read_only_tool,
    get_read_only_tool_names,
    is_read_only_tool,
)
from src.infrastructure.ai.tools.solver_tools import (
    SOLVER_TOOL_NAMES,
    SolverCandidateClientError,
    is_solver_tool,
)
from src.infrastructure.common.exceptions import (
    HTTP_EXCEPTIONS,
    JSON_EXCEPTIONS,
    LLM_EXCEPTIONS,
    POSTGRES_EXCEPTIONS,
    REDIS_EXCEPTIONS,
)
from src.infrastructure.logging.core import get_logger

if TYPE_CHECKING:  # pragma: no cover - typing only
    from src.infrastructure.ai.llm_stream_runner import StreamEvent

# Optional async callback for bubbling sub-agent StreamEvents up to the parent run.
# Mirrors dispatcher.OnChildEvent; only the subagent path forwards events through it.
OnChildEvent = Callable[["StreamEvent"], Awaitable[None]]

logger = get_logger(__name__)

_MCP_TOOL_PREFIX = "mcp."


@dataclass
class ToolResultCachePolicy:
    """Per-run cache policy for tool results.

    Passed through the execution chain so concurrent runs with different
    entity configs don't interfere with each other.
    """

    enabled: bool = False
    cacheable_tools: list[str] | None = None
    ttl: int = 60

    def is_tool_cacheable(self, tool_name: str) -> bool:
        """Check if a specific tool should be cached under this policy."""
        if not self.enabled:
            return False
        tools = self.cacheable_tools
        if tools is None:
            return False
        return tool_name in tools


# Machine-readable gate identifiers for governance rejections.
# Values are stable contracts for SSE tool_result / run.fail payloads and explain.
# Budget exhaustion (StreamEvent type=budget_exhausted) is a run-loop stop that
# still emits ``completed``; it is not a single-tool governance deny, so there
# is no BLOCKED_BY_BUDGET constant.
BLOCKED_BY_SNAPSHOT = "snapshot"
BLOCKED_BY_HOOK = "hook"
BLOCKED_BY_ACL = "acl"
BLOCKED_BY_LEASE = "lease"
BLOCKED_BY_TEMPLATE = "template"


@dataclass
class ToolExecutionResult:
    """Result of executing a single tool call during streaming inference.

    On governance rejection, ``blocked_by`` / ``rule`` / ``detail`` identify
    which gate stopped the call. ``error`` keeps the legacy human-readable
    string (including codes like ``TOOL_NOT_IN_ALLOWED_SET``) for model feedback
    and existing tests; structured fields are additive observability only.
    """

    tool_call_id: str
    tool_name: str
    success: bool
    result: Any | None = None
    error: str | None = None
    proposal: dict[str, Any] | None = None
    blocked_by: str | None = None
    rule: str | None = None
    detail: str | None = None

    def to_sse_payload(self) -> dict[str, Any]:
        """Convert to SSE tool.result payload format."""
        payload: dict[str, Any] = {
            "tool_call_id": self.tool_call_id,
            "tool_name": self.tool_name,
        }
        if self.success:
            payload["result"] = self.result
        else:
            payload["error"] = self.error
            if self.blocked_by is not None:
                payload["blocked_by"] = self.blocked_by
            if self.rule is not None:
                payload["rule"] = self.rule
            if self.detail is not None:
                payload["detail"] = self.detail
        return payload

    @classmethod
    def blocked(
        cls,
        *,
        tool_call_id: str,
        tool_name: str,
        blocked_by: str,
        rule: str,
        detail: str,
        error: str | None = None,
    ) -> "ToolExecutionResult":
        """Build a failed result with structured rejection metadata."""
        return cls(
            tool_call_id=tool_call_id,
            tool_name=tool_name,
            success=False,
            error=error if error is not None else f"{rule}: {detail}",
            blocked_by=blocked_by,
            rule=rule,
            detail=detail,
        )


# Tools that require Rust-side execution (write actions)
WRITE_ACTION_TOOLS: dict[str, tuple[str, str]] = {
    "add_flight_note": ("Flight", "add_note"),
    "update_flight_status": ("Flight", "update_status"),
    "assign_gate": ("Flight", "assign_gate"),
    "create_todo": ("Todo", "create"),
    "update_todo": ("Todo", "update"),
    "complete_todo": ("Todo", "complete"),
}


def is_write_action_tool(tool_name: str) -> bool:
    """Check if a tool is a registered write action tool."""
    return tool_name in WRITE_ACTION_TOOLS


def is_mcp_tool(tool_name: str) -> bool:
    """Check if a tool is an MCP tool (mcp.{server_id}.{tool_name})."""
    return tool_name.startswith(_MCP_TOOL_PREFIX)


def parse_mcp_tool_name(tool_name: str) -> tuple[str, str]:
    """Parse an MCP tool name into (server_id, original_tool_name).

    Format: mcp.{server_id}.{tool_name}
    """
    parts = tool_name.split(".", 2)
    if len(parts) < 3:
        raise ValueError(f"Invalid MCP tool name: {tool_name}. Expected format: mcp.<server_id>.<tool_name>")
    return parts[1], parts[2]


class ToolExecutor:
    """Routes tool calls to local execution or proposal generation."""

    MAX_TOOL_ROUNDS: int = 5

    def __init__(
        self,
        mcp_client_manager=None,
        mcp_repo=None,
        subagent_dispatcher=None,
        cache_manager=None,
        mq_gate: Any | None = None,
        read_only_backend: Any | None = None,
        ontology_tools: Any | None = None,
        solver_tools: Any | None = None,
    ):
        self._read_only_tools = READ_ONLY_TOOLS
        self._read_only_backend = read_only_backend
        self._mcp_client_manager = mcp_client_manager
        self._mcp_repo = mcp_repo
        self._subagent_dispatcher = subagent_dispatcher
        self._cache_manager = cache_manager
        self._mq_gate = mq_gate
        # OntologyTools adapter over the fail-closed OntologyActionClient.
        # None → ontology.* calls fail closed (never stubbed).
        self._ontology_tools = ontology_tools
        # SolverTools adapter over the fail-closed SolverCandidateClient.
        # None → dispatch.list_solver_candidates fails closed (never stubbed).
        self._solver_tools = solver_tools
        logger.debug("ToolExecutor initialized with %d read-only tools", len(self._read_only_tools))

    def get_available_tools(self) -> list[str]:
        builtin = (
            get_read_only_tool_names()
            + list(WRITE_ACTION_TOOLS.keys())
            + list(ONTOLOGY_TOOL_NAMES)
            + list(SOLVER_TOOL_NAMES)
        )
        return builtin

    def is_read_only_tool(self, tool_name: str) -> bool:
        return is_read_only_tool(tool_name)

    def get_tool_type(self, tool_name: str) -> str:
        if tool_name == "delegate_to_subagent":
            return "subagent"
        if is_read_only_tool(tool_name):
            return "read_only"
        if is_write_action_tool(tool_name):
            return "write_action"
        if is_ontology_tool(tool_name):
            return "ontology"
        if is_solver_tool(tool_name):
            return "solver"
        if is_mcp_tool(tool_name):
            return "mcp"
        return "unknown"

    def get_mcp_client_manager(self):
        return self._mcp_client_manager

    def get_mcp_repo(self):
        return self._mcp_repo

    @property
    def mq_gate(self) -> Any:
        return self._mq_gate

    async def execute(
        self,
        tool_call: dict[str, Any],
        run_id: str,
        envelope: Any | None = None,
        cache_policy: ToolResultCachePolicy | None = None,
        on_child_event: OnChildEvent | None = None,
        entity_id: str | None = None,
        allowed_tool_names: set[str] | None = None,
        round_index: int = 0,
        job_id: str = "",
    ) -> ToolExecutionResult:
        """Execute a tool call and record J1 metrics (tool x task_type x status x blocked_by).

        Single instrumentation point: every routed tool call — local,
        gated, blocked or failed — is counted here exactly once.
        """
        from src.infrastructure.ai.monitoring.prometheus_exporter import (
            inc_tool_call,
            observe_tool_duration,
        )

        tool_name = str(tool_call.get("tool_name", "") or "unknown")
        started = time.monotonic()
        try:
            result = await self._execute_inner(
                tool_call,
                run_id,
                envelope=envelope,
                cache_policy=cache_policy,
                on_child_event=on_child_event,
                entity_id=entity_id,
                allowed_tool_names=allowed_tool_names,
                round_index=round_index,
                job_id=job_id,
            )
        except Exception:
            # Metrics boundary only: record the error and re-raise, nothing is
            # swallowed here (K5/W2-5 exception convergence).
            inc_tool_call(tool_name, "error")
            observe_tool_duration(tool_name, time.monotonic() - started)
            raise
        if result.blocked_by is not None:
            status = "blocked"
        else:
            status = "success" if result.success else "error"
        inc_tool_call(tool_name, status, blocked_by=result.blocked_by)
        observe_tool_duration(tool_name, time.monotonic() - started)
        return result

    async def _execute_inner(
        self,
        tool_call: dict[str, Any],
        run_id: str,
        envelope: Any | None = None,
        cache_policy: ToolResultCachePolicy | None = None,
        on_child_event: OnChildEvent | None = None,
        entity_id: str | None = None,
        allowed_tool_names: set[str] | None = None,
        round_index: int = 0,
        job_id: str = "",
    ) -> ToolExecutionResult:
        tool_call_id = tool_call.get("tool_call_id", "unknown")
        tool_name = tool_call.get("tool_name", "")
        arguments = tool_call.get("arguments", {})
        if isinstance(arguments, str):
            try:
                arguments = parse_tool_arguments(arguments)
            except ValueError as exc:
                return ToolExecutionResult(
                    tool_call_id=tool_call_id,
                    tool_name=tool_name,
                    success=False,
                    error=f"INVALID_TOOL_ARGUMENTS: {exc}",
                )

        # Defense-in-depth: enforce resolved tool ACL at execution time.
        # This prevents replayed/hallucinated tool calls that were not in the
        # resolver's allowed set from executing, even if LLM produces them.
        if allowed_tool_names is not None and tool_name not in allowed_tool_names:
            detail = (
                f"Tool '{tool_name}' is not in the resolved allowed tool set for this run. "
                "This may indicate a replayed or hallucinated tool call."
            )
            return ToolExecutionResult.blocked(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                blocked_by=BLOCKED_BY_ACL,
                rule="TOOL_NOT_IN_ALLOWED_SET",
                detail=detail,
                error=f"TOOL_NOT_IN_ALLOWED_SET: {detail}",
            )

        tool_type = self.get_tool_type(tool_name)

        if self._mq_gate is not None:
            return await self._execute_with_gate(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                tool_type=tool_type,
                arguments=arguments,
                run_id=run_id,
                envelope=envelope,
                cache_policy=cache_policy,
                on_child_event=on_child_event,
                entity_id=entity_id,
                round_index=round_index,
                job_id=job_id,
            )

        # MQ gate unavailable — fail-closed for protected tools.
        # Only explicit public L0 tools may bypass the gate and execute locally.
        # This matches the Rust `RustToolGovernanceResolver` contract: non-L0
        # tools MUST be authorized by Rust before any execution.
        if is_public_l0_tool(tool_name):
            return await self._execute_local(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                tool_type=tool_type,
                arguments=arguments,
                run_id=run_id,
                envelope=envelope,
                cache_policy=cache_policy,
                on_child_event=on_child_event,
                entity_id=entity_id,
            )

        logger.error(
            "ai_mq_gate_unavailable_fail_closed",
            extra={
                "tool_name": tool_name,
                "tool_call_id": tool_call_id,
                "run_id": run_id,
                "job_id": job_id,
            },
        )
        detail = (
            f"Authorization gate is not available; protected tool '{tool_name}' "
            "cannot execute without Rust authorization. Verify RocketMQ connectivity "
            "and restart the sidecar."
        )
        return ToolExecutionResult.blocked(
            tool_call_id=tool_call_id,
            tool_name=tool_name,
            blocked_by=BLOCKED_BY_LEASE,
            rule="MQ_GATE_UNAVAILABLE",
            detail=detail,
            error=f"MQ_GATE_UNAVAILABLE: {detail}",
        )

    async def _execute_with_gate(
        self,
        *,
        tool_call_id: str,
        tool_name: str,
        tool_type: str,
        arguments: dict[str, Any],
        run_id: str,
        envelope: Any | None,
        cache_policy: ToolResultCachePolicy | None,
        on_child_event: OnChildEvent | None,
        entity_id: str | None,
        round_index: int,
        job_id: str,
    ) -> ToolExecutionResult:
        from .mq_gate import (
            ToolAuthContextRequired,
            ToolAuthorizationError,
            ToolAuthorizationTimeout,
        )

        started = time.monotonic()
        try:
            decision = await self._mq_gate.request_authorization(
                tool_name=tool_name,
                tool_call_id=tool_call_id,
                run_id=run_id,
                job_id=job_id,
                round_index=round_index,
                arguments=arguments,
                envelope=envelope,
                tool_type=tool_type,
            )
        except ToolAuthContextRequired as exc:
            return ToolExecutionResult.blocked(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                blocked_by=BLOCKED_BY_LEASE,
                rule="TOOL_AUTH_CONTEXT_REQUIRED",
                detail=str(exc),
                error=str(exc),
            )
        except ToolAuthorizationTimeout as exc:
            return ToolExecutionResult.blocked(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                blocked_by=BLOCKED_BY_LEASE,
                rule="TOOL_AUTHORIZATION_TIMEOUT",
                detail=str(exc),
                error=str(exc),
            )
        except ToolAuthorizationError as exc:
            code = getattr(exc, "code", None) or "TOOL_AUTHORIZATION_ERROR"
            return ToolExecutionResult.blocked(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                blocked_by=BLOCKED_BY_LEASE,
                rule=str(code),
                detail=str(exc),
                error=str(exc),
            )

        if decision.mode == "denied":
            return await self._publish_and_build_denied(
                decision=decision,
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                started=started,
            )

        if decision.mode == "proposal_only":
            local_result = self._build_proposal_for_mode(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                tool_type=tool_type,
                arguments=arguments,
                run_id=run_id,
                envelope=envelope,
            )
            duration_ms = int((time.monotonic() - started) * 1000)
            try:
                await self._mq_gate.publish_result(
                    context=decision.context,
                    status="proposal_only",
                    duration_ms=duration_ms,
                    result=local_result.result,
                    error_code=None,
                    error_message=None,
                )
            except REDIS_EXCEPTIONS + JSON_EXCEPTIONS + (KeyError,) as exc:  # best-effort publish (K5)
                logger.warning(
                    "ai_mq_proposal_only_result_publish_failed",
                    extra={"tool_name": tool_name, "tool_call_id": tool_call_id},
                    exc_info=exc,
                )
            return local_result

        heartbeat_task, heartbeat_stop = await self._mq_gate.start_heartbeat(context=decision.context)
        try:
            local_result = await self._execute_local(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                tool_type=tool_type,
                arguments=arguments,
                run_id=run_id,
                envelope=envelope,
                cache_policy=cache_policy,
                on_child_event=on_child_event,
                entity_id=entity_id,
            )
        except Exception as exc:
            # Tool execution boundary (K5/W2-5): arbitrary backend failures are
            # converted into a structured tool error instead of aborting the run.
            logger.error(
                "tool_execution_exception",
                extra={
                    "error_code": "TOOL_EXECUTION_EXCEPTION",
                    "tool_name": tool_name,
                    "tool_call_id": tool_call_id,
                    "run_id": run_id,
                },
                exc_info=exc,
            )
            await self._mq_gate.stop_heartbeat(heartbeat_task, heartbeat_stop)
            duration_ms = int((time.monotonic() - started) * 1000)
            try:
                await self._mq_gate.publish_result(
                    context=decision.context,
                    status="failed",
                    duration_ms=duration_ms,
                    error_code="TOOL_EXECUTION_ERROR",
                    error_message=str(exc),
                )
            except REDIS_EXCEPTIONS + JSON_EXCEPTIONS + (KeyError,) as publish_exc:  # best-effort publish (K5)
                logger.warning(
                    "ai_mq_failure_result_publish_failed",
                    extra={"tool_name": tool_name, "tool_call_id": tool_call_id},
                    exc_info=publish_exc,
                )
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error=f"Tool execution failed: {exc}",
            )
        await self._mq_gate.stop_heartbeat(heartbeat_task, heartbeat_stop)

        duration_ms = int((time.monotonic() - started) * 1000)
        if local_result.success:
            try:
                await self._mq_gate.publish_result(
                    context=decision.context,
                    status="succeeded",
                    duration_ms=duration_ms,
                    result=local_result.result,
                )
            except REDIS_EXCEPTIONS + JSON_EXCEPTIONS + (KeyError,) as exc:  # best-effort publish (K5)
                logger.warning(
                    "ai_mq_success_result_publish_failed",
                    extra={"tool_name": tool_name, "tool_call_id": tool_call_id},
                    exc_info=exc,
                )
        else:
            try:
                await self._mq_gate.publish_result(
                    context=decision.context,
                    status="failed",
                    duration_ms=duration_ms,
                    error_code="TOOL_EXECUTION_ERROR",
                    error_message=local_result.error or "Tool execution failed",
                )
            except REDIS_EXCEPTIONS + JSON_EXCEPTIONS + (KeyError,) as exc:  # best-effort publish (K5)
                logger.warning(
                    "ai_mq_failure_result_publish_failed",
                    extra={"tool_name": tool_name, "tool_call_id": tool_call_id},
                    exc_info=exc,
                )
        return local_result

    async def _publish_and_build_denied(
        self,
        *,
        decision: Any,
        tool_call_id: str,
        tool_name: str,
        started: float,
    ) -> ToolExecutionResult:
        duration_ms = int((time.monotonic() - started) * 1000)
        try:
            await self._mq_gate.publish_result(
                context=decision.context,
                status="denied",
                duration_ms=duration_ms,
                error_code=decision.denial_code or "TOOL_ACTOR_PERMISSION_DENIED",
                error_message=decision.denial_message or "denied by Rust",
            )
        except REDIS_EXCEPTIONS + JSON_EXCEPTIONS + (KeyError,) as exc:  # best-effort publish (K5)
            logger.warning(
                "ai_mq_denied_result_publish_failed",
                extra={"tool_name": tool_name, "tool_call_id": tool_call_id},
                exc_info=exc,
            )
        from .mq_gate import ToolDeniedByRust

        message = decision.denial_message or "denied by Rust"
        denial_code = decision.denial_code or "TOOL_ACTOR_PERMISSION_DENIED"
        denied_exc = ToolDeniedByRust(
            tool_name=tool_name,
            tool_call_id=tool_call_id,
            code=denial_code,
            message=message,
        )
        return ToolExecutionResult.blocked(
            tool_call_id=tool_call_id,
            tool_name=tool_name,
            blocked_by=BLOCKED_BY_LEASE,
            rule=denial_code,
            detail=message,
            error=str(denied_exc),
        )

    def _build_proposal_for_mode(
        self,
        *,
        tool_call_id: str,
        tool_name: str,
        tool_type: str,
        arguments: dict[str, Any],
        run_id: str,
        envelope: Any | None,
    ) -> ToolExecutionResult:
        if tool_type == "write_action":
            return self._build_write_proposal(tool_call_id, tool_name, arguments, run_id, envelope)
        if tool_type == "ontology":
            if tool_name == "ontology.propose_action":
                return self._build_ontology_write_proposal(
                    tool_call_id,
                    tool_name,
                    arguments,
                    run_id,
                    envelope,
                    action_name=str(arguments.get("action_name") or ""),
                    parameters=arguments.get("parameters") or {},
                )
            return self._build_ontology_read_proposal(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                arguments=arguments,
                run_id=run_id,
                envelope=envelope,
            )
        if tool_type == "mcp":
            return self._build_mcp_proposal_for_proposal_only_mode(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                arguments=arguments,
                run_id=run_id,
                envelope=envelope,
            )
        if tool_type == "read_only":
            return self._build_read_only_proposal(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                arguments=arguments,
                run_id=run_id,
                envelope=envelope,
            )
        if tool_type == "solver":
            # Read-only surface; under proposal_only it degrades to the same
            # approval-shaped record as other read-only tools.
            return self._build_read_only_proposal(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                arguments=arguments,
                run_id=run_id,
                envelope=envelope,
            )
        return ToolExecutionResult(
            tool_call_id=tool_call_id,
            tool_name=tool_name,
            success=False,
            error=f"proposal_only unsupported for tool type {tool_type!r}",
        )

    def _build_mcp_proposal_for_proposal_only_mode(
        self,
        *,
        tool_call_id: str,
        tool_name: str,
        arguments: dict[str, Any],
        run_id: str,
        envelope: Any | None,
    ) -> ToolExecutionResult:
        try:
            server_id, original_tool_name = parse_mcp_tool_name(tool_name)
        except ValueError as exc:
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error=str(exc),
            )
        proposal: dict[str, Any] = {
            "object_type": "MCPTool",
            "object_id": server_id,
            "action_name": original_tool_name,
            "arguments": arguments,
            "risk_level": "high",
            "confidence": 0.75,
            "reasoning": (f"LLM requested MCP tool '{tool_name}' but Rust required proposal_only path"),
            "requires_approval": True,
            "source": "streaming_tool_execution",
            "run_id": run_id,
            "tool_call_id": tool_call_id,
        }
        user_id = getattr(envelope, "user_id", None) if envelope else None
        if user_id:
            proposal["requester_user_id"] = user_id
        return ToolExecutionResult(
            tool_call_id=tool_call_id,
            tool_name=tool_name,
            success=True,
            result={"status": "proposal_created", "proposal": proposal},
            proposal=proposal,
        )

    def _build_read_only_proposal(
        self,
        *,
        tool_call_id: str,
        tool_name: str,
        arguments: dict[str, Any],
        run_id: str,
        envelope: Any | None,
    ) -> ToolExecutionResult:
        proposal: dict[str, Any] = {
            "object_type": "ReadOnlyTool",
            "object_id": tool_name,
            "action_name": "invoke",
            "arguments": arguments,
            "risk_level": "low",
            "confidence": 0.75,
            "reasoning": (f"LLM requested read-only tool '{tool_name}' but Rust required proposal_only path"),
            "requires_approval": True,
            "source": "streaming_tool_execution",
            "run_id": run_id,
            "tool_call_id": tool_call_id,
        }
        user_id = getattr(envelope, "user_id", None) if envelope else None
        if user_id:
            proposal["requester_user_id"] = user_id
        return ToolExecutionResult(
            tool_call_id=tool_call_id,
            tool_name=tool_name,
            success=True,
            result={"status": "proposal_created", "proposal": proposal},
            proposal=proposal,
        )

    async def _execute_local(
        self,
        *,
        tool_call_id: str,
        tool_name: str,
        tool_type: str,
        arguments: dict[str, Any],
        run_id: str,
        envelope: Any | None,
        cache_policy: ToolResultCachePolicy | None,
        on_child_event: OnChildEvent | None,
        entity_id: str | None,
    ) -> ToolExecutionResult:
        if tool_type == "read_only":
            return await self._execute_read_only(
                tool_call_id,
                tool_name,
                arguments,
                run_id,
                cache_policy=cache_policy,
                entity_id=entity_id,
            )
        if tool_type == "write_action":
            return self._build_write_proposal(tool_call_id, tool_name, arguments, run_id, envelope)
        if tool_type == "ontology":
            return await self._execute_ontology(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                arguments=arguments,
                run_id=run_id,
                envelope=envelope,
            )
        if tool_type == "mcp":
            return await self._execute_mcp_tool(tool_call_id, tool_name, arguments, run_id, envelope)
        if tool_type == "solver":
            return await self._execute_solver(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                arguments=arguments,
                run_id=run_id,
            )
        if tool_type == "subagent":
            return await self._execute_subagent(
                tool_call_id, tool_name, arguments, run_id, envelope, on_child_event=on_child_event
            )

        return ToolExecutionResult(
            tool_call_id=tool_call_id,
            tool_name=tool_name,
            success=False,
            error=f"Unknown tool: {tool_name}. Available: {self.get_available_tools()}",
        )

    async def _execute_read_only(
        self,
        tool_call_id: str,
        tool_name: str,
        arguments: dict[str, Any],
        run_id: str,
        cache_policy: ToolResultCachePolicy | None = None,
        entity_id: str | None = None,
    ) -> ToolExecutionResult:
        # Determine cache behavior from per-run policy (not instance state)
        use_cache = (
            cache_policy is not None and cache_policy.is_tool_cacheable(tool_name) and self._cache_manager is not None
        )

        # Check tool result cache
        if use_cache:
            try:
                cached = await self._cache_manager.get_tool_result(
                    tool_name=tool_name,
                    args=arguments,
                    cacheable_tools=[tool_name],
                    ttl_seconds=cache_policy.ttl,
                    entity_id=entity_id,
                )
                if cached is not None:
                    return ToolExecutionResult(
                        tool_call_id=tool_call_id,
                        tool_name=tool_name,
                        success=True,
                        result=cached,
                    )
            except REDIS_EXCEPTIONS + JSON_EXCEPTIONS + (KeyError,) as exc:  # cache read is best-effort (K5)
                logger.error(
                    "tool_result_cache_read_failed",
                    entity_id=entity_id,
                    tool_name=tool_name,
                    exc_info=exc,
                )

        try:
            result = await execute_read_only_tool(
                tool_name,
                arguments,
                backend=self._read_only_backend,
            )

            # Write to tool result cache
            if use_cache:
                try:
                    await self._cache_manager.set_tool_result(
                        tool_name=tool_name,
                        args=arguments,
                        result=result,
                        ttl_seconds=cache_policy.ttl,
                        entity_id=entity_id,
                    )
                except REDIS_EXCEPTIONS + JSON_EXCEPTIONS + (KeyError,) as exc:  # cache write is best-effort (K5)
                    logger.error(
                        "tool_result_cache_write_failed",
                        entity_id=entity_id,
                        tool_name=tool_name,
                        exc_info=exc,
                    )

            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=True,
                result=result,
            )
        except ValueError as exc:
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error=f"Invalid arguments: {exc}",
            )
        except Exception as exc:
            # Read-only backend boundary (K5/W2-5): read-only tool backends vary;
            # any failure becomes a tool error, never a run abort.
            logger.error(
                "read_only_tool_execution_exception",
                extra={
                    "error_code": "TOOL_EXECUTION_EXCEPTION",
                    "tool_name": tool_name,
                    "tool_call_id": tool_call_id,
                },
                exc_info=exc,
            )
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error=f"Tool execution failed: {exc}",
            )

    async def _execute_ontology(
        self,
        *,
        tool_call_id: str,
        tool_name: str,
        arguments: dict[str, Any],
        run_id: str,
        envelope: Any | None,
    ) -> ToolExecutionResult:
        """Route ontology.* tools through the fail-closed Rust-backed adapter.

        Read/explain calls are forwarded to ``OntologyTools`` (which calls
        the Rust internal endpoints). ``ontology.propose_action`` forwards
        advisory actions and turns controlled writes into approval
        proposals — they are never executed here. Without a wired adapter
        the call fails closed; no results are fabricated.
        """
        if self._ontology_tools is None:
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error=(
                    "ONTOLOGY_CLIENT_NOT_CONFIGURED: ontology action client is not wired; "
                    "refusing to fabricate results"
                ),
            )
        try:
            if tool_name == "ontology.lookup":
                entity_id = str(arguments.get("entity_id") or "").strip()
                if not entity_id:
                    return ToolExecutionResult(
                        tool_call_id=tool_call_id,
                        tool_name=tool_name,
                        success=False,
                        error="Invalid arguments: entity_id is required for ontology.lookup",
                    )
                include_relations = bool(arguments.get("include_relations", True))
                result = await self._ontology_tools.lookup(
                    run_id=run_id,
                    entity_id=entity_id,
                    include_relations=include_relations,
                )
            elif tool_name == "ontology.explain_constraints":
                entity_type = str(arguments.get("entity_type") or "").strip() or "Flight"
                proposed_change = arguments.get("proposed_change") or {}
                if not isinstance(proposed_change, dict):
                    return ToolExecutionResult(
                        tool_call_id=tool_call_id,
                        tool_name=tool_name,
                        success=False,
                        error="Invalid arguments: proposed_change must be an object",
                    )
                result = await self._ontology_tools.explain_constraints(
                    run_id=run_id,
                    entity_type=entity_type,
                    proposed_change=proposed_change,
                )
            elif tool_name == "ontology.propose_action":
                action_name = str(arguments.get("action_name") or "").strip()
                if not action_name:
                    return ToolExecutionResult(
                        tool_call_id=tool_call_id,
                        tool_name=tool_name,
                        success=False,
                        error="Invalid arguments: action_name is required for ontology.propose_action",
                    )
                parameters = arguments.get("parameters") or {}
                if not isinstance(parameters, dict):
                    return ToolExecutionResult(
                        tool_call_id=tool_call_id,
                        tool_name=tool_name,
                        success=False,
                        error="Invalid arguments: parameters must be an object",
                    )
                outcome = await self._ontology_tools.propose_action(
                    run_id=run_id,
                    action_name=action_name,
                    parameters=parameters,
                    allowed_actions=sorted(ADVISORY_ACTIONS | CONTROLLED_WRITE_ACTIONS),
                )
                if isinstance(outcome, dict) and outcome.get("execution_mode") == "rejected":
                    # Simulate-before-proposal found hard constraint violations:
                    # surface the failure, never create a proposal.
                    violations = outcome.get("hard_constraint_violations") or []
                    rule_ids = ", ".join(
                        str(v.get("rule_id")) for v in violations if isinstance(v, dict)
                    )
                    return ToolExecutionResult(
                        tool_call_id=tool_call_id,
                        tool_name=tool_name,
                        success=False,
                        error=(
                            f"HARD_CONSTRAINT_VIOLATION: controlled write '{action_name}' "
                            f"rejected before proposal ({rule_ids}); no proposal created"
                        ),
                        result=outcome,
                    )
                if isinstance(outcome, dict) and outcome.get("execution_mode") == "proposal_only":
                    simulate = outcome.get("simulate")
                    return self._build_ontology_write_proposal(
                        tool_call_id,
                        tool_name,
                        arguments,
                        run_id,
                        envelope,
                        action_name=action_name,
                        parameters=parameters,
                        simulate=simulate if isinstance(simulate, dict) else None,
                    )
                result = outcome
            else:
                return ToolExecutionResult(
                    tool_call_id=tool_call_id,
                    tool_name=tool_name,
                    success=False,
                    error=f"Unknown ontology tool: {tool_name}",
                )
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=True,
                result=result,
            )
        except UnregisteredActionError as exc:
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error=f"UNREGISTERED_ACTION: {exc}",
            )
        except OntologyActionClientError as exc:
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error=f"ONTOLOGY_ACTION_FAILED [{exc.error_code}]: {exc}",
            )
        except ValueError as exc:
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error=f"Invalid arguments: {exc}",
            )
        except HTTP_EXCEPTIONS + JSON_EXCEPTIONS as exc:  # transport leaks past the wrapped client errors (K5)
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error=f"ONTOLOGY_TOOL_EXECUTION_FAILED: {exc}",
            )

    async def _execute_solver(
        self,
        *,
        tool_call_id: str,
        tool_name: str,
        arguments: dict[str, Any],
        run_id: str,
    ) -> ToolExecutionResult:
        """Route solver candidate tools through the fail-closed Rust-backed adapter.

        Forwards ``dispatch.list_solver_candidates`` to ``SolverTools``
        (which calls the Rust internal replan-snapshot endpoint). Without
        a wired adapter the call fails closed; no candidates are
        fabricated.
        """
        window_start = str(arguments.get("window_start") or "").strip()
        window_end = str(arguments.get("window_end") or "").strip()
        if not window_start or not window_end:
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error=(
                    "INVALID_TOOL_ARGUMENTS: window_start and window_end are required "
                    f"for {tool_name}"
                ),
            )
        strategy = arguments.get("strategy")
        strategy = str(strategy).strip() if strategy else None
        order_ids = arguments.get("order_ids")
        if order_ids is not None and not isinstance(order_ids, list):
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error="INVALID_TOOL_ARGUMENTS: order_ids must be an array",
            )

        if self._solver_tools is None:
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error=(
                    "SOLVER_CLIENT_NOT_CONFIGURED: solver candidate client is not wired; "
                    "refusing to fabricate candidates"
                ),
            )
        try:
            result = await self._solver_tools.list_solver_candidates(
                run_id=run_id,
                window_start=window_start,
                window_end=window_end,
                strategy=strategy or None,
                order_ids=order_ids,
            )
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=True,
                result=result,
            )
        except SolverCandidateClientError as exc:
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error=f"SOLVER_SNAPSHOT_FAILED [{exc.error_code}]: {exc}",
            )
        except HTTP_EXCEPTIONS + JSON_EXCEPTIONS as exc:  # transport leaks past the wrapped client errors (K5)
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error=f"SOLVER_TOOL_EXECUTION_FAILED: {exc}",
            )

    def _build_ontology_write_proposal(
        self,
        tool_call_id: str,
        tool_name: str,
        arguments: dict[str, Any],
        run_id: str,
        envelope: Any | None = None,
        *,
        action_name: str = "",
        parameters: dict[str, Any] | None = None,
        simulate: dict[str, Any] | None = None,
    ) -> ToolExecutionResult:
        """Build an approval proposal for a controlled ontology write.

        Controlled writes (e.g. ``Flight.change_stand``) are never
        executed by the sidecar: the only outcome is a proposal routed
        through the existing approval surface. When provided, the
        ``simulate`` block (before/after + constraint outcome) rides on
        the proposal so the approval card can render the diff.
        """
        resolved_action = action_name or str(arguments.get("action_name") or "")
        resolved_params = parameters if parameters is not None else (arguments.get("parameters") or {})
        object_type, _, action = resolved_action.partition(".")
        object_id = (
            resolved_params.get("flight_id")
            or resolved_params.get("object_id")
            or resolved_params.get("dispatch_order_id")
            or ""
        )
        user_id = getattr(envelope, "user_id", None) if envelope else None

        proposal: dict[str, Any] = {
            "object_type": object_type or "Flight",
            "object_id": str(object_id) if object_id else "",
            "action_name": action or resolved_action,
            "arguments": resolved_params,
            "risk_level": "high",
            "confidence": 0.75,
            "reasoning": (
                f"LLM requested controlled write '{resolved_action}' via ontology.propose_action"
            ),
            "requires_approval": True,
            "execution_mode": "proposal_only",
            "source": "streaming_tool_execution",
            "run_id": run_id,
            "tool_call_id": tool_call_id,
        }
        if user_id:
            proposal["requester_user_id"] = user_id
        if simulate is not None:
            proposal["simulate"] = simulate

        return ToolExecutionResult(
            tool_call_id=tool_call_id,
            tool_name=tool_name,
            success=True,
            result={"status": "proposal_created", "proposal": proposal},
            proposal=proposal,
        )

    def _build_ontology_read_proposal(
        self,
        *,
        tool_call_id: str,
        tool_name: str,
        arguments: dict[str, Any],
        run_id: str,
        envelope: Any | None,
    ) -> ToolExecutionResult:
        """Proposal-only fallback for ontology read/explain tools under Rust proposal_only."""
        proposal: dict[str, Any] = {
            "object_type": "OntologyTool",
            "object_id": tool_name,
            "action_name": "invoke",
            "arguments": arguments,
            "risk_level": "low",
            "confidence": 0.75,
            "reasoning": (
                f"LLM requested ontology tool '{tool_name}' but Rust required proposal_only path"
            ),
            "requires_approval": True,
            "source": "streaming_tool_execution",
            "run_id": run_id,
            "tool_call_id": tool_call_id,
        }
        user_id = getattr(envelope, "user_id", None) if envelope else None
        if user_id:
            proposal["requester_user_id"] = user_id
        return ToolExecutionResult(
            tool_call_id=tool_call_id,
            tool_name=tool_name,
            success=True,
            result={"status": "proposal_created", "proposal": proposal},
            proposal=proposal,
        )

    async def _execute_subagent(
        self,
        tool_call_id: str,
        tool_name: str,
        arguments: dict[str, Any],
        run_id: str,
        envelope: Any | None = None,
        on_child_event: OnChildEvent | None = None,
    ) -> ToolExecutionResult:
        """Execute a subagent delegation.

        ``on_child_event`` is passed straight through to the dispatcher; when set,
        the dispatcher streams the child run and bubbles each child StreamEvent to
        the callback (out of band). The blocking SubagentResult mapped into the
        ToolExecutionResult below is unchanged whether or not forwarding is active.
        """
        if not self._subagent_dispatcher:
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error="SUBAGENTS_DISABLED: SubagentDispatcher not configured",
            )

        entity_id = arguments.get("entity_id", "")
        task = arguments.get("task", "")
        context_summary = arguments.get("context_summary")
        expected_output = arguments.get("expected_output")

        if not entity_id or not task:
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error="SUBAGENT_INVALID_ARGS: entity_id and task are required",
            )

        # Get subagent config from envelope metadata if available
        metadata = getattr(envelope, "metadata", None) or {}
        subagent_depth = metadata.get("subagent_depth", 0)
        subagent_trace = metadata.get("subagent_trace", [])

        # Get parent entity_id from envelope
        parent_entity_id = getattr(envelope, "entity_id", "default") if envelope else "default"

        # Read from envelope metadata (pre-populated from resolved_config by runtime service)
        allowed_entity_ids = metadata.get("subagent_allowed_entity_ids", [])
        max_depth = metadata.get("subagent_max_depth", 1)
        max_concurrency = metadata.get("subagent_max_concurrency", 2)
        inherit_parent_context = metadata.get("subagent_inherit_parent_context", True)

        try:
            result = await self._subagent_dispatcher.dispatch(
                parent_entity_id=parent_entity_id,
                target_entity_id=entity_id,
                task=task,
                context_summary=context_summary,
                expected_output=expected_output,
                subagent_depth=subagent_depth,
                subagent_trace=subagent_trace,
                max_depth=max_depth,
                max_concurrency=max_concurrency,
                allowed_entity_ids=allowed_entity_ids,
                inherit_parent_context=inherit_parent_context,
                parent_envelope=envelope,
                on_child_event=on_child_event,
            )

            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=True,
                result={
                    "entity_id": result.entity_id,
                    "status": result.status,
                    "answer": result.answer,
                    "limitations": result.limitations,
                    "proposal_count": result.proposal_count,
                },
            )
        except LLM_EXCEPTIONS + JSON_EXCEPTIONS as exc:  # subagent runs reuse the LLM/serialization failure surface (K5)
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error=f"SUBAGENT_EXECUTION_ERROR: {exc}",
            )

    async def _execute_mcp_tool(
        self,
        tool_call_id: str,
        tool_name: str,
        arguments: dict[str, Any],
        run_id: str,
        envelope: Any | None = None,
    ) -> ToolExecutionResult:
        """Execute an MCP tool via the MCP client manager.

        Format: mcp.{server_id}.{tool_name}
        Security: side_effect determined from repo capabilities BEFORE any connection.
        """
        try:
            server_id, original_tool_name = parse_mcp_tool_name(tool_name)
        except ValueError as exc:
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error=str(exc),
            )

        if not self._mcp_client_manager:
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error=f"MCP client manager not configured; cannot execute MCP tool '{tool_name}'",
            )

        if not self._mcp_repo:
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error=f"MCP_TOOL_CAPABILITIES_NOT_DISCOVERED: MCP repository not configured; cannot determine tool side_effect for '{tool_name}'",
            )

        # Step 1: Load capabilities from repo (trusted source)
        try:
            caps = await self._mcp_repo.get_capabilities(server_id)
        except POSTGRES_EXCEPTIONS + JSON_EXCEPTIONS as exc:  # pg-backed MCP repo (K5)
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error=f"MCP_TOOL_CAPABILITIES_NOT_DISCOVERED: Failed to load capabilities for server '{server_id}': {exc}",
            )

        if not caps:
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error=f"MCP_TOOL_CAPABILITIES_NOT_DISCOVERED: No capabilities discovered for server '{server_id}'. Probe the server first.",
            )

        # Step 1b: Enforce entity binding ACL before any connection/call (fail-closed).
        entity_id = getattr(envelope, "entity_id", None) if envelope else None
        if not entity_id:
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error=f"MCP_BINDING_MISSING_ENTITY: tool '{tool_name}' requires an entity envelope to enforce binding ACL",
            )

        try:
            bindings = await self._mcp_repo.find_bindings_by_entity(entity_id)
        except POSTGRES_EXCEPTIONS + JSON_EXCEPTIONS as exc:  # pg-backed MCP repo (K5)
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error=f"MCP_BINDING_LOOKUP_FAILED: Failed to load MCP bindings for entity '{entity_id}': {exc}",
            )

        binding = next(
            (b for b in bindings if b.get("server_id") == server_id and b.get("enabled")),
            None,
        )
        if not binding:
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error=f"MCP_BINDING_NOT_ENABLED: No enabled MCP binding for entity '{entity_id}' and server '{server_id}'",
            )

        try:
            binding = normalized_mcp_binding_tool_acl(binding)
        except ValueError as exc:
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error=f"MCP_BINDING_ACL_INVALID: {exc}",
            )

        # Step 2: Find tool in capabilities and determine side_effect
        tools_raw = caps.get("tools", [])
        if isinstance(tools_raw, str):
            try:
                tools_raw = json.loads(tools_raw)
            except JSON_EXCEPTIONS as exc:
                logger.warning("mcp_tool_tools_json_parse_failed", exc_info=exc)
                tools_raw = []

        tool_info = None
        for t in tools_raw:
            if isinstance(t, dict) and t.get("name") == original_tool_name:
                tool_info = t
                break

        if tool_info is None:
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error=f"MCP_TOOL_NOT_DISCOVERED: Tool '{original_tool_name}' not found in discovered capabilities for server '{server_id}'",
            )

        if not is_tool_allowed(
            {
                "name": original_tool_name,
                "category": tool_info.get("category", ""),
            },
            binding,
        ):
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error=(
                    f"MCP_TOOL_NOT_ALLOWED_BY_BINDING: Tool '{original_tool_name}' is not allowed "
                    f"for entity '{entity_id}' and server '{server_id}'"
                ),
            )

        # Step 3: Determine side_effect from annotations (fail closed)
        annotations = tool_info.get("annotations", {})
        normalized = normalize_mcp_tool_annotations(annotations)
        has_side_effect = normalized.side_effect

        if has_side_effect:
            # Side-effect MCP tool: generate proposal, do NOT connect or call
            proposal: dict[str, Any] = {
                "object_type": "MCPTool",
                "object_id": server_id,
                "action_name": original_tool_name,
                "arguments": arguments,
                "risk_level": "high",
                "confidence": 0.75,
                "reasoning": f"LLM requested MCP tool '{tool_name}' (side_effect=true) via streaming inference",
                "requires_approval": True,
                "source": "streaming_tool_execution",
                "run_id": run_id,
                "tool_call_id": tool_call_id,
            }
            user_id = getattr(envelope, "user_id", None) if envelope else None
            if user_id:
                proposal["requester_user_id"] = user_id

            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=True,
                result={"status": "proposal_created", "proposal": proposal},
                proposal=proposal,
            )

        # Step 4: Non-side-effect tool — check allowlist then lazy connect
        from src.infrastructure.ai.mcp.command_allowlist import is_command_allowed

        session = self._mcp_client_manager.get_session(server_id)
        if (not session or session.status != "connected") and self._mcp_repo:
            server_config = await self._mcp_repo.find_server_by_id(server_id)
            if not server_config:
                return ToolExecutionResult(
                    tool_call_id=tool_call_id,
                    tool_name=tool_name,
                    success=False,
                    error=f"MCP server '{server_id}' not found in repository",
                )

            command_ref = server_config.get("command_ref", "")
            if not is_command_allowed(command_ref):
                return ToolExecutionResult(
                    tool_call_id=tool_call_id,
                    tool_name=tool_name,
                    success=False,
                    error=f"MCP_COMMAND_NOT_ALLOWLISTED: command_ref '{command_ref}' for server '{server_id}' is not in the configured allowlist",
                )

            timeout = server_config.get("timeout_seconds", 30)
            startup_timeout = server_config.get("startup_timeout_seconds", 10)

            await self._mcp_client_manager.connect_server(
                server_id,
                dict(server_config),
                timeout=timeout,
                startup_timeout=startup_timeout,
            )

        try:
            result = await self._mcp_client_manager.call_tool(
                server_id,
                original_tool_name,
                arguments,
                timeout=30.0,
            )

            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=True,
                result=result,
            )

        except RuntimeError as exc:
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error=f"MCP tool execution failed: {exc}",
            )
        except HTTP_EXCEPTIONS + JSON_EXCEPTIONS + (OSError,) as exc:  # MCP transport: httpx/stdio/JSON (K5)
            return ToolExecutionResult(
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                success=False,
                error=f"MCP tool execution error: {exc}",
            )

    def _build_write_proposal(
        self,
        tool_call_id: str,
        tool_name: str,
        arguments: dict[str, Any],
        run_id: str,
        envelope: Any | None = None,
    ) -> ToolExecutionResult:
        object_type, action_name = WRITE_ACTION_TOOLS.get(tool_name, ("Unknown", tool_name))
        object_id = arguments.get("flight_id") or arguments.get("object_id") or arguments.get("todo_id") or ""
        user_id = getattr(envelope, "user_id", None) if envelope else None

        proposal: dict[str, Any] = {
            "object_type": object_type,
            "object_id": str(object_id) if object_id else "",
            "action_name": action_name,
            "arguments": arguments,
            "risk_level": "high" if tool_name in {"assign_gate", "update_flight_status"} else "medium",
            "confidence": 0.75,
            "reasoning": f"LLM requested {tool_name} via tool execution during streaming inference",
            "requires_approval": True,
            "source": "streaming_tool_execution",
            "run_id": run_id,
            "tool_call_id": tool_call_id,
        }
        if user_id:
            proposal["requester_user_id"] = user_id

        return ToolExecutionResult(
            tool_call_id=tool_call_id,
            tool_name=tool_name,
            success=True,
            result={"status": "proposal_created", "proposal": proposal},
            proposal=proposal,
        )

    async def execute_batch(
        self,
        tool_calls: list[dict[str, Any]],
        run_id: str,
        envelope: Any | None = None,
        cache_policy: ToolResultCachePolicy | None = None,
        on_child_event: OnChildEvent | None = None,
        entity_id: str | None = None,
        allowed_tool_names: set[str] | None = None,
        round_index: int = 0,
        job_id: str = "",
    ) -> list[ToolExecutionResult]:
        if not tool_calls:
            return []
        tasks = [
            self.execute(
                tc,
                run_id,
                envelope,
                cache_policy=cache_policy,
                on_child_event=on_child_event,
                entity_id=entity_id,
                allowed_tool_names=allowed_tool_names,
                round_index=round_index,
                job_id=job_id,
            )
            for tc in tool_calls
        ]
        results = await asyncio.gather(*tasks, return_exceptions=True)
        processed = []
        for i, result in enumerate(results):
            if isinstance(result, Exception):
                tc = tool_calls[i]
                processed.append(
                    ToolExecutionResult(
                        tool_call_id=tc.get("tool_call_id", "unknown"),
                        tool_name=tc.get("tool_name", "unknown"),
                        success=False,
                        error=f"Batch execution error: {result}",
                    )
                )
            else:
                processed.append(result)
        return processed

    def collect_proposals(self, results: list[ToolExecutionResult]) -> list[dict[str, Any]]:
        return [r.proposal for r in results if r.proposal is not None]


def parse_tool_calls_from_stream(content_blocks: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Parse tool calls from OpenAI streaming content blocks."""
    tool_calls = []
    for block in content_blocks:
        block_type = block.get("type", "")
        if block_type == "tool_call":
            tc = block.get("tool_call", {})
            tool_calls.append(
                {
                    "tool_call_id": tc.get("id", ""),
                    "tool_name": tc.get("function", {}).get("name", ""),
                    "arguments": tc.get("function", {}).get("arguments", ""),
                }
            )
        elif block_type == "function_call":
            tool_calls.append(
                {
                    "tool_call_id": block.get("id", ""),
                    "tool_name": block.get("name", ""),
                    "arguments": block.get("arguments", ""),
                }
            )
    return tool_calls


def parse_tool_arguments(arguments: str) -> dict[str, Any]:
    """Parse tool arguments JSON string into a dict.

    Raises:
        ValueError: If arguments is non-empty but not valid JSON.
    """
    if not arguments:
        return {}
    try:
        return json.loads(arguments)
    except json.JSONDecodeError as exc:
        raise ValueError(f"Invalid JSON in tool arguments: {exc}") from exc
