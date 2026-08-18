"""Resume functionality for LLMStreamRunner (Task D2).

Asserts (docs/plans/2026-08-14-hybrid-agent-architecture.md, Task D2):

1. Resume from checkpoint restores working memory + transcript summary
2. Resume connects to LLMStreamRunner.stream_chat_with_tools
3. Resume targets the same checkpoint types the Rust control plane picks
   (``before_tool`` / ``after_tool``; ``after_completion`` for replays)
4. Checkpoint data loaded from Postgres ai_run_checkpoints table
5. Partial execution rolls back to consistent state before resuming

The ``ai_run_checkpoints`` rows are written by the Rust
``AiExecutionControlService.handle_checkpoint`` consumer from the durable
MQ checkpoint events the sidecar publishes (Task D1). The resume command
(``ai_runtime_commands.resume_run``) is enqueued by the Rust resume route
and consumed by the sidecar command dispatcher, which hands it to
:class:`ResumeHandler`.
"""

from __future__ import annotations

import json
import logging
from collections.abc import AsyncIterator
from dataclasses import dataclass, field
from typing import Any

logger = logging.getLogger(__name__)

# Checkpoint types the runner can safely continue from. Mirrors the Rust
# resume contract: ``latest_recoverable`` returns the latest persisted
# BeforeTool/AfterTool row; after_completion allows replaying a finished run.
RESUMABLE_CHECKPOINT_TYPES = ("before_tool", "after_tool", "after_completion")

_CHECKPOINT_COLUMNS = (
    "checkpoint_id, run_id, sequence_no, checkpoint_type, snapshot, created_at"
)


@dataclass
class RunCheckpoint:
    """Represents a saved checkpoint for run restoration."""

    checkpoint_id: str
    run_id: str
    checkpoint_type: str  # "before_tool", "after_tool", "before_proposal_ingest", "after_completion", ...
    round_index: int
    created_at: float

    # Core state for resume
    messages: list[dict[str, Any]] = field(default_factory=list)
    tool_call_results: list[dict[str, Any]] = field(default_factory=list)
    context_snapshot: dict[str, Any] = field(default_factory=dict)

    # Metadata
    entity_id: str = ""
    model: str = ""
    tool_calls_pending: list[dict[str, Any]] = field(default_factory=list)
    error_state: str | None = None

    @property
    def is_resumeable(self) -> bool:
        """Check if this checkpoint type supports resume."""
        return self.checkpoint_type in RESUMABLE_CHECKPOINT_TYPES


@dataclass
class ResumeContext:
    """Context object for run restoration."""

    run_id: str
    checkpoint: RunCheckpoint
    restored_messages: list[dict[str, Any]] = field(default_factory=list)
    working_memory: dict[str, Any] = field(default_factory=dict)
    allowed_tool_names: set[str] = field(default_factory=set)

    def to_message_list(self) -> list[dict[str, Any]]:
        """Convert restored context to message list for LLM."""
        # Return copy of restored_messages, fallback to checkpoint.messages
        if self.restored_messages:
            return list(self.restored_messages)
        elif self.checkpoint.messages:
            return list(self.checkpoint.messages)
        return []


def _decode_jsonb(value: Any) -> Any:
    """asyncpg returns JSONB as ``str`` unless a codec is registered."""
    if isinstance(value, str):
        try:
            return json.loads(value)
        except ValueError:
            return {}
    return value or {}


def _row_to_checkpoint(row: Any) -> RunCheckpoint:
    """Map an ``ai_run_checkpoints`` row (asyncpg Record or dict) to a checkpoint."""
    getter = row.get if hasattr(row.get, "__call__") else None
    if getter is None:  # pragma: no cover - defensive
        raise TypeError(f"unsupported checkpoint row type: {type(row)!r}")
    snapshot = _decode_jsonb(getter("snapshot"))
    if not isinstance(snapshot, dict):
        snapshot = {}
    created_at = getter("created_at")
    return RunCheckpoint(
        checkpoint_id=str(getter("checkpoint_id") or ""),
        run_id=str(getter("run_id") or ""),
        checkpoint_type=str(getter("checkpoint_type") or ""),
        # The table stores the monotonic ``sequence_no``; for sidecar-emitted
        # checkpoints that is an epoch-millis value, for Rust-written rows a
        # small counter. It is the ordering key, surfaced here as round_index.
        round_index=int(getter("sequence_no") or 0),
        created_at=created_at.timestamp() if hasattr(created_at, "timestamp") else float(created_at or 0.0),
        messages=list(snapshot.get("messages") or []),
        tool_call_results=list(snapshot.get("results") or snapshot.get("pending_results") or []),
        context_snapshot=snapshot,
        entity_id=str(snapshot.get("entity_id") or ""),
        model=str(snapshot.get("model") or ""),
        tool_calls_pending=list(snapshot.get("tool_calls") or []),
    )


class CheckpointLoader:
    """Loads checkpoints from ``ai_run_checkpoints`` (Postgres, read-only).

    The pool follows the same contract as :class:`AiCommandPoller` /
    :class:`AsyncpgAIConfigStore`: an asyncpg-style pool whose ``acquire()``
    works as an async context manager. When no pool is available (degraded
    mode) every loader method returns ``None`` / ``[]``.
    """

    _instance: CheckpointLoader | None = None

    def __init__(self, pool: Any | None = None):
        self._db_pool = pool

    @classmethod
    def get_instance(cls) -> CheckpointLoader:
        """Get singleton instance."""
        if cls._instance is None:
            cls._instance = CheckpointLoader()
        return cls._instance

    def _resolve_pool(self) -> Any | None:
        """Return the configured pool, else the container's shared PG pool."""
        if self._db_pool is not None:
            return self._db_pool
        try:
            from src.infrastructure.ai.ai_container import get_ai_container

            container = get_ai_container()
            pool = container.resolve("pg_shared_context_pool", None)
            if pool is None:
                pool = container.resolve("db_pool", None)
            return pool
        except Exception:  # noqa: BLE001 - composition root lookups are best-effort
            return None

    async def _fetchrow(self, query: str, *args: Any) -> Any | None:
        pool = self._resolve_pool()
        if pool is None:
            logger.warning("[D2] CheckpointLoader has no Postgres pool; cannot load checkpoints")
            return None
        async with pool.acquire() as conn:
            return await conn.fetchrow(query, *args)

    async def _fetch(self, query: str, *args: Any) -> list[Any]:
        pool = self._resolve_pool()
        if pool is None:
            logger.warning("[D2] CheckpointLoader has no Postgres pool; cannot load checkpoints")
            return []
        async with pool.acquire() as conn:
            return await conn.fetch(query, *args)

    async def load_checkpoint(self, checkpoint_id: str) -> RunCheckpoint | None:
        """Load specific checkpoint by ID."""
        logger.info(f"Loading checkpoint {checkpoint_id}")
        row = await self._fetchrow(
            f"SELECT {_CHECKPOINT_COLUMNS} FROM ai_run_checkpoints WHERE checkpoint_id = $1",
            checkpoint_id,
        )
        return _row_to_checkpoint(row) if row is not None else None

    async def load_latest_checkpoint(
        self,
        run_id: str,
        max_round_index: int | None = None,
    ) -> RunCheckpoint | None:
        """Load latest checkpoint for a run (optionally capped by sequence)."""
        logger.info(f"Loading latest checkpoint for run={run_id}, sequence <= {max_round_index}")
        if max_round_index is None:
            row = await self._fetchrow(
                f"SELECT {_CHECKPOINT_COLUMNS} FROM ai_run_checkpoints "
                "WHERE run_id = $1 ORDER BY sequence_no DESC LIMIT 1",
                run_id,
            )
        else:
            row = await self._fetchrow(
                f"SELECT {_CHECKPOINT_COLUMNS} FROM ai_run_checkpoints "
                "WHERE run_id = $1 AND sequence_no <= $2 ORDER BY sequence_no DESC LIMIT 1",
                run_id,
                max_round_index,
            )
        return _row_to_checkpoint(row) if row is not None else None

    async def list_checkpoints(self, run_id: str) -> list[RunCheckpoint]:
        """List all checkpoints for a run in sequence order."""
        logger.info(f"Listing checkpoints for run={run_id}")
        rows = await self._fetch(
            f"SELECT {_CHECKPOINT_COLUMNS} FROM ai_run_checkpoints "
            "WHERE run_id = $1 ORDER BY sequence_no",
            run_id,
        )
        return [_row_to_checkpoint(row) for row in rows]

    async def load_run_input_snapshot(self, run_id: str) -> dict[str, Any] | None:
        """Load the ``run_input`` checkpoint snapshot (the original input envelope).

        Written by the Rust run starter right after ``ai_runs.input_envelope``;
        the resume path uses it to rebuild the :class:`ContextEnvelope`.
        """
        row = await self._fetchrow(
            f"SELECT {_CHECKPOINT_COLUMNS} FROM ai_run_checkpoints "
            "WHERE run_id = $1 AND checkpoint_type = 'run_input' "
            "ORDER BY sequence_no ASC LIMIT 1",
            run_id,
        )
        if row is None:
            return None
        snapshot = _decode_jsonb(row.get("snapshot"))
        return snapshot if isinstance(snapshot, dict) else None


class RunRestorer:
    """Restores run state from checkpoint."""

    def __init__(self, checkpoint_loader: CheckpointLoader):
        self._loader = checkpoint_loader

    async def restore_to_checkpoint(
        self,
        checkpoint: RunCheckpoint,
    ) -> ResumeContext:
        """Restore run state to given checkpoint.

        Strategy:
        1. Restore the transcript summary from the checkpoint snapshot
           (never a full token replay — the snapshot carries message
           references and tool-result digests).
        2. Reconstruct working memory from the checkpoint ``context_snapshot``
           (``WorkingMemory.from_dict`` accepts the nested ``working_memory``
           shape written by Task D1).
        3. Surface tool call results as a compact transcript note.

        Args:
            checkpoint: Target checkpoint for restoration

        Returns:
            ResumeContext with restored state
        """
        logger.info(f"Restoring to checkpoint {checkpoint.checkpoint_id} (round {checkpoint.round_index})")

        context = ResumeContext(
            run_id=checkpoint.run_id,
            checkpoint=checkpoint,
        )

        # Restore messages
        if checkpoint.messages:
            context.restored_messages = list(checkpoint.messages)
        else:
            # Fallback to checkpoint's context snapshot
            context.restored_messages = list(checkpoint.context_snapshot.get("messages") or [])

        # Restore working memory (Task B2 snapshot rides the checkpoint
        # context_snapshot under the "working_memory" key).
        context.working_memory.update(checkpoint.context_snapshot)

        # Summarize completed tool results into the transcript instead of
        # replaying raw tool payloads.
        if checkpoint.tool_call_results:
            summary_lines = [
                f"[resume from checkpoint {checkpoint.checkpoint_id} "
                f"seq={checkpoint.round_index} type={checkpoint.checkpoint_type}]"
            ]
            for result in checkpoint.tool_call_results:
                name = result.get("tool_name") or result.get("tool_call_id") or "tool"
                digest = json.dumps(result.get("result", result), ensure_ascii=False, default=str)[:300]
                summary_lines.append(f"- {name}: {digest}")
            context.restored_messages.append({
                "role": "assistant",
                "content": "\n".join(summary_lines),
            })

        return context


class ResumeHandler:
    """Handles resume commands and coordinates restoration."""

    def __init__(
        self,
        checkpoint_loader: CheckpointLoader,
        restorer: RunRestorer | None = None,
    ):
        self._loader = checkpoint_loader
        self._restorer = restorer or RunRestorer(checkpoint_loader)

    async def handle_resume(
        self,
        command: dict[str, Any],
    ) -> ResumeContext:
        """Handle resume_run command.

        Expected command payload (enqueued by the Rust resume route):
        {
            "checkpoint_id": "chk_xxx",  # or use run_id to find latest
            "run_id": "run_xxx",
            "force_round_index": 2  # optional, force specific round
        }

        Args:
            command: Resume command from MQ

        Returns:
            ResumeContext ready for LLMStreamRunner
        """
        payload = command.get("payload", {})
        checkpoint_id = payload.get("checkpoint_id")
        run_id = payload.get("run_id") or command.get("run_id")
        force_round = payload.get("force_round_index")

        if not checkpoint_id and not run_id:
            raise ValueError("resume_run requires checkpoint_id or run_id")

        # Load checkpoint
        if checkpoint_id:
            checkpoint = await self._loader.load_checkpoint(checkpoint_id)
        else:
            checkpoint = await self._loader.load_latest_checkpoint(
                run_id=run_id,
                max_round_index=force_round,
            )

        if not checkpoint:
            raise RuntimeError(f"No checkpoint found for checkpoint_id={checkpoint_id}, run_id={run_id}")

        # Validate checkpoint is resumeable
        if not checkpoint.is_resumeable:
            raise RuntimeError(
                f"Checkpoint type {checkpoint.checkpoint_type} not suitable for resume. "
                f"Only {', '.join(RESUMABLE_CHECKPOINT_TYPES)} supported."
            )

        # Restore state
        context = await self._restorer.restore_to_checkpoint(checkpoint)

        logger.info(
            f"Resume successful: run={context.run_id}, "
            f"round={context.checkpoint.round_index}, "
            f"messages={len(context.restored_messages)}"
        )

        return context

    async def resume_and_continue(
        self,
        command: dict[str, Any],
        envelope: Any,
        runner_class: Any,
        gateway: Any,
        tool_executor: Any,
        allowed_tools: set[str],
        on_child_event: Any = None,
    ) -> AsyncIterator[dict[str, Any]]:
        """Resume run and continue execution via LLMStreamRunner.

        This is the main entry point for Task D2 integration.

        Args:
            command: Resume command
            envelope: ContextEnvelope for the run
            runner_class: LLMStreamRunner class
            gateway: AiGateway client
            tool_executor: ToolExecutor instance
            allowed_tools: Set of allowed tool names
            on_child_event: Optional sub-agent/checkpoint event callback

        Yields:
            Events from the resumed execution
        """
        # Restore state (J1: resume outcome counter, success/failure).
        from src.infrastructure.ai.monitoring.prometheus_exporter import inc_resume
        from src.infrastructure.ai.working_memory import WorkingMemory

        try:
            context = await self.handle_resume(command)
        except Exception:
            inc_resume("failed")
            raise
        inc_resume("success")
        context.allowed_tool_names = set(allowed_tools or set())

        # B2: rebuild the workspace from the checkpoint snapshot; the runner
        # keeps spilling large tool results into it for the rest of the run.
        working_memory = WorkingMemory.from_dict(
            context.working_memory or context.checkpoint.context_snapshot
        )
        working_memory.run_id = context.run_id

        # Inject the restored transcript summary into the envelope.
        envelope.conversation_history = context.to_message_list()

        # Create runner
        runner = runner_class(
            client=gateway,
            tool_executor=tool_executor,
        )

        yield {
            "type": "progress",
            "step": "resumed",
            "run_id": context.run_id,
            "from_checkpoint": context.checkpoint.checkpoint_id,
            "round_index": context.checkpoint.round_index,
        }

        # Continue execution through the single production loop; checkpoint
        # emission (Task D1) and round-boundary cancellation (Task D4) apply.
        messages: list[dict[str, Any]] = [
            {
                "role": "system",
                "content": (
                    "This run was resumed from a durable checkpoint. The "
                    "conversation history contains the transcript summary; the "
                    "working memory workspace (plan.md / notes.md / "
                    "evidence.json) was restored. Continue the task without "
                    "repeating completed tool calls."
                ),
            },
            *context.to_message_list(),
        ]
        async for event in runner.stream_chat_with_tools(
            messages=messages,
            model=context.checkpoint.model or "default",
            run_id=context.run_id,
            envelope=envelope,
            working_memory=working_memory,
            on_child_event=on_child_event,
        ):
            yield event

    async def resume_via_runtime_service(self, command: dict[str, Any], runtime_service: Any) -> None:
        """Resume through the production entry point (``stream_run_with_tools``).

        Rebuilds the :class:`ContextEnvelope` from the run's ``run_input``
        checkpoint (the persisted input envelope), restores the transcript
        summary as ``conversation_history`` and injects the restored working
        memory, then consumes the run to completion.
        """
        # J1: resume outcome counter, success/failure.
        from src.infrastructure.ai.context_envelope import ContextEnvelope
        from src.infrastructure.ai.monitoring.prometheus_exporter import inc_resume
        from src.infrastructure.ai.working_memory import WorkingMemory

        try:
            context = await self.handle_resume(command)
        except Exception:
            inc_resume("failed")
            raise
        inc_resume("success")

        input_snapshot = await self._loader.load_run_input_snapshot(context.run_id)
        if not input_snapshot:
            raise RuntimeError(
                f"Cannot resume run {context.run_id}: no run_input checkpoint "
                "to rebuild the input envelope from"
            )
        envelope = ContextEnvelope(**input_snapshot)
        envelope.run_id = context.run_id
        envelope.conversation_history = context.to_message_list()
        # A resumed run starts un-cancelled even if a stale flag was persisted.
        envelope.cancelled = False

        working_memory = WorkingMemory.from_dict(
            context.working_memory or context.checkpoint.context_snapshot
        )
        working_memory.run_id = context.run_id

        logger.info(
            f"[D2] Resuming run {context.run_id} from checkpoint "
            f"{context.checkpoint.checkpoint_id} via runtime service"
        )
        async for _event in runtime_service.stream_run_with_tools(
            envelope,
            resume_working_memory=working_memory,
        ):
            pass


def build_resume_handler(
    checkpoint_loader: CheckpointLoader | None = None,
) -> ResumeHandler:
    """Build resume handler for command dispatcher."""
    loader = checkpoint_loader or CheckpointLoader.get_instance()
    return ResumeHandler(loader)


__all__ = [
    "RESUMABLE_CHECKPOINT_TYPES",
    "RunCheckpoint",
    "ResumeContext",
    "CheckpointLoader",
    "RunRestorer",
    "ResumeHandler",
    "build_resume_handler",
]
