"""Tests for critical-ID preservation through context compression (Task B3).

Asserts:
1. IDPreservationHook and the compression path share one extraction routine
2. After compression (all strategies), critical IDs — flight numbers,
   anomaly ids, proposal ids, order ids — are still present in the messages
3. RuntimeService._apply_context_budget consumes envelope metadata
   ``_protected_ids`` (the PreCompact hook stash) and re-injects missing IDs
"""

from __future__ import annotations

import asyncio
from types import SimpleNamespace

from src.infrastructure.ai.context_budget_planner import (
    ContextBudget,
    ContextBudgetPlanner,
)
from src.infrastructure.ai.hooks.pipeline import extract_critical_ids
from src.infrastructure.ai.openai_client import Message, MessageRole
from src.infrastructure.ai.runtime_service import RuntimeService


def _run(coro):
    return asyncio.run(coro)


_CRITICAL_IDS = [
    "F1234",
    "F5678",
    "CA1832",
    "8899",
    "3f2a9c1e-8b4d-4a67-9e2f-1c0d5b7a8e91",
    "ANOMALY-GT123",
    "PROP-AB12",
    "ORDER-77",
]


def _conversation_with_ids(turns: int = 20) -> list[dict]:
    """Over-budget conversation whose critical IDs live in the OLD turns."""
    messages = [{"role": "system", "content": "You are a flight ops assistant. " * 8}]
    messages.append(
        {
            "role": "user",
            "content": (
                "排查航班 F1234 与 F5678 的机位冲突，国内航班 CA1832 与四位数航班 8899，"
                "flight_id 为 3f2a9c1e-8b4d-4a67-9e2f-1c0d5b7a8e91，"
                "关联异常 ANOMALY-GT123，待审批提案 PROP-AB12，工单 ORDER-77。"
                + "背景细节。" * 40
            ),
        }
    )
    for i in range(turns):
        role = "assistant" if i % 2 == 0 else "user"
        messages.append({"role": role, "content": f"常规往返对话 {i} " + "填充内容。" * 40})
    return messages


def _messages_text(messages: list[dict]) -> str:
    return "\n".join(m.get("content", "") or "" for m in messages)


def _over_budget() -> ContextBudget:
    return ContextBudget(
        max_context_tokens=200,
        system_prompt_tokens=50,
        tool_schema_tokens=0,
        skill_instruction_tokens=0,
        available_for_messages=150,
        compression_needed=True,
        compression_threshold=150,
    )


class TestExtractCriticalIds:
    def test_extracts_all_critical_patterns(self):
        ids = extract_critical_ids(_conversation_with_ids(2))
        for expected in _CRITICAL_IDS:
            assert expected in ids

    def test_ignores_non_string_content_and_empty(self):
        assert extract_critical_ids(None) == []
        assert extract_critical_ids([{"role": "user", "content": [{"text": "F1234"}]}]) == []


class TestCompressionPreservesIds:
    def _assert_ids_present(self, strategy: str):
        planner = ContextBudgetPlanner()
        messages = _conversation_with_ids()
        compressed, result = _run(
            planner.compress(
                messages=messages,
                budget=_over_budget(),
                strategy=strategy,
                preserve_recent=4,
                protected_ids=_CRITICAL_IDS,
            )
        )

        assert result is not None
        assert len(compressed) < len(messages)
        text = _messages_text(compressed)
        for expected in _CRITICAL_IDS:
            assert expected in text, f"{expected} lost by {strategy} compression"

    def test_hybrid_preserves_ids(self):
        self._assert_ids_present("hybrid")

    def test_sliding_window_preserves_ids(self):
        self._assert_ids_present("sliding_window")

    def test_summary_compression_fallback_preserves_ids(self):
        # No summarizer configured -> degrades to sliding window; IDs still kept.
        self._assert_ids_present("summary_compression")

    def test_no_guard_message_when_ids_survive(self):
        planner = ContextBudgetPlanner()
        messages = _conversation_with_ids(turns=6)
        # Keep enough recent turns that the ID-bearing first turn survives.
        compressed, result = _run(
            planner.compress(
                messages=messages,
                budget=_over_budget(),
                strategy="hybrid",
                preserve_recent=30,
                protected_ids=_CRITICAL_IDS,
            )
        )
        if result is not None:
            assert not any("保留的关键标识" in (m.get("content") or "") for m in compressed)

    def test_noop_without_protected_ids(self):
        planner = ContextBudgetPlanner()
        messages = _conversation_with_ids()
        compressed, result = _run(
            planner.compress(
                messages=messages,
                budget=_over_budget(),
                strategy="hybrid",
                preserve_recent=4,
            )
        )
        assert result is not None
        assert not any("保留的关键标识" in (m.get("content") or "") for m in compressed)


class TestRuntimeBudgetConsumesProtectedIds:
    def _policy(self):
        return SimpleNamespace(
            strategy="hybrid",
            max_context_tokens=500,
            compression_threshold_tokens=480,
            preserve_recent_messages=2,
            summary_model=None,
            summary_max_tokens=1200,
            persist_summaries=True,
        )

    def _envelope(self, protected_ids):
        return SimpleNamespace(metadata={"_protected_ids": protected_ids})

    def test_envelope_protected_ids_survive_compression(self):
        rs = RuntimeService(context_budget_planner=ContextBudgetPlanner())
        raw = _conversation_with_ids()
        messages = [
            Message(role=MessageRole.SYSTEM if m["role"] == "system" else (
                MessageRole.USER if m["role"] == "user" else MessageRole.ASSISTANT
            ), content=m["content"])
            for m in raw
        ]

        new_messages, payload = _run(
            rs._apply_context_budget(
                messages=messages,
                system_prompt_text="You are a flight ops assistant. " * 8,
                tools=[],
                resolved_config=SimpleNamespace(context_policy=self._policy()),
                skill_instruction_tokens=0,
                envelope=self._envelope(list(_CRITICAL_IDS)),
            )
        )

        assert payload is not None
        text = "\n".join(
            m.content for m in new_messages if isinstance(m.content, str)
        )
        for expected in _CRITICAL_IDS:
            assert expected in text, f"{expected} lost through runtime compression"

    def test_direct_message_scan_without_envelope_stash(self):
        """Even without the hook stash, IDs in the messages themselves survive."""
        rs = RuntimeService(context_budget_planner=ContextBudgetPlanner())
        raw = _conversation_with_ids()
        messages = [
            Message(role=MessageRole.SYSTEM if m["role"] == "system" else (
                MessageRole.USER if m["role"] == "user" else MessageRole.ASSISTANT
            ), content=m["content"])
            for m in raw
        ]

        new_messages, payload = _run(
            rs._apply_context_budget(
                messages=messages,
                system_prompt_text="You are a flight ops assistant. " * 8,
                tools=[],
                resolved_config=SimpleNamespace(context_policy=self._policy()),
                skill_instruction_tokens=0,
            )
        )

        assert payload is not None
        text = "\n".join(
            m.content for m in new_messages if isinstance(m.content, str)
        )
        for expected in _CRITICAL_IDS:
            assert expected in text
