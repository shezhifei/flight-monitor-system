"""Tests for context-budget compression wired into RuntimeService.

Exercises ``RuntimeService._apply_context_budget`` with a real
``ContextBudgetPlanner`` and multi-turn input, verifying the produced
``context.compressed`` event payload matches the plan schema.
"""

from __future__ import annotations

import asyncio
from types import SimpleNamespace

from src.infrastructure.ai.context_budget_planner import ContextBudgetPlanner
from src.infrastructure.ai.openai_client import Message, MessageRole
from src.infrastructure.ai.runtime_service import RuntimeService


def _run(coro):
    return asyncio.run(coro)


def _policy(**overrides):
    base = dict(
        strategy="hybrid",
        max_context_tokens=500,
        compression_threshold_tokens=480,
        preserve_recent_messages=2,
        summary_model=None,
        summary_max_tokens=1200,
        persist_summaries=True,
    )
    base.update(overrides)
    return SimpleNamespace(**base)


def _resolved(policy):
    return SimpleNamespace(context_policy=policy)


def _multi_turn_messages(n: int):
    msgs = [Message(role=MessageRole.SYSTEM, content="system base prompt")]
    for i in range(n):
        role = MessageRole.USER if i % 2 == 0 else MessageRole.ASSISTANT
        msgs.append(Message(role=role, content=f"turn {i} content " * 10))
    return msgs


# A system prompt long enough to push the budget below threshold.
_LONG_SYSTEM = "You are a flight ops assistant. " * 8


class TestApplyContextBudget:
    def test_compression_triggers_and_emits_payload(self):
        rs = RuntimeService(context_budget_planner=ContextBudgetPlanner())
        messages = _multi_turn_messages(10)
        new_messages, payload = _run(
            rs._apply_context_budget(
                messages=messages,
                system_prompt_text=_LONG_SYSTEM,
                tools=[],
                resolved_config=_resolved(_policy()),
                skill_instruction_tokens=0,
            )
        )

        assert payload is not None
        assert payload["strategy"] == "hybrid"
        assert payload["before_tokens"] > payload["after_tokens"]
        assert payload["summary_model"] is None
        assert payload["persisted"] is True
        assert "latency_ms" in payload
        # Hybrid without a summarizer keeps system + last preserve_recent turns.
        assert len(new_messages) < len(messages)
        # System message is preserved.
        assert any(m.role == MessageRole.SYSTEM for m in new_messages)

    def test_no_compression_when_budget_is_ample(self):
        rs = RuntimeService(context_budget_planner=ContextBudgetPlanner())
        messages = _multi_turn_messages(4)
        # Huge window, tiny threshold -> compression_needed is False.
        policy = _policy(max_context_tokens=200000, compression_threshold_tokens=1)
        new_messages, payload = _run(
            rs._apply_context_budget(
                messages=messages,
                system_prompt_text="short",
                tools=[],
                resolved_config=_resolved(policy),
                skill_instruction_tokens=0,
            )
        )
        assert payload is None
        assert new_messages is messages

    def test_no_planner_is_noop(self):
        rs = RuntimeService(context_budget_planner=None)
        messages = _multi_turn_messages(10)
        new_messages, payload = _run(
            rs._apply_context_budget(
                messages=messages,
                system_prompt_text=_LONG_SYSTEM,
                tools=[],
                resolved_config=_resolved(_policy()),
                skill_instruction_tokens=0,
            )
        )
        assert payload is None
        assert new_messages is messages

    def test_single_turn_does_not_compress(self):
        """The live read-only path assembles a single user turn -> no compression."""
        rs = RuntimeService(context_budget_planner=ContextBudgetPlanner())
        messages = [
            Message(role=MessageRole.SYSTEM, content=_LONG_SYSTEM),
            Message(role=MessageRole.USER, content="just one question"),
        ]
        new_messages, payload = _run(
            rs._apply_context_budget(
                messages=messages,
                system_prompt_text=_LONG_SYSTEM,
                tools=[],
                resolved_config=_resolved(_policy()),
                skill_instruction_tokens=0,
            )
        )
        # Budget may flag compression, but hybrid returns None with <= preserve_recent turns.
        assert payload is None
        assert new_messages is messages

    def test_sliding_window_strategy(self):
        rs = RuntimeService(context_budget_planner=ContextBudgetPlanner())
        messages = _multi_turn_messages(12)
        new_messages, payload = _run(
            rs._apply_context_budget(
                messages=messages,
                system_prompt_text=_LONG_SYSTEM,
                tools=[],
                resolved_config=_resolved(_policy(strategy="sliding_window")),
                skill_instruction_tokens=0,
            )
        )
        assert payload is not None
        assert payload["strategy"] == "sliding_window"
        assert payload["after_tokens"] <= payload["before_tokens"]
