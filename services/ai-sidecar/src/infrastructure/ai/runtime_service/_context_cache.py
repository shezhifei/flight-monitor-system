"""Mixin for RuntimeService — _apply_context_budget, _context_conversation_id, _context_cache_enabled, _read_context_cache, _write_context_cache."""

from __future__ import annotations

import logging
import time
from typing import Any

from src.infrastructure.ai.context_envelope import ContextEnvelope
from src.infrastructure.ai.openai_client import Message
from src.infrastructure.common.exceptions import REDIS_EXCEPTIONS

logger = logging.getLogger(__name__)


class _ContextCacheMixin:
    async def _apply_context_budget(
        self,
        messages: list[Message],
        system_prompt_text: str,
        tools: list[dict[str, Any]],
        resolved_config: Any,
        skill_instruction_tokens: int,
        envelope: ContextEnvelope | None = None,
    ) -> tuple[list[Message], dict[str, Any] | None]:
        """Budget-driven context compression for the assembled run messages.

        Computes a token budget from the resolved ``context_policy`` and, when the
        budget is exceeded, compresses the message list via the wired
        :class:`ContextBudgetPlanner`. Returns the (possibly compressed) messages and
        a ``context.compressed`` event payload (or ``None`` when no compression was
        applied).

        Note: the read-only orchestration path currently assembles a single user
        turn, so compression typically only triggers when callers supply multi-turn
        history. The wiring is exercised in unit tests with multi-turn input.
        """
        planner = self._context_budget_planner
        if planner is None or resolved_config is None:
            return messages, None

        policy = getattr(resolved_config, "context_policy", None)
        if policy is None:
            return messages, None

        compress_started = time.monotonic()
        try:
            budget = planner.calculate_budget(
                max_context_tokens=policy.max_context_tokens,
                system_prompt=system_prompt_text,
                tool_schemas=tools,
                skill_instruction_tokens=skill_instruction_tokens,
                compression_threshold_tokens=policy.compression_threshold_tokens,
            )
            if not budget.compression_needed:
                return messages, None

            dict_messages = [m.to_dict() for m in messages]
            # Task B3: collect critical identifiers that must survive compression.
            # Sources: the PreCompact IDPreservationHook stash on envelope metadata
            # plus a direct scan of the outgoing messages (same shared patterns).
            from src.infrastructure.ai.hooks.pipeline import extract_critical_ids

            protected_ids = extract_critical_ids(dict_messages)
            envelope_metadata = getattr(envelope, "metadata", None) or {}
            for pid in envelope_metadata.get("_protected_ids") or []:
                if pid and pid not in protected_ids:
                    protected_ids.append(pid)
            compressed_dicts, result = await planner.compress(
                messages=dict_messages,
                budget=budget,
                strategy=policy.strategy,
                preserve_recent=policy.preserve_recent_messages,
                summary_model=policy.summary_model,
                summary_max_tokens=policy.summary_max_tokens,
                persist_summaries=policy.persist_summaries,
                protected_ids=protected_ids,
            )
        except Exception as exc:  # noqa: BLE001 - best-effort side effect must not abort main flow
            # Compression is best-effort: never fail a run because budgeting errored.
            logger.warning("Context compression skipped due to error: %s", exc)
            return messages, None

        if result is None:
            return messages, None

        new_messages = [Message.from_dict(m) for m in compressed_dicts]
        payload = {
            "strategy": result.strategy,
            "before_tokens": result.before_tokens,
            "after_tokens": result.after_tokens,
            "summary_model": result.summary_model,
            "persisted": result.persisted,
            "latency_ms": max(1, int((time.monotonic() - compress_started) * 1000)),
        }
        return new_messages, payload

    @staticmethod
    def _context_conversation_id(envelope: ContextEnvelope, run_id: str) -> str:
        """Stable per-conversation key for the context cache.

        Excludes run_id from the cache *value* identity but uses correlation/job id
        as the conversation grouping; falls back to run_id when neither is present.
        """
        return (
            (getattr(envelope, "correlation_id", "") or "").strip()
            or (getattr(envelope, "job_id", "") or "").strip()
            or run_id
        )

    def _context_cache_enabled(self, resolved_config: Any) -> bool:
        if self._cache_manager is None or resolved_config is None:
            return False
        policy = getattr(resolved_config, "cache_policy", None)
        return bool(policy and getattr(policy, "enabled", False) and getattr(policy, "context_cache_enabled", False))

    async def _read_context_cache(
        self, resolved_config: Any, entity_id: str, conversation_id: str
    ) -> dict[str, Any] | None:
        """Real context-cache read; returns cached payload or None. Records hit/miss."""
        if not self._context_cache_enabled(resolved_config):
            return None
        try:
            return await self._cache_manager.get_context(entity_id, conversation_id)
        except REDIS_EXCEPTIONS as exc:  # pragma: no cover - defensive
            logger.warning("Context cache read skipped: %s", exc)
            return None

    async def _write_context_cache(
        self,
        resolved_config: Any,
        entity_id: str,
        conversation_id: str,
        messages: list[Message],
    ) -> bool:
        """Real context-cache write of the assembled (post-compression) context."""
        if not self._context_cache_enabled(resolved_config):
            return False
        try:
            ttl = int(getattr(resolved_config.cache_policy, "context_cache_ttl", 86400))
            await self._cache_manager.set_context(
                entity_id,
                conversation_id,
                {"messages": [m.to_dict() for m in messages]},
                ttl_seconds=ttl,
            )
            return True
        except REDIS_EXCEPTIONS as exc:  # pragma: no cover - defensive
            logger.warning("Context cache write skipped: %s", exc)
            return False
