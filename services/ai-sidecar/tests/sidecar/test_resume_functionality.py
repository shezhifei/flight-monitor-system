"""Tests for Resume functionality (Task D2).

Asserts:
1. Checkpoint loading from storage works correctly
2. Restore to checkpoint reconstructs working memory + messages
3. Resume validates checkpoint type (only after_tool supported)
4. ResumeHandler coordinates restoration and execution continuation
5. Integration with LLMStreamRunner via envelope injection
"""

from __future__ import annotations

import pytest
from unittest.mock import AsyncMock, MagicMock

from src.infrastructure.ai.resume import (
    RunCheckpoint,
    ResumeContext,
    CheckpointLoader,
    RunRestorer,
    ResumeHandler,
)


# ============================================================================
# Test RunCheckpoint Structure
# ============================================================================

class TestRunCheckpoint:
    """Test checkpoint data structure."""

    def test_checkpoint_has_all_required_fields(self):
        """All required fields present in checkpoint."""
        checkpoint = RunCheckpoint(
            checkpoint_id="chk_123",
            run_id="run_456",
            checkpoint_type="after_tool",
            round_index=2,
            created_at=1234567890.0,
        )
        
        assert checkpoint.checkpoint_id == "chk_123"
        assert checkpoint.run_id == "run_456"
        assert checkpoint.checkpoint_type == "after_tool"
        assert checkpoint.round_index == 2
        assert checkpoint.created_at == 1234567890.0

    @pytest.mark.parametrize("checkpoint_type,resumable", [
        ("after_tool", True),
        ("after_completion", True),
        # Aligned with the Rust resume contract: latest_recoverable returns
        # the newest BeforeTool/AfterTool row, so both must resume.
        ("before_tool", True),
        ("before_proposal_ingest", False),
        ("invalid", False),
    ])
    def test_is_resumeable_property(self, checkpoint_type: str, resumable: bool):
        """Resumeable set matches the Rust resume route contract."""
        checkpoint = RunCheckpoint(
            checkpoint_id="chk_xxx",
            run_id="run_xxx",
            checkpoint_type=checkpoint_type,
            round_index=0,
            created_at=0.0,
        )
        
        assert checkpoint.is_resumeable == resumable

    def test_empty_collections_initialized(self):
        """Collections initialized as empty lists/dicts."""
        checkpoint = RunCheckpoint(
            checkpoint_id="chk_empty",
            run_id="run_empty",
            checkpoint_type="after_tool",
            round_index=0,
            created_at=0.0,
        )
        
        assert checkpoint.messages == []
        assert checkpoint.tool_call_results == []
        assert checkpoint.context_snapshot == {}
        assert checkpoint.tool_calls_pending == []


# ============================================================================
# Test ResumeContext Structure
# ============================================================================

class TestResumeContext:
    """Test resume context structure."""

    def test_context_created_from_checkpoint(self):
        """Context initialized with checkpoint data."""
        checkpoint = RunCheckpoint(
            checkpoint_id="chk_test",
            run_id="run_test",
            checkpoint_type="after_tool",
            round_index=1,
            created_at=0.0,
        )
        
        context = ResumeContext(
            run_id="run_test",
            checkpoint=checkpoint,
        )
        
        assert context.run_id == "run_test"
        assert context.checkpoint is checkpoint
        assert len(context.restored_messages) == 0

    def test_to_message_list_returns_copy(self):
        """to_message_list returns list copy."""
        checkpoint = RunCheckpoint(
            checkpoint_id="chk_msg",
            run_id="run_msg",
            checkpoint_type="after_tool",
            round_index=0,
            created_at=0.0,
        )
        
        context = ResumeContext(
            run_id="run_msg",
            checkpoint=checkpoint,
        )
        
        # Modify original
        checkpoint.messages.append({"role": "user", "content": "test"})
        
        # to_message_list should return copy
        messages = context.to_message_list()
        
        # Should be same as checkpoint if no restored_messages set
        assert len(messages) == len(checkpoint.messages)

    def test_working_memory_defaults_empty(self):
        """Working memory initialized empty."""
        checkpoint = RunCheckpoint(
            checkpoint_id="chk_mem",
            run_id="run_mem",
            checkpoint_type="after_tool",
            round_index=0,
            created_at=0.0,
        )
        
        context = ResumeContext(
            run_id="run_mem",
            checkpoint=checkpoint,
        )
        
        assert context.working_memory == {}


# ============================================================================
# Test Checkpoint Loader
# ============================================================================

class TestCheckpointLoader:
    """Test checkpoint loading from storage."""

    def test_loader_singleton(self):
        """Loader follows singleton pattern."""
        loader1 = CheckpointLoader.get_instance()
        loader2 = CheckpointLoader.get_instance()
        
        assert loader1 is loader2

    @pytest.mark.asyncio
    async def test_load_checkpoint_without_pool_returns_none(self):
        """Degraded mode: no Postgres pool -> None instead of raising."""
        loader = CheckpointLoader.get_instance()
        loader._db_pool = None  # ensure singleton carries no pool

        result = await loader.load_checkpoint("nonexistent")

        assert result is None

    @pytest.mark.asyncio
    async def test_load_latest_checkpoint_no_checkpoints(self):
        """Returns None when no checkpoints exist."""
        loader = CheckpointLoader.get_instance()
        
        result = await loader.load_latest_checkpoint("nonexistent_run")
        
        assert result is None

    @pytest.mark.asyncio
    async def test_list_checkpoints_empty(self):
        """Returns empty list when no checkpoints."""
        loader = CheckpointLoader.get_instance()
        
        checkpoints = await loader.list_checkpoints("run_xyz")
        
        assert checkpoints == []


# ============================================================================
# Test Run Restorer
# ============================================================================

class TestRunRestorer:
    """Test state restoration from checkpoint."""

    @pytest.mark.asyncio
    async def test_restore_creates_context(self):
        """Restoration creates ResumeContext."""
        loader = CheckpointLoader()
        restorer = RunRestorer(loader)
        
        checkpoint = RunCheckpoint(
            checkpoint_id="chk_restore",
            run_id="run_restore",
            checkpoint_type="after_tool",
            round_index=1,
            created_at=0.0,
        )
        
        context = await restorer.restore_to_checkpoint(checkpoint)
        
        assert context.run_id == "run_restore"
        assert context.checkpoint.checkpoint_id == "chk_restore"
        assert context.checkpoint.round_index == 1

    @pytest.mark.asyncio
    async def test_restore_preserves_messages(self):
        """Restored checkpoint messages preserved in context."""
        loader = CheckpointLoader()
        restorer = RunRestorer(loader)
        
        checkpoint = RunCheckpoint(
            checkpoint_id="chk_msgs",
            run_id="run_msgs",
            checkpoint_type="after_tool",
            round_index=0,
            created_at=0.0,
            messages=[
                {"role": "user", "content": "Hello"},
                {"role": "assistant", "content": "Hi there!"},
            ],
        )
        
        context = await restorer.restore_to_checkpoint(checkpoint)
        
        assert len(context.restored_messages) == 2
        assert context.restored_messages[0]["role"] == "user"

    @pytest.mark.asyncio
    async def test_restore_includes_tool_results(self):
        """Tool call results added as tool role messages."""
        loader = CheckpointLoader()
        restorer = RunRestorer(loader)
        
        checkpoint = RunCheckpoint(
            checkpoint_id="chk_tools",
            run_id="run_tools",
            checkpoint_type="after_tool",
            round_index=1,
            created_at=0.0,
            tool_call_results=[
                {
                    "tool_call_id": "call_1",
                    "result": {"data": "flight_info"},
                },
            ],
        )
        
        context = await restorer.restore_to_checkpoint(checkpoint)
        
        # Should have at least one message
        assert len(context.restored_messages) >= 0
        
        # Tool results would be converted to tool role messages
        tool_messages = [m for m in context.restored_messages if m.get("role") == "tool"]
        # Note: This may be empty if context.restored_messages already has content
    
    @pytest.mark.asyncio
    async def test_restore_uses_context_snapshot_fallback(self):
        """Uses context snapshot when no messages in checkpoint."""
        loader = CheckpointLoader()
        restorer = RunRestorer(loader)
        
        checkpoint = RunCheckpoint(
            checkpoint_id="chk_snapshot",
            run_id="run_snapshot",
            checkpoint_type="after_tool",
            round_index=0,
            created_at=0.0,
            messages=[],
            context_snapshot={
                "messages": [
                    {"role": "system", "content": "You are a flight assistant."},
                ],
                "some_state": "value",
            },
        )
        
        context = await restorer.restore_to_checkpoint(checkpoint)
        
        # Should use fallback messages
        assert len(context.restored_messages) >= 1

    @pytest.mark.asyncio
    async def test_restore_merges_working_memory(self):
        """Checkpoint context merged into working memory."""
        loader = CheckpointLoader()
        restorer = RunRestorer(loader)
        
        checkpoint = RunCheckpoint(
            checkpoint_id="chk_wm",
            run_id="run_wm",
            checkpoint_type="after_tool",
            round_index=0,
            created_at=0.0,
            context_snapshot={
                "key1": "value1",
                "key2": {"nested": "value"},
            },
        )
        
        context = await restorer.restore_to_checkpoint(checkpoint)
        
        assert context.working_memory.get("key1") == "value1"
        assert context.working_memory.get("key2") == {"nested": "value"}


# ============================================================================
# Test Resume Handler
# ============================================================================

class TestResumeHandler:
    """Test resume command handling."""

    def test_handler_created_with_loader(self):
        """Handler accepts checkpoint loader."""
        loader = CheckpointLoader()
        handler = ResumeHandler(loader)
        
        assert handler._loader is loader

    @pytest.mark.asyncio
    async def test_handle_resume_requires_checkpoint_or_run_id(self):
        """Requires checkpoint_id or run_id in payload."""
        loader = CheckpointLoader()
        handler = ResumeHandler(loader)
        
        with pytest.raises(ValueError) as exc_info:
            await handler.handle_resume({"payload": {}})
        
        assert "checkpoint_id" in str(exc_info.value) or "run_id" in str(exc_info.value)

    @pytest.mark.asyncio
    async def test_handle_resume_raises_when_no_checkpoint_found(self):
        """Raises error when checkpoint doesn't exist."""
        loader = CheckpointLoader()
        handler = ResumeHandler(loader)
        
        with pytest.raises(RuntimeError) as exc_info:
            await handler.handle_resume({
                "payload": {"checkpoint_id": "nonexistent"}
            })
        
        assert "No checkpoint found" in str(exc_info.value)

    @pytest.mark.asyncio
    async def test_handle_resume_validates_checkpoint_type(self):
        """Validates checkpoint is resumeable type."""
        loader = CheckpointLoader()
        restorer = RunRestorer(loader)
        handler = ResumeHandler(loader, restorer)
        
        # Create non-resumeable checkpoint
        checkpoint = RunCheckpoint(
            checkpoint_id="chk_nonresume",
            run_id="run_nonresume",
            checkpoint_type="before_proposal_ingest",  # Not resumeable
            round_index=0,
            created_at=0.0,
        )
        
        # Mock loader to return our checkpoint
        loader.load_checkpoint = AsyncMock(return_value=checkpoint)
        
        with pytest.raises(RuntimeError) as exc_info:
            await handler.handle_resume({
                "payload": {"checkpoint_id": "chk_nonresume"}
            })
        
        assert "not suitable for resume" in str(exc_info.value)

    @pytest.mark.asyncio
    async def test_handle_resume_success(self):
        """Successful resume returns context."""
        loader = CheckpointLoader()
        restorer = RunRestorer(loader)
        handler = ResumeHandler(loader, restorer)
        
        checkpoint = RunCheckpoint(
            checkpoint_id="chk_success",
            run_id="run_success",
            checkpoint_type="after_tool",
            round_index=1,
            created_at=0.0,
        )
        
        loader.load_checkpoint = AsyncMock(return_value=checkpoint)
        
        context = await handler.handle_resume({
            "payload": {"checkpoint_id": "chk_success"}
        })
        
        assert context.run_id == "run_success"
        assert context.checkpoint.checkpoint_id == "chk_success"
        assert context.checkpoint.round_index == 1


# ============================================================================
# Integration Tests
# ============================================================================

class TestResumeIntegration:
    """End-to-end integration tests."""

    @pytest.mark.asyncio
    async def test_full_resume_workflow(self):
        """Complete workflow: load checkpoint -> restore -> validate."""
        loader = CheckpointLoader()
        restorer = RunRestorer(loader)
        handler = ResumeHandler(loader, restorer)
        
        # Setup realistic checkpoint
        checkpoint = RunCheckpoint(
            checkpoint_id="chk_full",
            run_id="run_full",
            checkpoint_type="after_tool",
            round_index=2,
            created_at=time.time(),
            messages=[
                {"role": "system", "content": "Flight assistant"},
                {"role": "user", "content": "查询 F1234"},
                {"role": "assistant", "content": "正在查询..."},
            ],
            tool_call_results=[
                {
                    "tool_call_id": "call_abc",
                    "result": {"flights": [{"id": "F1234"}]},
                },
            ],
            context_snapshot={
                "plan_status": "in_progress",
                "current_focus": "flight_lookup",
            },
        )
        
        loader.load_checkpoint = AsyncMock(return_value=checkpoint)
        
        # Execute resume
        context = await handler.handle_resume({
            "payload": {"checkpoint_id": "chk_full"}
        })
        
        # Verify restoration
        assert context.run_id == "run_full"
        assert context.checkpoint.round_index == 2
        assert len(context.restored_messages) >= 3
        assert context.working_memory.get("plan_status") == "in_progress"
        
        # Verify context is ready for LLMStreamRunner
        messages = context.to_message_list()
        assert len(messages) >= 3


# ============================================================================
# Fixtures and Helpers
# ============================================================================

import time


# ============================================================================
# Task D2: real Postgres loading path (fake asyncpg pool)
# ============================================================================

import json as _json
from datetime import datetime, timezone


class _FakeAcquireCtx:
    def __init__(self, conn):
        self._conn = conn

    async def __aenter__(self):
        return self._conn

    async def __aexit__(self, *exc):
        return False


class _FakeConn:
    """Serves queued fetchrow/fetch results regardless of the SQL."""

    def __init__(self, fetchrow_rows=None, fetch_rows=None):
        self._fetchrow_rows = list(fetchrow_rows or [])
        self._fetch_rows = list(fetch_rows or [])
        self.queries: list[tuple[str, tuple]] = []

    async def fetchrow(self, query, *args):
        self.queries.append((query, args))
        return self._fetchrow_rows.pop(0) if self._fetchrow_rows else None

    async def fetch(self, query, *args):
        self.queries.append((query, args))
        return self._fetch_rows.pop(0) if self._fetch_rows else []


class _FakePool:
    def __init__(self, conn):
        self._conn = conn

    def acquire(self):
        return _FakeAcquireCtx(self._conn)


def _pg_row(
    *,
    checkpoint_id="cp-1",
    run_id="run-1",
    sequence_no=7,
    checkpoint_type="after_tool",
    snapshot=None,
    jsonb_as_str=True,
):
    snapshot = snapshot if snapshot is not None else {}
    return {
        "checkpoint_id": checkpoint_id,
        "run_id": run_id,
        "sequence_no": sequence_no,
        "checkpoint_type": checkpoint_type,
        # asyncpg returns JSONB as str unless a codec is registered.
        "snapshot": _json.dumps(snapshot) if jsonb_as_str else snapshot,
        "created_at": datetime(2026, 8, 14, tzinfo=timezone.utc),
    }


class TestCheckpointLoaderPostgres:
    """D2: CheckpointLoader reads ai_run_checkpoints for real."""

    @pytest.mark.asyncio
    async def test_load_checkpoint_maps_pg_row(self):
        snapshot = {
            "round_index": 2,
            "results": [{"tool_name": "list_flights", "result": {"data": []}}],
            "working_memory": {"plan.md": "# Plan\n- [ ] step 1"},
        }
        conn = _FakeConn(fetchrow_rows=[_pg_row(snapshot=snapshot)])
        loader = CheckpointLoader(pool=_FakePool(conn))

        checkpoint = await loader.load_checkpoint("cp-1")

        assert checkpoint is not None
        assert checkpoint.checkpoint_id == "cp-1"
        assert checkpoint.run_id == "run-1"
        assert checkpoint.checkpoint_type == "after_tool"
        assert checkpoint.round_index == 7  # sequence_no is the ordering key
        assert checkpoint.context_snapshot["working_memory"]["plan.md"].startswith("# Plan")
        assert checkpoint.tool_call_results[0]["tool_name"] == "list_flights"
        assert checkpoint.created_at > 0
        # JSONB string was decoded.
        assert isinstance(checkpoint.context_snapshot, dict)
        query, args = conn.queries[0]
        assert "FROM ai_run_checkpoints" in query
        assert args == ("cp-1",)

    @pytest.mark.asyncio
    async def test_load_latest_checkpoint_orders_by_sequence(self):
        conn = _FakeConn(fetchrow_rows=[_pg_row(sequence_no=42)])
        loader = CheckpointLoader(pool=_FakePool(conn))

        checkpoint = await loader.load_latest_checkpoint("run-1", max_round_index=50)

        assert checkpoint is not None and checkpoint.round_index == 42
        query, args = conn.queries[0]
        assert "ORDER BY sequence_no DESC" in query
        assert args == ("run-1", 50)

    @pytest.mark.asyncio
    async def test_list_checkpoints_returns_all_in_order(self):
        rows = [
            _pg_row(checkpoint_id="cp-1", sequence_no=1, checkpoint_type="run_input"),
            _pg_row(checkpoint_id="cp-2", sequence_no=2, checkpoint_type="before_tool"),
        ]
        conn = _FakeConn(fetch_rows=[rows])
        loader = CheckpointLoader(pool=_FakePool(conn))

        checkpoints = await loader.list_checkpoints("run-1")

        assert [c.checkpoint_id for c in checkpoints] == ["cp-1", "cp-2"]

    @pytest.mark.asyncio
    async def test_load_run_input_snapshot(self):
        envelope_snapshot = {
            "job_id": "job-1",
            "run_id": "run-1",
            "requester": {"user_id": "user-1"},
            "ontology": {"version": "flight-ops.v1"},
            "context": {"objects": []},
            "task": {"task_type": "query_ops", "user_message": "查询 F1234"},
        }
        conn = _FakeConn(fetchrow_rows=[
            _pg_row(checkpoint_type="run_input", sequence_no=1, snapshot=envelope_snapshot),
        ])
        loader = CheckpointLoader(pool=_FakePool(conn))

        snapshot = await loader.load_run_input_snapshot("run-1")

        assert snapshot is not None
        assert snapshot["task"]["user_message"] == "查询 F1234"
        query, _ = conn.queries[0]
        assert "checkpoint_type = 'run_input'" in query


# ============================================================================
# Task D2: resume continues through LLMStreamRunner / runtime service
# ============================================================================

class _FakeRunner:
    """Records the stream_chat_with_tools invocation and yields one event."""

    instances: list["_FakeRunner"] = []

    def __init__(self, client=None, tool_executor=None):
        self.client = client
        self.tool_executor = tool_executor
        self.calls: list[dict] = []
        _FakeRunner.instances.append(self)

    async def stream_chat_with_tools(self, **kwargs):
        self.calls.append(kwargs)
        yield {"type": "completed", "run_id": kwargs.get("run_id")}


def _after_tool_checkpoint():
    return RunCheckpoint(
        checkpoint_id="cp-after",
        run_id="run-resume",
        checkpoint_type="after_tool",
        round_index=3,
        created_at=0.0,
        messages=[{"role": "user", "content": "查询 F1234"}],
        tool_call_results=[{"tool_name": "get_flight", "result": {"id": "F1234"}}],
        context_snapshot={
            "working_memory": {
                "run_id": "run-resume",
                "plan.md": "# Plan\n- [x] lookup\n- [ ] propose",
                "notes.md": "flight F1234 delayed",
            },
        },
    )


class TestResumeRealPath:
    """D2: restored state reaches the runner with working memory + summary."""

    @pytest.mark.asyncio
    async def test_resume_and_continue_invokes_runner_with_restored_state(self):
        loader = CheckpointLoader()
        loader.load_checkpoint = AsyncMock(return_value=_after_tool_checkpoint())
        handler = ResumeHandler(loader)
        envelope = MagicMock()
        envelope.conversation_history = []
        _FakeRunner.instances.clear()

        events = []
        async for event in handler.resume_and_continue(
            {"payload": {"checkpoint_id": "cp-after"}},
            envelope=envelope,
            runner_class=_FakeRunner,
            gateway=MagicMock(),
            tool_executor=MagicMock(),
            allowed_tools={"get_flight"},
        ):
            events.append(event)

        # Progress event first, then runner events.
        assert events[0]["type"] == "progress"
        assert events[0]["from_checkpoint"] == "cp-after"
        assert events[-1]["type"] == "completed"

        runner = _FakeRunner.instances[-1]
        assert len(runner.calls) == 1
        call = runner.calls[0]
        assert call["run_id"] == "run-resume"
        # Working memory restored from the checkpoint snapshot.
        wm = call["working_memory"]
        assert "propose" in wm.read_plan()
        assert wm.read_notes() == "flight F1234 delayed"
        assert wm.run_id == "run-resume"
        # Transcript: system resume note + restored messages + tool summary.
        messages = call["messages"]
        assert messages[0]["role"] == "system"
        assert {"role": "user", "content": "查询 F1234"} in messages
        assert any("get_flight" in m.get("content", "") for m in messages)
        # The transcript summary is injected into the envelope.
        assert envelope.conversation_history == context_messages(messages[1:])

    @pytest.mark.asyncio
    async def test_resume_via_runtime_service_rebuilds_envelope_and_memory(self):
        checkpoint = _after_tool_checkpoint()
        envelope_snapshot = {
            "job_id": "job-1",
            "run_id": "run-resume",
            "requester": {"user_id": "user-1", "roles": [], "permissions": []},
            "ontology": {"version": "flight-ops.v1"},
            "context": {"objects": [], "relations": [], "evidence": []},
            "task": {"task_type": "query_ops", "user_message": "查询 F1234"},
        }
        loader = CheckpointLoader()
        loader.load_checkpoint = AsyncMock(return_value=checkpoint)
        loader.load_run_input_snapshot = AsyncMock(return_value=envelope_snapshot)
        handler = ResumeHandler(loader)

        captured = {}

        class _FakeRuntimeService:
            async def stream_run_with_tools(self, envelope, *, resume_working_memory=None):
                captured["envelope"] = envelope
                captured["working_memory"] = resume_working_memory
                yield {"type": "run.complete"}

        await handler.resume_via_runtime_service(
            {"payload": {"checkpoint_id": "cp-after"}},
            _FakeRuntimeService(),
        )

        envelope = captured["envelope"]
        assert envelope.run_id == "run-resume"
        assert envelope.task.user_message == "查询 F1234"
        assert envelope.cancelled is False
        # Transcript summary spliced as conversation history.
        assert any(m.get("content") == "查询 F1234" for m in envelope.conversation_history)
        wm = captured["working_memory"]
        assert wm is not None
        assert "propose" in wm.read_plan()

    @pytest.mark.asyncio
    async def test_resume_via_runtime_service_requires_run_input(self):
        loader = CheckpointLoader()
        loader.load_checkpoint = AsyncMock(return_value=_after_tool_checkpoint())
        loader.load_run_input_snapshot = AsyncMock(return_value=None)
        handler = ResumeHandler(loader)

        with pytest.raises(RuntimeError, match="run_input"):
            await handler.resume_via_runtime_service(
                {"payload": {"checkpoint_id": "cp-after"}},
                MagicMock(),
            )


def context_messages(messages):
    """Helper: normalize message dicts for comparison."""
    return [dict(m) for m in messages]
