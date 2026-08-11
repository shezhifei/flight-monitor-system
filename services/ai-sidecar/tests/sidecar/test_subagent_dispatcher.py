"""Tests for SubagentDispatcher runtime behavior.

Covers:
- allowlist fail-open (empty list = no delegation)
- concurrency control via per-parent-entity semaphore pool
- depth limit enforcement
- cycle detection
- max_concurrency from resolved_config is respected (not just constructor default)
"""

from __future__ import annotations

import asyncio
import threading
import time
from concurrent.futures import ThreadPoolExecutor
from unittest.mock import AsyncMock, MagicMock

import pytest

from src.infrastructure.ai.subagents import dispatcher as dispatcher_module
from src.infrastructure.ai.subagents.dispatcher import (
    SUBAGENT_TOOL_SCHEMA,
    SubagentDispatcher,
    SubagentResult,
)

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _run(coro):
    loop = asyncio.new_event_loop()
    try:
        return loop.run_until_complete(coro)
    finally:
        loop.close()


# ---------------------------------------------------------------------------
# Tests: allowlist
# ---------------------------------------------------------------------------


class TestSubagentAllowlist:
    def test_empty_allowlist_rejects_any_entity(self):
        """allowed_entity_ids=[] must reject any target_entity_id."""
        dispatcher = SubagentDispatcher()
        result = _run(
            dispatcher.dispatch(
                parent_entity_id="parent",
                target_entity_id="child",
                task="do something",
                allowed_entity_ids=[],
                max_depth=2,
            )
        )
        assert result.status == "failed"
        assert "SUBAGENT_ENTITY_NOT_ALLOWED" in result.answer

    def test_none_allowlist_rejects_any_entity(self):
        """allowed_entity_ids=None must reject any target_entity_id."""
        dispatcher = SubagentDispatcher()
        result = _run(
            dispatcher.dispatch(
                parent_entity_id="parent",
                target_entity_id="child",
                task="do something",
                allowed_entity_ids=None,
                max_depth=2,
            )
        )
        assert result.status == "failed"
        assert "SUBAGENT_ENTITY_NOT_ALLOWED" in result.answer

    def test_entity_not_in_allowlist_rejected(self):
        """target_entity_id not in non-empty allowlist must be rejected."""
        dispatcher = SubagentDispatcher()
        result = _run(
            dispatcher.dispatch(
                parent_entity_id="parent",
                target_entity_id="unauthorized",
                task="do something",
                allowed_entity_ids=["child-a", "child-b"],
                max_depth=2,
            )
        )
        assert result.status == "failed"
        assert "SUBAGENT_ENTITY_NOT_ALLOWED" in result.answer
        assert "unauthorized" in result.answer

    def test_entity_in_allowlist_accepted(self):
        """target_entity_id in allowlist must be accepted (depth check passes)."""
        factory = MagicMock(return_value=None)  # no runtime service
        dispatcher = SubagentDispatcher(runtime_service_factory=factory)
        result = _run(
            dispatcher.dispatch(
                parent_entity_id="parent",
                target_entity_id="child-a",
                task="do something",
                allowed_entity_ids=["child-a", "child-b"],
                max_depth=2,
            )
        )
        # Should proceed past allowlist check (may fail on runtime service)
        assert "SUBAGENT_ENTITY_NOT_ALLOWED" not in result.answer


# ---------------------------------------------------------------------------
# Tests: depth
# ---------------------------------------------------------------------------


class TestSubagentDepth:
    def test_depth_exceeded_returns_error(self):
        """subagent_depth >= max_depth must return SUBAGENT_MAX_DEPTH_EXCEEDED."""
        dispatcher = SubagentDispatcher()
        result = _run(
            dispatcher.dispatch(
                parent_entity_id="parent",
                target_entity_id="child",
                task="do something",
                subagent_depth=1,
                max_depth=1,
                allowed_entity_ids=["child"],
            )
        )
        assert result.status == "failed"
        assert "SUBAGENT_MAX_DEPTH_EXCEEDED" in result.answer

    def test_depth_zero_allowed(self):
        """subagent_depth=0 < max_depth=1 must pass depth check."""
        factory = MagicMock(return_value=None)
        dispatcher = SubagentDispatcher(runtime_service_factory=factory)
        result = _run(
            dispatcher.dispatch(
                parent_entity_id="parent",
                target_entity_id="child",
                task="do something",
                subagent_depth=0,
                max_depth=1,
                allowed_entity_ids=["child"],
            )
        )
        assert "SUBAGENT_MAX_DEPTH_EXCEEDED" not in result.answer


# ---------------------------------------------------------------------------
# Tests: cycle detection
# ---------------------------------------------------------------------------


class TestSubagentCycle:
    def test_cycle_detected_returns_error(self):
        """target_entity_id in subagent_trace must return SUBAGENT_CYCLE_DETECTED."""
        dispatcher = SubagentDispatcher()
        result = _run(
            dispatcher.dispatch(
                parent_entity_id="parent",
                target_entity_id="entity-x",
                task="do something",
                subagent_trace=["entity-x"],
                max_depth=5,
                allowed_entity_ids=["entity-x"],
            )
        )
        assert result.status == "failed"
        assert "SUBAGENT_CYCLE_DETECTED" in result.answer

    def test_trace_not_mutated_on_success(self):
        """subagent_trace passed to dispatch should not be mutated."""
        dispatcher = SubagentDispatcher()
        original_trace = ["entity-a"]
        _run(
            dispatcher.dispatch(
                parent_entity_id="parent",
                target_entity_id="entity-a",
                task="do something",
                subagent_trace=original_trace,
                max_depth=5,
                allowed_entity_ids=["entity-a"],
            )
        )
        assert original_trace == ["entity-a"]


# ---------------------------------------------------------------------------
# Tests: concurrency — per-parent-entity semaphore pool
# ---------------------------------------------------------------------------


class TestSubagentConcurrency:
    def test_get_semaphore_creates_one_semaphore_when_threads_race(self, monkeypatch):
        """Concurrent callers for the same key must share one semaphore object."""
        dispatcher = SubagentDispatcher()
        created_semaphores = []
        creation_guard = threading.Lock()

        def slow_semaphore_factory(value):
            semaphore = object()
            with creation_guard:
                created_semaphores.append((value, semaphore))
            time.sleep(0.05)
            return semaphore

        monkeypatch.setattr(dispatcher_module.asyncio, "Semaphore", slow_semaphore_factory)

        with ThreadPoolExecutor(max_workers=8) as executor:
            semaphores = list(executor.map(lambda _: dispatcher._get_semaphore("parent", 2), range(8)))

        assert len(created_semaphores) == 1
        assert created_semaphores[0][0] == 2
        assert len({id(semaphore) for semaphore in semaphores}) == 1
        assert dispatcher._semaphore_pool[("parent", 2)] is semaphores[0]

    @pytest.mark.asyncio
    async def test_get_semaphore_reuses_semaphore_for_concurrent_coroutines(self):
        """Coroutine callers for the same key must observe one shared semaphore."""
        dispatcher = SubagentDispatcher()

        async def get_semaphore():
            await asyncio.sleep(0)
            return dispatcher._get_semaphore("parent", 2)

        semaphores = await asyncio.gather(*(get_semaphore() for _ in range(8)))

        assert len({id(semaphore) for semaphore in semaphores}) == 1

    def test_get_semaphore_keeps_different_concurrency_limits_separate(self):
        dispatcher = SubagentDispatcher()

        one_at_a_time = dispatcher._get_semaphore("parent", 1)
        two_at_a_time = dispatcher._get_semaphore("parent", 2)

        assert one_at_a_time is not two_at_a_time

    def test_dispatch_max_concurrency_1_overrides_constructor_2(self):
        """dispatch(max_concurrency=1) must serialize even if dispatcher was constructed with max_concurrency=2.

        This proves the per-call max_concurrency from resolved_config is used,
        not just the constructor default.
        """
        call_log = []

        def fake_factory(entity_id):
            mock_service = AsyncMock()
            mock_service.execute_run = AsyncMock(
                return_value=MagicMock(
                    contract_version="ai-structured-output.v1",
                    run_id="test",
                    status="succeeded",
                    answer="ok",
                    reasoning_steps=[],
                    evidence=[],
                    proposals=[],
                    limitations=[],
                    metrics=None,
                    token_usage=None,
                )
            )
            # Patch execute_run to log timing
            original_execute = mock_service.execute_run

            async def timed_execute(envelope):
                call_log.append(("start", entity_id, asyncio.get_event_loop().time()))
                await asyncio.sleep(0.1)
                call_log.append(("end", entity_id, asyncio.get_event_loop().time()))
                return await original_execute(envelope)

            mock_service.execute_run = timed_execute
            return mock_service

        # Constructed with default max_concurrency=2
        dispatcher = SubagentDispatcher(
            runtime_service_factory=fake_factory,
            max_concurrency=2,
        )

        async def run_parallel():
            # But each dispatch call passes max_concurrency=1 (from resolved_config)
            tasks = [
                dispatcher.dispatch(
                    parent_entity_id="parent",
                    target_entity_id=f"child-{i}",
                    task=f"task-{i}",
                    allowed_entity_ids=["child-0", "child-1", "child-2"],
                    max_depth=2,
                    max_concurrency=1,  # from resolved_config
                )
                for i in range(3)
            ]
            return await asyncio.gather(*tasks)

        results = _run(run_parallel())
        assert all(r.status == "succeeded" for r in results)

        # With max_concurrency=1, starts must not overlap with ends
        starts = [e for e in call_log if e[0] == "start"]
        ends = [e for e in call_log if e[0] == "end"]
        for i in range(1, len(starts)):
            assert starts[i][2] >= ends[i - 1][2], (
                f"Start {i} at {starts[i][2]} should be >= end {i - 1} at {ends[i - 1][2]}"
            )

    def test_different_parents_get_independent_semaphores(self):
        """Different parent_entity_ids get independent semaphores."""
        call_log = []

        def fake_factory(entity_id):
            mock_service = AsyncMock()
            mock_service.execute_run = AsyncMock(
                return_value=MagicMock(
                    contract_version="ai-structured-output.v1",
                    run_id="test",
                    status="succeeded",
                    answer="ok",
                    reasoning_steps=[],
                    evidence=[],
                    proposals=[],
                    limitations=[],
                    metrics=None,
                    token_usage=None,
                )
            )
            original_execute = mock_service.execute_run

            async def timed_execute(envelope):
                call_log.append(("start", entity_id, asyncio.get_event_loop().time()))
                await asyncio.sleep(0.1)
                call_log.append(("end", entity_id, asyncio.get_event_loop().time()))
                return await original_execute(envelope)

            mock_service.execute_run = timed_execute
            return mock_service

        dispatcher = SubagentDispatcher(runtime_service_factory=fake_factory)

        async def run_parallel():
            tasks = [
                dispatcher.dispatch(
                    parent_entity_id="parent-a",
                    target_entity_id="child-0",
                    task="task-0",
                    allowed_entity_ids=["child-0"],
                    max_depth=2,
                    max_concurrency=1,
                ),
                dispatcher.dispatch(
                    parent_entity_id="parent-b",
                    target_entity_id="child-0",
                    task="task-1",
                    allowed_entity_ids=["child-0"],
                    max_depth=2,
                    max_concurrency=1,
                ),
            ]
            return await asyncio.gather(*tasks)

        results = _run(run_parallel())
        assert all(r.status == "succeeded" for r in results)

        # Different parents should run concurrently (not serialized)
        starts = [e for e in call_log if e[0] == "start"]
        ends = [e for e in call_log if e[0] == "end"]
        # Both starts should happen before either end (concurrent)
        assert starts[1][2] < ends[0][2], "Different parents should run concurrently"

    def test_tool_executor_passes_metadata_concurrency_to_dispatcher(self):
        """ToolExecutor reads max_concurrency from envelope.metadata and passes to dispatcher."""
        from src.infrastructure.ai.tools.tool_executor import ToolExecutor

        dispatch_calls = []

        async def mock_dispatch(**kwargs):
            dispatch_calls.append(kwargs)
            return SubagentResult(
                entity_id=kwargs.get("target_entity_id", ""),
                status="succeeded",
                answer="ok",
            )

        mock_dispatcher = MagicMock()
        mock_dispatcher.dispatch = mock_dispatch

        executor = ToolExecutor(subagent_dispatcher=mock_dispatcher)

        envelope = MagicMock()
        envelope.entity_id = "parent"
        envelope.metadata = {
            "subagent_depth": 0,
            "subagent_trace": [],
            "subagent_allowed_entity_ids": ["child"],
            "subagent_max_depth": 2,
            "subagent_max_concurrency": 1,
            "subagent_inherit_parent_context": True,
        }

        result = _run(
            executor.execute(
                {
                    "tool_call_id": "tc-1",
                    "tool_name": "delegate_to_subagent",
                    "arguments": {"entity_id": "child", "task": "do stuff"},
                },
                "run-1",
                envelope=envelope,
            )
        )

        assert result.success is True
        assert len(dispatch_calls) == 1
        assert dispatch_calls[0]["max_concurrency"] == 1


# ---------------------------------------------------------------------------
# Tests: tool schema
# ---------------------------------------------------------------------------


class TestSubagentToolSchema:
    def test_tool_schema_has_required_fields(self):
        assert SUBAGENT_TOOL_SCHEMA["type"] == "function"
        func = SUBAGENT_TOOL_SCHEMA["function"]
        assert func["name"] == "delegate_to_subagent"
        assert "entity_id" in func["parameters"]["properties"]
        assert "task" in func["parameters"]["properties"]
        assert "entity_id" in func["parameters"]["required"]
        assert "task" in func["parameters"]["required"]
