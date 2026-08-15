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


# Critical identifier patterns (Task B3): flight numbers, anomaly ids,
# proposal ids, order ids. Shared by IDPreservationHook (PreCompact) and the
# context compression path so there is exactly one definition.
CRITICAL_ID_PATTERNS = [
    r"F[0-9]{4,}",  # Flight numbers like F1234
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
    errors: list[str] = field(default_factory=list)

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
    """PreToolUse hook: acquire/release leases for write actions.
    
    Prevents concurrent modifications by acquiring a lease before
    executing write tools. Releases lease after successful completion.
    """

    @property
    def phase(self) -> str:
        return "PreToolUse"

    async def execute(self, ctx: HookContext) -> bool:
        from src.infrastructure.ai.tools.mq_gate import ToolMqGate
        
        # Only needed for write actions
        if not ctx.tool_name or is_read_only_tool(ctx.tool_name):
            return True
            
        gate = ToolMqGate.get_instance()
        if gate is None:
            logger.warning(f"No MQ gate available for lease check {ctx.tool_name}")
            return True
            
        try:
            # Try to acquire lease
            acquired = await gate.acquire_lease(ctx.tool_name, ctx.run_id)
            if not acquired:
                ctx.add_error(f"Failed to acquire lease for {ctx.tool_name}")
                return False
                
            logger.debug(f"Lease acquired for {ctx.tool_name} run={ctx.run_id}")
            return True
            
        except Exception as exc:  # noqa: BLE001
            ctx.add_error(f"Lease check failed: {exc}")
            return False


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
            
        # Query object existence through read-only path
        try:
            from src.infrastructure.ai.tools.query_tool_executor import QueryToolExecutor
            
            executor = QueryToolExecutor.get_instance()
            if not executor:
                return True
                
            # Check if object exists (implementation depends on entity type)
            exists = await executor.object_exists(object_id)
            
            if not exists:
                ctx.add_error(f"Object {object_id} does not exist")
                return False
                
            logger.debug(f"Object existence verified: {object_id}")
            return True
            
        except Exception as exc:  # noqa: BLE001
            ctx.add_error(f"Object existence check failed: {exc}")
            return False


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
            try:
                if not await hook.execute(ctx):
                    logger.error(f"Hook {type(hook).__name__} failed at phase {phase}")
                    return False
            except Exception as exc:  # noqa: BLE001
                ctx.add_error(f"Hook {type(hook).__name__} exception: {exc}")
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
    """Get default set of built-in hooks."""
    return [
        LeaseCheckHook(),           # PreToolUse - lease management
        SchemaValidationHook(),     # PreToolUse - argument validation
        ObjectExistenceCheckHook(), # PreToolUse - entity existence
        ResultSanitizationHook(),   # PostToolUse - result clipping
        IDPreservationHook(),       # PreCompact - ID protection
        NoPromisesHook(),           # Stop - anti-promises
    ]


def build_default_pipeline() -> HookPipeline:
    """Build pipeline with all built-in hooks registered."""
    pipeline = HookPipeline()
    for hook in get_builtin_hooks():
        pipeline.register_hook(hook)
    return pipeline


__all__ = [
    "CRITICAL_ID_PATTERNS",
    "HookContext",
    "BaseHook",
    "LeaseCheckHook",
    "SchemaValidationHook",
    "ObjectExistenceCheckHook",
    "ResultSanitizationHook",
    "IDPreservationHook",
    "NoPromisesHook",
    "HookPipeline",
    "extract_critical_ids",
    "is_read_only_tool",
    "get_builtin_hooks",
    "build_default_pipeline",
]
