"""Tool authorization gate between the executor and the MQ control plane.

Wires the :class:`ToolExecutor` to the durable MQ control channel
(``ai.runtime.events``) and the Rust-controlled command queue
(``ai_runtime_commands``).

The gate is **additive** to the existing ``ToolExecutor``:

* when no publisher / poller is wired, the executor falls through to
  its pre-existing behaviour (read-only executes locally, write-action
  builds a proposal, MCP enforces the binding ACL, subagent delegates
  to the dispatcher);
* when a :class:`ToolMqGate` is provided, every tool call goes through
  the gate before the existing logic runs, and a ``tool.result`` event
  is published after the call returns.

The gate is fail-closed for protected tools:

* a missing requester user id on the envelope raises
  :class:`ToolAuthContextRequired` instead of running a protected tool;
* a protected tool whose ``tool.call.requested`` publish fails is not
  executed;
* a protected tool that does not receive a ``tool_lease`` within
  ``governance.timeout_seconds + 5s`` raises
  :class:`ToolAuthorizationTimeout`.
"""

from __future__ import annotations

import asyncio
import hashlib
import time
import uuid
from collections.abc import Callable
from dataclasses import dataclass
from typing import Any, Literal

from src.infrastructure.ai.governance import (
    ResolvedToolGovernance,
    ToolGovernanceResolver,
    canonical_args_hash,
    tool_call_idempotency_key,
)
from src.infrastructure.ai.messaging import (
    AiRuntimeEventPublisher,
    AiRuntimeEventPublishError,
    build_heartbeat,
    build_tool_call_requested,
    build_tool_result,
)
from src.infrastructure.ai.messaging.command_dispatcher import ToolCommandWaiter
from src.infrastructure.ai.tools.tool_registry_snapshot import ToolDefinition
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


HEARTBEAT_INTERVAL_SECONDS: float = 10.0
AUTHORIZATION_MARGIN_SECONDS: float = 5.0
WAIT_POLL_INTERVAL_SECONDS: float = 0.05


# ---------------------------------------------------------------------------
# Exceptions
# ---------------------------------------------------------------------------


class ToolAuthorizationError(RuntimeError):
    """Base class for tool authorization errors."""


class ToolAuthContextRequired(ToolAuthorizationError):
    """Raised when a protected tool is requested without an authenticated actor context."""

    def __init__(self, tool_name: str, tool_call_id: str) -> None:
        super().__init__(
            f"TOOL_AUTH_CONTEXT_REQUIRED: protected tool {tool_name!r} "
            f"(tool_call_id={tool_call_id}) requires a requester context"
        )
        self.tool_name = tool_name
        self.tool_call_id = tool_call_id


class ToolDeniedByRust(ToolAuthorizationError):
    """Raised when Rust denies a protected tool call via ``tool_denied``."""

    def __init__(
        self,
        tool_name: str,
        tool_call_id: str,
        code: str,
        message: str,
    ) -> None:
        super().__init__(f"TOOL_DENIED: {tool_name!r} (tool_call_id={tool_call_id}) denied by Rust: [{code}] {message}")
        self.tool_name = tool_name
        self.tool_call_id = tool_call_id
        self.code = code
        self.message = message


class ToolAuthorizationTimeout(ToolAuthorizationError):
    """Raised when no authorization command arrives before the timeout."""

    def __init__(self, tool_name: str, tool_call_id: str, timeout_seconds: float) -> None:
        super().__init__(
            f"TOOL_AUTHORIZATION_TIMEOUT: no authorization decision for {tool_name!r} "
            f"(tool_call_id={tool_call_id}) within {timeout_seconds:.1f}s"
        )
        self.tool_name = tool_name
        self.tool_call_id = tool_call_id
        self.timeout_seconds = timeout_seconds


# ---------------------------------------------------------------------------
# Gate data classes
# ---------------------------------------------------------------------------


@dataclass
class GateContext:
    """Per-call context for the tool authorization gate."""

    run_id: str
    job_id: str
    round_index: int
    event_sequence: int
    tool_call_pk: str
    tool_call_id: str
    tool_name: str
    tool_type: str
    arguments: dict[str, Any]
    args_summary: dict[str, Any]
    envelope: Any | None
    requester: dict[str, Any] | None
    entity_allowlist: list[str]
    object_decisions: list[dict[str, Any]]
    governance: ResolvedToolGovernance
    idempotency_key: str
    args_hash_value: str


@dataclass
class AuthorizationDecision:
    """Authorization decision returned by :meth:`ToolMqGate.request_authorization`.

    ``mode`` tells the executor which local path to take:

    * ``"execute"`` — the tool may run locally (public L0 short-circuit
      or Rust ``tool_lease``);
    * ``"proposal_only"`` — the tool must be turned into a proposal;
      no direct execution;
    * ``"denied"`` — the tool must not run; ``denial_code`` /
      ``denial_message`` describe the reason.
    """

    mode: Literal["execute", "proposal_only", "denied"]
    context: GateContext
    denial_code: str | None = None
    denial_message: str | None = None


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _hash_tool_call_pk(run_id: str, tool_call_id: str) -> str:
    return hashlib.sha256(f"{run_id}:{tool_call_id}".encode()).hexdigest()[:32]


def make_tool_call_pk(run_id: str, tool_call_id: str) -> str:
    """Generate a deterministic ``tool_call_pk`` for a (run, tool_call_id) pair."""
    return _hash_tool_call_pk(run_id, tool_call_id)


def make_uuid_tool_call_pk() -> str:
    """Generate a random ``tool_call_pk`` when no stable identifier is available."""
    return uuid.uuid4().hex


def _summarize_arguments(arguments: dict[str, Any]) -> dict[str, Any]:
    """Recursively summarize arguments for the MQ payload.

    Truncates long string values to 200 characters; redacts well-known
    secret-looking fields; caps nested list sizes and recursion depth.
    """
    if not arguments:
        return {}

    secret_keys = {
        "api_key",
        "apikey",
        "authorization",
        "auth",
        "bearer",
        "password",
        "secret",
        "token",
    }

    def _walk(value: Any, depth: int) -> Any:
        if depth > 4:
            return "<truncated>"
        if isinstance(value, dict):
            return {
                str(k): ("<redacted>" if str(k).lower() in secret_keys else _walk(v, depth + 1))
                for k, v in value.items()
            }
        if isinstance(value, list):
            return [_walk(v, depth + 1) for v in value[:50]]
        if isinstance(value, str):
            if len(value) > 200:
                return value[:200] + "..."
            return value
        return value

    return _walk(arguments, 0) or {}


def _evaluate_object_decisions(
    arguments: dict[str, Any],
    governance: ResolvedToolGovernance,
) -> list[dict[str, Any]]:
    """Pre-evaluate per-object policy decisions for the Rust consumer.

    The sidecar can only inspect the JSON arguments; the actual
    authorization check still happens in Rust, so these decisions are
    hints, not grants. They let the Rust consumer short-circuit obvious
    "tool touched no object" cases and produce a useful audit trail.
    """
    object_policy = governance.get("object_policy", {}) or {}
    object_type_arg = object_policy.get("object_type_arg")
    object_id_arg = object_policy.get("object_id_arg")
    permission = object_policy.get("permission")

    decisions: list[dict[str, Any]] = []
    if not object_type_arg or not object_id_arg:
        return decisions
    object_type = arguments.get(object_type_arg)
    object_id = arguments.get(object_id_arg)
    if not object_type or not object_id:
        return decisions
    decisions.append(
        {
            "object_type": str(object_type),
            "object_id": str(object_id),
            "permission": str(permission) if permission else None,
            "allowed": True,
        }
    )
    return decisions


def _read_requester(envelope: Any) -> dict[str, Any] | None:
    """Read requester fields from a Pydantic envelope or a dict-like object."""
    if envelope is None:
        return None
    requester = getattr(envelope, "requester", None)
    if requester is None and isinstance(envelope, dict):
        requester = envelope.get("requester")
    if requester is None:
        return None
    user_id = getattr(requester, "user_id", None)
    if user_id is None and isinstance(requester, dict):
        user_id = requester.get("user_id")
    if not user_id:
        return None
    roles = getattr(requester, "roles", None)
    if not isinstance(roles, list):
        roles_value = requester.get("roles") if isinstance(requester, dict) else None
        roles = list(roles_value) if isinstance(roles_value, list) else []
    permissions = getattr(requester, "permissions", None)
    if not isinstance(permissions, list):
        permissions_value = requester.get("permissions") if isinstance(requester, dict) else None
        permissions = list(permissions_value) if isinstance(permissions_value, list) else []
    department_id = getattr(requester, "department_id", None)
    if department_id is None and isinstance(requester, dict):
        department_id = requester.get("department_id")
    return {
        "user_id": str(user_id),
        "roles": list(roles),
        "permissions": [str(p) for p in permissions],
        "department_id": department_id,
    }


# ---------------------------------------------------------------------------
# ToolMqGate
# ---------------------------------------------------------------------------


ToolTypeResolver = Callable[[str], str]
ToolDefinitionLookup = Callable[[str], ToolDefinition | None]
EntityAllowlistResolver = Callable[[Any], list[str] | None]


def _default_governance_for_unresolved(tool_name: str) -> ResolvedToolGovernance:
    """Return a conservative governance profile for tools not in the snapshot.

    Tools without a known ``ToolDefinition`` are treated as protected
    Rust-PDP tools: they cannot execute until the Rust consumer
    authorizes them. This is the fail-closed default.
    """
    return ResolvedToolGovernance(
        governance_version="1.0",
        tool_name=tool_name,
        tier="L2_REVERSIBLE_WRITE",
        side_effect=True,
        execution_mode="proposal_only",
        reversibility="reversible",
        risk_level="medium",
        public=False,
        required_account_permissions=[],
        authorization_mode="rust_pdp",
        object_policy={"object_type_arg": None, "object_id_arg": None, "permission": None},
        idempotency={"strategy": "run_tool_args_hash", "key_arg": None},
        retry_policy={"preset": "read_transient_default", "max_retries": 0},
        checkpoint_policy={"before": "summary", "after": "summary"},
        approval_policy={"required": True, "min_approver_permissions": []},
        compensation={"mode": "followup_action", "inverse_tool": None, "requires_approval": True},
        timeout_seconds=30,
        log_args="summary",
        log_result="summary",
        external_system=None,
        extra=None,
    )


class ToolMqGate:
    """MQ authorization gate.

    The gate owns the per-run ``event_sequence`` counter and is the
    single point that talks to the publisher and the command poller on
    behalf of the executor. It does not run any tool itself: the
    executor calls :meth:`request_authorization` before its local
    execution path and :meth:`publish_result` / :meth:`publish_proposal_only`
    after.
    """

    def __init__(
        self,
        *,
        publisher: AiRuntimeEventPublisher,
        poller: Any,
        governance_resolver: ToolGovernanceResolver | None = None,
        tool_definition_lookup: ToolDefinitionLookup | None = None,
        tool_type_resolver: ToolTypeResolver | None = None,
        run_owner: str = "python-sidecar",
        entity_allowlist_resolver: EntityAllowlistResolver | None = None,
        heartbeat_interval_seconds: float | None = None,
        wait_poll_interval_seconds: float | None = None,
        authorization_margin_seconds: float | None = None,
        command_waiter: ToolCommandWaiter | None = None,
    ) -> None:
        self._publisher = publisher
        self._poller = poller
        self._governance_resolver = governance_resolver or ToolGovernanceResolver()
        self._tool_definition_lookup = tool_definition_lookup
        self._tool_type_resolver = tool_type_resolver
        self._run_owner = run_owner
        self._entity_allowlist_resolver = entity_allowlist_resolver
        self._heartbeat_interval_seconds = (
            heartbeat_interval_seconds if heartbeat_interval_seconds is not None else HEARTBEAT_INTERVAL_SECONDS
        )
        self._wait_poll_interval_seconds = (
            wait_poll_interval_seconds if wait_poll_interval_seconds is not None else WAIT_POLL_INTERVAL_SECONDS
        )
        self._authorization_margin_seconds = (
            authorization_margin_seconds if authorization_margin_seconds is not None else AUTHORIZATION_MARGIN_SECONDS
        )
        self._command_waiter = command_waiter
        self._event_sequences: dict[str, int] = {}
        self._sequence_lock = asyncio.Lock()

    @property
    def publisher(self) -> AiRuntimeEventPublisher:
        return self._publisher

    @property
    def poller(self) -> Any:
        return self._poller

    @property
    def run_owner(self) -> str:
        return self._run_owner

    async def next_event_sequence(self, run_id: str) -> int:
        """Return and advance the per-run ``event_sequence`` counter."""
        async with self._sequence_lock:
            current = self._event_sequences.get(run_id, 0) + 1
            self._event_sequences[run_id] = current
            return current

    def reset_run_sequences(self, run_id: str | None = None) -> None:
        if run_id is None:
            self._event_sequences.clear()
        else:
            self._event_sequences.pop(run_id, None)

    def notify_command(self, command: dict[str, Any]) -> None:
        """Receive a command from the dispatcher and resolve any waiter.

        When the command consumer dispatches a ``tool_lease``,
        ``tool_denied`` or ``tool_proposal_only`` command, it calls this
        method so the gate can wake the waiting tool call without
        waiting for the next DB poll.
        """
        if self._command_waiter is None:
            return
        tool_call_pk = command.get("tool_call_pk")
        if tool_call_pk:
            self._command_waiter.notify(tool_call_pk, command)

    async def request_authorization(
        self,
        *,
        tool_name: str,
        tool_call_id: str,
        run_id: str,
        job_id: str,
        round_index: int,
        arguments: dict[str, Any],
        envelope: Any | None,
        tool_type: str | None = None,
        tool_call_pk: str | None = None,
    ) -> AuthorizationDecision:
        """Publish ``tool.call.requested`` and return the Rust authorization decision.

        Public L0 tools return ``mode="execute"`` without blocking.
        Protected tools block until a ``tool_lease``, ``tool_denied``,
        or ``tool_proposal_only`` command arrives for this
        ``tool_call_pk``, or until the timeout elapses.
        """
        resolved_tool_type = tool_type or self._resolve_tool_type(tool_name)
        resolved_tool_call_pk = tool_call_pk or _hash_tool_call_pk(run_id, tool_call_id)
        event_sequence = await self.next_event_sequence(run_id)
        args_hash_value = canonical_args_hash(arguments)
        args_summary = _summarize_arguments(arguments)
        idempotency_key = tool_call_idempotency_key(
            run_id,
            round_index,
            tool_call_id,
            tool_name,
            arguments,
        )

        tool_definition = self._lookup_tool_definition(tool_name)
        governance: ResolvedToolGovernance
        if tool_definition is not None:
            governance = self._governance_resolver.resolve(tool_definition)
        else:
            governance = _default_governance_for_unresolved(tool_name)

        requester = _read_requester(envelope)
        entity_allowlist = self._resolve_entity_allowlist(envelope, tool_name)
        object_decisions = _evaluate_object_decisions(arguments, governance)

        context = GateContext(
            run_id=run_id,
            job_id=job_id,
            round_index=round_index,
            event_sequence=event_sequence,
            tool_call_pk=resolved_tool_call_pk,
            tool_call_id=tool_call_id,
            tool_name=tool_name,
            tool_type=resolved_tool_type,
            arguments=arguments,
            args_summary=args_summary,
            envelope=envelope,
            requester=requester,
            entity_allowlist=entity_allowlist,
            object_decisions=object_decisions,
            governance=governance,
            idempotency_key=idempotency_key,
            args_hash_value=args_hash_value,
        )

        is_public_direct = bool(governance.get("authorization_mode") == "public_direct" and governance.get("public"))
        is_protected = not is_public_direct

        payload_extras: dict[str, Any] = {
            "requester": requester,
            "entity_allowlist": entity_allowlist,
            "governance": dict(governance),
            "object_decisions": object_decisions,
        }

        envelope_dict = build_tool_call_requested(
            run_id=run_id,
            job_id=job_id,
            round_index=round_index,
            event_sequence=event_sequence,
            idempotency_key=idempotency_key,
            tool_call_pk=resolved_tool_call_pk,
            tool_call_id=tool_call_id,
            tool_name=tool_name,
            tool_type=resolved_tool_type,
            args_hash=args_hash_value,
            args_summary=args_summary,
            authorization_mode=("public_direct" if is_public_direct else "rust_pdp"),
            max_retries=int((governance.get("retry_policy") or {}).get("max_retries") or 0),
            timeout_seconds=int(governance.get("timeout_seconds") or 30),
        )
        envelope_dict["payload"].update(payload_extras)

        try:
            await self._publish_event(envelope_dict, context="tool.call.requested")
        except AiRuntimeEventPublishError as exc:
            if is_protected:
                logger.error(
                    "ai_mq_publish_failed_protected_tool_aborted",
                    extra={
                        "tool_name": tool_name,
                        "tool_call_id": tool_call_id,
                        "run_id": run_id,
                    },
                )
                raise ToolAuthorizationError(
                    f"TOOL_MQ_PUBLISH_FAILED: refusing to execute protected tool {tool_name!r}: {exc}"
                ) from exc
            logger.warning(
                "ai_mq_publish_failed_public_tool_continues",
                extra={
                    "tool_name": tool_name,
                    "tool_call_id": tool_call_id,
                    "run_id": run_id,
                },
            )

        if not is_protected:
            from src.infrastructure.ai.monitoring.prometheus_exporter import inc_mq_gate_decision

            inc_mq_gate_decision("public_direct")
            return AuthorizationDecision(mode="execute", context=context)

        if requester is None or not requester.get("user_id"):
            raise ToolAuthContextRequired(tool_name, tool_call_id)

        timeout_seconds = float(governance.get("timeout_seconds") or 30) + self._authorization_margin_seconds
        command = await self._wait_for_authorization_decision(
            run_id=run_id,
            tool_call_pk=resolved_tool_call_pk,
            timeout_seconds=timeout_seconds,
        )
        if command is None:
            from src.infrastructure.ai.monitoring.prometheus_exporter import inc_mq_gate_decision

            inc_mq_gate_decision("timeout")
            raise ToolAuthorizationTimeout(tool_name, tool_call_id, timeout_seconds)

        command_type = command.get("command_type")
        payload_dict = command.get("payload") or {}
        if command_type == "tool_denied":
            code = str(payload_dict.get("code") or "TOOL_ACTOR_PERMISSION_DENIED")
            message = str(payload_dict.get("message") or "denied by Rust")
            from src.infrastructure.ai.monitoring.prometheus_exporter import inc_mq_gate_decision

            inc_mq_gate_decision("denied")
            return AuthorizationDecision(
                mode="denied",
                context=context,
                denial_code=code,
                denial_message=message,
            )
        if command_type == "tool_proposal_only":
            from src.infrastructure.ai.monitoring.prometheus_exporter import inc_mq_gate_decision

            inc_mq_gate_decision("proposal_only")
            return AuthorizationDecision(mode="proposal_only", context=context)
        if command_type == "tool_lease":
            from src.infrastructure.ai.monitoring.prometheus_exporter import inc_mq_gate_decision

            inc_mq_gate_decision("protected_lease")
            return AuthorizationDecision(mode="execute", context=context)

        logger.error(
            "ai_mq_unexpected_command_type",
            extra={
                "tool_name": tool_name,
                "tool_call_id": tool_call_id,
                "command_type": command_type,
            },
        )
        raise ToolAuthorizationError(
            f"TOOL_UNEXPECTED_COMMAND: unexpected command type {command_type!r} for {tool_name!r}"
        )

    async def publish_result(
        self,
        *,
        context: GateContext,
        status: Literal["succeeded", "failed", "cancelled", "expired", "denied", "proposal_only"],
        duration_ms: int,
        result: Any = None,
        error_code: str | None = None,
        error_message: str | None = None,
        proposal_ids: list[str] | None = None,
    ) -> None:
        """Publish a ``tool.result`` event for the gate's tool call."""
        event_sequence = await self.next_event_sequence(context.run_id)
        result_hash = None
        result_summary: dict[str, Any] | None = None
        if status == "succeeded" and result is not None:
            result_hash = canonical_args_hash({"result": result})
            summarized = _summarize_arguments({"result": result})
            result_summary = summarized if isinstance(summarized, dict) else {}
        envelope_dict = build_tool_result(
            run_id=context.run_id,
            job_id=context.job_id,
            round_index=context.round_index,
            event_sequence=event_sequence,
            idempotency_key=context.idempotency_key,
            tool_call_pk=context.tool_call_pk,
            tool_call_id=context.tool_call_id,
            tool_name=context.tool_name,
            status=status,
            duration_ms=duration_ms,
            result_hash=result_hash,
            result_summary=result_summary,
            error_code=error_code,
            error_message=error_message,
            proposal_ids=proposal_ids or [],
        )
        try:
            await self._publish_event(envelope_dict, context="tool.result")
        except AiRuntimeEventPublishError:
            logger.warning(
                "ai_mq_result_publish_failed_continuing",
                extra={
                    "tool_name": context.tool_name,
                    "tool_call_id": context.tool_call_id,
                    "status": status,
                },
            )

    async def start_heartbeat(
        self,
        *,
        context: GateContext,
    ) -> tuple[asyncio.Task[None] | None, asyncio.Event | None]:
        """Spawn a heartbeat background task for long-running tools.

        Returns ``(task, stop_event)`` when the tool is long enough to
        warrant heartbeats; otherwise returns ``(None, None)`` and the
        caller can skip heartbeat management entirely.
        """
        timeout_seconds = float(context.governance.get("timeout_seconds") or 30)
        threshold = max(self._heartbeat_interval_seconds, timeout_seconds / 3.0)
        if threshold <= self._heartbeat_interval_seconds:
            return None, None
        stop_event = asyncio.Event()
        task = asyncio.create_task(
            self._heartbeat_loop(
                context=context,
                idempotency_key=context.idempotency_key,
                stop_event=stop_event,
            )
        )
        task.set_name(f"tool-heartbeat-{context.tool_call_id}")
        return task, stop_event

    async def stop_heartbeat(
        self,
        task: asyncio.Task[None] | None,
        stop_event: asyncio.Event | None,
    ) -> None:
        if stop_event is not None:
            stop_event.set()
        if task is None:
            return
        task.cancel()
        try:
            await task
        except (asyncio.CancelledError, Exception):  # noqa: BLE001 - cancel cleanup
            return

    async def _publish_event(
        self,
        envelope: dict[str, Any],
        *,
        context: str,
    ) -> None:
        try:
            await self._publisher.publish(envelope)
        except AiRuntimeEventPublishError as exc:
            logger.error(
                "ai_mq_publish_failed",
                extra={
                    "event_id": envelope.get("event_id"),
                    "event_type": envelope.get("event_type"),
                    "context": context,
                },
                exc_info=exc,
            )
            raise

    async def _heartbeat_loop(
        self,
        *,
        context: GateContext,
        idempotency_key: str,
        stop_event: asyncio.Event,
    ) -> None:
        try:
            while not stop_event.is_set():
                await asyncio.sleep(self._heartbeat_interval_seconds)
                if stop_event.is_set():
                    return
                seq = await self.next_event_sequence(context.run_id)
                envelope_dict = build_heartbeat(
                    run_id=context.run_id,
                    job_id=context.job_id,
                    round_index=context.round_index,
                    event_sequence=seq,
                    idempotency_key=idempotency_key,
                    tool_call_pk=context.tool_call_pk,
                )
                try:
                    await self._publisher.publish(envelope_dict)
                except Exception as exc:  # noqa: BLE001 - heartbeat failures are non-fatal
                    logger.warning(
                        "ai_mq_heartbeat_publish_failed",
                        extra={
                            "tool_name": context.tool_name,
                            "tool_call_id": context.tool_call_id,
                        },
                        exc_info=exc,
                    )
        except asyncio.CancelledError:
            return

    async def _wait_for_authorization_decision(
        self,
        *,
        run_id: str,
        tool_call_pk: str,
        timeout_seconds: float,
    ) -> dict[str, Any] | None:
        """Wait for a Rust authorization command targeted at this tool_call_pk.

        Only uses the targeted ``CommandDispatcher``/``ToolCommandWaiter``
        path. There is no global ``fetch_pending`` fallback — consuming the
        global queue inside a single-tool wait path would steal commands
        from other tools and cause lease starvation.

        Returns ``None`` if the command waiter is not configured or the
        deadline expires without a matching command.
        """
        deadline = time.monotonic() + max(0.0, float(timeout_seconds))
        if self._command_waiter is None:
            logger.debug(
                "ai_mq_wait_no_waiter",
                extra={"run_id": run_id, "tool_call_pk": tool_call_pk},
            )
            return None
        remaining = max(0.0, deadline - time.monotonic())
        return await self._command_waiter.wait(tool_call_pk, timeout=remaining)

    def _lookup_tool_definition(self, tool_name: str) -> ToolDefinition | None:
        if self._tool_definition_lookup is None:
            return None
        try:
            return self._tool_definition_lookup(tool_name)
        except Exception as exc:  # noqa: BLE001 - resolver must never break the gate
            logger.warning(
                "ai_mq_tool_definition_lookup_failed",
                extra={"tool_name": tool_name},
                exc_info=exc,
            )
            return None

    def _resolve_tool_type(self, tool_name: str) -> str:
        if self._tool_type_resolver is None:
            return "unknown"
        try:
            value = self._tool_type_resolver(tool_name)
        except Exception as exc:  # noqa: BLE001 - type resolver must never break the gate
            logger.warning(
                "ai_mq_tool_type_resolver_failed",
                extra={"tool_name": tool_name},
                exc_info=exc,
            )
            return "unknown"
        return value or "unknown"

    def _resolve_entity_allowlist(self, envelope: Any, tool_name: str) -> list[str]:
        if self._entity_allowlist_resolver is None:
            return []
        try:
            result = self._entity_allowlist_resolver(envelope)
        except Exception as exc:  # noqa: BLE001 - resolver must never break the gate
            logger.warning(
                "ai_mq_entity_allowlist_resolver_failed",
                extra={"tool_name": tool_name},
                exc_info=exc,
            )
            return []
        if result is None:
            return []
        return [str(item) for item in result]


def build_runtime_event_idempotency_key(
    run_id: str,
    round_index: int,
    tool_call_id: str,
    tool_name: str,
    args: dict[str, Any],
) -> str:
    """Public re-export of :func:`tool_call_idempotency_key` for the gate."""
    return tool_call_idempotency_key(run_id, round_index, tool_call_id, tool_name, args)


__all__ = [
    "AUTHORIZATION_MARGIN_SECONDS",
    "HEARTBEAT_INTERVAL_SECONDS",
    "WAIT_POLL_INTERVAL_SECONDS",
    "AuthorizationDecision",
    "EntityAllowlistResolver",
    "GateContext",
    "ToolAuthContextRequired",
    "ToolAuthorizationError",
    "ToolAuthorizationTimeout",
    "ToolDefinitionLookup",
    "ToolDeniedByRust",
    "ToolMqGate",
    "ToolTypeResolver",
    "build_runtime_event_idempotency_key",
    "make_tool_call_pk",
    "make_uuid_tool_call_pk",
]
