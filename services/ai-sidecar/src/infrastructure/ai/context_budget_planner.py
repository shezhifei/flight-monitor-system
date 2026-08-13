"""上下文预算规划器 - 预算驱动的上下文压缩

在每次 LLM 请求前计算 token budget，决定是否需要压缩。
"""

from __future__ import annotations

import logging
from dataclasses import dataclass
from typing import Any

from src.infrastructure.common.exceptions import LLM_EXCEPTIONS

logger = logging.getLogger(__name__)


@dataclass
class ContextBudget:
    """上下文预算"""

    max_context_tokens: int
    system_prompt_tokens: int
    tool_schema_tokens: int
    skill_instruction_tokens: int
    available_for_messages: int
    compression_needed: bool
    compression_threshold: int


@dataclass
class CompressionResult:
    """压缩结果"""

    strategy: str
    before_tokens: int
    after_tokens: int
    preserved_messages: list[dict[str, Any]]
    summary: str | None
    summary_model: str | None
    persisted: bool


class ContextBudgetPlanner:
    """上下文预算规划器

    职责：
    1. 计算系统提示、工具 schema、skill 指令的 token 占用
    2. 确定消息可用的 token 预算
    3. 判断是否需要压缩
    4. 执行压缩策略
    """

    def __init__(self, token_counter=None, summarizer=None):
        self._token_counter = token_counter
        self._summarizer = summarizer

    def calculate_budget(
        self,
        max_context_tokens: int,
        system_prompt: str,
        tool_schemas: list[dict[str, Any]],
        skill_instruction_tokens: int = 0,
        compression_threshold_tokens: int = 0,
    ) -> ContextBudget:
        """计算上下文预算"""
        system_tokens = self._count_tokens(system_prompt)
        tool_tokens = sum(self._count_tokens(str(schema)) for schema in tool_schemas)

        overhead = system_tokens + tool_tokens + skill_instruction_tokens
        available = max_context_tokens - overhead

        threshold = compression_threshold_tokens or int(max_context_tokens * 0.75)
        needs_compression = available < threshold

        return ContextBudget(
            max_context_tokens=max_context_tokens,
            system_prompt_tokens=system_tokens,
            tool_schema_tokens=tool_tokens,
            skill_instruction_tokens=skill_instruction_tokens,
            available_for_messages=max(0, available),
            compression_needed=needs_compression,
            compression_threshold=threshold,
        )

    async def compress(
        self,
        messages: list[dict[str, Any]],
        budget: ContextBudget,
        strategy: str = "hybrid",
        preserve_recent: int = 12,
        summary_model: str | None = None,
        summary_max_tokens: int = 1200,
        persist_summaries: bool = True,
    ) -> tuple[list[dict[str, Any]], CompressionResult | None]:
        """压缩上下文

        Args:
            messages: 消息列表
            budget: 上下文预算
            strategy: 压缩策略 (sliding_window/summary_compression/hybrid)
            preserve_recent: 保留最近消息数
            summary_model: 摘要模型
            summary_max_tokens: 摘要最大 token 数
            persist_summaries: 是否持久化摘要

        Returns:
            (压缩后的消息列表, 压缩结果)
        """
        if not budget.compression_needed:
            return messages, None

        self._count_messages_tokens(messages)

        if strategy == "sliding_window":
            return await self._sliding_window_compression(messages, budget, preserve_recent)
        elif strategy == "summary_compression":
            return await self._summary_compression(
                messages, budget, summary_model, summary_max_tokens, persist_summaries
            )
        else:  # hybrid
            return await self._hybrid_compression(
                messages, budget, preserve_recent, summary_model, summary_max_tokens, persist_summaries
            )

    async def _sliding_window_compression(
        self,
        messages: list[dict[str, Any]],
        budget: ContextBudget,
        preserve_recent: int,
    ) -> tuple[list[dict[str, Any]], CompressionResult | None]:
        """滑动窗口压缩：只保留最近消息"""
        before_tokens = self._count_messages_tokens(messages)

        # 分离 system 消息和非 system 消息
        system_msgs = [m for m in messages if m.get("role") == "system"]
        non_system_msgs = [m for m in messages if m.get("role") != "system"]

        # 保留最近的消息
        preserved = non_system_msgs[-preserve_recent:] if preserve_recent > 0 else []

        # 确保在预算内
        result_messages = system_msgs + preserved
        after_tokens = self._count_messages_tokens(result_messages)

        while after_tokens > budget.available_for_messages and len(preserved) > 2:
            preserved = preserved[1:]  # 移除最旧的消息
            result_messages = system_msgs + preserved
            after_tokens = self._count_messages_tokens(result_messages)

        result = CompressionResult(
            strategy="sliding_window",
            before_tokens=before_tokens,
            after_tokens=after_tokens,
            preserved_messages=result_messages,
            summary=None,
            summary_model=None,
            persisted=False,
        )

        logger.info(
            f"Sliding window compression: {before_tokens} -> {after_tokens} tokens, "
            f"removed {len(messages) - len(result_messages)} messages"
        )

        return result_messages, result

    async def _summary_compression(
        self,
        messages: list[dict[str, Any]],
        budget: ContextBudget,
        summary_model: str | None,
        summary_max_tokens: int,
        persist: bool,
    ) -> tuple[list[dict[str, Any]], CompressionResult | None]:
        """摘要压缩：将旧消息压缩为摘要"""
        before_tokens = self._count_messages_tokens(messages)

        system_msgs = [m for m in messages if m.get("role") == "system"]
        non_system_msgs = [m for m in messages if m.get("role") != "system"]

        if len(non_system_msgs) <= 2:
            return messages, None

        # 分割旧消息和最近消息
        split_point = max(1, len(non_system_msgs) - 4)
        old_messages = non_system_msgs[:split_point]
        recent_messages = non_system_msgs[split_point:]

        # 生成摘要
        summary = None
        if self._summarizer and summary_model:
            try:
                summary = await self._summarizer.summarize(
                    old_messages,
                    model=summary_model,
                    max_tokens=summary_max_tokens,
                )
            except LLM_EXCEPTIONS as e:
                logger.warning(f"Summary generation failed: {e}")

        if not summary:
            # 降级到滑动窗口
            return await self._sliding_window_compression(messages, budget, 8)

        # 构建摘要消息
        summary_message = {
            "role": "system",
            "content": f"[对话历史摘要]\n{summary}",
        }

        result_messages = [*system_msgs, summary_message, *recent_messages]
        after_tokens = self._count_messages_tokens(result_messages)

        result = CompressionResult(
            strategy="summary_compression",
            before_tokens=before_tokens,
            after_tokens=after_tokens,
            preserved_messages=result_messages,
            summary=summary,
            summary_model=summary_model,
            persisted=persist,
        )

        logger.info(f"Summary compression: {before_tokens} -> {after_tokens} tokens")

        return result_messages, result

    async def _hybrid_compression(
        self,
        messages: list[dict[str, Any]],
        budget: ContextBudget,
        preserve_recent: int,
        summary_model: str | None,
        summary_max_tokens: int,
        persist: bool,
    ) -> tuple[list[dict[str, Any]], CompressionResult | None]:
        """混合压缩：system + 最近 N 轮原文 + 历史摘要"""
        before_tokens = self._count_messages_tokens(messages)

        system_msgs = [m for m in messages if m.get("role") == "system"]
        non_system_msgs = [m for m in messages if m.get("role") != "system"]

        if len(non_system_msgs) <= preserve_recent:
            return messages, None

        # 分割
        old_messages = non_system_msgs[:-preserve_recent]
        recent_messages = non_system_msgs[-preserve_recent:]

        # 生成摘要
        summary = None
        if self._summarizer and summary_model:
            try:
                summary = await self._summarizer.summarize(
                    old_messages,
                    model=summary_model,
                    max_tokens=summary_max_tokens,
                )
            except LLM_EXCEPTIONS as e:
                logger.warning(f"Summary generation failed: {e}")

        if summary:
            summary_message = {
                "role": "system",
                "content": f"[对话历史摘要]\n{summary}",
            }
            result_messages = [*system_msgs, summary_message, *recent_messages]
        else:
            result_messages = system_msgs + recent_messages

        after_tokens = self._count_messages_tokens(result_messages)

        result = CompressionResult(
            strategy="hybrid",
            before_tokens=before_tokens,
            after_tokens=after_tokens,
            preserved_messages=result_messages,
            summary=summary,
            summary_model=summary_model,
            persisted=persist,
        )

        logger.info(f"Hybrid compression: {before_tokens} -> {after_tokens} tokens")

        return result_messages, result

    def _count_tokens(self, text: str) -> int:
        """估算 token 数"""
        if self._token_counter:
            return self._token_counter.count_tokens(text)
        ascii_chars = sum(1 for c in text if ord(c) < 128)
        non_ascii_chars = len(text) - ascii_chars
        return (ascii_chars // 4) + non_ascii_chars

    def _count_messages_tokens(self, messages: list[dict[str, Any]]) -> int:
        """计算消息列表的 token 数"""
        total = 0
        for msg in messages:
            content = msg.get("content", "")
            if isinstance(content, str):
                total += self._count_tokens(content)
            elif isinstance(content, list):
                for item in content:
                    if isinstance(item, dict) and "text" in item:
                        total += self._count_tokens(item["text"])
        return total


__all__ = [
    "CompressionResult",
    "ContextBudget",
    "ContextBudgetPlanner",
]
