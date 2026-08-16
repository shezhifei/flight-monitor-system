"""Structured rejection observability for tool governance gates.

Asserts that ACL / hook / lease deny paths attach machine-readable
``blocked_by`` + ``rule`` (+ ``detail``) without changing allow/deny
semantics. SSE ``to_sse_payload`` must surface the same fields.
"""

from __future__ import annotations

from unittest.mock import MagicMock

import pytest

from src.infrastructure.ai.hooks.pipeline import (
    HookContext,
    HookPipeline,
    LeaseCheckHook,
    PlanFirstHook,
)
from src.infrastructure.ai.llm_stream_runner import StreamEvent
from src.infrastructure.ai.tools.tool_executor import (
    BLOCKED_BY_ACL,
    BLOCKED_BY_HOOK,
    BLOCKED_BY_LEASE,
    ToolExecutionResult,
    ToolExecutor,
)


class TestAclBlockedByFields:
    @pytest.mark.asyncio
    async def test_acl_rejection_includes_blocked_by_and_rule(self):
        executor = ToolExecutor()
        result = await executor.execute(
            tool_call={
                "tool_call_id": "tc-acl",
                "tool_name": "get_flight_status",
                "arguments": {"flight_id": "CA123"},
            },
            run_id="run-acl",
            allowed_tool_names={"get_stand_status"},
        )

        assert result.success is False
        assert result.blocked_by == BLOCKED_BY_ACL
        assert result.rule == "TOOL_NOT_IN_ALLOWED_SET"
        assert "get_flight_status" in (result.detail or "")
        assert "TOOL_NOT_IN_ALLOWED_SET" in (result.error or "")

        payload = result.to_sse_payload()
        assert payload["blocked_by"] == "acl"
        assert payload["rule"] == "TOOL_NOT_IN_ALLOWED_SET"
        assert "detail" in payload


class TestLeaseBlockedByFields:
    @pytest.mark.asyncio
    async def test_mq_gate_unavailable_includes_lease_blocked_by(self):
        """Protected tool with no MQ gate → lease fail-closed with structured fields."""
        executor = ToolExecutor(mq_gate=None)
        result = await executor.execute(
            tool_call={
                "tool_call_id": "tc-lease",
                "tool_name": "assign_gate",
                "arguments": {"flight_id": "F1234", "gate": "A12"},
            },
            run_id="run-lease",
            allowed_tool_names=None,
        )

        assert result.success is False
        assert result.blocked_by == BLOCKED_BY_LEASE
        assert result.rule == "MQ_GATE_UNAVAILABLE"
        assert "MQ_GATE_UNAVAILABLE" in (result.error or "")

        payload = result.to_sse_payload()
        assert payload["blocked_by"] == "lease"
        assert payload["rule"] == "MQ_GATE_UNAVAILABLE"

    @pytest.mark.asyncio
    async def test_rust_lease_deny_includes_blocked_by_and_denial_code(self):
        """decision.mode=denied → blocked_by=lease, rule=<denial_code>."""
        from types import SimpleNamespace

        executor = ToolExecutor(mq_gate=MagicMock())
        decision = SimpleNamespace(
            mode="denied",
            denial_code="TOOL_ACTOR_PERMISSION_DENIED",
            denial_message="lacks write permission",
            context=SimpleNamespace(),
        )
        executor._mq_gate.request_authorization = _async_return(decision)
        executor._mq_gate.publish_result = _async_return("ok")

        result = await executor.execute(
            tool_call={
                "tool_call_id": "tc-deny",
                "tool_name": "add_flight_note",
                "arguments": {"flight_id": "CA1234", "note_content": "n"},
            },
            run_id="run-deny",
            envelope=SimpleNamespace(
                requester=SimpleNamespace(user_id="u1"),
                entity_id="ent-1",
            ),
            job_id="job-1",
        )

        assert result.success is False
        assert result.blocked_by == BLOCKED_BY_LEASE
        assert result.rule == "TOOL_ACTOR_PERMISSION_DENIED"
        assert "lacks write permission" in (result.detail or "")
        assert "TOOL_DENIED" in (result.error or "")


class TestHookBlockedByFields:
    @pytest.mark.asyncio
    async def test_lease_check_hook_sets_blocked_rule_on_context(self, monkeypatch):
        monkeypatch.setattr(
            "src.infrastructure.ai.ai_container.resolve_tool_mq_gate",
            lambda default=None: None,
        )
        pipeline = HookPipeline()
        pipeline.register_hook(LeaseCheckHook())
        ctx = HookContext(
            phase="PreToolUse",
            run_id="run-hook",
            tool_name="assign_gate",
            tool_args={"flight_id": "F1234", "gate": "A12"},
            mq_gate=None,
        )
        ok = await pipeline.execute_phase("PreToolUse", ctx)
        assert ok is False
        assert ctx.blocked_rule == "LeaseCheckHook"
        assert any("LEASE_GATE_UNAVAILABLE" in e for e in ctx.errors)

    @pytest.mark.asyncio
    async def test_pretooluse_hook_block_surfaces_on_runner_tool_result(self):
        """Write tool → PreToolUse intercept → tool_result blocked_by=hook."""
        from src.infrastructure.ai.context_envelope import (
            ContextEnvelope,
            EnvelopeContext,
            EnvelopeLimits,
            EnvelopeOntology,
            EnvelopeRequester,
            EnvelopeTask,
        )
        from src.infrastructure.ai.llm_stream_runner import LLMStreamRunner
        from src.infrastructure.ai.working_memory import WorkingMemory

        pipeline = HookPipeline()
        pipeline.register_hook(PlanFirstHook())

        runner = LLMStreamRunner(client=MagicMock())
        runner._stream_chat_impl = _scripted_stream_impl(
            [
                {
                    "tool_calls": [
                        {
                            "id": "t1",
                            "function": {
                                "name": "assign_gate",
                                "arguments": '{"flight_id": "F1234", "gate": "A12"}',
                            },
                        }
                    ],
                    "text": "",
                },
                {"tool_calls": [], "text": "blocked"},
            ]
        )
        executor = _RecordingExecutor()
        runner._tool_executor = executor
        memory = WorkingMemory(run_id="hook-obs-1")
        envelope = ContextEnvelope(
            job_id="job-hook-obs-1",
            run_id="hook-obs-1",
            entity_id="test_entity",
            requester=EnvelopeRequester(user_id="test_user", permissions=["read"]),
            ontology=EnvelopeOntology(),
            context=EnvelopeContext(limits=EnvelopeLimits(redaction="standard")),
            task=EnvelopeTask(task_type="dispatch_ops", user_message="assign a gate"),
        )

        events: list[StreamEvent] = []
        async for event in runner.stream_chat_with_tools(
            messages=[],
            model="test-model",
            run_id="hook-obs-1",
            envelope=envelope,
            max_tool_rounds=4,
            working_memory=memory,
            hook_pipeline=pipeline,
        ):
            events.append(event)

        tool_results = [e for e in events if e.type == "tool_result"]
        assert tool_results, [e.type for e in events]
        payload = tool_results[0].tool_call or {}
        assert payload["blocked_by"] == BLOCKED_BY_HOOK
        assert payload["rule"] == "PlanFirstHook"
        assert "HOOK_BLOCKED" in (payload.get("error") or "")
        assert executor.calls == []

    @pytest.mark.asyncio
    async def test_plan_first_hook_rule_name(self):
        from types import SimpleNamespace

        from src.infrastructure.ai.working_memory import WorkingMemory

        pipeline = HookPipeline()
        pipeline.register_hook(PlanFirstHook())
        # Empty plan → plan-first templates block write tools.
        ctx = HookContext(
            phase="PreToolUse",
            run_id="run-plan",
            tool_name="assign_gate",
            tool_args={"flight_id": "F1", "gate": "A1"},
            envelope=SimpleNamespace(task=SimpleNamespace(task_type="dispatch_ops")),
            working_memory=WorkingMemory(run_id="run-plan"),
        )
        ok = await pipeline.execute_phase("PreToolUse", ctx)
        assert ok is False
        assert ctx.blocked_rule == "PlanFirstHook"
        assert any("PLAN_REQUIRED" in e for e in ctx.errors)


class TestSsePayloadOmitsFieldsOnSuccess:
    def test_success_payload_has_no_blocked_fields(self):
        result = ToolExecutionResult(
            tool_call_id="ok",
            tool_name="list_flights",
            success=True,
            result={"flights": []},
        )
        payload = result.to_sse_payload()
        assert "blocked_by" not in payload
        assert "rule" not in payload
        assert "detail" not in payload
        assert payload["result"] == {"flights": []}


def _async_return(value):
    async def _inner(*_args, **_kwargs):
        return value

    return _inner


class _RecordingExecutor:
    """Minimal executor stand-in; records batches that actually reach it."""

    mq_gate = None

    def __init__(self):
        self.calls: list[list[dict]] = []

    async def execute_batch(self, parsed_calls, **kwargs):
        self.calls.append(list(parsed_calls))
        return [
            ToolExecutionResult(
                tool_call_id=pc.get("tool_call_id", ""),
                tool_name=pc.get("tool_name", ""),
                success=True,
                result={"ok": True},
            )
            for pc in parsed_calls
        ]


def _scripted_stream_impl(rounds: list[dict]):
    state = {"idx": 0}

    async def mock_impl(*args, **kwargs):
        run_result = kwargs.get("result")
        spec = rounds[min(state["idx"], len(rounds) - 1)]
        state["idx"] += 1
        if run_result is not None:
            run_result.tool_calls = [dict(tc) for tc in spec["tool_calls"]]
            run_result.text = spec["text"]
        yield StreamEvent(type="completed", result=run_result, round_index=state["idx"] - 1)

    return mock_impl
